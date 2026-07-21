use std::path::PathBuf;

use ramo::core::input::LayoutMode;
use ramo::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, SourceSpec,
};
use ramo::review::{
    HunkTarget, ReviewAction, ReviewController, ReviewEffect, ReviewFileStatus, ReviewOptions,
    ScrollUnit, SidebarEntrySnapshot, Viewport,
};

fn file(path: &str, previous_path: Option<&str>, hunk_count: usize) -> DiffFile {
    let mut old_line = 1;
    let mut new_line = 1;
    let hunks = (0..hunk_count)
        .map(|hunk_index| {
            let hunk = Hunk {
                old_start: old_line,
                new_start: new_line,
                header: format!("@@ -{old_line},2 +{new_line},2 @@ hunk {hunk_index}"),
                lines: vec![
                    DiffLine {
                        kind: LineType::Deletion,
                        content: format!("old {path} {hunk_index}"),
                        old_lineno: Some(old_line),
                        new_lineno: None,
                        moved: None,
                    },
                    DiffLine {
                        kind: LineType::Addition,
                        content: format!("new {path} {hunk_index}"),
                        old_lineno: None,
                        new_lineno: Some(new_line),
                        moved: None,
                    },
                    DiffLine {
                        kind: LineType::Context,
                        content: "same".into(),
                        old_lineno: Some(old_line + 1),
                        new_lineno: Some(new_line + 1),
                        moved: None,
                    },
                ],
            };
            old_line += 10;
            new_line += 10;
            hunk
        })
        .collect();
    DiffFile {
        id: format!("file:{}->{path}", previous_path.unwrap_or(path)),
        path: path.into(),
        previous_path: previous_path.map(str::to_owned),
        summary: None,
        agent: None,
        patch: String::new(),
        hunks,
        change_kind: previous_path.map_or(FileChangeKind::Modified, |_| FileChangeKind::Renamed),
        is_binary: false,
        is_untracked: false,
        is_too_large: false,
        stats_truncated: false,
        language: None,
        stats: FileStats {
            additions: hunk_count,
            deletions: hunk_count,
        },
        old_source: SourceSpec::File(PathBuf::from("old")),
        new_source: SourceSpec::File(PathBuf::from("new")),
    }
}

fn viewport(width: u16, height: u16) -> Viewport {
    Viewport { width, height }
}

#[test]
fn continuous_stream_filter_and_sidebar_selection_preserve_file_order() {
    let mut alpha = file("src/alpha.rs", None, 2);
    alpha.summary = Some("authentication path".into());
    let beta = file("src/beta.rs", Some("src/old_beta.rs"), 2);
    let gamma = file("docs/gamma.md", None, 2);
    let mut controller = ReviewController::new(vec![alpha, beta, gamma], ReviewOptions::default());

    let initial = controller.snapshot(viewport(220, 20)).clone();
    assert_eq!(
        initial
            .visible_files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["src/alpha.rs", "src/beta.rs", "docs/gamma.md"]
    );
    assert!(initial.show_sidebar);
    assert_eq!(
        initial.sidebar_entries,
        vec![
            SidebarEntrySnapshot::Group {
                id: "group:src:0".into(),
                label: "src/".into(),
            },
            SidebarEntrySnapshot::File {
                id: "file:src/alpha.rs->src/alpha.rs".into(),
                name: "alpha.rs".into(),
                annotations_text: None,
                additions_text: Some("+2".into()),
                deletions_text: Some("-2".into()),
                status: ReviewFileStatus::Modified,
            },
            SidebarEntrySnapshot::File {
                id: "file:src/old_beta.rs->src/beta.rs".into(),
                name: "old_beta.rs -> beta.rs".into(),
                annotations_text: None,
                additions_text: Some("+2".into()),
                deletions_text: Some("-2".into()),
                status: ReviewFileStatus::Renamed,
            },
            SidebarEntrySnapshot::Group {
                id: "group:docs:2".into(),
                label: "docs/".into(),
            },
            SidebarEntrySnapshot::File {
                id: "file:docs/gamma.md->docs/gamma.md".into(),
                name: "gamma.md".into(),
                annotations_text: None,
                additions_text: Some("+2".into()),
                deletions_text: Some("-2".into()),
                status: ReviewFileStatus::Modified,
            },
        ]
    );

    controller.apply(
        ReviewAction::SelectFile("file:src/old_beta.rs->src/beta.rs".into()),
        viewport(220, 20),
    );
    let selected = controller.snapshot(viewport(220, 20)).clone();
    assert_eq!(
        selected.selected_file_id.as_deref(),
        Some("file:src/old_beta.rs->src/beta.rs")
    );
    assert_eq!(selected.visible_files.len(), 3);
    assert!(selected.scroll_top > 0);

    controller.apply(
        ReviewAction::SetFilter("OLD_BETA".into()),
        viewport(220, 20),
    );
    let renamed = controller.snapshot(viewport(220, 20)).clone();
    assert_eq!(renamed.visible_files.len(), 1);
    assert_eq!(renamed.visible_files[0].path, "src/beta.rs");

    controller.apply(
        ReviewAction::SetFilter("AUTHENTICATION".into()),
        viewport(220, 20),
    );
    let summary = controller.snapshot(viewport(220, 20)).clone();
    assert_eq!(summary.visible_files.len(), 1);
    assert_eq!(summary.visible_files[0].path, "src/alpha.rs");
    assert_eq!(
        summary.selected_file_id.as_deref(),
        Some("file:src/alpha.rs->src/alpha.rs")
    );

    controller.apply(ReviewAction::SetFilter(String::new()), viewport(220, 20));
    assert_eq!(
        controller.snapshot(viewport(220, 20)).visible_files.len(),
        3
    );
}

#[test]
fn snapshots_classify_every_file_state_and_format_truncated_stats() {
    let mut added = file("added.rs", None, 1);
    added.change_kind = FileChangeKind::Added;
    added.stats_truncated = true;
    let mut deleted = file("deleted.rs", None, 1);
    deleted.change_kind = FileChangeKind::Deleted;
    let renamed = file("renamed.rs", Some("old.rs"), 1);
    let mut copied = file("copied.rs", Some("source.rs"), 1);
    copied.change_kind = FileChangeKind::Copied;
    let mut binary = file("binary.dat", None, 0);
    binary.is_binary = true;
    let mut large = file("large.txt", None, 0);
    large.is_too_large = true;
    let mut untracked = file("new.txt", None, 1);
    untracked.change_kind = FileChangeKind::Added;
    untracked.is_untracked = true;

    let mut controller = ReviewController::new(
        vec![added, deleted, renamed, copied, binary, large, untracked],
        ReviewOptions::default(),
    );
    let snapshot = controller.snapshot(viewport(220, 20));
    assert_eq!(
        snapshot
            .visible_files
            .iter()
            .map(|file| file.status)
            .collect::<Vec<_>>(),
        [
            ReviewFileStatus::Added,
            ReviewFileStatus::Deleted,
            ReviewFileStatus::Renamed,
            ReviewFileStatus::Copied,
            ReviewFileStatus::Binary,
            ReviewFileStatus::TooLarge,
            ReviewFileStatus::Untracked,
        ]
    );
    assert_eq!(
        snapshot.sidebar_entries[0],
        SidebarEntrySnapshot::File {
            id: "file:added.rs->added.rs".into(),
            name: "added.rs".into(),
            annotations_text: None,
            additions_text: Some("+1+".into()),
            deletions_text: Some("-1".into()),
            status: ReviewFileStatus::Added,
        }
    );
}

#[test]
fn navigation_scrolls_clamps_and_wraps_hunks_and_files() {
    let mut controller = ReviewController::new(
        vec![file("a.rs", None, 3), file("b.rs", None, 2)],
        ReviewOptions::default(),
    );
    let view = viewport(80, 8);

    controller.apply(ReviewAction::MoveHunk(-1), view);
    let last_hunk = controller.snapshot(view).clone();
    assert_eq!(
        last_hunk.selected_file_id.as_deref(),
        Some("file:b.rs->b.rs")
    );
    assert_eq!(last_hunk.selected_hunk_index, Some(1));

    controller.apply(ReviewAction::MoveFile(1), view);
    assert_eq!(
        controller.snapshot(view).selected_file_id.as_deref(),
        Some("file:a.rs->a.rs")
    );

    controller.apply(ReviewAction::JumpTop, view);
    controller.apply(
        ReviewAction::Scroll {
            delta: 1,
            unit: ScrollUnit::Step,
        },
        view,
    );
    assert_eq!(controller.snapshot(view).scroll_top, 1);
    controller.apply(
        ReviewAction::Scroll {
            delta: 1,
            unit: ScrollUnit::HalfPage,
        },
        view,
    );
    assert_eq!(controller.snapshot(view).scroll_top, 5);
    controller.apply(
        ReviewAction::Scroll {
            delta: 1,
            unit: ScrollUnit::Page,
        },
        view,
    );
    assert_eq!(controller.snapshot(view).scroll_top, 13);
    controller.apply(ReviewAction::JumpBottom, view);
    let bottom = controller.snapshot(view).clone();
    assert_eq!(bottom.scroll_top, bottom.max_scroll_top);
    controller.apply(ReviewAction::JumpTop, view);
    assert_eq!(controller.snapshot(view).scroll_top, 0);
}

#[test]
fn view_changes_preserve_semantic_selection_and_clamp_horizontal_scroll() {
    let mut long = file("long.rs", None, 20);
    long.hunks[8].lines[1].content = "界".repeat(120);
    let mut controller = ReviewController::new(vec![long], ReviewOptions::default());
    let view = viewport(80, 10);
    for _ in 0..18 {
        controller.apply(
            ReviewAction::Scroll {
                delta: 1,
                unit: ScrollUnit::Step,
            },
            view,
        );
    }
    let selected = controller.snapshot(view).selected_position.clone();

    for action in [
        ReviewAction::SetLayout(LayoutMode::Split),
        ReviewAction::ToggleLineNumbers,
        ReviewAction::ToggleHunkHeaders,
        ReviewAction::ToggleSidebar,
    ] {
        controller.apply(action, view);
        assert_eq!(controller.snapshot(view).selected_position, selected);
    }

    controller.apply(ReviewAction::ScrollHorizontal(10_000), view);
    let horizontal = controller.snapshot(view).horizontal_offset;
    assert!(horizontal > 0);
    assert!(horizontal < 10_000);
    controller.apply(ReviewAction::ToggleWrap, view);
    let wrapped = controller.snapshot(view).clone();
    assert!(wrapped.wrap_lines);
    assert_eq!(wrapped.horizontal_offset, 0);
    assert_eq!(wrapped.selected_position, selected);
}

#[test]
fn annotated_navigation_respects_filter_and_empty_filters_are_safe() {
    let options = ReviewOptions {
        annotated_hunks: vec![
            HunkTarget::new("file:a.rs->a.rs", 1),
            HunkTarget::new("file:b.rs->b.rs", 0),
        ],
        ..ReviewOptions::default()
    };
    let mut controller =
        ReviewController::new(vec![file("a.rs", None, 2), file("b.rs", None, 2)], options);
    let view = viewport(80, 10);
    controller.apply(ReviewAction::SetFilter("b.rs".into()), view);
    controller.apply(ReviewAction::MoveAnnotatedHunk(1), view);
    let selected = controller.snapshot(view).clone();
    assert_eq!(
        selected.selected_file_id.as_deref(),
        Some("file:b.rs->b.rs")
    );
    assert_eq!(selected.selected_hunk_index, Some(0));

    controller.apply(ReviewAction::SetFilter("does-not-match".into()), view);
    let empty = controller.snapshot(view).clone();
    assert!(empty.visible_files.is_empty());
    assert!(empty.selected_file_id.is_none());
    assert_eq!(empty.total_height, 0);
    controller.apply(ReviewAction::MoveFile(1), view);
    controller.apply(ReviewAction::MoveHunk(1), view);
    assert!(controller.snapshot(view).selected_file_id.is_none());
}

#[test]
fn pager_mode_rejects_application_only_actions_but_keeps_navigation() {
    let mut controller = ReviewController::new(
        vec![file("a.rs", None, 4)],
        ReviewOptions {
            pager_mode: true,
            ..ReviewOptions::default()
        },
    );
    let view = viewport(80, 5);
    for action in [
        ReviewAction::FocusFilter,
        ReviewAction::OpenHelp,
        ReviewAction::OpenThemeSelector,
    ] {
        assert_eq!(controller.apply(action, view), ReviewEffect::None);
    }
    assert_eq!(
        controller.apply(ReviewAction::StartNote, view),
        ReviewEffect::StartNote
    );
    assert_eq!(
        controller.apply(
            ReviewAction::Scroll {
                delta: 1,
                unit: ScrollUnit::Page,
            },
            view
        ),
        ReviewEffect::Redraw
    );
    assert!(controller.snapshot(view).scroll_top > 0);
}
