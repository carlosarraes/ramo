use std::io::Cursor;
use std::path::PathBuf;

use pdiff::core::input::{CommonOptions, PatchSource, ReviewInput};
use pdiff::diff::model::{FileChangeKind, MovedLineKind, SourceSpec};
use pdiff::input::{LoadError, ReloadPlan, ReviewLoader, normalize_patch_text};

#[test]
fn patch_stdin_loads_a_changeset() {
    let patch = include_str!("fixtures/simple.patch");
    let loaded = ReviewLoader
        .load(
            &ReviewInput::Patch {
                source: PatchSource::Stdin,
                options: CommonOptions::default(),
            },
            &mut Cursor::new(patch),
        )
        .unwrap();
    assert_eq!(loaded.changeset.files[0].path, "src/main.rs");
    assert_eq!(loaded.changeset.source_label, "patch stdin");
}

#[test]
fn each_file_retains_only_its_own_exact_patch_chunk() {
    let patch = concat!(
        "diff --git a/src/a.rs b/src/a.rs\n",
        "--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-old a\n+new a\n",
        "diff --git a/src/b.rs b/src/b.rs\n",
        "--- a/src/b.rs\n+++ b/src/b.rs\n@@ -2 +2 @@\n-old b\n+new b\n",
    );
    let loaded = ReviewLoader
        .load(
            &ReviewInput::Patch {
                source: PatchSource::Stdin,
                options: CommonOptions::default(),
            },
            &mut Cursor::new(patch),
        )
        .unwrap();

    assert_eq!(loaded.changeset.files.len(), 2);
    assert_eq!(
        loaded.changeset.files[0].patch,
        "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-old a\n+new a\n"
    );
    assert_eq!(
        loaded.changeset.files[1].patch,
        "diff --git a/src/b.rs b/src/b.rs\n--- a/src/b.rs\n+++ b/src/b.rs\n@@ -2 +2 @@\n-old b\n+new b\n"
    );
}

#[test]
fn direct_files_are_diffed_without_an_external_program() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("before.txt");
    let right = temp.path().join("after.txt");
    std::fs::write(&left, "before\n").unwrap();
    std::fs::write(&right, "after\n").unwrap();
    let loaded = ReviewLoader
        .load(
            &ReviewInput::FilePair {
                left,
                right,
                display_path: None,
                options: CommonOptions::default(),
            },
            &mut Cursor::new([]),
        )
        .unwrap();
    assert_eq!(loaded.changeset.files.len(), 1);
    assert_eq!(loaded.changeset.stats().additions, 1);
    assert_eq!(loaded.changeset.stats().deletions, 1);
}

#[test]
fn patch_file_uses_its_path_as_source_context() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("review.patch");
    std::fs::write(&path, include_str!("fixtures/simple.patch")).unwrap();
    let loaded = ReviewLoader
        .load(
            &ReviewInput::Patch {
                source: PatchSource::File(path.clone()),
                options: CommonOptions::default(),
            },
            &mut Cursor::new([]),
        )
        .unwrap();
    assert!(
        loaded
            .changeset
            .source_label
            .contains(&path.display().to_string())
    );
    assert_eq!(loaded.changeset.title, "review.patch");
}

#[test]
fn terminal_controls_and_crlf_are_removed_before_parsing() {
    let controlled = "\x1b]8;;https://example.com\x1b\\\x1b[31mdiff --git a/a b/a\x1b[0m\r\n";
    assert_eq!(normalize_patch_text(controlled), "diff --git a/a b/a\n");
}

#[test]
fn empty_and_malformed_patch_inputs_have_distinct_errors() {
    let input = ReviewInput::Patch {
        source: PatchSource::Stdin,
        options: CommonOptions::default(),
    };
    assert!(matches!(
        ReviewLoader
            .load(&input, &mut Cursor::new(" \n"))
            .unwrap_err(),
        LoadError::EmptyInput
    ));
    assert!(matches!(
        ReviewLoader
            .load(&input, &mut Cursor::new("ordinary text\n"))
            .unwrap_err(),
        LoadError::InvalidPatch { .. }
    ));
}

#[test]
fn missing_and_non_utf8_direct_files_name_the_failed_path() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing.txt");
    let right = temp.path().join("right.txt");
    std::fs::write(&right, "right\n").unwrap();
    let missing_error = ReviewLoader
        .load(
            &ReviewInput::FilePair {
                left: missing.clone(),
                right: right.clone(),
                display_path: None,
                options: CommonOptions::default(),
            },
            &mut Cursor::new([]),
        )
        .unwrap_err();
    assert!(
        missing_error
            .to_string()
            .contains(&missing.display().to_string())
    );

    let invalid = temp.path().join("invalid.txt");
    std::fs::write(&invalid, [0xff, b'a']).unwrap();
    let invalid_error = ReviewLoader
        .load(
            &ReviewInput::FilePair {
                left: invalid.clone(),
                right,
                display_path: None,
                options: CommonOptions::default(),
            },
            &mut Cursor::new([]),
        )
        .unwrap_err();
    assert!(matches!(invalid_error, LoadError::NonUtf8 { path } if path == invalid));
}

#[test]
fn identical_files_produce_an_empty_changeset_with_a_reload_plan() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("left.txt");
    let right = temp.path().join("right.txt");
    std::fs::write(&left, "same\n").unwrap();
    std::fs::write(&right, "same\n").unwrap();
    let loaded = ReviewLoader
        .load(
            &ReviewInput::FilePair {
                left: left.clone(),
                right: right.clone(),
                display_path: None,
                options: CommonOptions::default(),
            },
            &mut Cursor::new([]),
        )
        .unwrap();
    assert!(loaded.changeset.files.is_empty());
    assert_eq!(
        loaded.reload_plan,
        ReloadPlan::Files {
            left,
            right,
            display_path: None,
        }
    );
}

#[test]
fn binary_pairs_use_a_placeholder_without_decoding_contents() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("before.bin");
    let right = temp.path().join("after.bin");
    std::fs::write(&left, [0, 1, 2]).unwrap();
    std::fs::write(&right, [0, 1, 3]).unwrap();
    let loaded = ReviewLoader
        .load(
            &ReviewInput::FilePair {
                left,
                right,
                display_path: Some(PathBuf::from("assets/image.bin")),
                options: CommonOptions::default(),
            },
            &mut Cursor::new([]),
        )
        .unwrap();
    assert_eq!(loaded.changeset.source_label, "vcs difftool");
    assert_eq!(loaded.changeset.files[0].path, "assets/image.bin");
    assert!(loaded.changeset.files[0].is_binary);
    assert!(loaded.changeset.files[0].hunks.is_empty());
}

#[test]
fn unsupported_inputs_fail_before_terminal_startup() {
    let input = ReviewInput::Pager {
        options: CommonOptions::default(),
    };
    assert!(matches!(
        ReviewLoader.load(&input, &mut Cursor::new([])).unwrap_err(),
        LoadError::UnsupportedInput(pdiff::core::input::InputKind::Pager)
    ));
}

#[test]
fn deterministic_git_ansi_colors_become_moved_line_classes() {
    let patch = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n\x1b[1;35m-old\x1b[m\n\x1b[1;36m+new\x1b[m\n";
    let files = pdiff::vcs::git::parse_git_patch(patch);
    assert_eq!(
        files[0].hunks[0].lines[0].moved,
        Some(MovedLineKind::OldMoved)
    );
    assert_eq!(
        files[0].hunks[0].lines[1].moved,
        Some(MovedLineKind::NewMoved)
    );
    assert_eq!(files[0].hunks[0].lines[0].content, "old");
}

#[cfg(unix)]
#[test]
fn difftool_dev_null_side_is_an_added_file() {
    let temp = tempfile::tempdir().unwrap();
    let added = temp.path().join("added.txt");
    std::fs::write(&added, "new\n").unwrap();
    let loaded = ReviewLoader
        .load(
            &ReviewInput::FilePair {
                left: PathBuf::from("/dev/null"),
                right: added.clone(),
                display_path: Some(PathBuf::from("src/added.txt")),
                options: CommonOptions::default(),
            },
            &mut Cursor::new([]),
        )
        .unwrap();
    assert_eq!(loaded.changeset.files[0].change_kind, FileChangeKind::Added);
    assert_eq!(loaded.changeset.files[0].old_source, SourceSpec::None);
    assert_eq!(
        loaded.changeset.files[0].new_source,
        SourceSpec::File(added)
    );
}

#[cfg(unix)]
#[test]
fn difftool_dev_null_right_side_is_a_deleted_file() {
    let temp = tempfile::tempdir().unwrap();
    let deleted = temp.path().join("deleted.txt");
    std::fs::write(&deleted, "old\n").unwrap();
    let loaded = ReviewLoader
        .load(
            &ReviewInput::FilePair {
                left: deleted.clone(),
                right: PathBuf::from("/dev/null"),
                display_path: Some(PathBuf::from("src/deleted.txt")),
                options: CommonOptions::default(),
            },
            &mut Cursor::new([]),
        )
        .unwrap();
    assert_eq!(
        loaded.changeset.files[0].change_kind,
        FileChangeKind::Deleted
    );
    assert_eq!(
        loaded.changeset.files[0].old_source,
        SourceSpec::File(deleted)
    );
    assert_eq!(loaded.changeset.files[0].new_source, SourceSpec::None);
}
