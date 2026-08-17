use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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

/// Four added lines, so a visual selection can span a range wider than one row.
fn wide_file() -> DiffFile {
    let mut file = file();
    file.hunks = vec![Hunk {
        old_start: 1,
        new_start: 1,
        header: "@@ -1,0 +1,4 @@ fn demo()".into(),
        lines: (1..=4)
            .map(|number| DiffLine {
                kind: LineType::Addition,
                content: format!("let x{number} = {number};"),
                old_lineno: None,
                new_lineno: Some(number),
                moved: None,
            })
            .collect(),
    }];
    file
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

/// Records every prompt the provider is handed, so a test can assert what a follow-up carried.
fn recording_app(files: Vec<DiffFile>, answer: &'static str) -> (App, Arc<Mutex<Vec<String>>>) {
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&prompts);
    let app =
        App::new_with_config(files, &enabled_config(), false).with_ask_runner(move |request| {
            seen.lock().unwrap().push(request.prompt.clone());
            move || Ok(answer.to_owned())
        });
    (app, prompts)
}

#[test]
fn re_asking_inside_an_earlier_question_continues_its_thread() {
    let (mut app, prompts) = recording_app(vec![file()], "Because 2 is the new default.");

    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    type_question(&mut app, "why bump x?");
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    settle(&mut app);

    // Same lines again: this must continue the thread, not start a new one.
    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    assert_eq!(app.input_mode(), InputMode::Ask);
    type_question(&mut app, "why not 3?");
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    settle(&mut app);

    let notes = app.review_controller.ask_notes();
    assert_eq!(notes.len(), 2, "a follow-up is still its own card");
    assert_eq!(
        notes[1].thread_id, notes[0].id,
        "the follow-up must join the root's thread"
    );
    assert!(notes[1].is_follow_up());
    assert!(!notes[0].is_follow_up());

    // Cloned, not borrowed: holding the guard here would deadlock the assertions below.
    let sent = prompts.lock().unwrap().clone();
    let follow_up = &sent[1];
    assert!(follow_up.contains("QUESTION\nwhy not 3?"), "{follow_up}");
    assert!(follow_up.contains("PRIOR TURNS"), "{follow_up}");
    assert!(follow_up.contains("Q1 why bump x?"), "{follow_up}");
    assert!(
        follow_up.contains("A1 Because 2 is the new default."),
        "{follow_up}"
    );
    // Every pi call is stateless, so the code must be re-sent with each turn.
    assert!(follow_up.contains("+let x = 2;"), "{follow_up}");

    // The root question carried no thread.
    assert!(!sent[0].contains("PRIOR TURNS"));
}

#[test]
fn a_follow_up_is_refused_while_the_thread_is_still_pending() {
    let mut app = App::new_with_config(vec![file()], &enabled_config(), false)
        .with_ask_runner(|_| move || Ok("eventually".to_owned()));

    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    type_question(&mut app, "why bump x?");
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    // Deliberately not settled: the first turn is still Pending.

    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);

    assert_eq!(app.input_mode(), InputMode::Normal, "no draft opens");
    assert_eq!(app.review_controller.ask_notes().len(), 1);
    assert!(
        app.toast
            .as_deref()
            .is_some_and(|toast| toast.contains("follow-up")),
        "{:?}",
        app.toast
    );
}

#[test]
fn a_follow_up_from_the_answer_card_keeps_the_original_range() {
    let (mut app, _) = recording_app(vec![wide_file()], "It initializes four locals.");

    // Select three rows, then ask about the range.
    app.handle_ui_key(key(KeyCode::Char('V')), VIEW);
    app.handle_ui_key(key(KeyCode::Char('j')), VIEW);
    app.handle_ui_key(key(KeyCode::Char('j')), VIEW);
    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    type_question(&mut app, "what do these do?");
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    settle(&mut app);

    let root = app.review_controller.ask_notes()[0].target.clone();
    assert!(
        root.new_range.is_some_and(|range| range.end > range.start),
        "the setup must produce a multi-line range, got {root:?}"
    );

    // `o` parks the cursor on the answer card; asking from there used to collapse the
    // range to its first line, because a note row's key carries only the range starts.
    app.handle_ui_key(key(KeyCode::Char('o')), VIEW);
    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    type_question(&mut app, "why?");
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    settle(&mut app);

    let notes = app.review_controller.ask_notes();
    assert_eq!(notes[1].target, root, "a follow-up reuses the root target");
}

#[test]
fn an_unrelated_line_starts_a_new_thread() {
    let (mut app, prompts) = recording_app(vec![wide_file()], "answer");

    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    type_question(&mut app, "first?");
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    settle(&mut app);

    // Move well clear of the first question's single line.
    app.handle_ui_key(key(KeyCode::Char('j')), VIEW);
    app.handle_ui_key(key(KeyCode::Char('j')), VIEW);
    app.handle_ui_key(key(KeyCode::Char('a')), VIEW);
    type_question(&mut app, "second?");
    app.handle_ui_key(key(KeyCode::Enter), VIEW);
    settle(&mut app);

    let notes = app.review_controller.ask_notes();
    assert_eq!(notes[1].thread_id, notes[1].id, "{:?}", notes[1]);
    assert!(!notes[1].is_follow_up());
    assert!(!prompts.lock().unwrap()[1].contains("PRIOR TURNS"));
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
