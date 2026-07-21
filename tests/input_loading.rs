use std::io::Cursor;
use std::path::PathBuf;

use pdiff::core::input::{CommonOptions, PatchSource, ReviewInput};
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
    assert_eq!(loaded.reload_plan, ReloadPlan::Files { left, right });
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
