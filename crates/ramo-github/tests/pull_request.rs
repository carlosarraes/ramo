use httpmock::prelude::*;
use ramo_core::github::PullRequestKey;
use ramo_core::remote_review::{GithubThreadSubject, RemoteLineSide};
use ramo_github::GithubClient;

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

#[test]
fn snapshot_freezes_metadata_and_preserves_paginated_file_order() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/user");
        then.status(200)
            .json_body_obj(&serde_json::json!({"login":"reviewer","id":7}));
    });
    server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/pulls/42");
        then.status(200).json_body_obj(&serde_json::json!({
            "node_id":"PR_node",
            "title":"Native Android review",
            "html_url":"https://github.com/owner/repo/pull/42",
            "user":{"login":"author"},
            "base":{"ref":"main","sha":"base-sha"},
            "head":{"ref":"feature","sha":"head-sha"}
        }));
    });
    let next = format!(
        "<{}/repos/owner/repo/pulls/42/files?per_page=100&page=2>; rel=\"next\"",
        server.base_url()
    );
    server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/pulls/42/files")
            .query_param("per_page", "100")
            .query_param("page", "1");
        then.status(200).header("link", &next).json_body_obj(&serde_json::json!([
            {"filename":"src/old.rs","previous_filename":"src/older.rs","status":"renamed","additions":3,"deletions":1,"changes":4,"patch":"@@ -1 +1 @@","viewer_viewed_state":"VIEWED"}
        ]));
    });
    server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/pulls/42/files")
            .query_param("per_page", "100")
            .query_param("page", "2");
        then.status(200).json_body_obj(&serde_json::json!([
            {"filename":"assets/logo.png","status":"modified","additions":0,"deletions":0,"changes":12,"patch":null,"viewer_viewed_state":"UNVIEWED"}
        ]));
    });

    let snapshot = client(&server).load_snapshot(&key()).unwrap();

    assert_eq!(snapshot.node_id, "PR_node");
    assert_eq!(snapshot.context.base_revision, "base-sha");
    assert_eq!(snapshot.context.captured_revision, "head-sha");
    assert_eq!(snapshot.context.author_login, "author");
    assert_eq!(snapshot.context.viewer_login, "reviewer");
    assert_eq!(
        snapshot
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["src/old.rs", "assets/logo.png"]
    );
    assert_eq!(
        snapshot.files[0].previous_path.as_deref(),
        Some("src/older.rs")
    );
    assert!(snapshot.files[0].viewed);
    assert!(snapshot.files[1].binary);
}

#[test]
fn diff_source_and_threads_use_exact_media_types_and_encoded_paths() {
    let server = MockServer::start();
    let diff = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/pulls/42")
            .header("accept", "application/vnd.github.diff");
        then.status(200).body("diff --git a/a b/a\n");
    });
    let source = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/contents/src/space%20%23/%C3%BC.rs")
            .query_param("ref", "head sha")
            .header("accept", "application/vnd.github.raw+json");
        then.status(200).body("fn main() {}\n");
    });
    server.mock(|when, then| {
        when.method(POST).path("/graphql").body_includes("\"number\":42");
        then.status(200).json_body_obj(&serde_json::json!({
            "data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[{
                "id":"thread-1","isResolved":true,"isOutdated":false,
                "subjectType":"LINE","path":"src/lib.rs","diffSide":"RIGHT",
                "startDiffSide":"RIGHT","startLine":4,"line":6,
                "comments":{"nodes":[{"id":"comment-1","bodyText":"Please simplify","createdAt":"2026-07-27T10:00:00Z","url":"https://github.com/c/1","author":{"login":"reviewer"}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}
            }],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}
        ));
    });

    let client = client(&server);
    assert_eq!(
        client.load_unified_diff(&key()).unwrap(),
        "diff --git a/a b/a\n"
    );
    let source_result = client.load_source("owner/repo", "head sha", "src/space #/ü.rs");
    source.assert();
    assert_eq!(source_result.unwrap(), "fn main() {}\n");
    let threads = client.load_review_threads(&key()).unwrap();
    assert!(threads[0].is_resolved);
    assert!(!threads[0].is_outdated);
    assert!(matches!(
        threads[0].subject,
        GithubThreadSubject::Line {
            side: Some(RemoteLineSide::Right),
            start_line: Some(4),
            end_line: Some(6),
            ..
        }
    ));
    diff.assert();
}
