use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_lists_every_foundation_review_command() {
    Command::cargo_bin("ramo")
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
        )
        .stdout(
            predicate::str::contains("Common review options:")
                .and(predicate::str::contains("--mode <MODE>"))
                .and(predicate::str::contains("--watch"))
                .and(predicate::str::contains("--agent-context <PATH>"))
                .and(predicate::str::contains("--exclude-untracked")),
        );
}

#[test]
fn version_is_plain_and_successful() {
    Command::cargo_bin("ramo")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("ramo "));
}

#[test]
fn invalid_layout_fails_before_terminal_startup() {
    Command::cargo_bin("ramo")
        .unwrap()
        .args(["diff", "--mode", "columns"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value 'columns'"))
        .stdout(predicate::str::contains("\x1b[?1049h").not());
}

#[test]
fn unsupported_integration_fails_without_terminal_output() {
    Command::cargo_bin("ramo")
        .unwrap()
        .args(["install", "vscode"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("expected pi"))
        .stdout(predicate::str::contains("\x1b[?1049h").not());
}
