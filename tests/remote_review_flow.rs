use std::cell::RefCell;
use std::rc::Rc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ramo::app::{App, RemoteReviewOutcome};
use ramo::config::ResolvedConfig;
use ramo::core::input::LayoutMode;
use ramo::diff::parser::parse_unified_diff;
use ramo::remote_review::{
    GithubReviewThread, GithubThreadComment, GithubThreadSubject, PullRequestReviewContext,
    RemoteLineSide, RemoteReviewError, RemoteReviewPublisher, RemoteReviewRequest,
};
use ramo::review::{ReviewOptions, Viewport};
use ramo::ui::input::InputMode;
use ramo::ui::review::ReviewHeading;

const VIEWPORT: Viewport = Viewport {
    width: 100,
    height: 24,
};

#[derive(Default)]
struct Calls {
    revisions: usize,
    submissions: Vec<RemoteReviewRequest>,
}

struct FakePublisher {
    calls: Rc<RefCell<Calls>>,
    revision: Result<String, RemoteReviewError>,
    submit: Result<(), RemoteReviewError>,
}

impl RemoteReviewPublisher for FakePublisher {
    fn current_revision(
        &mut self,
        _context: &PullRequestReviewContext,
    ) -> Result<String, RemoteReviewError> {
        self.calls.borrow_mut().revisions += 1;
        self.revision.clone()
    }

    fn submit_review(
        &mut self,
        _context: &PullRequestReviewContext,
        request: &RemoteReviewRequest,
    ) -> Result<(), RemoteReviewError> {
        self.calls.borrow_mut().submissions.push(request.clone());
        self.submit.clone()
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn context(author: &str, viewer: &str) -> PullRequestReviewContext {
    PullRequestReviewContext {
        repository: "owner/repo".into(),
        repository_url: "https://github.com/owner/repo".into(),
        number: 123,
        title: "Improve review flow".into(),
        url: "https://github.com/owner/repo/pull/123".into(),
        base_ref: "main".into(),
        base_revision: "base123".into(),
        head_ref: "feature".into(),
        captured_revision: "abc123".into(),
        author_login: author.into(),
        viewer_login: viewer.into(),
    }
}

fn app(revision: &str) -> (App, Rc<RefCell<Calls>>) {
    app_with_config(revision, &ResolvedConfig::default())
}

fn app_with_config(revision: &str, config: &ResolvedConfig) -> (App, Rc<RefCell<Calls>>) {
    let files = parse_unified_diff(concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -0,0 +1 @@\n",
        "+new\n",
    ));
    let mut app = App::new_with_config(files, config, false);
    app.review_controller = ramo::review::ReviewController::new(
        app.files.clone(),
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    app.review_controller.snapshot(VIEWPORT);
    app.review_controller
        .begin_remote_human_note(None, VIEWPORT)
        .unwrap();
    app.review_controller
        .update_human_note_draft("Inline feedback", VIEWPORT);
    app.review_controller.save_human_note_draft(VIEWPORT);
    let calls = Rc::new(RefCell::new(Calls::default()));
    app.attach_pull_request(
        context("author", "reviewer"),
        Box::new(FakePublisher {
            calls: Rc::clone(&calls),
            revision: Ok(revision.into()),
            submit: Ok(()),
        }),
    );
    (app, calls)
}

#[test]
fn quit_confirms_then_submits_one_review_after_a_fresh_head_check() {
    let (mut app, calls) = app("abc123");
    app.handle_ui_key(key(KeyCode::Char('q')), VIEWPORT);
    assert_eq!(app.input_mode(), InputMode::PublishPrompt);
    assert_eq!(calls.borrow().revisions, 0);
    app.handle_ui_key(key(KeyCode::Char('y')), VIEWPORT);
    assert_eq!(app.input_mode(), InputMode::VerdictPrompt);
    app.handle_ui_key(key(KeyCode::Char('c')), VIEWPORT);

    assert_eq!(calls.borrow().revisions, 1);
    assert_eq!(calls.borrow().submissions.len(), 1);
    assert_eq!(calls.borrow().submissions[0].comments.len(), 1);
    assert_eq!(
        calls.borrow().submissions[0].body,
        "Review submitted from Ramo with 1 inline comment."
    );
    assert_eq!(app.remote_outcome(), Some(RemoteReviewOutcome::Published));
    assert!(app.should_quit);
}

#[test]
fn a_configured_review_message_replaces_the_default_body_for_every_verdict() {
    for verdict in [KeyCode::Char('c'), KeyCode::Char('a'), KeyCode::Char('r')] {
        let (mut app, calls) = app_with_config(
            "abc123",
            &ResolvedConfig {
                review_message: Some("approved".into()),
                ..ResolvedConfig::default()
            },
        );
        app.handle_ui_key(key(KeyCode::Char('q')), VIEWPORT);
        app.handle_ui_key(key(KeyCode::Char('y')), VIEWPORT);
        app.handle_ui_key(key(verdict), VIEWPORT);

        assert_eq!(calls.borrow().submissions.len(), 1, "{verdict:?}");
        assert_eq!(
            calls.borrow().submissions[0].body,
            "approved",
            "{verdict:?}"
        );
    }
}

#[test]
fn an_edited_overall_comment_still_wins_over_a_configured_review_message() {
    let (mut app, calls) = app_with_config(
        "abc123",
        &ResolvedConfig {
            review_message: Some("approved".into()),
            ..ResolvedConfig::default()
        },
    );
    app.handle_ui_key(key(KeyCode::Char('q')), VIEWPORT);
    app.handle_ui_key(key(KeyCode::Char('y')), VIEWPORT);
    // `o` opens the editor prefilled with the configured message, so clear it first.
    app.handle_ui_key(key(KeyCode::Char('o')), VIEWPORT);
    for _ in 0.."approved".len() {
        app.handle_ui_key(key(KeyCode::Backspace), VIEWPORT);
    }
    for character in "typed by hand".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), VIEWPORT);
    }
    app.handle_ui_key(key(KeyCode::Enter), VIEWPORT);
    app.handle_ui_key(key(KeyCode::Char('c')), VIEWPORT);

    assert_eq!(calls.borrow().submissions[0].body, "typed by hand");
}

#[test]
fn publication_uses_only_new_local_comments() {
    let (mut app, calls) = app("abc123");
    app.review_controller.attach_github_threads(
        vec![GithubReviewThread {
            id: "T1".into(),
            path: "src/lib.rs".into(),
            is_resolved: false,
            is_outdated: false,
            subject: GithubThreadSubject::Line {
                side: Some(RemoteLineSide::Right),
                start_side: Some(RemoteLineSide::Right),
                start_line: Some(1),
                end_line: Some(1),
            },
            comments: vec![GithubThreadComment {
                id: "C1".into(),
                author: "alice".into(),
                body: "existing github feedback".into(),
                created_at: "now".into(),
                url: "https://example.test/T1".into(),
            }],
            url: "https://example.test/T1".into(),
        }],
        VIEWPORT,
    );

    app.handle_ui_key(key(KeyCode::Char('q')), VIEWPORT);
    app.handle_ui_key(key(KeyCode::Char('y')), VIEWPORT);
    app.handle_ui_key(key(KeyCode::Char('c')), VIEWPORT);

    let calls = calls.borrow();
    assert_eq!(calls.submissions.len(), 1);
    assert_eq!(calls.submissions[0].comments.len(), 1);
    assert_eq!(calls.submissions[0].comments[0].body, "Inline feedback");
    assert!(
        calls.submissions[0]
            .comments
            .iter()
            .all(|comment| comment.body != "existing github feedback")
    );
}

#[test]
fn cancel_keeps_reviewing_and_discard_is_the_only_abandon_path() {
    let (mut app, calls) = app("abc123");
    app.handle_ui_key(key(KeyCode::Char('q')), VIEWPORT);
    app.handle_ui_key(key(KeyCode::Char('n')), VIEWPORT);
    assert_eq!(app.input_mode(), InputMode::Normal);
    assert!(!app.should_quit);
    assert_eq!(app.review_controller.human_notes().len(), 1);

    app.handle_ui_key(key(KeyCode::Char('q')), VIEWPORT);
    app.handle_ui_key(key(KeyCode::Char('d')), VIEWPORT);
    assert_eq!(app.remote_outcome(), Some(RemoteReviewOutcome::Discarded));
    assert!(app.should_quit);
    assert_eq!(calls.borrow().revisions, 0);
}

#[test]
fn stale_head_error_is_dismissible_and_preserves_the_review() {
    let (mut app, calls) = app("changed");
    app.handle_ui_key(key(KeyCode::Char('q')), VIEWPORT);
    app.handle_ui_key(key(KeyCode::Char('y')), VIEWPORT);
    app.handle_ui_key(key(KeyCode::Char('r')), VIEWPORT);
    assert_eq!(app.input_mode(), InputMode::Message);
    assert_eq!(calls.borrow().submissions.len(), 0);
    assert_eq!(app.review_controller.human_notes().len(), 1);
    app.handle_ui_key(key(KeyCode::Enter), VIEWPORT);
    assert_eq!(app.input_mode(), InputMode::VerdictPrompt);
    assert!(!app.should_quit);
}

#[test]
fn attaching_pull_request_sets_the_review_heading() {
    let (app, _) = app("abc123");
    assert_eq!(
        app.review_heading(),
        &ReviewHeading::PullRequest {
            number: 123,
            title: "Improve review flow".into(),
            base_ref: "main".into(),
            head_ref: "feature".into(),
        }
    );
}

#[test]
fn local_app_defaults_to_working_tree_heading() {
    let app = App::new(parse_unified_diff(concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -0,0 +1 @@\n",
        "+new\n",
    )));
    assert_eq!(
        app.review_heading(),
        &ReviewHeading::Local("Working tree".into())
    );
}
