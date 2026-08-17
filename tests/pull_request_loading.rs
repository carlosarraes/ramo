use ramo::config::ResolvedConfig;
use ramo::core::input::{CommonOptions, ReviewInput};
use ramo::diff::model::SourceSpec;
use ramo::github::{GithubError, GithubPullRequestSource};
use ramo::input::{LoadContext, LoadError, ReloadPlan, ReviewLoader};
use ramo::remote_review::{
    GithubReviewThread, GithubThreadComment, GithubThreadSubject, PullRequestReviewContext,
};
use ramo::vcs::SystemCommandRunner;

struct FakeSource {
    context: PullRequestReviewContext,
    diff: String,
    threads: Vec<GithubReviewThread>,
    thread_calls: usize,
}

impl GithubPullRequestSource for FakeSource {
    fn resolve_pr(&mut self, _number: u64) -> Result<PullRequestReviewContext, GithubError> {
        Ok(self.context.clone())
    }

    fn load_diff(&mut self, _number: u64) -> Result<String, GithubError> {
        Ok(self.diff.clone())
    }

    fn load_review_threads(
        &mut self,
        _context: &PullRequestReviewContext,
    ) -> Result<Vec<GithubReviewThread>, GithubError> {
        self.thread_calls += 1;
        Ok(std::mem::take(&mut self.threads))
    }
}

fn context() -> PullRequestReviewContext {
    PullRequestReviewContext {
        repository: "owner/repo".into(),
        repository_url: "https://github.com/owner/repo".into(),
        number: 123,
        title: "Improve review flow".into(),
        body: String::new(),
        url: "https://github.com/owner/repo/pull/123".into(),
        base_ref: "main".into(),
        base_revision: "base123".into(),
        head_ref: "feature".into(),
        captured_revision: "head123".into(),
        author_login: "author".into(),
        viewer_login: "reviewer".into(),
    }
}

fn input(with_comments: bool) -> ReviewInput {
    ReviewInput::PullRequest {
        number: 123,
        with_comments,
        options: CommonOptions {
            watch: Some(false),
            ..CommonOptions::default()
        },
    }
}

fn load(
    source: &mut dyn GithubPullRequestSource,
    with_comments: bool,
) -> Result<ramo::input::LoadedPullRequest, LoadError> {
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    ReviewLoader.load_pull_request(
        &input(with_comments),
        &mut std::io::empty(),
        &LoadContext {
            cwd: std::path::Path::new("."),
            config: &config,
            runner: &runner,
        },
        source,
    )
}

fn thread() -> GithubReviewThread {
    GithubReviewThread {
        id: "T1".into(),
        path: "src/lib.rs".into(),
        is_resolved: false,
        is_outdated: false,
        subject: GithubThreadSubject::File,
        comments: vec![GithubThreadComment {
            id: "C1".into(),
            author: "alice".into(),
            body: "Please simplify this".into(),
            created_at: "2026-07-27T12:00:00Z".into(),
            url: "https://github.com/owner/repo/pull/123#discussion_r1".into(),
        }],
        url: "https://github.com/owner/repo/pull/123#discussion_r1".into(),
    }
}

#[test]
fn valid_metadata_and_diff_become_a_frozen_review() {
    let mut source = FakeSource {
        context: context(),
        diff: concat!(
            "diff --git a/src/lib.rs b/src/lib.rs\n",
            "--- a/src/lib.rs\n",
            "+++ b/src/lib.rs\n",
            "@@ -1 +1 @@\n",
            "-old\n",
            "+new\n",
        )
        .into(),
        threads: vec![thread()],
        thread_calls: 0,
    };
    let loaded = load(&mut source, false).unwrap();
    assert_eq!(loaded.context, context());
    assert_eq!(loaded.review.changeset.source_label, "GitHub PR #123");
    assert_eq!(loaded.review.changeset.title, "Improve review flow");
    assert_eq!(loaded.review.changeset.files.len(), 1);
    assert_eq!(loaded.review.changeset.files[0].path, "src/lib.rs");
    assert_eq!(loaded.review.reload_plan, ReloadPlan::None);
    assert!(loaded.imported_threads.is_empty());
    assert_eq!(source.thread_calls, 0);

    let loaded = load(&mut source, true).unwrap();
    assert_eq!(loaded.imported_threads, vec![thread()]);
    assert_eq!(source.thread_calls, 1);
}

#[test]
fn empty_and_unparseable_pr_diffs_fail_before_terminal_entry() {
    for (diff, expected) in [
        ("", "pull request #123 has no diff"),
        (
            "ordinary prose",
            "pull request #123 did not return a parseable diff",
        ),
    ] {
        let mut source = FakeSource {
            context: context(),
            diff: diff.into(),
            threads: vec![thread()],
            thread_calls: 0,
        };
        assert!(
            load(&mut source, true)
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
        assert_eq!(source.thread_calls, 0);
    }
}

fn remote(repository: &str, revision: &str, path: &str) -> SourceSpec {
    SourceSpec::RemoteBlob {
        repository: repository.into(),
        revision: revision.into(),
        path: path.into(),
    }
}

#[test]
fn valid_pr_files_receive_immutable_remote_sources() {
    let mut source = FakeSource {
        context: context(),
        diff: concat!(
            "diff --git a/src/modified.rs b/src/modified.rs\n",
            "--- a/src/modified.rs\n",
            "+++ b/src/modified.rs\n",
            "@@ -2 +2 @@\n",
            "-old\n",
            "+new\n",
            "diff --git a/src/added.rs b/src/added.rs\n",
            "new file mode 100644\n",
            "--- /dev/null\n",
            "+++ b/src/added.rs\n",
            "@@ -0,0 +1 @@\n",
            "+new\n",
            "diff --git a/src/deleted.rs b/src/deleted.rs\n",
            "deleted file mode 100644\n",
            "--- a/src/deleted.rs\n",
            "+++ /dev/null\n",
            "@@ -1 +0,0 @@\n",
            "-old\n",
            "diff --git a/src/old.rs b/src/new.rs\n",
            "similarity index 80%\n",
            "rename from src/old.rs\n",
            "rename to src/new.rs\n",
            "--- a/src/old.rs\n",
            "+++ b/src/new.rs\n",
            "@@ -1 +1 @@\n",
            "-old\n",
            "+new\n",
            "diff --git a/src/copied.rs b/src/copy.rs\n",
            "similarity index 80%\n",
            "copy from src/copied.rs\n",
            "copy to src/copy.rs\n",
            "--- a/src/copied.rs\n",
            "+++ b/src/copy.rs\n",
            "@@ -1 +1 @@\n",
            "-old\n",
            "+new\n",
            "diff --git a/assets/logo.png b/assets/logo.png\n",
            "Binary files a/assets/logo.png and b/assets/logo.png differ\n",
        )
        .into(),
        threads: Vec::new(),
        thread_calls: 0,
    };

    let loaded = load(&mut source, false).unwrap();
    let files = &loaded.review.changeset.files;
    let file = |path: &str| files.iter().find(|file| file.path == path).unwrap();

    let modified = file("src/modified.rs");
    assert_eq!(
        modified.old_source,
        remote("owner/repo", "base123", "src/modified.rs")
    );
    assert_eq!(
        modified.new_source,
        remote("owner/repo", "head123", "src/modified.rs")
    );

    let added = file("src/added.rs");
    assert_eq!(added.old_source, SourceSpec::None);
    assert_eq!(
        added.new_source,
        remote("owner/repo", "head123", "src/added.rs")
    );

    let deleted = file("src/deleted.rs");
    assert_eq!(
        deleted.old_source,
        remote("owner/repo", "base123", "src/deleted.rs")
    );
    assert_eq!(deleted.new_source, SourceSpec::None);

    let renamed = file("src/new.rs");
    assert_eq!(
        renamed.old_source,
        remote("owner/repo", "base123", "src/old.rs")
    );
    assert_eq!(
        renamed.new_source,
        remote("owner/repo", "head123", "src/new.rs")
    );

    let copied = file("src/copy.rs");
    assert_eq!(
        copied.old_source,
        remote("owner/repo", "base123", "src/copied.rs")
    );
    assert_eq!(
        copied.new_source,
        remote("owner/repo", "head123", "src/copy.rs")
    );

    let binary = file("assets/logo.png");
    assert_eq!(binary.old_source, SourceSpec::None);
    assert_eq!(binary.new_source, SourceSpec::None);
}
