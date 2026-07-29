use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ramo_core::github::PullRequestKey;
use ramo_core::review_map::{
    EnrichmentCoverage, EnrichmentProposal, ProposedGroup, ReviewFileKind, ReviewMapFailureCode,
    ReviewMapIdentity, ReviewMapInput, ReviewMapInputFile,
};
use ramo_server::ReviewMapFailure;
use ramo_server::analysis::{AnalysisBudget, AnalyzerIdentity};
use ramo_server::benchmark::{
    BenchmarkAnalyzerFactory, BenchmarkManifest, BenchmarkRun, BenchmarkRunner,
    CandidateMeasurement, CompletionState,
};
use ramo_server::github::PullRequestProvider;
use ramo_server::ollama::{AnalysisResult, Analyzer};

#[tokio::test]
async fn every_candidate_receives_the_same_request_digest_and_each_pr_loads_once() {
    let mut fixture = Fixture::new();

    fixture
        .runner
        .run(&fixture.manifest, &mut fixture.run)
        .await
        .unwrap();

    let run = &fixture.run;
    for pull_request in &fixture.manifest.pull_requests {
        let digests = run
            .measurements
            .iter()
            .filter(|measurement| measurement.case.pull_request == *pull_request)
            .map(|measurement| measurement.request_digest.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(digests.len(), 1);
    }
    assert_eq!(fixture.provider.loads.load(Ordering::SeqCst), 6);
    assert_eq!(fixture.factory.max_active.load(Ordering::SeqCst), 1);
    assert_eq!(run.measurements.len(), 18);
}

#[tokio::test]
async fn rerun_skips_completed_candidate_case_pairs() {
    let mut fixture = Fixture::new();
    fixture
        .run
        .record(completed(&fixture.manifest, 1, "qwen3:8b"));

    fixture
        .runner
        .run(&fixture.manifest, &mut fixture.run)
        .await
        .unwrap();

    assert_eq!(fixture.factory.calls_for("qwen3:8b", 1), 0);
    assert_eq!(fixture.run.measurements.len(), 18);
}

#[tokio::test]
async fn failed_candidate_is_recorded_without_stopping_later_candidates() {
    let mut fixture = Fixture::new();
    fixture
        .factory
        .fail_model
        .lock()
        .unwrap()
        .replace("qwen3-coder:30b".into());

    fixture
        .runner
        .run(&fixture.manifest, &mut fixture.run)
        .await
        .unwrap();

    let run = &fixture.run;
    assert_eq!(
        run.measurements
            .iter()
            .filter(|measurement| measurement.completion == CompletionState::Failed)
            .count(),
        6
    );
    assert_eq!(fixture.factory.calls_for("qwen2.5-coder:7b", 6), 1);
    assert!(
        run.measurements
            .iter()
            .filter(|measurement| measurement.model == "qwen3-coder:30b")
            .all(|measurement| {
                measurement.failure_code == Some(ReviewMapFailureCode::AnalysisFailed)
            })
    );
}

#[tokio::test]
async fn public_metrics_exclude_bodies_while_private_outputs_keep_them() {
    let mut fixture = Fixture::new();
    fixture
        .runner
        .run(&fixture.manifest, &mut fixture.run)
        .await
        .unwrap();

    let metrics =
        std::fs::read_to_string(fixture.directory.path().join("measurements.jsonl")).unwrap();
    let private =
        std::fs::read_to_string(fixture.directory.path().join("private/1/candidate-1.json"))
            .unwrap();

    assert!(!metrics.contains("private patch sentinel"));
    assert!(!metrics.contains("Implementation summary"));
    assert!(private.contains("private patch sentinel"));
    assert!(private.contains("Implementation summary"));
}

struct Fixture {
    directory: tempfile::TempDir,
    manifest: BenchmarkManifest,
    run: BenchmarkRun,
    provider: Arc<TestProvider>,
    factory: Arc<TestFactory>,
    runner: BenchmarkRunner,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let manifest = BenchmarkManifest::new(
            PathBuf::from("/tmp/repository"),
            "owner/repository".into(),
            vec![1, 2, 3, 4, 5, 6],
            ["qwen3:8b", "qwen3-coder:30b", "qwen2.5-coder:7b"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )
        .unwrap();
        let provider = Arc::new(TestProvider::default());
        let factory = Arc::new(TestFactory::default());
        let runner = BenchmarkRunner::new(
            provider.clone(),
            factory.clone(),
            directory.path().to_path_buf(),
        );
        Self {
            run: BenchmarkRun::new("run-1".into(), &manifest, 42),
            directory,
            manifest,
            provider,
            factory,
            runner,
        }
    }
}

#[derive(Default)]
struct TestProvider {
    loads: AtomicUsize,
}

#[async_trait]
impl PullRequestProvider for TestProvider {
    async fn load(&self, key: &PullRequestKey) -> Result<ReviewMapInput, ReviewMapFailure> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(ReviewMapInput {
            identity: ReviewMapIdentity {
                repository: key.repository.clone(),
                pull_request: key.number,
                base_sha: "base".into(),
                head_sha: format!("head-{}", key.number),
            },
            files: vec![ReviewMapInputFile {
                path: format!("src/case_{}.rs", key.number),
                previous_path: None,
                status: "modified".into(),
                additions: 2,
                deletions: 1,
                patch: Some("private patch sentinel".into()),
                binary: false,
            }],
            codeowners: None,
        })
    }
}

#[derive(Default)]
struct TestFactory {
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    calls: Arc<Mutex<HashMap<(String, u64), usize>>>,
    fail_model: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl BenchmarkAnalyzerFactory for TestFactory {
    async fn create(
        &self,
        model: &str,
        _budget: AnalysisBudget,
    ) -> Result<Arc<dyn Analyzer>, ReviewMapFailure> {
        Ok(Arc::new(TestAnalyzer {
            model: model.into(),
            active: self.active.clone(),
            max_active: self.max_active.clone(),
            calls: self.calls.clone(),
            fail_model: self.fail_model.clone(),
        }))
    }
}

impl TestFactory {
    fn calls_for(&self, model: &str, pull_request: u64) -> usize {
        self.calls
            .lock()
            .unwrap()
            .get(&(model.into(), pull_request))
            .copied()
            .unwrap_or_default()
    }
}

struct TestAnalyzer {
    model: String,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    calls: Arc<Mutex<HashMap<(String, u64), usize>>>,
    fail_model: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl Analyzer for TestAnalyzer {
    async fn identity(&self) -> Result<AnalyzerIdentity, ReviewMapFailure> {
        Ok(AnalyzerIdentity {
            model: self.model.clone(),
            model_digest: format!("digest:{}", self.model),
            prompt_version: 1,
            generation_parameters: vec![("seed".into(), "42".into())],
        })
    }

    async fn analyze(
        &self,
        request: ramo_core::review_map::EnrichmentRequest,
    ) -> Result<AnalysisResult, ReviewMapFailure> {
        let pull_request = request.identity.pull_request;
        *self
            .calls
            .lock()
            .unwrap()
            .entry((self.model.clone(), pull_request))
            .or_default() += 1;
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        tokio::task::yield_now().await;
        self.active.fetch_sub(1, Ordering::SeqCst);
        if self.fail_model.lock().unwrap().as_deref() == Some(self.model.as_str()) {
            return Err(ReviewMapFailure::new(
                ramo_core::review_map::ReviewMapFailureCode::AnalysisFailed,
                "candidate failed",
            ));
        }
        let paths = request
            .files
            .iter()
            .filter(|file| !matches!(file.kind, ReviewFileKind::Test | ReviewFileKind::Generated))
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        Ok(AnalysisResult {
            proposal: EnrichmentProposal {
                groups: vec![ProposedGroup {
                    label: "Implementation".into(),
                    summary: "Implementation summary".into(),
                    risk: None,
                    review_priority: 1,
                    paths: paths.clone(),
                }],
                files: Vec::new(),
                review_order: paths,
                coverage: EnrichmentCoverage::default(),
            },
            model: self.model.clone(),
            model_digest: format!("digest:{}", self.model),
            prompt_eval_count: 10,
            eval_count: 20,
            total_duration_ns: 30,
            repair_count: 0,
        })
    }
}

fn completed(manifest: &BenchmarkManifest, pull_request: u64, model: &str) -> CandidateMeasurement {
    CandidateMeasurement {
        case: ramo_server::benchmark::BenchmarkCase::new(pull_request),
        candidate_id: "candidate-1".into(),
        model: model.into(),
        model_digest: format!("digest:{model}"),
        prompt_version: manifest.prompt_version,
        request_digest: "request".into(),
        wall_time_ms: 1,
        ollama_total_duration_ns: 1,
        prompt_eval_count: 1,
        eval_count: 1,
        schema_valid: true,
        semantic_valid: true,
        repair_count: 0,
        unknown_reference_count: 0,
        peak_rss_bytes: None,
        completion: CompletionState::Completed,
        failure_code: None,
    }
}
