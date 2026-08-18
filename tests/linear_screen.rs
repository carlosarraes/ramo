use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ramo::app::{App, AppScreen};
use ramo::config::ResolvedConfig;
use ramo::diff::parser::parse_unified_diff;
use ramo::linear::{LinearError, LinearTicket};
use ramo::remote_review::{
    PullRequestReviewContext, RemoteReviewError, RemoteReviewPublisher, RemoteReviewRequest,
};
use ramo::review::Viewport;
use ramo::ui::input::InputMode;

const VIEW: Viewport = Viewport {
    width: 100,
    height: 24,
};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
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

fn context(head_ref: &str, body: &str) -> PullRequestReviewContext {
    PullRequestReviewContext {
        repository: "owner/repo".into(),
        repository_url: "https://github.com/owner/repo".into(),
        number: 2289,
        title: "fix the thing".into(),
        body: body.into(),
        url: "https://github.com/owner/repo/pull/2289".into(),
        base_ref: "main".into(),
        base_revision: "base".into(),
        head_ref: head_ref.into(),
        captured_revision: "abc123".into(),
        author_login: "author".into(),
        viewer_login: "reviewer".into(),
    }
}

fn ticket(linked_pr: Option<u64>) -> LinearTicket {
    let attachments = linked_pr.map_or_else(String::new, |number| {
        format!(
            r#","attachments":{{"nodes":[{{"sourceType":"github","metadata":{{"number":{number}}}}}]}}"#
        )
    });
    serde_json::from_str(&format!(
        r#"{{"identifier":"MON-2799","title":"Reject quote-only filters",
            "description":"Problem\n\nThe list endpoint accepts a filter it cannot honour.",
            "state":{{"name":"Done"}},"assignee":{{"displayName":"carlos"}}{attachments}}}"#
    ))
    .unwrap()
}

fn app(head_ref: &str, body: &str) -> App {
    let files = parse_unified_diff(concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -0,0 +1 @@\n",
        "+new\n",
    ));
    let mut app = App::new_with_config(files, &ResolvedConfig::default(), false)
        .with_linear_runner(|_command, id| {
            assert_eq!(id, "MON-2799", "the inferred key is upper-cased");
            Ok(ticket(Some(2289)))
        });
    app.attach_pull_request(context(head_ref, body), Box::new(NoPublisher));
    app
}

#[test]
fn l_opens_the_ticket_inferred_from_a_lowercase_branch_and_returns() {
    let mut app = app("feature/mon-2799-reject-quote-only", "");
    assert_eq!(app.screen(), AppScreen::Review);

    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);
    assert_eq!(app.screen(), AppScreen::LinearTicket);
    assert_eq!(app.input_mode(), InputMode::LinearTicket);

    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);
    assert_eq!(app.screen(), AppScreen::Review);
    assert_eq!(app.input_mode(), InputMode::Normal);

    // `q` closes the screen rather than quitting the app.
    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);
    app.handle_ui_key(key(KeyCode::Char('q')), VIEW);
    assert_eq!(app.screen(), AppScreen::Review);
    assert!(!app.should_quit);
}

#[test]
fn a_pull_request_with_no_inferable_ticket_explains_itself() {
    let mut app = app("carraes/patch-1", "no ticket here");

    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);

    assert_eq!(app.screen(), AppScreen::Review);
    assert!(
        app.toast
            .as_deref()
            .is_some_and(|toast| toast.contains("No Linear ticket")),
        "{:?}",
        app.toast
    );
}

#[test]
fn a_cli_failure_surfaces_its_remediation_instead_of_opening() {
    let files = parse_unified_diff("diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -0,0 +1 @@\n+x\n");
    let mut app = App::new_with_config(files, &ResolvedConfig::default(), false)
        .with_linear_runner(|command, _id| Err(LinearError::MissingCli { command }));
    app.attach_pull_request(context("feature/mon-2799-x", ""), Box::new(NoPublisher));

    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);

    assert_eq!(app.screen(), AppScreen::Review);
    let toast = app.toast.clone().unwrap_or_default();
    assert!(toast.contains("MON-2799"), "{toast}");
    assert!(toast.contains("not found on PATH"), "{toast}");
    assert!(toast.contains("[linear]"), "{toast}");
}

#[test]
fn the_screen_is_unavailable_without_a_pull_request() {
    let files = parse_unified_diff("diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -0,0 +1 @@\n+x\n");
    let mut app = App::new_with_config(files, &ResolvedConfig::default(), false);

    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);

    assert!(
        app.toast
            .as_deref()
            .is_some_and(|toast| toast.contains("No pull request")),
        "{:?}",
        app.toast
    );
}

#[test]
fn a_ticket_linked_to_a_different_pull_request_is_flagged_not_hidden() {
    let files = parse_unified_diff("diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -0,0 +1 @@\n+x\n");
    let mut app = App::new_with_config(files, &ResolvedConfig::default(), false)
        // Linear says this ticket belongs to PR 1111, but PR 2289 is open.
        .with_linear_runner(|_command, _id| Ok(ticket(Some(1111))));
    app.attach_pull_request(context("feature/mon-2799-x", ""), Box::new(NoPublisher));

    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);
    assert_eq!(app.screen(), AppScreen::LinearTicket);

    let frame = app.render_to_string(100, 24);
    assert!(frame.contains("MON-2799"), "{frame}");
    assert!(frame.contains("1111"), "the mismatch is surfaced: {frame}");
}

#[test]
fn the_ticket_description_renders_and_scrolls() {
    let mut app = app("feature/mon-2799-x", "");
    app.handle_ui_key(key(KeyCode::Char('L')), VIEW);

    let frame = app.render_to_string(100, 24);
    assert!(frame.contains("Reject quote-only filters"), "{frame}");
    assert!(frame.contains("The list endpoint"), "{frame}");
    assert!(frame.contains("Done"), "{frame}");
    assert!(frame.contains("@carlos"), "{frame}");
}
