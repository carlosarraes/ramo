use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ramo::app::App;
use ramo::config::ResolvedConfig;
use ramo::diff::parser::parse_unified_diff;
use ramo::review::Viewport;
use ramo::ui::input::InputMode;

const VIEW: Viewport = Viewport {
    width: 120,
    height: 24,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn files() -> Vec<ramo::diff::model::DiffFile> {
    parse_unified_diff(concat!(
        "diff --git a/src/retry.rs b/src/retry.rs\n",
        "--- a/src/retry.rs\n",
        "+++ b/src/retry.rs\n",
        "@@ -0,0 +1,2 @@\n",
        "+let backoff = base * 2;\n",
        "+retry(attempt);\n",
    ))
}

fn enabled() -> ResolvedConfig {
    ResolvedConfig {
        chat_enabled: true,
        ..ResolvedConfig::default()
    }
}

fn recording_app(answer: &'static str) -> (App, Arc<Mutex<Vec<ramo::ask::AskRequest>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let app = App::new_with_config(files(), &enabled(), false).with_ask_runner(move |request| {
        sink.lock().unwrap().push(request.clone());
        move || Ok(answer.to_owned())
    });
    (app, seen)
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if app.poll_chat_for_tests() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no chat reply arrived");
}

#[test]
fn c_opens_the_pane_and_the_diff_keeps_rendering_beside_it() {
    let (mut app, _) = recording_app("because the upstream 429s");

    let before = app.render_to_string(120, 24);
    assert!(before.contains("let backoff"), "{before}");

    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    assert_eq!(app.input_mode(), InputMode::Chat);

    let after = app.render_to_string(120, 24);
    // Both panes are visible: the diff reflowed rather than being covered.
    assert!(after.contains("let backoff"), "{after}");
    assert!(after.contains("Ask about this pull request"), "{after}");
}

#[test]
fn c_hands_focus_back_without_closing_the_pane() {
    let (mut app, _) = recording_app("answer");

    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    assert_eq!(app.input_mode(), InputMode::Chat);

    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    assert_eq!(
        app.input_mode(),
        InputMode::Normal,
        "focus returns to the diff"
    );

    // The pane stays on screen so a reply can land while you read.
    let frame = app.render_to_string(120, 24);
    assert!(frame.contains("Ask about this pull request"), "{frame}");
}

#[test]
fn typing_goes_to_the_chat_draft_not_the_diff() {
    let (mut app, _) = recording_app("answer");
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);

    // `q` would quit and `j` would move the cursor if these leaked to the diff.
    for character in "why?".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
    }
    assert!(!app.should_quit);

    let frame = app.render_to_string(120, 24);
    assert!(frame.contains("> why?"), "{frame}");
}

#[test]
fn a_turn_is_pending_then_answered_and_the_reply_lands_while_reading_the_diff() {
    let (mut app, _) = recording_app("because the upstream 429s");
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    for character in "why the backoff?".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
    }
    app.handle_ui_key(key(KeyCode::Enter), VIEW);

    let pending = app.render_to_string(120, 24);
    assert!(pending.contains("you: why the backoff?"), "{pending}");
    assert!(pending.contains("thinking"), "{pending}");

    // Hand focus back to the diff; the reply must still arrive.
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    assert_eq!(app.input_mode(), InputMode::Normal);
    settle(&mut app);

    let answered = app.render_to_string(120, 24);
    assert!(answered.contains("upstream 429s"), "{answered}");
    assert!(!answered.contains("thinking"), "{answered}");
}

#[test]
fn the_first_turn_carries_context_and_is_read_only_with_one_session() {
    let (mut app, seen) = recording_app("answer");
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    for character in "first".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
    }
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    settle(&mut app);
    for character in "second".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
    }
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    settle(&mut app);

    let requests = seen.lock().unwrap().clone();
    assert_eq!(requests.len(), 2);

    // Read-only by construction, and both turns share one named session.
    for request in &requests {
        assert_eq!(
            request.tools,
            ramo::ask::PiTools::Allow(vec!["read".to_owned()])
        );
        assert!(matches!(request.session, ramo::ask::PiSession::Id(_)));
    }
    assert_eq!(requests[0].session, requests[1].session);

    assert!(
        requests[0].prompt.contains("CURRENTLY READING"),
        "{:?}",
        requests[0].prompt
    );
    assert!(
        requests[0].prompt.contains("src/retry.rs"),
        "{:?}",
        requests[0].prompt
    );
    // The session carries the thread, so the second turn does not repeat the context.
    assert!(
        !requests[1].prompt.contains("CURRENTLY READING"),
        "{:?}",
        requests[1].prompt
    );
}

#[test]
fn the_pane_is_refused_when_disabled_and_suppressed_on_a_narrow_terminal() {
    let mut off = App::new_with_config(files(), &ResolvedConfig::default(), false);
    off.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    assert_eq!(off.input_mode(), InputMode::Normal);
    assert!(
        off.toast.as_deref().is_some_and(|t| t.contains("[chat]")),
        "{:?}",
        off.toast
    );

    // Open on a wide terminal, then render narrow: the diff keeps the width rather than
    // being squeezed, exactly as the sidebar behaves.
    let (mut app, _) = recording_app("answer");
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    let narrow = app.render_to_string(80, 24);
    assert!(narrow.contains("let backoff"), "{narrow}");
    assert!(!narrow.contains("Ask about this pull request"), "{narrow}");
}

fn shift(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

#[test]
fn ctrl_q_closes_the_pane_and_gives_the_width_back_to_the_diff() {
    let (mut app, _) = recording_app("answer");
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    assert!(app.render_to_string(120, 24).contains("Ask about this"));

    app.handle_ui_key(ctrl(KeyCode::Char('q')), VIEW);

    assert_eq!(app.input_mode(), InputMode::Normal);
    let frame = app.render_to_string(120, 24);
    assert!(
        !frame.contains("Ask about this"),
        "pane still drawn:\n{frame}"
    );
    assert!(frame.contains("let backoff"), "{frame}");
}

#[test]
fn a_turn_keeps_running_while_the_pane_is_closed_and_is_there_on_reopen() {
    let (mut app, _) = recording_app("because the upstream 429s");
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    for character in "why?".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
    }
    app.handle_ui_key(key(KeyCode::Enter), VIEW);

    // Closing is a display decision; the request must not be cancelled with it.
    app.handle_ui_key(ctrl(KeyCode::Char('q')), VIEW);
    settle(&mut app);

    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    let frame = app.render_to_string(120, 24);
    assert!(frame.contains("upstream 429s"), "reply lost:\n{frame}");
}

#[test]
fn a_multi_line_draft_stays_on_screen_while_typing() {
    let (mut app, _) = recording_app("answer");
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);

    for character in "first line here".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
    }
    app.handle_ui_key(shift(KeyCode::Enter), VIEW);
    for character in "second line here".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
    }

    let frame = app.render_to_string(120, 24);
    let first = row_of(&frame, "first line here").expect("first line missing");
    let second = row_of(&frame, "second line here").expect("second line fell out of view");
    assert!(
        second > first,
        "both lines rendered onto row {first}; the input never grew:\n{frame}"
    );
}

/// Which rendered row a fragment lands on, so a test can tell "visible" from "on its own line".
fn row_of(frame: &str, needle: &str) -> Option<usize> {
    frame.lines().position(|line| line.contains(needle))
}

#[test]
fn a_long_draft_wraps_instead_of_running_off_the_pane() {
    let (mut app, _) = recording_app("answer");
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);

    let long = "why does the retry backoff double on every attempt instead of staying flat";
    for character in long.chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
    }

    let frame = app.render_to_string(120, 24);
    assert!(
        frame.contains("staying flat"),
        "the tail of the draft is unreachable:\n{frame}"
    );
}

#[test]
fn page_up_scrolls_back_through_the_transcript() {
    let (mut app, _) = recording_app("answer");
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    for turn in 0..8 {
        for character in format!("question number {turn} about the retry code").chars() {
            app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
        }
        app.handle_ui_key(key(KeyCode::Enter), VIEW);
        settle(&mut app);
    }

    let bottom = app.render_to_string(120, 24);
    assert!(!bottom.contains("question number 0"), "{bottom}");

    app.handle_ui_key(key(KeyCode::PageUp), VIEW);
    app.handle_ui_key(key(KeyCode::PageUp), VIEW);
    app.handle_ui_key(key(KeyCode::PageUp), VIEW);
    let scrolled = app.render_to_string(120, 24);
    assert!(
        scrolled.contains("question number 0"),
        "cannot reach the oldest turn:\n{scrolled}"
    );
}
