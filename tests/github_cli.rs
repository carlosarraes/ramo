use std::collections::VecDeque;
use std::io;

use ramo::github::{GithubCli, GithubPullRequestSource};
use ramo::process::command::{CommandExecutor, CommandRequest, CommandResult};
use ramo::remote_review::{
    GithubThreadSubject, InlineCommentTarget, PullRequestReviewContext, RemoteLineSide,
    RemoteReviewComment, RemoteReviewPublisher, RemoteReviewRequest, ReviewVerdict,
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
        body: "Summary\n\n- faster\n- clearer".into(),
        url: "https://github.com/owner/repo/pull/123".into(),
        base_ref: "main".into(),
        base_revision: "base123".into(),
        head_ref: "feature".into(),
        captured_revision: "head123".into(),
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
                r#"{"number":123,"title":"Improve review flow","body":"Summary\n\n- faster\n- clearer","url":"https://github.com/owner/repo/pull/123","author":{"login":"author"},"baseRefName":"main","baseRefOid":"base123","headRefName":"feature","headRefOid":"head123"}"#,
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
            "number,title,body,url,author,baseRefName,baseRefOid,headRefName,headRefOid"
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

    let missing_base = FakeExecutor {
        results: VecDeque::from([
            result(0, "reviewer", ""),
            result(
                0,
                r#"{"nameWithOwner":"owner/repo","url":"https://github.com/owner/repo"}"#,
                "",
            ),
            result(
                0,
                r#"{"number":123,"title":"Improve review flow","url":"https://github.com/owner/repo/pull/123","author":{"login":"author"},"baseRefName":"main","baseRefOid":"","headRefName":"feature","headRefOid":"head123"}"#,
                "",
            ),
        ]),
        ..FakeExecutor::default()
    };
    let error = GithubCli::new(missing_base).resolve_pr(123).unwrap_err();
    assert!(error.to_string().contains("base revision"));
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

#[test]
fn review_threads_use_graphql_and_keep_only_active_conversations() {
    let page = r#"{
      "data":{"repository":{"pullRequest":{"reviewThreads":{
        "nodes":[
          {"id":"T1","isResolved":false,"isOutdated":false,
           "subjectType":"LINE","path":"src/lib.rs","diffSide":"RIGHT",
           "startDiffSide":"RIGHT","startLine":41,"line":42,
           "comments":{"nodes":[
             {"id":"C1","bodyText":"root\u001b[31m","createdAt":"2026-07-26T14:32:00Z",
              "url":"https://github.com/o/r/pull/123#discussion_r1","author":{"login":"alice"}},
             {"id":"C2","bodyText":"reply","createdAt":"2026-07-26T15:10:00Z",
              "url":"https://github.com/o/r/pull/123#discussion_r2","author":{"login":"bob"}}
           ],"pageInfo":{"hasNextPage":false,"endCursor":null}}},
          {"id":"T2","isResolved":true,"isOutdated":false,"subjectType":"FILE",
           "path":"src/lib.rs","diffSide":"RIGHT","startDiffSide":null,
           "startLine":null,"line":null,"comments":{"nodes":[],
           "pageInfo":{"hasNextPage":false,"endCursor":null}}},
          {"id":"T3","isResolved":false,"isOutdated":true,"subjectType":"FILE",
           "path":"src/lib.rs","diffSide":"RIGHT","startDiffSide":null,
           "startLine":null,"line":null,"comments":{"nodes":[],
           "pageInfo":{"hasNextPage":false,"endCursor":null}}}
        ],"pageInfo":{"hasNextPage":false,"endCursor":null}
      }}}}
    }"#;
    let mut github = GithubCli::new(FakeExecutor {
        results: VecDeque::from([result(0, page, "")]),
        ..FakeExecutor::default()
    });

    let threads = github.load_review_threads(&context()).unwrap();

    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].comments.len(), 2);
    assert_eq!(threads[0].comments[0].body, "root");
    assert_eq!(
        threads[0].subject,
        GithubThreadSubject::Line {
            side: Some(RemoteLineSide::Right),
            start_side: Some(RemoteLineSide::Right),
            start_line: Some(41),
            end_line: Some(42),
        }
    );
    let executor = github.into_executor();
    let arguments = argv(&executor.requests[0]);
    assert_eq!(&arguments[..3], ["gh", "api", "graphql"]);
    assert!(
        arguments
            .iter()
            .any(|argument| argument.contains("reviewThreads"))
    );
    assert!(
        !arguments
            .iter()
            .any(|argument| argument.contains("position"))
    );
    assert!(arguments.iter().any(|argument| argument == "number=123"));
    assert!(arguments.iter().any(|argument| argument == "first=100"));
    assert!(arguments.iter().any(|argument| argument == "after=null"));
}

#[test]
fn review_threads_paginate_and_preserve_deleted_authors() {
    let first = r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{
      "nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":"cursor-1"}
    }}}}}"#;
    let second = r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{
      "nodes":[{"id":"T1","isResolved":false,"isOutdated":false,
        "subjectType":"FILE","path":"README.md","diffSide":null,
        "startDiffSide":null,"startLine":null,"line":null,
        "comments":{"nodes":[{"id":"C1","bodyText":"note","createdAt":"now",
          "url":"https://example.test/thread","author":null}],
          "pageInfo":{"hasNextPage":false,"endCursor":null}}}],
      "pageInfo":{"hasNextPage":false,"endCursor":null}
    }}}}}"#;
    let mut github = GithubCli::new(FakeExecutor {
        results: VecDeque::from([result(0, first, ""), result(0, second, "")]),
        ..FakeExecutor::default()
    });

    let threads = github.load_review_threads(&context()).unwrap();

    assert_eq!(threads[0].subject, GithubThreadSubject::File);
    assert_eq!(threads[0].comments[0].author, "[deleted]");
    let executor = github.into_executor();
    assert_eq!(executor.requests.len(), 2);
    assert!(
        argv(&executor.requests[1])
            .iter()
            .any(|argument| argument == "after=cursor-1")
    );
}

#[test]
fn review_thread_limits_and_malformed_pages_are_actionable() {
    let too_large_body = "x".repeat(64 * 1024 + 1);
    let oversized = serde_json::json!({
        "data": {"repository": {"pullRequest": {"reviewThreads": {
            "nodes": [{
                "id": "T1", "isResolved": false, "isOutdated": false,
                "subjectType": "LINE", "path": "src/lib.rs", "diffSide": "RIGHT",
                "startDiffSide": null, "startLine": null, "line": 1,
                "comments": {"nodes": [{
                    "id": "C1", "bodyText": too_large_body, "createdAt": "now",
                    "url": "https://example.test/thread", "author": {"login": "alice"}
                }], "pageInfo": {"hasNextPage": false, "endCursor": null}}
            }], "pageInfo": {"hasNextPage": false, "endCursor": null}
        }}}}
    })
    .to_string();
    let error = GithubCli::new(FakeExecutor {
        results: VecDeque::from([result(0, &oversized, "")]),
        ..FakeExecutor::default()
    })
    .load_review_threads(&context())
    .unwrap_err();
    assert!(error.to_string().contains("64 KiB"));

    let nested_page = r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{
      "nodes":[{"id":"T1","isResolved":false,"isOutdated":false,
        "subjectType":"LINE","path":"src/lib.rs","diffSide":"RIGHT",
        "startDiffSide":null,"startLine":null,"line":1,
        "comments":{"nodes":[{"id":"C1","bodyText":"note","createdAt":"now",
          "url":"https://example.test/thread","author":{"login":"alice"}}],
          "pageInfo":{"hasNextPage":true,"endCursor":"more"}}}],
      "pageInfo":{"hasNextPage":false,"endCursor":null}
    }}}}}"#;
    let error = GithubCli::new(FakeExecutor {
        results: VecDeque::from([result(0, nested_page, "")]),
        ..FakeExecutor::default()
    })
    .load_review_threads(&context())
    .unwrap_err();
    assert!(error.to_string().contains("more than 100 comments"));

    let error = GithubCli::new(FakeExecutor {
        results: VecDeque::from([result(0, "not json", "")]),
        ..FakeExecutor::default()
    })
    .load_review_threads(&context())
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("load GitHub pull request review threads returned invalid JSON")
    );

    let nodes = (0..501)
        .map(|index| {
            serde_json::json!({
                "id": format!("T{index}"), "isResolved": false, "isOutdated": false,
                "subjectType": "LINE", "path": "src/lib.rs", "diffSide": "RIGHT",
                "startDiffSide": null, "startLine": null, "line": 1,
                "comments": {"nodes": [{
                    "id": format!("C{index}"), "bodyText": "note", "createdAt": "now",
                    "url": "https://example.test/thread", "author": {"login": "alice"}
                }], "pageInfo": {"hasNextPage": false, "endCursor": null}}
            })
        })
        .collect::<Vec<_>>();
    let too_many = serde_json::json!({
        "data": {"repository": {"pullRequest": {"reviewThreads": {
            "nodes": nodes, "pageInfo": {"hasNextPage": false, "endCursor": null}
        }}}}
    })
    .to_string();
    let error = GithubCli::new(FakeExecutor {
        results: VecDeque::from([result(0, &too_many, "")]),
        ..FakeExecutor::default()
    })
    .load_review_threads(&context())
    .unwrap_err();
    assert!(error.to_string().contains("more than 500 eligible"));
}
