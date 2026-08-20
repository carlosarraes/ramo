//! Every full-screen overlay is reachable from every other one, and one key always returns to the
//! code. Before this, each overlay was an island: its mode ignored the other overlays' keys.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ramo::app::{App, AppScreen};
use ramo::config::ResolvedConfig;
use ramo::diff::parser::parse_unified_diff;
use ramo::linear::LinearTicket;
use ramo::remote_review::{
    PullRequestReviewContext, RemoteReviewError, RemoteReviewPublisher, RemoteReviewRequest,
};
use ramo::review::Viewport;
use ramo::ui::input::InputMode;

const VIEW: Viewport = Viewport {
    width: 120,
    height: 30,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

struct NoPublisher;

impl RemoteReviewPublisher for NoPublisher {
    fn current_revision(
        &mut self,
        _context: &PullRequestReviewContext,
    ) -> Result<String, RemoteReviewError> {
        Ok("abc123".into())
    }
    fn submit_review(
        &mut self,
        _context: &PullRequestReviewContext,
        _request: &RemoteReviewRequest,
    ) -> Result<(), RemoteReviewError> {
        Ok(())
    }
}

fn ticket() -> LinearTicket {
    serde_json::from_str(
        r#"{"identifier":"MON-2799","title":"Reject quote-only filters","description":"Problem\n\nA long ticket body that wraps across several lines so the document has something to scroll.","state":{"name":"Done"},"assignee":{"displayName":"carlos"}}"#,
    )
    .unwrap()
}

fn context() -> PullRequestReviewContext {
    PullRequestReviewContext {
        repository: "owner/repo".into(),
        repository_url: "https://github.com/owner/repo".into(),
        number: 2289,
        title: "fix the thing".into(),
        body: (1..=60)
            .map(|n| format!("description line {n}"))
            .collect::<Vec<_>>()
            .join("\n"),
        url: "https://github.com/owner/repo/pull/2289".into(),
        base_ref: "main".into(),
        base_revision: "base".into(),
        head_ref: "feature/mon-2799-reject".into(),
        captured_revision: "abc123".into(),
        author_login: "author".into(),
        viewer_login: "reviewer".into(),
    }
}

fn config() -> ResolvedConfig {
    ResolvedConfig {
        chat_enabled: true,
        ..ResolvedConfig::default()
    }
}

fn app_with(config: ResolvedConfig) -> App {
    let files = parse_unified_diff(concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -0,0 +1 @@\n",
        "+new\n",
    ));
    let mut app = App::new_with_config(files, &config, false)
        .with_linear_runner(|_command, _id| Ok(ticket()))
        .with_ask_runner(|_request| move || Ok("answer".to_owned()));
    app.attach_pull_request(context(), Box::new(NoPublisher));
    app
}

fn app() -> App {
    app_with(config())
}

/// The key that opens a screen, and the screen it opens.
const OVERLAYS: [(char, AppScreen); 3] = [
    ('P', AppScreen::PrDescription),
    ('L', AppScreen::LinearTicket),
    ('C', AppScreen::Chat),
];

#[test]
fn every_overlay_reaches_every_other_overlay_in_one_key() {
    for (from_key, from_screen) in OVERLAYS {
        for (to_key, to_screen) in OVERLAYS {
            if from_key == to_key {
                continue;
            }
            let mut app = app();
            app.handle_ui_key(key(KeyCode::Char(from_key)), VIEW);
            assert_eq!(app.screen(), from_screen, "opening {from_key}");

            app.handle_ui_key(key(KeyCode::Char(to_key)), VIEW);
            assert_eq!(app.screen(), to_screen, "{from_key} -> {to_key}");
            assert_eq!(app.input_mode(), to_screen_mode(to_screen));
        }
    }
}

fn to_screen_mode(screen: AppScreen) -> InputMode {
    match screen {
        AppScreen::Review => InputMode::Normal,
        AppScreen::ReviewMap => InputMode::ReviewMap,
        AppScreen::PrDescription => InputMode::PrDescription,
        AppScreen::LinearTicket => InputMode::LinearTicket,
        AppScreen::Chat => InputMode::Chat,
    }
}

#[test]
fn the_same_key_returns_to_the_code_from_each_overlay() {
    for (overlay_key, screen) in OVERLAYS {
        let mut app = app();
        app.handle_ui_key(key(KeyCode::Char(overlay_key)), VIEW);
        assert_eq!(app.screen(), screen);

        app.handle_ui_key(key(KeyCode::Char(overlay_key)), VIEW);
        assert_eq!(app.screen(), AppScreen::Review, "{overlay_key} twice");
        assert_eq!(app.input_mode(), InputMode::Normal);
        assert!(!app.should_quit);
    }
}

#[test]
fn ctrl_q_returns_to_the_code_from_each_overlay() {
    for (overlay_key, screen) in OVERLAYS {
        let mut app = app();
        app.handle_ui_key(key(KeyCode::Char(overlay_key)), VIEW);
        assert_eq!(app.screen(), screen);

        app.handle_ui_key(ctrl(KeyCode::Char('q')), VIEW);
        assert_eq!(app.screen(), AppScreen::Review, "Ctrl-Q from {overlay_key}");
        assert!(!app.should_quit);
    }
}

#[test]
fn an_unavailable_overlay_reports_itself_without_leaving_the_current_one() {
    // Chat off: `C` from another overlay must say so somewhere the reviewer can actually see it.
    let mut app = app_with(ResolvedConfig {
        chat_enabled: false,
        ..ResolvedConfig::default()
    });
    app.handle_ui_key(key(KeyCode::Char('P')), VIEW);
    assert_eq!(app.screen(), AppScreen::PrDescription);

    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    assert_eq!(
        app.screen(),
        AppScreen::PrDescription,
        "a refusal must not move the reviewer"
    );
    let frame = app.render_to_string(120, 30);
    assert!(
        frame.contains("Chat is off"),
        "toast not rendered:\n{frame}"
    );
}

#[test]
fn the_pr_description_keeps_its_scroll_across_a_round_trip() {
    let mut app = app();
    app.handle_ui_key(key(KeyCode::Char('P')), VIEW);
    for _ in 0..6 {
        app.handle_ui_key(key(KeyCode::Char('d')), VIEW);
    }
    let scrolled = app.render_to_string(120, 30);

    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);
    app.handle_ui_key(key(KeyCode::Char('P')), VIEW);

    assert_eq!(
        app.render_to_string(120, 30),
        scrolled,
        "switching away and back restarted the document"
    );
}

#[test]
fn an_empty_chat_draft_switches_screens_but_a_written_one_types() {
    let mut app = app();
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    assert_eq!(app.screen(), AppScreen::Chat);

    // Nothing typed: `L` is a switch.
    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);
    assert_eq!(app.screen(), AppScreen::LinearTicket);

    // Mid-message: the same key is a literal capital.
    app.handle_ui_key(key(KeyCode::Char('C')), VIEW);
    for character in "why".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEW);
    }
    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);
    assert_eq!(app.screen(), AppScreen::Chat, "typing must not navigate");
    assert!(app.render_to_string(120, 30).contains("whyL"));
}
