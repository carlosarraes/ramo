use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ramo::app::App;
use ramo::ask::AskError;
use ramo::config::ResolvedConfig;
use ramo::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, SourceSpec,
};
use ramo::notes::AskNoteState;
use ramo::review::Viewport;
use ramo::ui::input::InputMode;

const VIEW: Viewport = Viewport {
    width: 100,
    height: 24,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn file() -> DiffFile {
    DiffFile {
        id: "file:src/lib.rs".into(),
        path: "src/lib.rs".into(),
        previous_path: None,
        summary: None,
        agent: None,
        patch: String::new(),
        hunks: vec![Hunk {
            old_start: 1,
            new_start: 1,
            header: "@@ -1,2 +1,2 @@ fn demo()".into(),
            lines: vec![
                DiffLine {
                    kind: LineType::Deletion,
                    content: "let x = 1;".into(),
                    old_lineno: Some(1),
                    new_lineno: None,
                    moved: None,
                },
                DiffLine {
                    kind: LineType::Addition,
                    content: "let x = 2;".into(),
                    old_lineno: None,
                    new_lineno: Some(1),
                    moved: None,
                },
            ],
        }],
        change_kind: FileChangeKind::Modified,
        is_binary: false,
        is_untracked: false,
        is_too_large: false,
        stats_truncated: false,
        language: Some("rs".into()),
        stats: FileStats {
            additions: 1,
            deletions: 1,
        },
        old_source: SourceSpec::File(PathBuf::from("old")),
        new_source: SourceSpec::File(PathBuf::from("new")),
    }
}

fn enabled_config() -> ResolvedConfig {
    ResolvedConfig {
        ask_enabled: true,
        ..ResolvedConfig::default()
    }
}

fn type_question(app: &mut App, question: &str) {
    for character in question.chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
    }
}

fn settle(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if app.poll_ask_for_tests(VIEW) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no ask update arrived");
}

#[test]
fn a_question_becomes_a_pending_card_then_an_answer() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let mut app = App::new_with_config(vec![file()], &enabled_config(), false).with_ask_runner(
        move |request| {
            seen.fetch_add(1, Ordering::SeqCst);
            // The payload carries the question and the hunk, nothing else.
            assert!(
                request.prompt.contains("QUESTION\nwhy bump x?"),
                "{:?}",
                request.prompt
            );
            assert!(
                request.prompt.contains("src/lib.rs"),
                "{:?}",
                request.prompt
            );
            assert!(
                request.prompt.contains("+let x = 2;"),
                "{:?}",
                request.prompt
            );
            move || Ok("Because 2 is the new default.".to_owned())
        },
    );

    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    assert_eq!(app.input_mode(), InputMode::Ask);
    type_question(&mut app, "why bump x?");
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    assert_eq!(app.input_mode(), InputMode::Normal);

    let pending = app.review_controller.ask_notes();
    assert_eq!(pending.len(), 1);
    assert!(matches!(pending[0].state, AskNoteState::Pending));

    settle(&mut app);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        app.review_controller.ask_notes()[0].state,
        AskNoteState::Answered(ref body) if body == "Because 2 is the new default."
    ));
}

#[test]
fn a_rejected_model_is_reported_on_the_card_with_the_model_id() {
    let mut app =
        App::new_with_config(vec![file()], &enabled_config(), false).with_ask_runner(|_| {
            move || {
                Err(AskError::ModelRejected {
                    provider: "opencode-go".into(),
                    model: "deepseek-v4-flash".into(),
                    stderr: "Model is not supported".into(),
                })
            }
        });

    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    type_question(&mut app, "what is this?");
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    settle(&mut app);

    match &app.review_controller.ask_notes()[0].state {
        AskNoteState::Failed(message) => {
            assert!(message.contains("deepseek-v4-flash"), "{message}");
            assert!(message.contains("pi --list-models"), "{message}");
        }
        other => panic!("expected a failed card, got {other:?}"),
    }
}

#[test]
fn the_answer_badge_survives_navigation_and_o_jumps_to_the_card() {
    let mut app = App::new_with_config(vec![file()], &enabled_config(), false)
        .with_ask_runner(|_| move || Ok("It bumps the default.".to_owned()));

    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    type_question(&mut app, "why?");
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    settle(&mut app);
    assert_eq!(app.unseen_ask_answers(), 1);

    // A nav key clears the toast but must not clear the badge.
    app.handle_ui_key(key(KeyCode::Char('j')), VIEW);
    assert_eq!(app.unseen_ask_answers(), 1);

    app.handle_ui_key(key(KeyCode::Char('o')), VIEW);
    assert_eq!(app.unseen_ask_answers(), 0);
    assert_eq!(app.review_controller.selected_note_id(), Some("ask:1"));

    // Nothing left to jump to.
    app.handle_ui_key(key(KeyCode::Char('o')), VIEW);
    assert!(
        app.toast
            .as_deref()
            .is_some_and(|toast| toast.contains("No AI answer"))
    );
}

#[test]
fn asking_is_inert_when_the_feature_is_disabled() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    // Default config: ask_enabled is false.
    let mut app = App::new_with_config(vec![file()], &ResolvedConfig::default(), false)
        .with_ask_runner(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            move || Ok("should never run".to_owned())
        });

    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);

    assert_eq!(app.input_mode(), InputMode::Normal, "no popup opens");
    assert!(app.review_controller.ask_notes().is_empty());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "a disabled feature must never reach the provider"
    );
    assert!(
        app.toast
            .as_deref()
            .is_some_and(|toast| toast.contains("ask_enabled"))
    );
}

#[test]
fn an_empty_question_never_reaches_the_provider() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let mut app =
        App::new_with_config(vec![file()], &enabled_config(), false).with_ask_runner(move |_| {
            seen.fetch_add(1, Ordering::SeqCst);
            move || Ok("should never run".to_owned())
        });

    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    app.handle_ui_key(key(KeyCode::Enter), VIEW);

    assert!(app.review_controller.ask_notes().is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
