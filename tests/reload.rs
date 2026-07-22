use std::fs;
use std::io;

use ramo::config::ResolvedConfig;
use ramo::core::changeset::stable_file_id;
use ramo::core::input::{CommonOptions, PatchSource, ReviewInput};
use ramo::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, SourceSpec,
};
use ramo::input::{LoadContext, ReloadPlan, ReviewLoader};
use ramo::review::{ReviewAction, ReviewController, ReviewOptions, ScrollUnit, Viewport};
use ramo::vcs::SystemCommandRunner;

fn patch(replacement: &str) -> String {
    format!(
        "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+{replacement}\n"
    )
}

fn review_file(path: &str) -> DiffFile {
    let lines = (1..=40)
        .map(|line| DiffLine {
            kind: LineType::Context,
            content: format!("{path} line {line}"),
            old_lineno: Some(line),
            new_lineno: Some(line),
            moved: None,
        })
        .collect();
    DiffFile {
        id: stable_file_id(path, None),
        path: path.into(),
        previous_path: None,
        summary: None,
        agent: None,
        patch: String::new(),
        hunks: vec![Hunk {
            old_start: 1,
            new_start: 1,
            header: "@@ -1,40 +1,40 @@".into(),
            lines,
        }],
        change_kind: FileChangeKind::Modified,
        is_binary: false,
        is_untracked: false,
        is_too_large: false,
        stats_truncated: false,
        language: None,
        stats: FileStats::default(),
        old_source: SourceSpec::None,
        new_source: SourceSpec::None,
    }
}

#[test]
fn patch_file_reload_reads_replacement_contents() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("review.patch");
    fs::write(&path, patch("first")).unwrap();
    let input = ReviewInput::Patch {
        source: PatchSource::File(path.clone()),
        options: CommonOptions::default(),
    };
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    let context = LoadContext {
        cwd: temp.path(),
        config: &config,
        runner: &runner,
    };

    let loaded = ReviewLoader
        .load_with_context(&input, &mut io::empty(), &context)
        .unwrap();
    assert_eq!(
        loaded.reload_plan,
        ReloadPlan::PatchFile { path: path.clone() }
    );

    fs::write(&path, patch("second")).unwrap();
    let reloaded = ReviewLoader.reload(&loaded.reload_plan, &context).unwrap();
    assert!(reloaded.changeset.files[0].patch.contains("+second"));
}

#[test]
fn difftool_reload_preserves_the_display_path() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("left.tmp");
    let right = temp.path().join("right.tmp");
    let display = "src/lib.rs".into();
    fs::write(&left, "old\n").unwrap();
    fs::write(&right, "first\n").unwrap();
    let input = ReviewInput::FilePair {
        left: left.clone(),
        right: right.clone(),
        display_path: Some(display),
        options: CommonOptions::default(),
    };
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    let context = LoadContext {
        cwd: temp.path(),
        config: &config,
        runner: &runner,
    };

    let loaded = ReviewLoader
        .load_with_context(&input, &mut io::empty(), &context)
        .unwrap();
    fs::write(&right, "second\n").unwrap();
    let reloaded = ReviewLoader.reload(&loaded.reload_plan, &context).unwrap();

    assert_eq!(reloaded.changeset.files[0].path, "src/lib.rs");
    assert!(reloaded.changeset.files[0].patch.contains("+second"));
}

#[test]
fn replacing_files_preserves_selected_file_and_viewport_anchor() {
    let viewport = Viewport {
        width: 120,
        height: 12,
    };
    let a = review_file("src/a.rs");
    let b = review_file("src/b.rs");
    let mut controller =
        ReviewController::new(vec![a.clone(), b.clone()], ReviewOptions::default());
    controller.apply(ReviewAction::SelectFile(b.id.clone()), viewport);
    controller.apply(
        ReviewAction::Scroll {
            delta: 1,
            unit: ScrollUnit::HalfPage,
        },
        viewport,
    );
    let before = controller.snapshot(viewport).clone();

    controller.replace_files(vec![b, a], viewport);
    let after = controller.snapshot(viewport);

    assert_eq!(after.selected_file_id, before.selected_file_id);
    assert_eq!(after.selected_position, before.selected_position);
    assert!(after.scroll_top > 0);
}
