use httpmock::prelude::*;
use ramo_core::github::PullRequestKey;
use ramo_core::remote_review::{
    InlineCommentTarget, RemoteLineSide, RemoteReviewComment, RemoteReviewRequest, ReviewVerdict,
};
use ramo_github::{GithubClient, GithubErrorKind};

fn client(server: &MockServer) -> GithubClient {
    GithubClient::with_endpoints(
        "token".into(),
        server.base_url(),
        format!("{}/graphql", server.base_url()),
    )
    .unwrap()
}

fn key() -> PullRequestKey {
    PullRequestKey {
        repository: "owner/repo".into(),
        number: 42,
    }
}

fn request() -> RemoteReviewRequest {
    RemoteReviewRequest {
        commit_id: "expected-sha".into(),
        body: "Overall review".into(),
        verdict: ReviewVerdict::RequestChanges,
        comments: vec![RemoteReviewComment {
            target: InlineCommentTarget {
                path: "src/lib.rs".into(),
                side: RemoteLineSide::Right,
                start_line: 4,
                end_line: 6,
            },
            body: "Please simplify".into(),
        }],
    }
}

#[test]
fn stale_head_aborts_before_the_review_post() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/pulls/42");
        then.status(200)
            .json_body_obj(&serde_json::json!({"head":{"sha":"new-sha"}}));
    });
    let post = server.mock(|when, then| {
        when.method(POST).path("/repos/owner/repo/pulls/42/reviews");
        then.status(200);
    });

    let error = client(&server)
        .submit_review(&key(), "expected-sha", &request())
        .unwrap_err();

    assert_eq!(
        error.kind(),
        &GithubErrorKind::StaleRevision {
            expected: "expected-sha".into(),
            actual: "new-sha".into()
        }
    );
    post.assert_calls(0);
}

#[test]
fn unchanged_head_posts_one_atomic_review_with_exact_range_payload() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/pulls/42");
        then.status(200)
            .json_body_obj(&serde_json::json!({"head":{"sha":"expected-sha"}}));
    });
    let post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/pulls/42/reviews")
            .json_body_obj(&serde_json::json!({
                "commit_id":"expected-sha",
                "body":"Overall review",
                "event":"REQUEST_CHANGES",
                "comments":[{
                    "path":"src/lib.rs",
                    "body":"Please simplify",
                    "line":6,
                    "side":"RIGHT",
                    "start_line":4,
                    "start_side":"RIGHT"
                }]
            }));
        then.status(200).json_body_obj(&serde_json::json!({"id":7}));
    });

    client(&server)
        .submit_review(&key(), "expected-sha", &request())
        .unwrap();

    post.assert_calls(1);
}
