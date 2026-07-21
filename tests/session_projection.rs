use pdiff::core::input::LayoutMode;
use pdiff::diff::parser::parse_unified_diff;
use pdiff::notes::{LineRange, LiveNoteInput, NoteAnchorSide};
use pdiff::review::{ReviewController, ReviewOptions, Viewport};
use pdiff::session::{
    SESSION_API_VERSION, SESSION_DAEMON_VERSION, SessionDescriptor, SessionSelector,
    build_registration, build_session_context, build_session_review, build_snapshot,
    supported_session_actions,
};

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n";

fn viewport() -> Viewport {
    Viewport {
        width: 100,
        height: 18,
    }
}

fn controller() -> ReviewController {
    ReviewController::new(
        parse_unified_diff(PATCH),
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    )
}

fn descriptor() -> SessionDescriptor {
    SessionDescriptor {
        session_id: "session-a".into(),
        pid: 42,
        cwd: "/work/repo".into(),
        repo_root: Some("/work/repo".into()),
        launched_at: "2026-07-21T12:00:00Z".into(),
        input_kind: "diff".into(),
        title: "Working tree changes".into(),
        source_label: "/work/repo".into(),
    }
}

#[test]
fn protocol_versions_capabilities_and_selectors_serialize_stably() {
    assert_eq!(SESSION_API_VERSION, 1);
    assert_eq!(SESSION_DAEMON_VERSION, 1);
    assert_eq!(
        supported_session_actions(),
        [
            "list",
            "get",
            "context",
            "review",
            "navigate",
            "reload",
            "comment-add",
            "comment-apply",
            "comment-list",
            "comment-rm",
            "comment-clear",
        ]
    );
    let selector = SessionSelector {
        session_id: Some("session-a".into()),
        session_path: None,
        repo_root: None,
    };
    assert_eq!(
        serde_json::to_value(selector).unwrap(),
        serde_json::json!({"sessionId":"session-a"})
    );
}

#[test]
fn registration_and_snapshot_project_stable_files_hunks_and_selection() {
    let mut controller = controller();
    let registration = build_registration(&descriptor(), controller.files());
    assert_eq!(registration.registration_version, 1);
    assert_eq!(registration.files.len(), 1);
    assert_eq!(registration.files[0].path, "src/lib.rs");
    assert_eq!(registration.files[0].patch, PATCH);
    assert_eq!(registration.files[0].hunks[0].old_range, Some([1, 3]));
    assert_eq!(registration.files[0].hunks[0].new_range, Some([1, 3]));

    let snapshot = build_snapshot(&mut controller, viewport(), "2026-07-21T12:01:00Z");
    assert_eq!(
        snapshot.state.selected_file_path.as_deref(),
        Some("src/lib.rs")
    );
    assert_eq!(snapshot.state.selected_hunk_index, 0);
    assert!(!snapshot.state.show_agent_notes);
    assert_eq!(snapshot.state.live_comment_count, 0);
    assert!(snapshot.state.note_markup_width.unwrap() >= 8);
}

#[test]
fn live_notes_use_canonical_geometry_validate_targets_and_remain_source_distinct() {
    let mut controller = controller();
    let view = viewport();
    controller.add_human_note(
        "src/lib.rs",
        Some(LineRange { start: 2, end: 2 }),
        None,
        "human",
        view,
    );
    let applied = controller
        .add_live_note(
            LiveNoteInput {
                id: "mcp:req-1".into(),
                file_path: "src/lib.rs".into(),
                hunk_index: None,
                side: NoteAnchorSide::New,
                line: 2,
                summary: "agent summary".into(),
                rationale: Some("agent rationale".into()),
                markup: Some("<wat>degraded</wat>".into()),
                author: Some("Pi".into()),
                created_at: "2026-07-21T12:02:00Z".into(),
            },
            view,
        )
        .unwrap();
    assert_eq!(
        applied.target.new_range,
        Some(LineRange { start: 2, end: 2 })
    );
    assert_eq!(controller.live_notes().len(), 1);
    assert_eq!(
        controller.snapshot(view).note_count,
        1,
        "human note stays visible"
    );

    controller.toggle_agent_notes(true, view);
    assert_eq!(controller.snapshot(view).note_count, 2);
    assert!(
        controller.live_notes()[0]
            .markup_notes
            .iter()
            .any(|note| note.contains("unknown tag"))
    );

    let wrong_file = controller.add_live_note(
        LiveNoteInput {
            file_path: "other.rs".into(),
            ..LiveNoteInput::minimal("bad", "src/lib.rs", NoteAnchorSide::New, 2, "bad")
        },
        view,
    );
    assert!(wrong_file.unwrap_err().contains("other.rs"));
    let wrong_side_line = controller.add_live_note(
        LiveNoteInput::minimal("bad-line", "src/lib.rs", NoteAnchorSide::Old, 99, "bad"),
        view,
    );
    assert!(wrong_side_line.unwrap_err().contains("line 99"));
}

#[test]
fn projections_filter_patch_and_notes_and_clearing_requires_user_opt_in() {
    let mut controller = controller();
    let view = viewport();
    let human = controller.add_human_note(
        "src/lib.rs",
        None,
        Some(LineRange { start: 2, end: 2 }),
        "human",
        view,
    );
    controller
        .add_live_note(
            LiveNoteInput::minimal("mcp:req-2", "src/lib.rs", NoteAnchorSide::New, 2, "live"),
            view,
        )
        .unwrap();
    let registration = build_registration(&descriptor(), controller.files());
    let snapshot = build_snapshot(&mut controller, view, "2026-07-21T12:03:00Z");
    let context = build_session_context(&registration, &snapshot);
    assert_eq!(context.selected_file.unwrap().path, "src/lib.rs");
    assert_eq!(context.live_comment_count, 1);

    let compact = build_session_review(&registration, &snapshot, false, false);
    assert!(compact.files[0].patch.is_none());
    assert!(compact.review_notes.is_none());
    let full = build_session_review(&registration, &snapshot, true, true);
    assert_eq!(full.files[0].patch.as_deref(), Some(PATCH));
    let notes = full.review_notes.unwrap();
    assert!(
        notes
            .iter()
            .any(|note| note.note_id == human && note.source == "user")
    );
    assert!(
        notes
            .iter()
            .any(|note| note.note_id == "mcp:req-2" && note.source == "agent")
    );

    let cleared = controller.clear_session_notes(None, false, view);
    assert_eq!(cleared.removed_live, 1);
    assert_eq!(cleared.removed_user, 0);
    assert_eq!(controller.human_notes().len(), 1);
    let cleared = controller.clear_session_notes(None, true, view);
    assert_eq!(cleared.removed_user, 1);
}
