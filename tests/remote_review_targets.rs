use ramo::core::input::LayoutMode;
use ramo::diff::parser::parse_unified_diff;
use ramo::remote_review::{InlineCommentTarget, RemoteLineSide};
use ramo::review::{ReviewAction, ReviewController, ReviewOptions, ReviewSide, Viewport};

const VIEWPORT: Viewport = Viewport {
    width: 180,
    height: 30,
};

fn controller(patch: &str, layout: LayoutMode) -> ReviewController {
    ReviewController::new(
        parse_unified_diff(patch),
        ReviewOptions {
            layout,
            ..ReviewOptions::default()
        },
    )
}

fn added_patch() -> &'static str {
    concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -0,0 +10,3 @@\n",
        "+ten\n",
        "+eleven\n",
        "+twelve\n",
    )
}

#[test]
fn stack_additions_map_forward_and_reverse_ranges_to_right() {
    let mut controller = controller(added_patch(), LayoutMode::Stack);
    controller.snapshot(VIEWPORT);
    let (anchor, _) = controller.selected_line_range(VIEWPORT).unwrap();
    controller.apply(ReviewAction::MoveCursor(2), VIEWPORT);
    let (_, focus) = controller.selected_line_range(VIEWPORT).unwrap();

    let forward = controller
        .begin_remote_human_note(Some((anchor, focus)), VIEWPORT)
        .unwrap()
        .unwrap();
    assert_eq!(
        forward,
        InlineCommentTarget {
            path: "src/lib.rs".into(),
            side: RemoteLineSide::Right,
            start_line: 10,
            end_line: 12,
        }
    );
    controller.cancel_human_note_draft(VIEWPORT);
    let reverse = controller
        .begin_remote_human_note(Some((focus, anchor)), VIEWPORT)
        .unwrap()
        .unwrap();
    assert_eq!(reverse, forward);
}

#[test]
fn stack_deletions_map_to_left_and_saved_notes_retain_the_target() {
    let patch = concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -7,2 +7,0 @@\n",
        "-seven\n",
        "-eight\n",
    );
    let mut controller = controller(patch, LayoutMode::Stack);
    controller.snapshot(VIEWPORT);
    let target = controller
        .begin_remote_human_note(None, VIEWPORT)
        .unwrap()
        .unwrap();
    assert_eq!(target.side, RemoteLineSide::Left);
    assert_eq!(target.start_line, 7);
    controller.update_human_note_draft("Keep this deletion?", VIEWPORT);
    controller.save_human_note_draft(VIEWPORT).unwrap();
    assert_eq!(
        controller.human_notes()[0].remote_target.as_ref(),
        Some(&target)
    );
}

#[test]
fn split_context_uses_focus_and_mixed_stack_sides_are_rejected() {
    let patch = concat!(
        "diff --git a/src/lib.rs b/src/lib.rs\n",
        "--- a/src/lib.rs\n",
        "+++ b/src/lib.rs\n",
        "@@ -1,3 +1,3 @@\n",
        " context\n",
        "-old\n",
        "+new\n",
        " tail\n",
    );
    let mut split = controller(patch, LayoutMode::Split);
    split.snapshot(VIEWPORT);
    split.apply(ReviewAction::FocusSide(ReviewSide::Left), VIEWPORT);
    let left = split
        .begin_remote_human_note(None, VIEWPORT)
        .unwrap()
        .unwrap();
    assert_eq!(left.side, RemoteLineSide::Left);
    split.cancel_human_note_draft(VIEWPORT);
    split.apply(ReviewAction::FocusSide(ReviewSide::Right), VIEWPORT);
    let right = split
        .begin_remote_human_note(None, VIEWPORT)
        .unwrap()
        .unwrap();
    assert_eq!(right.side, RemoteLineSide::Right);

    let mut stack = controller(patch, LayoutMode::Stack);
    stack.snapshot(VIEWPORT);
    stack.apply(ReviewAction::MoveCursor(1), VIEWPORT);
    let (anchor, _) = stack.selected_line_range(VIEWPORT).unwrap();
    stack.apply(ReviewAction::MoveCursor(1), VIEWPORT);
    let (_, focus) = stack.selected_line_range(VIEWPORT).unwrap();
    let error = stack
        .begin_remote_human_note(Some((anchor, focus)), VIEWPORT)
        .unwrap_err();
    assert!(error.to_string().contains("split the selection"));
    assert!(stack.human_note_draft().is_none());
}
