use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use ramo_core::review_map::{
    EnrichmentCoverage, EnrichmentExactGroup, EnrichmentInputFile, EnrichmentProposal,
    EnrichmentRequest, PatchCoverage, ProposedFileInsight, ProposedGroup,
    REVIEW_MAP_SCHEMA_VERSION, ReviewFileKind, ReviewMapFailureCode, ReviewMapIdentity,
};
use ramo_server::analysis::{AnalysisBudget, budget_batches};
use ramo_server::ollama::{Analyzer, OllamaAnalyzer, PROMPT_VERSION, estimate_prompt_tokens};
use serde_json::{Value, json};

struct FakeOllama {
    url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}

impl FakeOllama {
    async fn responses(responses: Vec<(StatusCode, Value)>) -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = FakeState {
            requests: requests.clone(),
            responses: Arc::new(Mutex::new(responses.into())),
        };
        let app = axum::Router::new()
            .route("/api/chat", post(fake_chat))
            .route("/api/tags", get(fake_tags))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            url: format!("http://{address}"),
            requests,
            task,
        }
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl Drop for FakeOllama {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Clone)]
struct FakeState {
    requests: Arc<Mutex<Vec<Value>>>,
    responses: Arc<Mutex<VecDeque<(StatusCode, Value)>>>,
}

async fn fake_chat(
    State(state): State<FakeState>,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    state.requests.lock().unwrap().push(body);
    let (status, body) = state.responses.lock().unwrap().pop_front().unwrap();
    (status, Json(body))
}

async fn fake_tags() -> Json<Value> {
    Json(json!({
        "models": [{
            "name": "qwen3:8b",
            "model": "qwen3:8b",
            "digest": "sha256:fixture"
        }]
    }))
}

#[tokio::test]
async fn ollama_request_uses_local_structured_schema_and_no_streaming() {
    let fake = FakeOllama::responses(vec![(StatusCode::OK, response(valid_proposal()))]).await;
    let analyzer = OllamaAnalyzer::new(&fake.url, "qwen3:8b", Duration::from_secs(30));
    let mut request = request_fixture();
    request.files.push(input_file(
        "tests/lib_test.rs",
        ReviewFileKind::Test,
        Some("+test"),
        PatchCoverage::Full,
    ));
    request
        .coverage
        .analyzed_paths
        .push("tests/lib_test.rs".into());

    let result = analyzer.analyze(request).await.unwrap();

    let requests = fake.requests.lock().unwrap();
    let sent = &requests[0];
    assert_eq!(sent["model"], "qwen3:8b");
    assert_eq!(sent["stream"], false);
    assert_eq!(sent["format"]["type"], "object");
    assert_eq!(
        sent["format"]["properties"]["groups"]["items"]["properties"]["paths"]["items"]["enum"],
        json!(["src/lib.rs"])
    );
    assert_eq!(
        sent["format"]["properties"]["files"]["items"]["properties"]["path"]["enum"],
        json!(["src/lib.rs", "tests/lib_test.rs"])
    );
    assert_eq!(
        sent["format"]["properties"]["review_order"]["items"]["enum"],
        json!(["src/lib.rs"])
    );
    assert_eq!(
        sent["format"]["properties"]["groups"]["items"]["properties"]["paths"]["uniqueItems"],
        true
    );
    assert_eq!(
        sent["format"]["properties"]["review_order"]["uniqueItems"],
        true
    );
    assert_eq!(sent["options"]["temperature"], 0);
    assert_eq!(sent["options"]["num_ctx"], 32_768);
    assert_eq!(sent["options"]["num_predict"], 6_144);
    let system_prompt = sent["messages"][0]["content"].as_str().unwrap();
    assert!(
        system_prompt
            .contains("Never claim tests passed, coverage is complete, or deployment is safe")
    );
    assert!(system_prompt.contains("use null when no concrete risk is visible"));
    assert_eq!(sent["format"]["properties"]["files"]["minItems"], 1);
    assert_eq!(PROMPT_VERSION, 2);
    assert_eq!(result.model_digest, "sha256:fixture");
    assert_eq!(result.proposal.groups[0].label, "Core billing path");
}

#[tokio::test]
async fn malformed_output_is_repaired_once_then_fails_typed() {
    let fake = FakeOllama::responses(vec![
        (StatusCode::OK, response(json!("not json"))),
        (StatusCode::OK, response(json!("still not json"))),
    ])
    .await;
    let error = OllamaAnalyzer::new(&fake.url, "qwen3:8b", Duration::from_secs(30))
        .analyze(request_fixture())
        .await
        .unwrap_err();

    assert_eq!(fake.request_count(), 2);
    assert_eq!(error.code, ReviewMapFailureCode::AnalysisInvalid);
}

#[tokio::test]
async fn semantic_validation_rejects_invented_paths_and_repairs_once() {
    let fake = FakeOllama::responses(vec![
        (StatusCode::OK, response(proposal_for(&["src/invented.rs"]))),
        (StatusCode::OK, response(valid_proposal())),
    ])
    .await;

    let result = OllamaAnalyzer::new(&fake.url, "qwen3:8b", Duration::from_secs(30))
        .analyze(request_fixture())
        .await
        .unwrap();

    assert_eq!(fake.request_count(), 2);
    assert_eq!(result.proposal.review_order, ["src/lib.rs"]);
}

#[tokio::test]
async fn low_quality_output_is_repaired_once() {
    let fake = FakeOllama::responses(vec![
        (StatusCode::OK, response(generic_proposal())),
        (StatusCode::OK, response(valid_proposal())),
    ])
    .await;

    let result = OllamaAnalyzer::new(&fake.url, "qwen3:8b", Duration::from_secs(90))
        .analyze(request_fixture())
        .await
        .unwrap();

    assert_eq!(fake.request_count(), 2);
    assert_eq!(result.repair_count, 1);
}

#[tokio::test]
async fn low_quality_output_repairs_once_then_fails_typed() {
    let fake = FakeOllama::responses(vec![
        (StatusCode::OK, response(generic_proposal())),
        (StatusCode::OK, response(generic_proposal())),
    ])
    .await;

    let error = OllamaAnalyzer::new(&fake.url, "qwen3:8b", Duration::from_secs(90))
        .analyze(request_fixture())
        .await
        .unwrap_err();

    assert_eq!(fake.request_count(), 2);
    assert_eq!(error.code, ReviewMapFailureCode::AnalysisLowQuality);
    assert_eq!(
        error.message,
        "Local AI did not add reliable review guidance; the exact map is still ready"
    );
    let requests = fake.requests.lock().unwrap();
    let repair = requests[1]["messages"][2]["content"].as_str().unwrap();
    assert!(repair.contains("generic_summary"));
    assert!(!repair.contains("src/lib.rs"));
}

#[tokio::test]
async fn omitted_and_duplicate_assignments_trigger_one_repair() {
    let mut request = request_fixture();
    request.files.push(input_file(
        "src/billing.rs",
        ReviewFileKind::Authored,
        Some("+billing"),
        PatchCoverage::Full,
    ));
    request.groups[0].paths.push("src/billing.rs".into());
    request
        .coverage
        .analyzed_paths
        .push("src/billing.rs".into());
    let mut incomplete = proposal_for(&["src/lib.rs"]);
    incomplete["groups"][0]["paths"] = json!(["src/lib.rs", "src/lib.rs"]);
    incomplete["review_order"] = json!(["src/lib.rs", "src/lib.rs"]);
    let fake = FakeOllama::responses(vec![
        (StatusCode::OK, response(incomplete)),
        (
            StatusCode::OK,
            response(proposal_for(&["src/lib.rs", "src/billing.rs"])),
        ),
    ])
    .await;

    let result = OllamaAnalyzer::new(&fake.url, "qwen3:8b", Duration::from_secs(30))
        .analyze(request)
        .await
        .unwrap();

    assert_eq!(fake.request_count(), 2);
    assert_eq!(
        result.proposal.review_order,
        ["src/lib.rs", "src/billing.rs"]
    );
    assert_eq!(result.repair_count, 1);
}

#[tokio::test]
async fn missing_model_is_a_specific_failure() {
    let fake = FakeOllama::responses(vec![(
        StatusCode::NOT_FOUND,
        json!({"error": "model not found"}),
    )])
    .await;
    let error = OllamaAnalyzer::new(&fake.url, "missing", Duration::from_secs(30))
        .analyze(request_fixture())
        .await
        .unwrap_err();

    assert_eq!(error.code, ReviewMapFailureCode::ModelMissing);
}

#[tokio::test]
async fn non_loopback_ollama_is_rejected_before_sending_code() {
    let error = OllamaAnalyzer::new(
        "https://ollama.example.com",
        "qwen3:8b",
        Duration::from_secs(30),
    )
    .analyze(request_fixture())
    .await
    .unwrap_err();

    assert_eq!(error.code, ReviewMapFailureCode::OllamaUnavailable);
}

#[tokio::test]
async fn connection_refusal_and_timeout_have_distinct_failures() {
    let unused = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let unused_address = unused.local_addr().unwrap();
    drop(unused);
    let unavailable = OllamaAnalyzer::new(
        format!("http://{unused_address}"),
        "qwen3:8b",
        Duration::from_secs(1),
    )
    .analyze(request_fixture())
    .await
    .unwrap_err();
    assert_eq!(unavailable.code, ReviewMapFailureCode::OllamaUnavailable);

    let app = axum::Router::new()
        .route(
            "/api/chat",
            post(|| async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Json(response(valid_proposal()))
            }),
        )
        .route("/api/tags", get(fake_tags));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let timed_out = OllamaAnalyzer::new(
        format!("http://{address}"),
        "qwen3:8b",
        Duration::from_millis(5),
    )
    .analyze(request_fixture())
    .await
    .unwrap_err();
    task.abort();
    assert_eq!(timed_out.code, ReviewMapFailureCode::AnalysisTimedOut);
}

#[tokio::test]
async fn multiple_budget_batches_are_validated_then_synthesized() {
    let mut request = request_fixture();
    request.files.push(input_file(
        "src/billing.rs",
        ReviewFileKind::Authored,
        Some("+billing"),
        PatchCoverage::Full,
    ));
    request.groups[0].paths.push("src/billing.rs".into());
    request
        .coverage
        .analyzed_paths
        .push("src/billing.rs".into());
    let fake = FakeOllama::responses(vec![
        (StatusCode::OK, response(proposal_for(&["src/lib.rs"]))),
        (StatusCode::OK, response(proposal_for(&["src/billing.rs"]))),
        (
            StatusCode::OK,
            response(proposal_for(&["src/lib.rs", "src/billing.rs"])),
        ),
    ])
    .await;
    let analyzer = OllamaAnalyzer::new(&fake.url, "qwen3:8b", Duration::from_secs(30)).with_budget(
        AnalysisBudget {
            max_prompt_tokens: usize::MAX,
            max_files_per_batch: 1,
        },
    );

    let result = analyzer.analyze(request).await.unwrap();

    assert_eq!(fake.request_count(), 3);
    assert_eq!(result.proposal.review_order.len(), 2);
    let requests = fake.requests.lock().unwrap();
    assert!(
        requests[2]["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("validated_batch_results")
    );
}

#[test]
fn budgeting_omits_generated_bodies_and_bounds_batches() {
    let mut request = request_fixture();
    request.files.extend([
        input_file(
            "src/client.generated.ts",
            ReviewFileKind::Generated,
            Some("secret generated body"),
            PatchCoverage::Full,
        ),
        input_file(
            "assets/logo.png",
            ReviewFileKind::Other,
            None,
            PatchCoverage::Binary,
        ),
        input_file(
            "tests/billing_test.rs",
            ReviewFileKind::Test,
            Some("test patch"),
            PatchCoverage::Full,
        ),
    ]);
    request.groups.push(EnrichmentExactGroup {
        id: "fixed".into(),
        label: "Generated".into(),
        kind: ReviewFileKind::Generated,
        paths: vec!["src/client.generated.ts".into()],
    });
    let batches = budget_batches(
        &request,
        &AnalysisBudget {
            max_prompt_tokens: 5_000,
            max_files_per_batch: 2,
        },
        |batch| estimate_prompt_tokens(batch, None).unwrap(),
    );

    assert!(batches.len() >= 2);
    let files = batches
        .iter()
        .flat_map(|batch| &batch.files)
        .collect::<Vec<_>>();
    assert_eq!(files.len(), request.files.len());
    assert_eq!(
        files
            .iter()
            .find(|file| file.kind == ReviewFileKind::Generated)
            .unwrap()
            .patch,
        None
    );
    assert!(batches.iter().all(|batch| batch.files.len() <= 2));
}

#[test]
fn token_budget_counts_complete_prompt_and_splits_on_file_boundaries() {
    let request = request_with_large_authored_patches(2, 50_000);
    let batches = budget_batches(&request, &AnalysisBudget::default(), |batch| {
        estimate_prompt_tokens(batch, None).unwrap()
    });

    assert!(batches.len() >= 2);
    assert!(
        batches
            .iter()
            .all(|batch| estimate_prompt_tokens(batch, None).unwrap() <= 24_576)
    );
    assert_eq!(
        batches.iter().map(|batch| batch.files.len()).sum::<usize>(),
        2
    );
}

#[test]
fn token_budget_truncates_one_oversized_file_at_a_utf8_boundary() {
    let request = request_with_large_authored_patches(1, 100_000);
    let batches = budget_batches(&request, &AnalysisBudget::default(), |batch| {
        estimate_prompt_tokens(batch, None).unwrap()
    });

    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].files[0].coverage, PatchCoverage::Truncated);
    assert!(
        batches[0].files[0]
            .patch
            .as_ref()
            .unwrap()
            .is_char_boundary(batches[0].files[0].patch.as_ref().unwrap().len())
    );
    assert!(estimate_prompt_tokens(&batches[0], None).unwrap() <= 24_576);
}

fn request_fixture() -> EnrichmentRequest {
    EnrichmentRequest {
        schema_version: REVIEW_MAP_SCHEMA_VERSION,
        identity: ReviewMapIdentity {
            repository: "owner/repo".into(),
            pull_request: 7,
            base_sha: "base".into(),
            head_sha: "head".into(),
        },
        groups: vec![EnrichmentExactGroup {
            id: "authored".into(),
            label: "src/".into(),
            kind: ReviewFileKind::Authored,
            paths: vec!["src/lib.rs".into()],
        }],
        files: vec![input_file(
            "src/lib.rs",
            ReviewFileKind::Authored,
            Some("@@ -1 +1 @@\n-old\n+new\n"),
            PatchCoverage::Full,
        )],
        coverage: EnrichmentCoverage {
            analyzed_paths: vec!["src/lib.rs".into()],
            ..EnrichmentCoverage::default()
        },
    }
}

fn request_with_large_authored_patches(file_count: usize, patch_bytes: usize) -> EnrichmentRequest {
    let mut request = request_fixture();
    request.files.clear();
    request.groups[0].paths.clear();
    request.coverage = EnrichmentCoverage::default();
    for index in 0..file_count {
        let path = format!("src/large_{index}.rs");
        request.files.push(EnrichmentInputFile {
            path: path.clone(),
            kind: ReviewFileKind::Authored,
            additions: patch_bytes,
            deletions: 0,
            coverage: PatchCoverage::Full,
            patch: Some("+x\n".repeat(patch_bytes.div_ceil(3))),
        });
        request.groups[0].paths.push(path.clone());
        request.coverage.analyzed_paths.push(path);
    }
    request
}

fn input_file(
    path: &str,
    kind: ReviewFileKind,
    patch: Option<&str>,
    coverage: PatchCoverage,
) -> EnrichmentInputFile {
    EnrichmentInputFile {
        path: path.into(),
        kind,
        additions: 4,
        deletions: 2,
        coverage,
        patch: patch.map(str::to_owned),
    }
}

fn valid_proposal() -> Value {
    proposal_for(&["src/lib.rs"])
}

fn generic_proposal() -> Value {
    let mut proposal = proposal_for(&["src/lib.rs"]);
    proposal["groups"][0]["summary"] = json!("This group contains source changes.");
    proposal["files"][0]["summary"] = json!("This file contains source changes.");
    proposal
}

fn proposal_for(paths: &[&str]) -> Value {
    serde_json::to_value(EnrichmentProposal {
        groups: vec![ProposedGroup {
            label: "Core billing path".into(),
            summary: "Changes billing behavior.".into(),
            risk: Some("Check invoice totals.".into()),
            review_priority: 1,
            paths: paths.iter().map(|path| (*path).into()).collect(),
        }],
        files: paths
            .iter()
            .map(|path| ProposedFileInsight {
                path: (*path).into(),
                summary: format!("Changes behavior implemented in {path}."),
                risk: None,
            })
            .collect(),
        review_order: paths.iter().map(|path| (*path).into()).collect(),
        coverage: EnrichmentCoverage::default(),
    })
    .unwrap()
}

fn response(content: Value) -> Value {
    json!({
        "model": "qwen3:8b",
        "model_digest": "sha256:fixture",
        "message": { "role": "assistant", "content": content.to_string() },
        "prompt_eval_count": 120,
        "eval_count": 40,
        "total_duration": 1_000_000
    })
}
