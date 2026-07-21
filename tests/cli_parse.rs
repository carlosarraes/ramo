use std::ffi::OsString;
use std::path::PathBuf;

use ramo::cli::{Action, parse_from};
use ramo::core::input::{InputKind, LayoutMode, PatchSource, ReviewInput};

#[test]
fn bare_pipe_is_patch_stdin() {
    let invocation = parse_from(["ramo"], false).unwrap();
    assert!(matches!(
        invocation.action,
        Action::Review(ReviewInput::Patch {
            source: PatchSource::Stdin,
            ..
        })
    ));
}

#[test]
fn diff_supports_range_flags_and_pathspecs() {
    let invocation = parse_from(
        [
            "ramo",
            "diff",
            "main...HEAD",
            "--mode",
            "split",
            "--watch",
            "--",
            "src",
            "tests",
        ],
        true,
    )
    .unwrap();
    let Action::Review(ReviewInput::VcsDiff {
        range,
        staged,
        pathspecs,
        options,
    }) = invocation.action
    else {
        panic!("expected vcs diff")
    };
    assert_eq!(range.as_deref(), Some("main...HEAD"));
    assert!(!staged);
    assert_eq!(pathspecs, ["src", "tests"]);
    assert_eq!(options.mode, Some(LayoutMode::Split));
    assert_eq!(options.watch, Some(true));
}

#[test]
fn existing_two_file_operands_become_a_file_pair() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("before.rs");
    let right = temp.path().join("after.rs");
    std::fs::write(&left, "old\n").unwrap();
    std::fs::write(&right, "new\n").unwrap();
    let args = vec![
        OsString::from("ramo"),
        OsString::from("diff"),
        left.into_os_string(),
        right.clone().into_os_string(),
    ];
    let invocation = parse_from(args, true).unwrap();
    assert!(matches!(
        invocation.action,
        Action::Review(ReviewInput::FilePair { right: value, .. }) if value == right
    ));
}

#[test]
fn legacy_input_and_output_flags_remain_accepted() {
    let invocation = parse_from(
        ["ramo", "--input", "review.patch", "--output", "review.md"],
        true,
    )
    .unwrap();
    assert!(matches!(
        invocation.action,
        Action::Review(ReviewInput::Patch {
            source: PatchSource::File(_),
            ..
        })
    ));
    assert_eq!(
        invocation.output.markdown_path,
        Some(PathBuf::from("review.md"))
    );
}

#[test]
fn more_than_two_diff_targets_is_rejected() {
    let error = parse_from(["ramo", "diff", "one", "two", "three"], true).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("one revision or two existing files")
    );
}

#[test]
fn show_and_stash_show_preserve_their_references() {
    let show = parse_from(["ramo", "show", "HEAD~1", "--", "src"], true).unwrap();
    assert!(matches!(
        show.action,
        Action::Review(ReviewInput::Show { reference, pathspecs, .. })
            if reference.as_deref() == Some("HEAD~1") && pathspecs == ["src"]
    ));

    let stash = parse_from(["ramo", "stash", "show", "stash@{2}"], true).unwrap();
    assert!(matches!(
        stash.action,
        Action::Review(ReviewInput::StashShow { reference, .. })
            if reference.as_deref() == Some("stash@{2}")
    ));
}

#[test]
fn patch_dash_and_pager_have_distinct_input_kinds() {
    let patch = parse_from(["ramo", "patch", "-"], true).unwrap();
    let Action::Review(patch) = patch.action else {
        panic!("expected patch review")
    };
    assert_eq!(patch.kind(), InputKind::Patch);
    assert!(matches!(
        patch,
        ReviewInput::Patch {
            source: PatchSource::Stdin,
            ..
        }
    ));

    let pager = parse_from(["ramo", "pager"], false).unwrap();
    let Action::Review(pager) = pager.action else {
        panic!("expected pager review")
    };
    assert_eq!(pager.kind(), InputKind::Pager);
}

#[test]
fn difftool_preserves_display_path_and_watch() {
    let invocation = parse_from(
        [
            "ramo",
            "difftool",
            "before.rs",
            "after.rs",
            "src/lib.rs",
            "--watch",
        ],
        true,
    )
    .unwrap();
    assert!(matches!(
        invocation.action,
        Action::Review(ReviewInput::FilePair { display_path, options, .. })
            if display_path == Some(PathBuf::from("src/lib.rs")) && options.watch == Some(true)
    ));
}

#[test]
fn cached_alias_and_boolean_overrides_are_normalized() {
    let invocation = parse_from(
        [
            "ramo",
            "diff",
            "--cached",
            "--exclude-untracked",
            "--no-line-numbers",
            "--wrap",
            "--no-hunk-headers",
            "--agent-notes",
            "--transparent-bg",
        ],
        true,
    )
    .unwrap();
    assert!(matches!(
        invocation.action,
        Action::Review(ReviewInput::VcsDiff { staged: true, options, .. })
            if options.exclude_untracked == Some(true)
                && options.line_numbers == Some(false)
                && options.wrap_lines == Some(true)
                && options.hunk_headers == Some(false)
                && options.agent_notes == Some(true)
                && options.transparent_background == Some(true)
    ));
}

#[test]
fn invalid_layout_is_a_clap_error() {
    let error = parse_from(["ramo", "diff", "--mode", "columns"], true).unwrap_err();
    assert!(error.to_string().contains("invalid value 'columns'"));
}

#[test]
fn pi_installation_actions_and_invalid_targets_are_explicit() {
    assert!(matches!(
        parse_from(["ramo", "install", "pi"], true).unwrap().action,
        Action::InstallPi
    ));
    assert!(matches!(
        parse_from(["ramo", "uninstall", "pi"], true)
            .unwrap()
            .action,
        Action::UninstallPi
    ));
    let error = parse_from(["ramo", "install", "vscode"], true).unwrap_err();
    assert!(error.to_string().contains("expected pi"));
}

#[test]
fn help_and_version_are_successful_print_actions() {
    assert!(matches!(
        parse_from(["ramo", "--help"], true).unwrap().action,
        Action::Print(text) if text.contains("Usage:")
    ));
    assert!(matches!(
        parse_from(["ramo", "--version"], true).unwrap().action,
        Action::Print(text) if text.starts_with("ramo ")
    ));
}
