mod support;

use std::io;
use std::sync::{Arc, Mutex};

use axum::http::Method;
use ramo_server::api::build_router;

#[tokio::test]
async fn request_logs_exclude_tokens_patches_prompts_and_model_content() {
    let output = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(output.clone())
        .finish();
    let (_directory, state, credential) = support::state();
    let app = build_router(state);

    let _guard = tracing::subscriber::set_default(subscriber);
    let _ = api_request(
        &app,
        Method::POST,
        "/v1/review-maps",
        Some(&credential.token),
        support::create_body(),
    )
    .await;

    let logs = output.contents();
    assert!(logs.contains("/v1/review-maps"));
    for secret in [
        credential.token.as_str(),
        "@@ secret patch body",
        "You organize a pull request",
        "Core implementation.",
    ] {
        assert!(!logs.contains(secret), "logs contained {secret:?}: {logs}");
    }
}

async fn api_request(
    app: &axum::Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: serde_json::Value,
) -> (axum::http::StatusCode, serde_json::Value) {
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;

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
    let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn contents(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = CapturedWriter;

    fn make_writer(&'a self) -> Self::Writer {
        CapturedWriter(self.0.clone())
    }
}

struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for CapturedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
