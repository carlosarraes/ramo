use std::io::Cursor;
use std::path::Path;
use std::process::Command;

use ramo::config::ResolvedConfig;
use ramo::core::input::{CommonOptions, ReviewInput};
use ramo::diff::model::SourceSpec;
use ramo::input::{LoadContext, LoadError, LoadedReview, ReloadPlan, ReviewLoader};
use ramo::vcs::SystemCommandRunner;
use ramo::vcs::source::{SourceError, SourceReader};

struct GitFixture {
    temp: tempfile::TempDir,
}

impl GitFixture {
    fn new() -> Self {
        let fixture = Self::non_repository();
        fixture.git(["init", "-q"]);
        fixture.git(["config", "user.name", "Ramo Test"]);
        fixture.git(["config", "user.email", "ramo@example.invalid"]);
        fixture
    }

    fn non_repository() -> Self {
        Self {
            temp: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self) -> &Path {
        self.temp.path()
    }

    fn git<const N: usize>(&self, args: [&str; N]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(self.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn write(&self, path: &str, contents: &str) {
        let absolute = self.path().join(path);
        if let Some(parent) = absolute.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(absolute, contents).unwrap();
    }

    fn commit_file(&self, path: &str, contents: &str) {
        self.write(path, contents);
        self.git(["add", path]);
        self.git(["commit", "-q", "-m", path]);
    }

    fn commit_all(&self, message: &str) {
        self.git(["add", "-A"]);
        self.git(["commit", "-q", "-m", message]);
    }

    fn load(&self, input: ReviewInput) -> LoadedReview {
        self.try_load(input).unwrap()
    }

    fn load_error(&self, input: ReviewInput) -> LoadError {
        self.try_load(input).unwrap_err()
    }

    fn try_load(&self, input: ReviewInput) -> Result<LoadedReview, LoadError> {
        let config = ResolvedConfig::default();
        let runner = SystemCommandRunner;
        let context = LoadContext {
            cwd: self.path(),
            config: &config,
            runner: &runner,
        };
        ReviewLoader.load_with_context(&input, &mut Cursor::new([]), &context)
    }
}

fn working_tree_input() -> ReviewInput {
    ReviewInput::VcsDiff {
        range: None,
        staged: false,
        pathspecs: vec![],
        options: CommonOptions::default(),
    }
}

#[test]
fn working_tree_includes_tracked_and_untracked_files() {
    let repo = GitFixture::new();
    repo.commit_file("tracked.txt", "before\n");
    repo.write("tracked.txt", "after\n");
    repo.write("new file;safe.txt", "new\n");
    let loaded = repo.load(working_tree_input());
    assert_eq!(
        loaded
            .changeset
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["tracked.txt", "new file;safe.txt"]
    );
    assert!(loaded.changeset.files[1].is_untracked);
    assert!(matches!(loaded.reload_plan, ReloadPlan::Vcs { .. }));
}

#[test]
fn clean_working_tree_is_a_valid_empty_changeset() {
    let repo = GitFixture::new();
    repo.commit_file("tracked.txt", "unchanged\n");
    let loaded = repo.load(working_tree_input());
    assert!(loaded.changeset.files.is_empty());
    assert!(matches!(loaded.reload_plan, ReloadPlan::Vcs { .. }));
}

#[test]
fn staged_diff_excludes_untracked_and_unstaged_changes() {
    let repo = GitFixture::new();
    repo.commit_file("staged.txt", "base\n");
    repo.commit_file("unstaged.txt", "base\n");
    repo.write("staged.txt", "index\n");
    repo.write("unstaged.txt", "worktree\n");
    repo.write("unknown.txt", "unknown\n");
    repo.git(["add", "staged.txt"]);
    let loaded = repo.load(ReviewInput::VcsDiff {
        range: None,
        staged: true,
        pathspecs: vec![],
        options: CommonOptions::default(),
    });
    assert_eq!(
        loaded
            .changeset
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["staged.txt"]
    );
}

#[test]
fn range_and_pathspec_review_only_the_requested_history() {
    let repo = GitFixture::new();
    repo.commit_file("src/lib.rs", "one\n");
    repo.commit_file("docs/readme.md", "one\n");
    repo.write("src/lib.rs", "two\n");
    repo.write("docs/readme.md", "two\n");
    repo.commit_all("change both");
    let loaded = repo.load(ReviewInput::VcsDiff {
        range: Some("HEAD^..HEAD".into()),
        staged: false,
        pathspecs: vec!["src".into()],
        options: CommonOptions::default(),
    });
    assert_eq!(
        loaded
            .changeset
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["src/lib.rs"]
    );
    assert!(matches!(
        loaded.changeset.files[0].old_source,
        SourceSpec::GitBlob { .. }
    ));
    assert!(matches!(
        loaded.changeset.files[0].new_source,
        SourceSpec::GitBlob { .. }
    ));
}

#[test]
fn show_defaults_to_head_and_accepts_an_explicit_ref() {
    let repo = GitFixture::new();
    repo.commit_file("file.txt", "one\n");
    repo.write("file.txt", "two\n");
    repo.commit_all("second");
    let head = repo.load(ReviewInput::Show {
        reference: None,
        pathspecs: vec![],
        options: CommonOptions::default(),
    });
    assert!(head.changeset.files[0].patch.contains("+two"));
    let parent = repo.load(ReviewInput::Show {
        reference: Some("HEAD^".into()),
        pathspecs: vec![],
        options: CommonOptions::default(),
    });
    assert!(parent.changeset.files[0].patch.contains("+one"));
}

#[test]
fn stash_show_defaults_to_latest_stash_and_accepts_a_ref() {
    let repo = GitFixture::new();
    repo.commit_file("file.txt", "base\n");
    repo.write("file.txt", "first\n");
    repo.git(["stash", "push", "-m", "first"]);
    repo.write("file.txt", "second\n");
    repo.git(["stash", "push", "-m", "second"]);
    let latest = repo.load(ReviewInput::StashShow {
        reference: None,
        options: CommonOptions::default(),
    });
    assert!(latest.changeset.files[0].patch.contains("+second"));
    let first = repo.load(ReviewInput::StashShow {
        reference: Some("stash@{1}".into()),
        options: CommonOptions::default(),
    });
    assert!(first.changeset.files[0].patch.contains("+first"));
}

#[test]
fn exclude_untracked_removes_only_synthetic_files() {
    let repo = GitFixture::new();
    repo.commit_file("tracked.txt", "base\n");
    repo.write("tracked.txt", "changed\n");
    repo.write("unknown.txt", "unknown\n");
    let loaded = repo.load(ReviewInput::VcsDiff {
        range: None,
        staged: false,
        pathspecs: vec![],
        options: CommonOptions {
            exclude_untracked: Some(true),
            ..Default::default()
        },
    });
    assert_eq!(
        loaded
            .changeset
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["tracked.txt"]
    );
}

#[test]
fn invalid_repo_revision_and_empty_stash_are_actionable() {
    let outside = GitFixture::non_repository();
    assert!(
        outside
            .load_error(working_tree_input())
            .to_string()
            .contains("inside a Git repository")
    );

    let repo = GitFixture::new();
    repo.commit_file("file.txt", "base\n");
    let invalid_ref = repo
        .load_error(ReviewInput::Show {
            reference: Some("missing".into()),
            pathspecs: vec![],
            options: CommonOptions::default(),
        })
        .to_string();
    assert!(invalid_ref.contains("missing"));
    assert!(invalid_ref.contains("Check the ref"));

    let missing_stash = repo
        .load_error(ReviewInput::StashShow {
            reference: None,
            options: CommonOptions::default(),
        })
        .to_string();
    assert!(missing_stash.contains("git stash push"));
}

#[test]
fn large_tracked_and_untracked_files_are_bounded_placeholders_with_stats() {
    let repo = GitFixture::new();
    repo.commit_file("tracked.txt", "base\n");
    repo.write("tracked.txt", &"changed\n".repeat(20_001));
    repo.write("untracked.txt", &"new\n".repeat(300_001));
    let loaded = repo.load(working_tree_input());
    let tracked = loaded
        .changeset
        .files
        .iter()
        .find(|file| file.path == "tracked.txt")
        .unwrap();
    let untracked = loaded
        .changeset
        .files
        .iter()
        .find(|file| file.path == "untracked.txt")
        .unwrap();
    assert!(tracked.is_too_large);
    assert!(tracked.hunks.is_empty());
    assert_eq!(tracked.stats.additions, 20_001);
    assert_eq!(tracked.stats.deletions, 1);
    assert!(untracked.is_too_large);
    assert!(untracked.hunks.is_empty());
    assert!(untracked.stats_truncated);
}

#[test]
fn source_specs_match_worktree_staged_show_and_rename_endpoints() {
    let repo = GitFixture::new();
    repo.commit_file("old.txt", "base\n");
    repo.write("old.txt", "worktree\n");
    let worktree = repo.load(working_tree_input());
    assert!(matches!(
        worktree.changeset.files[0].old_source,
        SourceSpec::GitIndex { .. }
    ));
    assert!(matches!(
        worktree.changeset.files[0].new_source,
        SourceSpec::File(_)
    ));

    repo.git(["add", "old.txt"]);
    let staged = repo.load(ReviewInput::VcsDiff {
        range: None,
        staged: true,
        pathspecs: vec![],
        options: CommonOptions::default(),
    });
    assert!(matches!(
        staged.changeset.files[0].old_source,
        SourceSpec::GitBlob { .. }
    ));
    assert!(matches!(
        staged.changeset.files[0].new_source,
        SourceSpec::GitIndex { .. }
    ));

    repo.commit_all("update");
    repo.git(["mv", "old.txt", "new.txt"]);
    let renamed = repo.load(ReviewInput::VcsDiff {
        range: None,
        staged: true,
        pathspecs: vec![],
        options: CommonOptions::default(),
    });
    assert_eq!(
        renamed.changeset.files[0].previous_path.as_deref(),
        Some("old.txt")
    );
    assert!(matches!(
        &renamed.changeset.files[0].old_source,
        SourceSpec::GitBlob { path, .. } if path == "old.txt"
    ));
    assert!(matches!(
        &renamed.changeset.files[0].new_source,
        SourceSpec::GitIndex { path, .. } if path == "new.txt"
    ));

    let shown = repo.load(ReviewInput::Show {
        reference: Some("HEAD".into()),
        pathspecs: vec![],
        options: CommonOptions::default(),
    });
    assert!(matches!(
        shown.changeset.files[0].old_source,
        SourceSpec::GitBlob { .. }
    ));
    assert!(matches!(
        shown.changeset.files[0].new_source,
        SourceSpec::GitBlob { .. }
    ));
}

#[test]
fn source_reader_bounds_text_and_returns_none_for_absent_sides() {
    let repo = GitFixture::new();
    repo.commit_file("file.txt", "12345\n");
    let runner = SystemCommandRunner;
    let mut reader = SourceReader::new(&runner, "git", 4);
    assert_eq!(reader.read(&SourceSpec::None).unwrap(), None);
    let spec = SourceSpec::GitBlob {
        repo_root: repo.path().into(),
        reference: "HEAD".into(),
        path: "file.txt".into(),
    };
    assert!(matches!(
        reader.read(&spec),
        Err(SourceError::TooLarge { limit: 4 })
    ));
    let mut reader = SourceReader::new(&runner, "git", 1024);
    let missing_parent = SourceSpec::GitBlob {
        repo_root: repo.path().into(),
        reference: "HEAD^".into(),
        path: "file.txt".into(),
    };
    assert_eq!(reader.read(&missing_parent).unwrap(), None);
}

#[test]
fn source_reader_reads_and_caches_the_git_index_side() {
    let repo = GitFixture::new();
    repo.commit_file("file.txt", "base\n");
    repo.write("file.txt", "staged\n");
    repo.git(["add", "file.txt"]);
    let runner = SystemCommandRunner;
    let mut reader = SourceReader::new(&runner, "git", 1024);
    let spec = SourceSpec::GitIndex {
        repo_root: repo.path().into(),
        path: "file.txt".into(),
    };
    assert_eq!(reader.read(&spec).unwrap().as_deref(), Some("staged\n"));
    repo.write("file.txt", "new index\n");
    repo.git(["add", "file.txt"]);
    assert_eq!(reader.read(&spec).unwrap().as_deref(), Some("staged\n"));
}
