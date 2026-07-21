use std::fs;
use std::io;
use std::process::Command;
use std::time::Instant;

use pdiff::config::ResolvedConfig;
use pdiff::core::input::{CommonOptions, PatchSource, ReviewInput};
use pdiff::input::{LoadContext, ReviewLoader};
use pdiff::notes::{LiveNoteInput, NoteAnchorSide};
use pdiff::review::{ReviewAction, ReviewController, ReviewOptions, Viewport};
use pdiff::session::{
    SessionDescriptor, apply_session_reload, build_registration, build_session_review,
    build_snapshot, refresh_session_descriptor,
};
use pdiff::vcs::SystemCommandRunner;
use pdiff::watch::WatchRuntime;
use serde_json::json;

const FIRST_PATCH: &str = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-old a\n+new a\ndiff --git a/src/b.rs b/src/b.rs\n--- a/src/b.rs\n+++ b/src/b.rs\n@@ -1 +1 @@\n-old b\n+new b\n";
const SECOND_PATCH: &str = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-old a\n+newer a\n";

fn viewport() -> Viewport {
    Viewport {
        width: 100,
        height: 20,
    }
}

fn load(input: &ReviewInput, cwd: &std::path::Path) -> pdiff::input::LoadedReview {
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    ReviewLoader
        .load_with_context(
            input,
            &mut io::empty(),
            &LoadContext {
                cwd,
                config: &config,
                runner: &runner,
            },
        )
        .unwrap()
}

fn descriptor() -> SessionDescriptor {
    SessionDescriptor {
        session_id: "stable-session".into(),
        pid: 42,
        cwd: "/initial".into(),
        repo_root: Some("/initial".into()),
        launched_at: "2026-07-21T12:00:00Z".into(),
        input_kind: "diff".into(),
        title: "Initial".into(),
        source_label: "initial".into(),
    }
}

#[test]
fn direct_file_reload_resolves_source_and_preserves_live_notes() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let source = repo.path().join("fixtures");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("before.rs"), "old\n").unwrap();
    fs::write(source.join("after.rs"), "first\n").unwrap();
    let initial_input = ReviewInput::FilePair {
        left: source.join("before.rs"),
        right: source.join("after.rs"),
        display_path: Some("src/lib.rs".into()),
        options: CommonOptions::default(),
    };
    let initial = load(&initial_input, repo.path());
    let mut runtime = WatchRuntime::new(
        &initial,
        repo.path().to_path_buf(),
        ResolvedConfig::default(),
        false,
        Instant::now(),
    );
    let mut controller =
        ReviewController::new(initial.changeset.files.clone(), ReviewOptions::default());
    controller
        .add_live_note(
            LiveNoteInput::minimal(
                "mcp:stable",
                "src/lib.rs",
                NoteAnchorSide::New,
                1,
                "keep me",
            ),
            viewport(),
        )
        .unwrap();
    fs::write(source.join("after.rs"), "second\n").unwrap();

    let applied = apply_session_reload(
        &mut controller,
        &mut runtime,
        &json!({
            "action":"reload",
            "sourcePath":source,
            "nextInput":{
                "kind":"diff","left":"before.rs","right":"after.rs",
                "displayPath":"src/lib.rs","options":{}
            }
        }),
        viewport(),
    )
    .unwrap();

    assert!(controller.files()[0].patch.contains("+second"));
    assert_eq!(controller.live_notes()[0].note.summary, "keep me");
    assert_eq!(applied.cwd, source.canonicalize().unwrap());
    let refreshed =
        refresh_session_descriptor(&descriptor(), &applied.input, &applied.loaded, &applied.cwd);
    assert_eq!(refreshed.session_id, "stable-session");
    assert_eq!(refreshed.launched_at, "2026-07-21T12:00:00Z");
    assert_eq!(refreshed.title, "before.rs ↔ after.rs");
}

#[test]
fn patch_reload_falls_back_when_the_selected_target_disappears() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let patch = repo.path().join("review.patch");
    fs::write(&patch, FIRST_PATCH).unwrap();
    let input = ReviewInput::Patch {
        source: PatchSource::File(patch.clone()),
        options: CommonOptions::default(),
    };
    let initial = load(&input, repo.path());
    let mut runtime = WatchRuntime::new(
        &initial,
        repo.path().to_path_buf(),
        ResolvedConfig::default(),
        false,
        Instant::now(),
    );
    let mut controller =
        ReviewController::new(initial.changeset.files.clone(), ReviewOptions::default());
    let selected = controller.files()[1].id.clone();
    controller.apply(ReviewAction::SelectFile(selected), viewport());
    fs::write(&patch, SECOND_PATCH).unwrap();

    apply_session_reload(
        &mut controller,
        &mut runtime,
        &json!({"action":"reload","nextInput":{"kind":"patch","path":patch,"options":{}}}),
        viewport(),
    )
    .unwrap();

    assert_eq!(controller.files().len(), 1);
    let remaining_id = controller.files()[0].id.clone();
    assert_eq!(
        controller.snapshot(viewport()).selected_file_id.as_deref(),
        Some(remaining_id.as_str())
    );
    assert_eq!(controller.snapshot(viewport()).selected_hunk_index, Some(0));
}

#[test]
fn rejected_reload_does_not_replace_review_or_runtime_plan() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let before = repo.path().join("before.rs");
    let after = repo.path().join("after.rs");
    fs::write(&before, "old\n").unwrap();
    fs::write(&after, "first\n").unwrap();
    let input = ReviewInput::FilePair {
        left: before.clone(),
        right: after.clone(),
        display_path: Some("src/lib.rs".into()),
        options: CommonOptions::default(),
    };
    let initial = load(&input, repo.path());
    let mut runtime = WatchRuntime::new(
        &initial,
        repo.path().to_path_buf(),
        ResolvedConfig::default(),
        false,
        Instant::now(),
    );
    let mut controller =
        ReviewController::new(initial.changeset.files.clone(), ReviewOptions::default());
    let original_patch = controller.files()[0].patch.clone();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("before.rs"), "secret old\n").unwrap();
    fs::write(outside.path().join("after.rs"), "secret new\n").unwrap();

    let error = apply_session_reload(
        &mut controller,
        &mut runtime,
        &json!({
            "action":"reload","sourcePath":outside.path(),
            "nextInput":{"kind":"files","left":"before.rs","right":"after.rs","options":{}}
        }),
        viewport(),
    )
    .unwrap_err();
    assert!(error.contains("outside the initial pdiff root"));
    assert_eq!(controller.files()[0].patch, original_patch);

    fs::write(&after, "valid second\n").unwrap();
    apply_session_reload(
        &mut controller,
        &mut runtime,
        &json!({
            "action":"reload",
            "nextInput":{"kind":"files","left":before,"right":after,"displayPath":"src/lib.rs","options":{}}
        }),
        viewport(),
    )
    .unwrap();
    assert!(controller.files()[0].patch.contains("+valid second"));
}

#[test]
fn git_reload_uses_the_requested_source_and_refreshes_metadata() {
    let repo = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["config", "user.name", "Pdiff Test"]);
    git(&["config", "user.email", "pdiff@example.invalid"]);
    fs::write(repo.path().join("file.rs"), "committed\n").unwrap();
    git(&["add", "file.rs"]);
    git(&["commit", "-q", "-m", "initial"]);
    fs::write(repo.path().join("file.rs"), "working\n").unwrap();
    let initial_input = ReviewInput::VcsDiff {
        range: None,
        staged: false,
        pathspecs: vec![],
        options: CommonOptions::default(),
    };
    let initial = load(&initial_input, repo.path());
    let mut runtime = WatchRuntime::new(
        &initial,
        repo.path().to_path_buf(),
        ResolvedConfig::default(),
        false,
        Instant::now(),
    );
    let mut controller =
        ReviewController::new(initial.changeset.files.clone(), ReviewOptions::default());

    let applied = apply_session_reload(
        &mut controller,
        &mut runtime,
        &json!({
            "action":"reload","sourcePath":repo.path(),
            "nextInput":{"kind":"show","reference":"HEAD","pathspecs":[],"options":{}}
        }),
        viewport(),
    )
    .unwrap();

    assert!(controller.files()[0].patch.contains("+committed"));
    let refreshed =
        refresh_session_descriptor(&descriptor(), &applied.input, &applied.loaded, &applied.cwd);
    assert_eq!(refreshed.input_kind, "show");
    assert_eq!(
        refreshed.repo_root.as_deref(),
        Some(repo.path().to_str().unwrap())
    );
}

#[test]
fn reload_rejects_stdin_shapes_and_review_export_is_opt_in() {
    let repo = tempfile::tempdir().unwrap();
    fs::create_dir(repo.path().join(".git")).unwrap();
    let patch = repo.path().join("review.patch");
    fs::write(&patch, FIRST_PATCH).unwrap();
    let input = ReviewInput::Patch {
        source: PatchSource::File(patch),
        options: CommonOptions::default(),
    };
    let initial = load(&input, repo.path());
    let mut runtime = WatchRuntime::new(
        &initial,
        repo.path().to_path_buf(),
        ResolvedConfig::default(),
        false,
        Instant::now(),
    );
    let mut controller =
        ReviewController::new(initial.changeset.files.clone(), ReviewOptions::default());
    let error = apply_session_reload(
        &mut controller,
        &mut runtime,
        &json!({"action":"reload","nextInput":{"kind":"patch","options":{}}}),
        viewport(),
    )
    .unwrap_err();
    assert!(error.contains("stdin-backed patch"));

    let registration = build_registration(&descriptor(), controller.files());
    let snapshot = build_snapshot(&mut controller, viewport(), "now");
    let compact = build_session_review(&registration, &snapshot, false, false);
    assert!(compact.files.iter().all(|file| file.patch.is_none()));
    assert!(compact.review_notes.is_none());
    let complete = build_session_review(&registration, &snapshot, true, true);
    assert!(complete.files.iter().all(|file| file.patch.is_some()));
    assert!(complete.review_notes.is_some());
}
