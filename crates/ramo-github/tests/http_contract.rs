use httpmock::prelude::*;
use ramo_github::{GithubClient, GithubErrorKind};

fn test_client(server: &MockServer, token: &str) -> GithubClient {
    GithubClient::with_endpoints(
        token.into(),
        server.base_url(),
        format!("{}/graphql", server.base_url()),
    )
    .unwrap()
}

#[test]
fn viewer_request_uses_bearer_auth_and_never_formats_the_token() {
    let server = MockServer::start();
    let expected = server.mock(|when, then| {
        when.method(GET)
            .path("/user")
            .header("authorization", "Bearer secret-token")
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2026-03-10");
        then.status(200)
            .json_body_obj(&serde_json::json!({"login":"carraes","id":7}));
    });

    let client = test_client(&server, "secret-token");
    assert_eq!(client.viewer().unwrap().login, "carraes");
    assert!(!format!("{client:?}").contains("secret-token"));
    expected.assert();
}

#[test]
fn empty_tokens_and_http_statuses_are_typed_without_leaking_secrets() {
    let error = GithubClient::new("   ".into()).unwrap_err();
    assert_eq!(error.kind(), &GithubErrorKind::InvalidCredentials);

    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path("/user");
        then.status(403)
            .header("x-ratelimit-remaining", "0")
            .header("x-ratelimit-reset", "1924992000")
            .json_body_obj(&serde_json::json!({"message":"API rate limit exceeded"}));
    });
    let client = test_client(&server, "another-secret");
    let error = client.viewer().unwrap_err();
    assert_eq!(
        error.kind(),
        &GithubErrorKind::RateLimited {
            reset_at: Some(1_924_992_000)
        }
    );
    assert!(!error.to_string().contains("another-secret"));
}
