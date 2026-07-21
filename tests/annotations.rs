use pdiff::core::input::LayoutMode;
use pdiff::diff::parser::parse_unified_diff;
use pdiff::notes::LineRange;
use pdiff::review::{ReviewController, ReviewOptions, Viewport};

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n";

#[test]
fn export_contains_human_side_ranges_and_context_but_not_external_notes() {
    let mut files = parse_unified_diff(PATCH);
    files[0].agent = Some(
        pdiff::notes::parse_agent_context(
            "agent.json",
            br#"{"files":[{"path":"src/lib.rs","annotations":[{
              "newRange":[2,2],"summary":"external","source":"user"
            }]}]}"#,
        )
        .unwrap()
        .files
        .remove(0),
    );
    let viewport = Viewport {
        width: 100,
        height: 16,
    };
    let mut controller = ReviewController::new(
        files,
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    controller.add_human_note(
        "src/lib.rs",
        Some(LineRange { start: 2, end: 2 }),
        Some(LineRange { start: 2, end: 2 }),
        "human review",
        viewport,
    );

    let annotations = controller.export_annotations();
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].file, "src/lib.rs");
    assert_eq!(annotations[0].display_range, "L2 → R2");
    assert!(annotations[0].diff_context.contains("-old"));
    assert!(annotations[0].diff_context.contains("+new"));
    let markdown = pdiff::annotations::output::format_markdown(&annotations);
    assert!(markdown.contains("human review"));
    assert!(!markdown.contains("external"));
}
