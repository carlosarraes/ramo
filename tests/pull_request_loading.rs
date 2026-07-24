use ramo::config::ResolvedConfig;
use ramo::core::input::{CommonOptions, ReviewInput};
use ramo::github::{GithubError, GithubPullRequestSource};
use ramo::input::{LoadContext, LoadError, ReloadPlan, ReviewLoader};
use ramo::remote_review::PullRequestReviewContext;
use ramo::vcs::SystemCommandRunner;

struct FakeSource {
    context: PullRequestReviewContext,
    diff: String,
}

impl GithubPullRequestSource for FakeSource {
    fn resolve_pr(&mut self, _number: u64) -> Result<PullRequestReviewContext, GithubError> {
        Ok(self.context.clone())
    }

    fn load_diff(&mut self, _number: u64) -> Result<String, GithubError> {
        Ok(self.diff.clone())
    }
}

fn context() -> PullRequestReviewContext {
    PullRequestReviewContext {
        repository: "owner/repo".into(),
        repository_url: "https://github.com/owner/repo".into(),
        number: 123,
        title: "Improve review flow".into(),
        url: "https://github.com/owner/repo/pull/123".into(),
        base_ref: "main".into(),
        head_ref: "feature".into(),
        captured_revision: "abc123".into(),
        author_login: "author".into(),
        viewer_login: "reviewer".into(),
    }
}

fn input() -> ReviewInput {
    ReviewInput::PullRequest {
        number: 123,
        options: CommonOptions {
            watch: Some(false),
            ..CommonOptions::default()
        },
    }
}

fn load(
    source: &mut dyn GithubPullRequestSource,
) -> Result<ramo::input::LoadedPullRequest, LoadError> {
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    ReviewLoader.load_pull_request(
        &input(),
        &mut std::io::empty(),
        &LoadContext {
            cwd: std::path::Path::new("."),
            config: &config,
            runner: &runner,
        },
        source,
    )
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
    };
    let loaded = load(&mut source).unwrap();
    assert_eq!(loaded.context, context());
    assert_eq!(loaded.review.changeset.source_label, "GitHub PR #123");
    assert_eq!(loaded.review.changeset.title, "Improve review flow");
    assert_eq!(loaded.review.changeset.files.len(), 1);
    assert_eq!(loaded.review.changeset.files[0].path, "src/lib.rs");
    assert_eq!(loaded.review.reload_plan, ReloadPlan::None);
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
        };
        assert!(
            load(&mut source)
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
}
