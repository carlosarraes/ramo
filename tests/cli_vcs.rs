use std::path::Path;
use std::process::Command as ProcessCommand;

use assert_cmd::Command;
use predicates::prelude::*;

struct GitFixture {
    temp: tempfile::TempDir,
}

impl GitFixture {
    fn with_commit() -> Self {
        let fixture = Self {
            temp: tempfile::tempdir().unwrap(),
        };
        fixture.git(["init", "-q"]);
        fixture.git(["config", "user.name", "Pdiff Test"]);
        fixture.git(["config", "user.email", "pdiff@example.invalid"]);
        std::fs::write(fixture.path().join("tracked.txt"), "base\n").unwrap();
        fixture.git(["add", "tracked.txt"]);
        fixture.git(["commit", "-q", "-m", "base"]);
        fixture
    }

    fn path(&self) -> &Path {
        self.temp.path()
    }

    fn git<const N: usize>(&self, args: [&str; N]) {
        let output = ProcessCommand::new("git")
            .args(args)
            .current_dir(self.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn invalid_git_ref_fails_before_terminal_startup() {
    let repo = GitFixture::with_commit();
    Command::cargo_bin("pdiff")
        .unwrap()
        .current_dir(repo.path())
        .args(["show", "does-not-exist"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("does-not-exist")
                .and(predicate::str::contains("Check the ref")),
        )
        .stdout(predicate::str::contains("\x1b[?1049h").not());
}

#[test]
fn git_show_reaches_native_loader_without_terminal_startup_for_empty_pathspec() {
    let repo = GitFixture::with_commit();
    Command::cargo_bin("pdiff")
        .unwrap()
        .current_dir(repo.path())
        .args(["show", "HEAD", "--", "absent-path"])
        .assert()
        .success()
        .stderr(predicate::str::contains("No changes to review."))
        .stdout(predicate::str::contains("\x1b[?1049h").not());
}
