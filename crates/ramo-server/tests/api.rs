mod support;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use ramo_server::api::build_router;
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn review_map_routes_require_a_valid_client_token() {
    let (_directory, state, credential) = support::state();
    let app = build_router(state);

    assert_eq!(
        request(
            &app,
            Method::POST,
            "/v1/review-maps",
            None,
            support::create_body()
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = request(
        &app,
        Method::POST,
        "/v1/review-maps",
        Some(&credential.token),
        support::create_body(),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["schema_version"], 1);
    assert_eq!(body["map"]["identity"]["head_sha"], "head");
    assert!(body.get("job_id").is_some());

    let job_uri = format!("/v1/review-maps/{}", body["job_id"].as_str().unwrap());
    let (poll_status, polled) = request(
        &app,
        Method::GET,
        &job_uri,
        Some(&credential.token),
        Value::Null,
    )
    .await;
    assert!(matches!(poll_status, StatusCode::OK | StatusCode::ACCEPTED));
    assert_eq!(polled["map"]["identity"]["head_sha"], "head");
}

#[tokio::test]
async fn invalid_repository_and_stale_expected_head_are_typed() {
    let (_directory, state, credential) = support::state();
    let app = build_router(state);
    let (status, _) = request(
        &app,
        Method::POST,
        "/v1/review-maps",
        Some(&credential.token),
        serde_json::json!({
            "repository": "not/a/repository",
            "pull_request": 7,
            "expected_head_sha": "head"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, body) = request(
        &app,
        Method::POST,
        "/v1/review-maps",
        Some(&credential.token),
        serde_json::json!({
            "repository": "owner/repo",
            "pull_request": 7,
            "expected_head_sha": "old-head"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["failure"]["code"], "result_stale");

    let (status, body) = request(
        &app,
        Method::POST,
        "/v1/review-maps",
        Some(&credential.token),
        serde_json::json!({
            "schema_version": 999,
            "repository": "owner/repo",
            "pull_request": 7,
            "expected_head_sha": "head"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UPGRADE_REQUIRED);
    assert_eq!(body["failure"]["code"], "server_incompatible");
}

#[tokio::test]
async fn pairing_exchange_and_client_revocation_work_through_the_api() {
    let (_directory, state, credential) = support::state();
    let code = state
        .pairing
        .issue(std::time::Duration::from_secs(300))
        .unwrap();
    let app = build_router(state);

    let (status, paired) = request(
        &app,
        Method::POST,
        "/v1/pair/exchange",
        None,
        serde_json::json!({"code": code, "label": "Android"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(paired["token"].as_str().unwrap().starts_with("ramo_"));

    let (status, _) = request(
        &app,
        Method::DELETE,
        &format!("/v1/clients/{}", credential.client_id),
        Some(&credential.token),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        request(
            &app,
            Method::POST,
            "/v1/review-maps",
            Some(&credential.token),
            support::create_body(),
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

pub async fn request(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}
