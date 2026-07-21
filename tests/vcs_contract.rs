use std::path::Path;

use pdiff::core::input::VcsId;
use pdiff::vcs::{CommandSpec, VcsOperation};

#[test]
fn vcs_ids_are_closed_and_display_native_executable_names() {
    assert_eq!(VcsId::Git.executable(), "git");
    assert_eq!(VcsId::Jj.executable(), "jj");
    assert_eq!(VcsId::Sl.executable(), "sl");
}

#[test]
fn command_specs_are_argv_not_shell_strings() {
    let spec = CommandSpec::new("git", Path::new("/repo")).args([
        "diff",
        "--",
        "file; touch /tmp/not-run",
    ]);
    assert_eq!(spec.program, "git");
    assert_eq!(spec.args[2], "file; touch /tmp/not-run");
    assert_eq!(spec.accepted_exit_codes, vec![0]);
}

#[test]
fn neutral_operations_do_not_leak_cli_command_names() {
    assert_eq!(VcsOperation::WorkingTree.kind_name(), "working-tree diff");
    assert_eq!(VcsOperation::RevisionShow.kind_name(), "revision show");
    assert_eq!(VcsOperation::StashShow.kind_name(), "stash show");
}
