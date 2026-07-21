use ramo::core::changeset::Changeset;
use ramo::core::input::LayoutMode;
use ramo::diff::model::DiffFile;
use ramo::diff::parser::parse_unified_diff;
use ramo::notes::{
    LineRange, NoteAnchorSide, NoteSource, annotated_hunks, annotation_range_label,
    note_box_layout, note_source, resolve_note_target, stable_note_id,
};
use ramo::review::{
    ReviewAction, ReviewController, ReviewHit, ReviewOptions, ReviewPoint, Viewport,
};

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n@@ -10 +10 @@\n-before\n+after\n";

fn annotated_file() -> DiffFile {
    let mut changeset = Changeset::new("test", "test", parse_unified_diff(PATCH));
    let context = ramo::notes::parse_agent_context(
        "agent.json",
        br#"{
          "files":[{"path":"src/lib.rs","annotations":[
            {"newRange":[2,2],"summary":"AI finding","source":"ai"},
            {"oldRange":[10,10],"summary":"User finding","source":"user"}
          ]}]
        }"#,
    )
    .unwrap();
    changeset.apply_agent_context(&context);
    changeset.files.remove(0)
}

#[test]
fn targets_prefer_new_lines_and_overlap_the_correct_hunks() {
    let file = annotated_file();
    let notes = &file.agent.as_ref().unwrap().annotations;
    let first = resolve_note_target(&file, &notes[0]);
    assert_eq!(first.hunk_index, Some(0));
    assert_eq!(first.anchor_side, Some(NoteAnchorSide::New));
    assert_eq!(first.anchor_line, Some(2));
    assert_eq!(
        annotation_range_label(&notes[0], Some(&file)),
        "src/lib.rs R2"
    );

    let second = resolve_note_target(&file, &notes[1]);
    assert_eq!(second.hunk_index, Some(1));
    assert_eq!(second.anchor_side, Some(NoteAnchorSide::Old));
    assert_eq!(annotated_hunks(&file), [0, 1]);
}

#[test]
fn source_classification_and_synthesized_ids_are_stable() {
    let file = annotated_file();
    let notes = &file.agent.as_ref().unwrap().annotations;
    assert_eq!(note_source(&notes[0]), NoteSource::Ai);
    assert_eq!(note_source(&notes[1]), NoteSource::User);
    assert_eq!(
        stable_note_id(&file, &notes[0]),
        stable_note_id(&file, &notes[0])
    );
    assert_ne!(
        stable_note_id(&file, &notes[0]),
        stable_note_id(&file, &notes[1])
    );
}

#[test]
fn annotated_hunk_navigation_is_derived_from_attached_notes() {
    let viewport = Viewport {
        width: 100,
        height: 10,
    };
    let mut controller = ReviewController::new(
        vec![annotated_file()],
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    controller.snapshot(viewport);
    controller.apply(ReviewAction::MoveAnnotatedHunk(1), viewport);
    assert_eq!(controller.snapshot(viewport).selected_hunk_index, Some(1));
}

#[test]
fn note_box_placement_docks_split_sides_and_has_a_narrow_fallback() {
    let left = note_box_layout(LayoutMode::Split, Some(NoteAnchorSide::Old), 120);
    let right = note_box_layout(LayoutMode::Split, Some(NoteAnchorSide::New), 120);
    let stack = note_box_layout(LayoutMode::Stack, Some(NoteAnchorSide::New), 72);
    assert_eq!(left.box_left, 0);
    assert!(right.box_left > left.box_left);
    assert_eq!(left.content_width + 4, left.box_width);
    assert!(stack.box_left <= 4);
    assert!(stack.box_width >= 28);
    assert!(stack.box_width <= 68);
}

#[test]
fn external_visibility_and_human_notes_drive_shared_geometry_and_survive_reload() {
    let file = annotated_file();
    let viewport = Viewport {
        width: 120,
        height: 14,
    };
    let mut controller = ReviewController::new(
        vec![file.clone()],
        ReviewOptions {
            layout: LayoutMode::Stack,
            agent_notes: false,
            ..ReviewOptions::default()
        },
    );

    let hidden_ai = controller.snapshot(viewport).clone();
    assert_eq!(
        hidden_ai.note_count, 1,
        "user-source external notes stay visible"
    );
    controller.apply(ReviewAction::ToggleAgentNotes, viewport);
    let shown_ai = controller.snapshot(viewport).clone();
    assert_eq!(shown_ai.note_count, 2);
    assert!(shown_ai.total_height > hidden_ai.total_height);
    let note_hits = (0..viewport.height)
        .filter_map(|y| controller.hit_test(ReviewPoint::new(10, y), viewport))
        .filter(|hit| matches!(hit, ReviewHit::Note(_)))
        .count();
    assert!(
        note_hits >= 2,
        "inserted note rows participate in hit geometry"
    );

    let human_id = controller.add_human_note(
        "src/lib.rs",
        Some(LineRange { start: 2, end: 2 }),
        None,
        "Human draft saved",
        viewport,
    );
    assert!(!human_id.is_empty());
    assert_eq!(controller.snapshot(viewport).note_count, 3);

    controller.replace_files(vec![file], viewport);
    assert_eq!(controller.human_notes().len(), 1);
    assert_eq!(controller.human_notes()[0].body, "Human draft saved");
    assert_eq!(controller.snapshot(viewport).note_count, 3);
}

#[test]
fn controller_drafts_save_edit_cancel_remove_and_clear_by_stable_id() {
    let viewport = Viewport {
        width: 100,
        height: 16,
    };
    let mut controller = ReviewController::new(
        vec![annotated_file()],
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    controller.snapshot(viewport);

    let draft_id = controller.begin_human_note(viewport).unwrap();
    assert!(draft_id.starts_with("draft:"));
    assert_eq!(controller.snapshot(viewport).note_count, 2);
    controller.update_human_note_draft("first body", viewport);
    let saved_id = controller.save_human_note_draft(viewport).unwrap();
    assert_eq!(controller.human_notes()[0].body, "first body");

    assert!(controller.edit_human_note(&saved_id, viewport));
    controller.update_human_note_draft("changed body", viewport);
    controller.cancel_human_note_draft(viewport);
    assert_eq!(controller.human_notes()[0].body, "first body");

    assert!(controller.edit_human_note(&saved_id, viewport));
    controller.update_human_note_draft("changed body", viewport);
    assert_eq!(
        controller.save_human_note_draft(viewport).as_deref(),
        Some(saved_id.as_str())
    );
    assert_eq!(controller.human_notes()[0].body, "changed body");
    assert!(controller.remove_human_note(&saved_id, viewport));
    assert!(controller.human_notes().is_empty());

    controller.add_human_note("src/lib.rs", None, None, "one", viewport);
    controller.add_human_note("src/lib.rs", None, None, "two", viewport);
    assert_eq!(
        controller.clear_human_notes(Some("src/lib.rs"), viewport),
        2
    );
    assert!(controller.human_notes().is_empty());
}
