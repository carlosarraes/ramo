use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ramo::review_map::{
    MAX_REVIEW_MAP_RESPONSE, ReviewMapClient, ReviewMapClientError, ReviewMapPoll,
    ReviewMapResolveRequest, ReviewMapRuntime, ReviewMapService, ReviewMapUpdate,
};
use ramo_core::review_map::{
    PatchCoverage, ReviewFileKind, ReviewMap, ReviewMapFile, ReviewMapGroup, ReviewMapIdentity,
    ReviewMapStatus, ReviewMapTotals,
};

#[test]
fn client_sends_bearer_auth_and_decodes_a_bounded_response() {
    let server = FakeHttpServer::json(202, response_json(1, ReviewMapStatus::Analyzing));
    let client = ReviewMapClient::new(server.endpoint(), "secret-token").unwrap();

    let result = client.resolve(&request()).unwrap();

    assert_eq!(result.job_id, "job-1");
    assert_eq!(result.state, ReviewMapStatus::Analyzing);
    let received = server.finish();
    assert!(received.starts_with("POST /v1/review-maps HTTP/1.1\r\n"));
    assert!(received.contains("Authorization: Bearer secret-token\r\n"));
    assert!(!format!("{client:?}").contains("secret-token"));
}

#[test]
fn client_waits_for_a_server_that_fetches_before_it_answers() {
    // `POST /v1/review-maps` blocks on a GitHub round trip and an analyzer pre-flight before it
    // hands back a job id, so seconds of silence is the normal case rather than a dead server.
    let server = FakeHttpServer::json_after(
        Duration::from_millis(2_500),
        202,
        response_json(1, ReviewMapStatus::Analyzing),
    );
    let client = ReviewMapClient::new(server.endpoint(), "token").unwrap();

    let result = client.resolve(&request()).unwrap();

    assert_eq!(result.state, ReviewMapStatus::Analyzing);
    server.finish();
}

#[test]
fn client_rejects_oversized_or_incompatible_responses() {
    let oversized = FakeHttpServer::raw(format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        MAX_REVIEW_MAP_RESPONSE + 1
    ));
    let error = ReviewMapClient::new(oversized.endpoint(), "token")
        .unwrap()
        .resolve(&request())
        .unwrap_err();
    assert_eq!(
        error.code(),
        ramo_core::review_map::ReviewMapFailureCode::ServerIncompatible
    );
    oversized.finish();

    let incompatible = FakeHttpServer::json(200, response_json(999, ReviewMapStatus::Enriched));
    let error = ReviewMapClient::new(incompatible.endpoint(), "token")
        .unwrap()
        .resolve(&request())
        .unwrap_err();
    assert_eq!(
        error.code(),
        ramo_core::review_map::ReviewMapFailureCode::ServerIncompatible
    );
    incompatible.finish();
}

#[test]
fn client_rejects_remote_https_and_path_bearing_endpoints() {
    for endpoint in [
        "https://127.0.0.1:47831",
        "http://192.0.2.1:47831",
        "http://127.0.0.1:47831/path",
    ] {
        let error = ReviewMapClient::new(endpoint, "token").unwrap_err();
        assert_eq!(
            error.code(),
            ramo_core::review_map::ReviewMapFailureCode::ServerIncompatible
        );
    }
}

#[test]
fn background_runtime_emits_analyzing_then_enriched() {
    let client = ScriptedClient::new([
        poll(ReviewMapStatus::Analyzing),
        poll(ReviewMapStatus::Enriched),
    ]);
    let runtime = ReviewMapRuntime::start(client, request());

    assert!(matches!(
        runtime.recv_timeout(Duration::from_secs(1)),
        Some(ReviewMapUpdate::Analyzing)
    ));
    assert!(matches!(
        runtime.recv_timeout(Duration::from_secs(1)),
        Some(ReviewMapUpdate::Enriched(_))
    ));
}

#[test]
fn dropping_runtime_cancels_poll_wait_promptly() {
    let started = Instant::now();
    let runtime = ReviewMapRuntime::start(
        ScriptedClient::new([poll(ReviewMapStatus::Analyzing)]),
        request(),
    );
    assert!(matches!(
        runtime.recv_timeout(Duration::from_secs(1)),
        Some(ReviewMapUpdate::Analyzing)
    ));
    drop(runtime);
    assert!(started.elapsed() < Duration::from_secs(1));
}

fn request() -> ReviewMapResolveRequest {
    ReviewMapResolveRequest::new("owner/repository", 7, "head")
}

fn poll(state: ReviewMapStatus) -> ReviewMapPoll {
    ReviewMapPoll {
        job_id: "job-1".into(),
        state,
        map: map(state),
        failure: None,
    }
}

#[derive(Clone)]
struct ScriptedClient {
    responses: Arc<Mutex<VecDeque<ReviewMapPoll>>>,
}

impl ScriptedClient {
    fn new(responses: impl IntoIterator<Item = ReviewMapPoll>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
        }
    }

    fn next(&self) -> Result<ReviewMapPoll, ReviewMapClientError> {
        self.responses.lock().unwrap().pop_front().ok_or_else(|| {
            ReviewMapClientError::new(
                ramo_core::review_map::ReviewMapFailureCode::AnalysisFailed,
                "script exhausted",
            )
        })
    }
}

impl ReviewMapService for ScriptedClient {
    fn resolve(
        &self,
        _request: &ReviewMapResolveRequest,
    ) -> Result<ReviewMapPoll, ReviewMapClientError> {
        self.next()
    }

    fn poll(&self, _job_id: &str) -> Result<ReviewMapPoll, ReviewMapClientError> {
        self.next()
    }
}

struct FakeHttpServer {
    endpoint: String,
    worker: std::thread::JoinHandle<String>,
}

impl FakeHttpServer {
    fn json(status: u16, value: serde_json::Value) -> Self {
        Self::json_after(Duration::ZERO, status, value)
    }

    fn json_after(delay: Duration, status: u16, value: serde_json::Value) -> Self {
        let body = serde_json::to_vec(&value).unwrap();
        Self::raw_after(
            delay,
            format!(
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                String::from_utf8(body).unwrap()
            ),
        )
    }

    fn raw(response: String) -> Self {
        Self::raw_after(Duration::ZERO, response)
    }

    fn raw_after(delay: Duration, response: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let count = stream.read(&mut chunk).unwrap_or_default();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..count]);
                if let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&request[..header_end]);
                    let content_length = head
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or_default();
                    if request.len() >= header_end + 4 + content_length {
                        break;
                    }
                }
            }
            std::thread::sleep(delay);
            stream.write_all(response.as_bytes()).unwrap();
            String::from_utf8(request).unwrap()
        });
        Self { endpoint, worker }
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn finish(self) -> String {
        self.worker.join().unwrap()
    }
}

fn response_json(schema_version: u16, state: ReviewMapStatus) -> serde_json::Value {
    serde_json::json!({
        "schema_version": schema_version,
        "job_id": "job-1",
        "state": state,
        "map": map(state),
        "failure": null
    })
}

fn map(status: ReviewMapStatus) -> ReviewMap {
    ReviewMap {
        schema_version: 1,
        identity: ReviewMapIdentity {
            repository: "owner/repository".into(),
            pull_request: 7,
            base_sha: "base".into(),
            head_sha: "head".into(),
        },
        status,
        totals: ReviewMapTotals {
            files: 1,
            additions: 2,
            deletions: 1,
            authored: 1,
            ..ReviewMapTotals::default()
        },
        groups: vec![ReviewMapGroup {
            id: "group:head:src".into(),
            label: "src/".into(),
            kind: ReviewFileKind::Authored,
            file_ids: vec!["file:head:src/lib.rs".into()],
            additions: 2,
            deletions: 1,
            collapsed_by_default: false,
            insight: None,
        }],
        files: vec![ReviewMapFile {
            id: "file:head:src/lib.rs".into(),
            path: "src/lib.rs".into(),
            previous_path: None,
            status: "modified".into(),
            additions: 2,
            deletions: 1,
            kind: ReviewFileKind::Authored,
            owner: None,
            coverage: PatchCoverage::Full,
            insight: None,
            recommended_order: None,
        }],
        analysis: None,
    }
}
