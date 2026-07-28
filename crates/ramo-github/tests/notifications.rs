use httpmock::prelude::*;
use ramo_core::github::ConditionalCursor;
use ramo_github::GithubClient;

fn client(server: &MockServer) -> GithubClient {
    GithubClient::with_endpoints(
        "token".into(),
        server.base_url(),
        format!("{}/graphql", server.base_url()),
    )
    .unwrap()
}

#[test]
fn conditional_304_returns_the_existing_cursor_without_notifications() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/notifications")
            .header("if-none-match", "etag-old")
            .header("if-modified-since", "Sun, 26 Jul 2026 10:00:00 GMT");
        then.status(304);
    });
    let cursor = ConditionalCursor {
        etag: Some("etag-old".into()),
        last_modified: Some("Sun, 26 Jul 2026 10:00:00 GMT".into()),
    };

    let page = client(&server).review_notifications(&cursor).unwrap();

    assert!(page.not_modified);
    assert!(page.notifications.is_empty());
    assert_eq!(page.cursor, cursor);
}

#[test]
fn notifications_filter_review_requests_and_resolve_pull_request_numbers() {
    let server = MockServer::start();
    let subject_url = format!("{}/repos/owner/repo/pulls/42", server.base_url());
    server.mock(|when, then| {
        when.method(GET).path("/notifications");
        then.status(200)
            .header("etag", "etag-new")
            .header("last-modified", "Mon, 27 Jul 2026 10:00:00 GMT")
            .json_body_obj(&serde_json::json!([
                {"id":"n1","reason":"review_requested","updated_at":"2026-07-27T10:00:00Z","subject":{"title":"Review me","type":"PullRequest","url":subject_url},"repository":{"full_name":"owner/repo"}},
                {"id":"n2","reason":"mention","updated_at":"2026-07-27T09:00:00Z","subject":{"title":"Ignore","type":"PullRequest","url":subject_url},"repository":{"full_name":"owner/repo"}},
                {"id":"n3","reason":"review_requested","updated_at":"2026-07-27T08:00:00Z","subject":{"title":"Issue","type":"Issue","url":null},"repository":{"full_name":"owner/repo"}}
            ]));
    });
    let pull = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/pulls/42");
        then.status(200)
            .json_body_obj(&serde_json::json!({"number":42}));
    });

    let page = client(&server)
        .review_notifications(&ConditionalCursor::default())
        .unwrap();

    assert!(!page.not_modified);
    assert_eq!(page.notifications.len(), 1);
    assert_eq!(page.notifications[0].key.repository, "owner/repo");
    assert_eq!(page.notifications[0].key.number, 42);
    assert_eq!(page.cursor.etag.as_deref(), Some("etag-new"));
    pull.assert_calls(1);
}
