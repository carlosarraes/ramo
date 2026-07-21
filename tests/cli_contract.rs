use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_lists_every_foundation_review_command() {
    Command::cargo_bin("pdiff")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("diff")
                .and(predicate::str::contains("show"))
                .and(predicate::str::contains("stash"))
                .and(predicate::str::contains("patch"))
                .and(predicate::str::contains("pager"))
                .and(predicate::str::contains("difftool")),
        );
}

#[test]
fn version_is_plain_and_successful() {
    Command::cargo_bin("pdiff")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("pdiff "));
}

#[test]
fn invalid_layout_fails_before_terminal_startup() {
    Command::cargo_bin("pdiff")
        .unwrap()
        .args(["diff", "--mode", "columns"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value 'columns'"));
}

#[test]
fn unsupported_integration_fails_without_terminal_output() {
    Command::cargo_bin("pdiff")
        .unwrap()
        .args(["install", "vscode"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("expected pi"));
}
