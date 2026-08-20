//! A conversation outlives the process that started it.

use ramo::chat::store::{CHAT_STORE_VERSION, ChatStore, INTERRUPTED};
use ramo::chat::{ChatState, ChatTurn};
use ramo_core::chat::ConversationKey;

fn key(project: &str, identity: &str) -> ConversationKey {
    ConversationKey {
        project: project.into(),
        identity: identity.into(),
        version: CHAT_STORE_VERSION,
    }
}

fn turn(question: &str, state: ChatState) -> ChatTurn {
    ChatTurn {
        question: question.into(),
        state,
    }
}

fn store() -> (tempfile::TempDir, ChatStore) {
    let temp = tempfile::tempdir().unwrap();
    let store = ChatStore::with_directory(temp.path().join("chat"));
    (temp, store)
}

#[test]
fn a_conversation_survives_a_restart_and_is_restored_in_order() {
    let (_temp, store) = store();
    let key = key("/p", "github:owner/repo#482");
    let turns = vec![
        turn("why?", ChatState::Answered("because".into())),
        turn("and then?", ChatState::Answered("it retries".into())),
    ];
    store.save(&key, "ramo-abc", &turns).unwrap();

    let restored = store.load(&key).expect("conversation");
    assert_eq!(restored.turns, turns);
    assert_eq!(restored.session_id, "ramo-abc");
}

#[test]
fn a_pending_turn_is_stored_as_interrupted_and_never_restored_as_pending() {
    // A restored pending turn has no worker left to answer it, so the pane would show
    // "thinking…" for the rest of the session with nothing able to resolve it.
    let (_temp, store) = store();
    let key = key("/p", "github:owner/repo#1");
    store
        .save(&key, "ramo-abc", &[turn("in flight", ChatState::Pending)])
        .unwrap();

    let restored = store.load(&key).expect("conversation");
    assert_eq!(
        restored.turns[0].state,
        ChatState::Failed(INTERRUPTED.into())
    );
}

#[test]
fn two_local_reviews_in_different_directories_do_not_share_a_conversation() {
    // Every local diff used to seed the literal string "local", collapsing unrelated reviews
    // into one thread.
    let (_temp, store) = store();
    let one = key("/projs/a", "local:/projs/a");
    let two = key("/projs/b", "local:/projs/b");
    store
        .save(
            &one,
            "ramo-a",
            &[turn("a", ChatState::Answered("A".into()))],
        )
        .unwrap();
    store
        .save(
            &two,
            "ramo-b",
            &[turn("b", ChatState::Answered("B".into()))],
        )
        .unwrap();

    assert_eq!(store.load(&one).unwrap().turns[0].question, "a");
    assert_eq!(store.load(&two).unwrap().turns[0].question, "b");
}

#[test]
fn a_version_bump_misses_cleanly_rather_than_failing_to_parse() {
    let (_temp, store) = store();
    let current = key("/p", "github:owner/repo#1");
    store
        .save(
            &current,
            "ramo-abc",
            &[turn("q", ChatState::Answered("a".into()))],
        )
        .unwrap();

    let future = ConversationKey {
        version: CHAT_STORE_VERSION + 1,
        ..current.clone()
    };
    assert!(store.load(&future).is_none());
    // The current entry is untouched: a different version is a different file.
    assert!(store.load(&current).is_some());
}

#[test]
fn a_corrupt_entry_is_removed_rather_than_returned() {
    let (_temp, store) = store();
    let key = key("/p", "github:owner/repo#1");
    store
        .save(
            &key,
            "ramo-abc",
            &[turn("q", ChatState::Answered("a".into()))],
        )
        .unwrap();

    let directory = std::fs::read_dir(_temp.path().join("chat")).unwrap();
    let path = directory
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "json"))
        .expect("entry");
    std::fs::write(&path, b"{ not json").unwrap();

    assert!(store.load(&key).is_none());
    assert!(
        !path.exists(),
        "a doubtful entry must not be left to fail again"
    );
}

#[test]
fn the_fifty_first_conversation_evicts_the_least_recently_used() {
    let (temp, store) = store();
    for index in 0..52 {
        let key = key("/p", &format!("github:owner/repo#{index}"));
        store
            .save(
                &key,
                "ramo-abc",
                &[turn("q", ChatState::Answered("a".into()))],
            )
            .unwrap();
    }
    let count = std::fs::read_dir(temp.path().join("chat"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .count();
    assert!(count <= 50, "kept {count} conversations");
}

mod restored_into_the_app {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ramo::app::App;
    use ramo::config::ResolvedConfig;
    use ramo::diff::parser::parse_unified_diff;
    use ramo::remote_review::{
        PullRequestReviewContext, RemoteReviewError, RemoteReviewPublisher, RemoteReviewRequest,
    };
    use ramo::review::Viewport;
    use std::sync::{Arc, Mutex};

    const VIEW: Viewport = Viewport {
        width: 120,
        height: 24,
    };

    struct NoPublisher;
    impl RemoteReviewPublisher for NoPublisher {
        fn current_revision(
            &mut self,
            _c: &PullRequestReviewContext,
        ) -> Result<String, RemoteReviewError> {
            Ok("abc".into())
        }
        fn submit_review(
            &mut self,
            _c: &PullRequestReviewContext,
            _r: &RemoteReviewRequest,
        ) -> Result<(), RemoteReviewError> {
            Ok(())
        }
    }

    fn context() -> PullRequestReviewContext {
        PullRequestReviewContext {
            repository: "owner/repo".into(),
            repository_url: "https://github.com/owner/repo".into(),
            number: 482,
            title: "Add retry backoff".into(),
            body: "the body".into(),
            url: "https://github.com/owner/repo/pull/482".into(),
            base_ref: "main".into(),
            base_revision: "base".into(),
            head_ref: "feat/backoff".into(),
            captured_revision: "abc".into(),
            author_login: "author".into(),
            viewer_login: "reviewer".into(),
        }
    }

    /// `project_root` is what a conversation is keyed on, so the app has to be told about it
    /// before the store is attached — exactly as `runtime.rs` does.
    fn app_restored_from(
        store: ChatStore,
        sessions: &std::path::Path,
        project: &std::path::Path,
    ) -> (App, Arc<Mutex<Vec<ramo::ask::AskRequest>>>) {
        let files = parse_unified_diff(concat!(
            "diff --git a/src/retry.rs b/src/retry.rs\n",
            "--- a/src/retry.rs\n",
            "+++ b/src/retry.rs\n",
            "@@ -0,0 +1 @@\n",
            "+let backoff = base * 2;\n",
        ));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let config = ResolvedConfig {
            chat_enabled: true,
            ..ResolvedConfig::default()
        };
        let mut app = App::new_with_config(files, &config, false).with_ask_runner(move |request| {
            sink.lock().unwrap().push(request.clone());
            move || Ok("answer".to_owned())
        });
        app.attach_pull_request(context(), Box::new(NoPublisher));
        app.set_project_root(project.to_path_buf());
        app.restore_chat(store, sessions);
        (app, seen)
    }

    fn ask(app: &mut App, question: &str) {
        app.handle_ui_key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::NONE), VIEW);
        for character in question.chars() {
            app.handle_ui_key(
                KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE),
                VIEW,
            );
        }
        app.handle_ui_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), VIEW);
    }

    fn stored(store: &ChatStore, project: &std::path::Path, session_id: &str) {
        store
            .save(
                &ConversationKey {
                    project: project.display().to_string(),
                    identity: "owner/repo#482".into(),
                    version: CHAT_STORE_VERSION,
                },
                session_id,
                &[turn(
                    "why?",
                    ChatState::Answered("because it retries".into()),
                )],
            )
            .unwrap();
    }

    #[test]
    fn the_first_question_after_a_restore_still_carries_the_context() {
        // The old "first turn" test was `turns.is_empty()`, which a restored transcript makes
        // false — the model would have received a bare question with no idea what it is reading.
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("projs/ramo");
        let sessions = temp.path().join("sessions");
        let store = ChatStore::with_directory(temp.path().join("chat"));
        let session_id = ramo::chat::new_session_id("owner/repo#482");
        stored(&store, &project, &session_id);

        // Make pi's session look alive, so this test is only about the header.
        let directory = ramo::chat::session::project_session_dir(&sessions, &project);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join(format!("t_{session_id}.jsonl")), "{}").unwrap();

        let (mut app, seen) = app_restored_from(store, &sessions, &project);
        ask(&mut app, "and now?");

        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].prompt.contains("PULL REQUEST"),
            "a restored conversation suppressed the context:\n{}",
            requests[0].prompt
        );
        assert!(
            !requests[0].prompt.contains("PREVIOUS CONVERSATION"),
            "the session was resumable, so there was nothing to replay:\n{}",
            requests[0].prompt
        );
    }

    #[test]
    fn a_restore_without_a_pi_session_replays_the_thread_and_says_so() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("projs/ramo");
        let sessions = temp.path().join("sessions");
        let store = ChatStore::with_directory(temp.path().join("chat"));
        // Saved, but pi's own transcript is not there — pruned, upgraded away, or deleted.
        stored(
            &store,
            &project,
            &ramo::chat::new_session_id("owner/repo#482"),
        );

        let (mut app, seen) = app_restored_from(store, &sessions, &project);
        let frame = app.render_to_string(120, 24);
        assert!(
            frame.contains("session is gone") || app.toast.is_some(),
            "the reviewer was not told the model lost the thread:\n{frame}"
        );

        ask(&mut app, "and now?");
        let requests = seen.lock().unwrap();
        assert!(
            requests[0].prompt.contains("PREVIOUS CONVERSATION"),
            "the thread was not replayed:\n{}",
            requests[0].prompt
        );
        assert!(requests[0].prompt.contains("because it retries"));
    }
}
