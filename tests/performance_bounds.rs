use std::collections::HashMap;

use ramo::diff::parser::parse_unified_diff;
use ramo::review::{ReviewAction, ReviewController, ReviewOptions, ScrollUnit, Viewport};

fn patch(files: usize, changed_pairs: usize) -> String {
    let mut patch = String::new();
    for file in 0..files {
        patch.push_str(&format!(
            "diff --git a/src/file_{file}.rs b/src/file_{file}.rs\n--- a/src/file_{file}.rs\n+++ b/src/file_{file}.rs\n@@ -1,{changed_pairs} +1,{changed_pairs} @@\n"
        ));
        for line in 0..changed_pairs {
            patch.push_str(&format!("-let antigo_{line} = \"界 café 🦀\";\n"));
            patch.push_str(&format!("+let novo_{line} = \"差分 ação 🚀\";\n"));
        }
    }
    patch
}

#[test]
fn repeated_navigation_and_resize_cycles_do_not_accumulate_review_geometry() {
    let files = parse_unified_diff(&patch(64, 8));
    let expected_files = files.len();
    let mut controller = ReviewController::new(files, ReviewOptions::default());
    let viewports = [
        Viewport {
            width: 159,
            height: 24,
        },
        Viewport {
            width: 160,
            height: 40,
        },
        Viewport {
            width: 220,
            height: 24,
        },
    ];
    let mut stable_shapes = HashMap::new();

    for cycle in 0..300 {
        let viewport = viewports[cycle % viewports.len()];
        controller.apply(
            ReviewAction::Scroll {
                delta: if cycle % 2 == 0 { 1 } else { -1 },
                unit: ScrollUnit::Page,
            },
            viewport,
        );
        if cycle % 17 == 0 {
            controller.apply(ReviewAction::MoveFile(1), viewport);
        }
        let snapshot = controller.snapshot(viewport);
        assert_eq!(snapshot.visible_files.len(), expected_files);
        let shape = (
            snapshot.total_height,
            snapshot.sidebar_entries.len(),
            snapshot.max_scroll_top,
        );
        assert_eq!(*stable_shapes.entry(viewport.width).or_insert(shape), shape);
    }

    assert_eq!(stable_shapes.len(), viewports.len());
}

#[test]
fn repeated_replacements_clear_prior_geometry_and_context_state() {
    let files = parse_unified_diff(&patch(32, 8));
    let mut controller = ReviewController::new(files.clone(), ReviewOptions::default());
    let viewport = Viewport {
        width: 220,
        height: 30,
    };
    let expected = {
        let snapshot = controller.snapshot(viewport);
        (snapshot.visible_files.len(), snapshot.total_height)
    };

    for _ in 0..100 {
        controller.replace_files(files.clone(), viewport);
        let snapshot = controller.snapshot(viewport);
        assert_eq!(
            (snapshot.visible_files.len(), snapshot.total_height),
            expected
        );
    }
}
