use std::collections::VecDeque;
use std::io;

use ramo::github::{GithubCli, GithubPullRequestSource};
use ramo::process::command::{CommandExecutor, CommandRequest, CommandResult};
use ramo::remote_review::{
    InlineCommentTarget, PullRequestReviewContext, RemoteLineSide, RemoteReviewComment,
    RemoteReviewPublisher, RemoteReviewRequest, ReviewVerdict,
};

#[derive(Default)]
struct FakeExecutor {
    requests: Vec<CommandRequest>,
    results: VecDeque<io::Result<CommandResult>>,
}

impl CommandExecutor for FakeExecutor {
    fn execute(&mut self, request: CommandRequest) -> io::Result<CommandResult> {
        self.requests.push(request);
        self.results.pop_front().expect("scripted result")
    }
}

fn result(code: i32, stdout: &str, stderr: &str) -> io::Result<CommandResult> {
    Ok(CommandResult {
        code: Some(code),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: false,
    })
}

fn argv(request: &CommandRequest) -> Vec<String> {
    request
        .argv
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect()
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

#[test]
fn resolve_and_diff_use_exact_literal_argv() {
    let executor = FakeExecutor {
        results: VecDeque::from([
            result(0, "reviewer\n", ""),
            result(
                0,
                r#"{"nameWithOwner":"owner/repo","url":"https://github.com/owner/repo"}"#,
                "",
            ),
            result(
                0,
                r#"{"number":123,"title":"Improve review flow","url":"https://github.com/owner/repo/pull/123","author":{"login":"author"},"baseRefName":"main","headRefName":"feature","headRefOid":"abc123"}"#,
                "",
            ),
            result(0, "diff --git a/a b/a\n", ""),
        ]),
        ..FakeExecutor::default()
    };
    let mut github = GithubCli::new(executor);

    assert_eq!(github.resolve_pr(123).unwrap(), context());
    assert_eq!(github.load_diff(123).unwrap(), "diff --git a/a b/a\n");
    let executor = github.into_executor();
    assert_eq!(
        argv(&executor.requests[0]),
        ["gh", "api", "user", "--jq", ".login"]
    );
    assert_eq!(
        argv(&executor.requests[1]),
        ["gh", "repo", "view", "--json", "nameWithOwner,url"]
    );
    assert_eq!(
        argv(&executor.requests[2]),
        [
            "gh",
            "pr",
            "view",
            "123",
            "--json",
            "number,title,url,author,baseRefName,headRefName,headRefOid"
        ]
    );
    assert_eq!(
        argv(&executor.requests[3]),
        ["gh", "pr", "diff", "123", "--color=never"]
    );
    assert!(
        executor
            .requests
            .iter()
            .all(|request| request.limits.is_some())
    );
}

#[test]
fn submission_sends_one_exact_json_document_through_stdin() {
    let executor = FakeExecutor {
        results: VecDeque::from([result(0, "abc123\n", ""), result(0, "", "")]),
        ..FakeExecutor::default()
    };
    let mut github = GithubCli::new(executor);
    assert_eq!(github.current_revision(&context()).unwrap(), "abc123");
    github
        .submit_review(
            &context(),
            &RemoteReviewRequest {
                commit_id: "abc123".into(),
                body: "Overall".into(),
                verdict: ReviewVerdict::Approve,
                comments: vec![
                    RemoteReviewComment {
                        target: InlineCommentTarget {
                            path: "src/lib.rs".into(),
                            side: RemoteLineSide::Right,
                            start_line: 42,
                            end_line: 42,
                        },
                        body: "Single".into(),
                    },
                    RemoteReviewComment {
                        target: InlineCommentTarget {
                            path: "src/old.rs".into(),
                            side: RemoteLineSide::Left,
                            start_line: 7,
                            end_line: 9,
                        },
                        body: "Range".into(),
                    },
                ],
            },
        )
        .unwrap();

    let executor = github.into_executor();
    assert_eq!(
        argv(&executor.requests[0]),
        [
            "gh",
            "pr",
            "view",
            "123",
            "--json",
            "headRefOid",
            "--jq",
            ".headRefOid"
        ]
    );
    assert_eq!(
        argv(&executor.requests[1]),
        [
            "gh",
            "api",
            "--method",
            "POST",
            "repos/owner/repo/pulls/123/reviews",
            "--input",
            "-"
        ]
    );
    let payload: serde_json::Value =
        serde_json::from_slice(executor.requests[1].stdin.as_deref().unwrap()).unwrap();
    assert_eq!(payload["commit_id"], "abc123");
    assert_eq!(payload["event"], "APPROVE");
    assert_eq!(payload["comments"][0]["line"], 42);
    assert_eq!(payload["comments"][0]["side"], "RIGHT");
    assert!(payload["comments"][0].get("start_line").is_none());
    assert_eq!(payload["comments"][1]["start_line"], 7);
    assert_eq!(payload["comments"][1]["start_side"], "LEFT");
    assert!(
        !argv(&executor.requests[1])
            .iter()
            .any(|argument| argument.contains("Overall"))
    );
}

#[test]
fn missing_auth_and_malformed_metadata_are_actionable() {
    let missing = FakeExecutor {
        results: VecDeque::from([Err(io::Error::new(io::ErrorKind::NotFound, "missing"))]),
        ..FakeExecutor::default()
    };
    let error = GithubCli::new(missing).resolve_pr(123).unwrap_err();
    assert!(error.to_string().contains("install GitHub CLI"));

    let auth = FakeExecutor {
        results: VecDeque::from([result(1, "", "\u{1b}[31mnot logged in\u{1b}[0m")]),
        ..FakeExecutor::default()
    };
    let error = GithubCli::new(auth).resolve_pr(123).unwrap_err();
    assert!(error.to_string().contains("gh auth login"));
    assert!(!error.to_string().contains('\u{1b}'));

    let malformed = FakeExecutor {
        results: VecDeque::from([
            result(0, "reviewer", ""),
            result(0, r#"{"nameWithOwner":""}"#, ""),
        ]),
        ..FakeExecutor::default()
    };
    let error = GithubCli::new(malformed).resolve_pr(123).unwrap_err();
    assert!(error.to_string().contains("repository"));
}

#[test]
fn timeout_and_truncation_are_distinct() {
    let timed_out = CommandResult {
        code: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: true,
    };
    let error = GithubCli::new(FakeExecutor {
        results: VecDeque::from([Ok(timed_out)]),
        ..FakeExecutor::default()
    })
    .resolve_pr(123)
    .unwrap_err();
    assert!(error.to_string().contains("timed out"));

    let truncated = CommandResult {
        code: Some(0),
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdout_truncated: true,
        stderr_truncated: false,
        timed_out: false,
    };
    let error = GithubCli::new(FakeExecutor {
        results: VecDeque::from([result(0, "reviewer", ""), Ok(truncated)]),
        ..FakeExecutor::default()
    })
    .resolve_pr(123)
    .unwrap_err();
    assert!(error.to_string().contains("too much output"));
}
