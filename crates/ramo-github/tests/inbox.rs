use httpmock::prelude::*;
use ramo_core::github::InboxKind;
use ramo_github::GithubClient;

const SEARCH_DOCUMENT: &str = "query SearchPullRequests($query: String!, $first: Int!, $after: String) { search(query: $query, type: ISSUE, first: $first, after: $after) { nodes { ... on PullRequest { id number title url updatedAt isDraft additions deletions changedFiles author { login } repository { nameWithOwner } } } pageInfo { endCursor hasNextPage } } }";

fn client(server: &MockServer) -> GithubClient {
    GithubClient::with_endpoints(
        "token".into(),
        server.base_url(),
        format!("{}/graphql", server.base_url()),
    )
    .unwrap()
}

fn search_response(
    nodes: serde_json::Value,
    cursor: Option<&str>,
    has_next: bool,
) -> serde_json::Value {
    serde_json::json!({
        "data": {"search": {
            "nodes": nodes,
            "pageInfo": {"endCursor": cursor, "hasNextPage": has_next}
        }}
    })
}

fn pr(id: &str, number: u64, updated_at: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "number": number,
        "title": format!("PR {number}"),
        "url": format!("https://github.com/owner/repo/pull/{number}"),
        "updatedAt": updated_at,
        "isDraft": false,
        "additions": 10,
        "deletions": 2,
        "changedFiles": 3,
        "author": {"login": "author"},
        "repository": {"nameWithOwner": "owner/repo"}
    })
}

#[test]
fn authored_inbox_sends_exact_search_variables_and_preserves_pagination() {
    let server = MockServer::start();
    let search = server.mock(|when, then| {
        when.method(POST)
            .path("/graphql")
            .json_body_obj(&serde_json::json!({
                "query": SEARCH_DOCUMENT,
                "variables": {
                    "query": "is:open is:pr author:@me",
                    "first": 20,
                    "after": "cursor-1"
                }
            }));
        then.status(200).json_body_obj(&search_response(
            serde_json::json!([pr("PR_1", 1, "2026-07-27T10:00:00Z")]),
            Some("cursor-2"),
            true,
        ));
    });

    let page = client(&server)
        .list_inbox(InboxKind::Authored, Some("cursor-1"))
        .unwrap();

    assert_eq!(page.items[0].key.number, 1);
    assert_eq!(page.end_cursor.as_deref(), Some("cursor-2"));
    assert!(page.has_next_page);
    search.assert();
}

#[test]
fn requested_inbox_merges_teams_deduplicates_and_sorts_newest_first() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET)
            .path("/user/teams")
            .query_param("per_page", "100");
        then.status(200).json_body_obj(&serde_json::json!([
            {"slug":"backend","organization":{"login":"owner"}}
        ]));
    });
    server.mock(|when, then| {
        when.method(POST)
            .path("/graphql")
            .body_includes("user-review-requested:@me");
        then.status(200).json_body_obj(&search_response(
            serde_json::json!([pr("PR_DUP", 2, "2026-07-26T10:00:00Z")]),
            None,
            false,
        ));
    });
    server.mock(|when, then| {
        when.method(POST)
            .path("/graphql")
            .body_includes("team-review-requested:owner/backend");
        then.status(200).json_body_obj(&search_response(
            serde_json::json!([
                pr("PR_DUP", 2, "2026-07-26T10:00:00Z"),
                pr("PR_NEW", 3, "2026-07-27T10:00:00Z")
            ]),
            None,
            false,
        ));
    });

    let page = client(&server)
        .list_inbox(InboxKind::ReviewRequests, None)
        .unwrap();

    assert_eq!(
        page.items
            .iter()
            .map(|item| item.key.number)
            .collect::<Vec<_>>(),
        [3, 2]
    );
    assert!(page.warnings.is_empty());
}

#[test]
fn missing_team_permission_keeps_direct_results_and_returns_a_warning() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/user/teams");
        then.status(403)
            .json_body_obj(&serde_json::json!({"message":"Resource not accessible"}));
    });
    server.mock(|when, then| {
        when.method(POST).path("/graphql");
        then.status(200).json_body_obj(&search_response(
            serde_json::json!([pr("PR_1", 1, "2026-07-27T10:00:00Z")]),
            None,
            false,
        ));
    });

    let page = client(&server)
        .list_inbox(InboxKind::ReviewRequests, None)
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(
        page.warnings,
        ["Team review requests need a token whose resource owner is that organization."]
    );
}
