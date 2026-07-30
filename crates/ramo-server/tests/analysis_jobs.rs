use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use ramo_core::github::PullRequestKey;
use ramo_core::review_map::{
    EnrichmentCoverage, EnrichmentProposal, ProposedGroup, ReviewFileKind, ReviewMapFailureCode,
    ReviewMapIdentity, ReviewMapInput, ReviewMapInputFile, ReviewMapStatus,
};
use ramo_server::ReviewMapFailure;
use ramo_server::analysis::{
    AnalysisCoordinator, AnalysisJobId, AnalyzerIdentity, CoordinatorConfig, JobState,
    ResolveRequest,
};
use ramo_server::cache::{CacheLimits, ReviewMapCache};
use ramo_server::github::PullRequestProvider;
use ramo_server::ollama::{AnalysisResult, Analyzer};

struct FakeProvider;

#[async_trait]
impl PullRequestProvider for FakeProvider {
    async fn load(&self, key: &PullRequestKey) -> Result<ReviewMapInput, ReviewMapFailure> {
        Ok(input(key, "head"))
    }
}

struct MutableProvider {
    head: Mutex<String>,
}

#[async_trait]
impl PullRequestProvider for MutableProvider {
    async fn load(&self, key: &PullRequestKey) -> Result<ReviewMapInput, ReviewMapFailure> {
        Ok(input(key, &self.head.lock().unwrap()))
    }
}

#[derive(Clone)]
struct CountingAnalyzer {
    calls: Arc<AtomicUsize>,
    permit: Arc<tokio::sync::Semaphore>,
}

impl CountingAnalyzer {
    fn blocked() -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            permit: Arc::new(tokio::sync::Semaphore::new(0)),
        }
    }

    fn release(&self) {
        self.permit.add_permits(1);
    }
}

#[async_trait]
impl Analyzer for CountingAnalyzer {
    async fn identity(&self) -> Result<AnalyzerIdentity, ReviewMapFailure> {
        Ok(AnalyzerIdentity {
            model: "qwen3:8b".into(),
            model_digest: "sha256:model".into(),
            prompt_version: 1,
            generation_parameters: vec![("temperature".into(), "0".into())],
        })
    }

    async fn analyze(
        &self,
        request: ramo_core::review_map::EnrichmentRequest,
    ) -> Result<AnalysisResult, ReviewMapFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.permit.acquire().await.unwrap().forget();
        let paths = request
            .files
            .iter()
            .filter(|file| !matches!(file.kind, ReviewFileKind::Test | ReviewFileKind::Generated))
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        Ok(AnalysisResult {
            proposal: EnrichmentProposal {
                groups: vec![ProposedGroup {
                    label: "Core".into(),
                    summary: "Core implementation.".into(),
                    risk: None,
                    review_priority: 1,
                    paths: paths.clone(),
                }],
                files: Vec::new(),
                review_order: paths,
                coverage: EnrichmentCoverage::default(),
            },
            model: "qwen3:8b".into(),
            model_digest: "sha256:model".into(),
            prompt_eval_count: 10,
            eval_count: 5,
            total_duration_ns: 100,
            repair_count: 0,
        })
    }
}

struct UnavailableAnalyzer;

#[async_trait]
impl Analyzer for UnavailableAnalyzer {
    async fn identity(&self) -> Result<AnalyzerIdentity, ReviewMapFailure> {
        Ok(AnalyzerIdentity {
            model: "qwen3:8b".into(),
            model_digest: "sha256:model".into(),
            prompt_version: 1,
            generation_parameters: Vec::new(),
        })
    }

    async fn analyze(
        &self,
        _request: ramo_core::review_map::EnrichmentRequest,
    ) -> Result<AnalysisResult, ReviewMapFailure> {
        Err(ReviewMapFailure::new(
            ReviewMapFailureCode::OllamaUnavailable,
            "Ollama is offline",
        ))
    }
}

struct LowQualityAnalyzer;

#[async_trait]
impl Analyzer for LowQualityAnalyzer {
    async fn identity(&self) -> Result<AnalyzerIdentity, ReviewMapFailure> {
        Ok(AnalyzerIdentity {
            model: "qwen3:8b".into(),
            model_digest: "sha256:model".into(),
            prompt_version: 2,
            generation_parameters: Vec::new(),
        })
    }

    async fn analyze(
        &self,
        _request: ramo_core::review_map::EnrichmentRequest,
    ) -> Result<AnalysisResult, ReviewMapFailure> {
        Err(ReviewMapFailure::new(
            ReviewMapFailureCode::AnalysisLowQuality,
            "Local AI did not add reliable review guidance; the exact map is still ready",
        ))
    }
}

#[tokio::test]
async fn identical_requests_share_one_analysis_job() {
    let analyzer = CountingAnalyzer::blocked();
    let (_directory, coordinator) = coordinator(analyzer.clone());

    let first = coordinator.resolve(request()).await.unwrap();
    let second = coordinator.resolve(request()).await.unwrap();

    assert_eq!(first.job_id, second.job_id);
    analyzer.release();
    let finished = wait_terminal(&coordinator, &first.job_id).await;
    assert!(matches!(finished, JobState::Enriched));
    assert_eq!(analyzer.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_valid_cache_hit_skips_analysis() {
    let analyzer = CountingAnalyzer::blocked();
    let (_directory, coordinator) = coordinator(analyzer.clone());
    let first = coordinator.resolve(request()).await.unwrap();
    analyzer.release();
    assert!(matches!(
        wait_terminal(&coordinator, &first.job_id).await,
        JobState::Enriched
    ));

    let cached = coordinator.resolve(request()).await.unwrap();
    assert!(matches!(cached.state, JobState::Enriched));
    assert_eq!(analyzer.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_new_head_cancels_older_queued_work_before_model_execution() {
    let analyzer = CountingAnalyzer::blocked();
    let provider = Arc::new(MutableProvider {
        head: Mutex::new("head-1".into()),
    });
    let directory = tempfile::tempdir().unwrap();
    let cache = ReviewMapCache::new(
        directory.path(),
        CacheLimits {
            max_bytes: 1024 * 1024,
            max_age: Duration::from_secs(3600),
        },
    )
    .unwrap();
    let coordinator = AnalysisCoordinator::new(
        provider.clone(),
        Arc::new(analyzer.clone()),
        cache,
        CoordinatorConfig::default(),
    );
    let first = coordinator.resolve(request()).await.unwrap();
    wait_for_state(&coordinator, &first.job_id, |state| {
        matches!(state, JobState::Analyzing)
    })
    .await;

    *provider.head.lock().unwrap() = "head-2".into();
    let stale = coordinator.resolve(request()).await.unwrap();
    *provider.head.lock().unwrap() = "head-3".into();
    let newest = coordinator.resolve(request()).await.unwrap();

    let stale_snapshot = coordinator.job(&stale.job_id).await.unwrap();
    assert!(matches!(
        stale_snapshot.state,
        JobState::Unavailable(ref failure)
            if failure.code == ReviewMapFailureCode::ResultStale
    ));
    assert_eq!(stale_snapshot.map.status, ReviewMapStatus::Stale);

    analyzer.permit.add_permits(2);
    assert!(matches!(
        wait_terminal(&coordinator, &newest.job_id).await,
        JobState::Enriched
    ));
    assert_eq!(analyzer.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn analyzer_failures_become_terminal_job_states_without_panicking() {
    let directory = tempfile::tempdir().unwrap();
    let cache = ReviewMapCache::new(
        directory.path(),
        CacheLimits {
            max_bytes: 1024 * 1024,
            max_age: Duration::from_secs(3600),
        },
    )
    .unwrap();
    let coordinator = AnalysisCoordinator::new(
        Arc::new(FakeProvider),
        Arc::new(UnavailableAnalyzer),
        cache,
        CoordinatorConfig::default(),
    );

    let result = coordinator.resolve(request()).await.unwrap();
    let state = wait_terminal(&coordinator, &result.job_id).await;

    assert!(matches!(
        state,
        JobState::Unavailable(failure)
            if failure.code == ReviewMapFailureCode::OllamaUnavailable
    ));
}

#[tokio::test]
async fn low_quality_analysis_keeps_the_exact_map_and_does_not_cache() {
    let directory = tempfile::tempdir().unwrap();
    let cache = ReviewMapCache::new(
        directory.path(),
        CacheLimits {
            max_bytes: 1024 * 1024,
            max_age: Duration::from_secs(3600),
        },
    )
    .unwrap();
    let coordinator = AnalysisCoordinator::new(
        Arc::new(FakeProvider),
        Arc::new(LowQualityAnalyzer),
        cache,
        CoordinatorConfig::default(),
    );

    let result = coordinator.resolve(request()).await.unwrap();
    let state = wait_terminal(&coordinator, &result.job_id).await;
    let snapshot = coordinator.job(&result.job_id).await.unwrap();

    assert!(matches!(
        state,
        JobState::Failed(failure)
            if failure.code == ReviewMapFailureCode::AnalysisLowQuality
                && failure.message.contains("exact map is still ready")
    ));
    assert_eq!(snapshot.map.status, ReviewMapStatus::Failed);
    assert_eq!(snapshot.map.totals.files, 1);
    assert_eq!(snapshot.map.files[0].path, "src/lib.rs");
    assert_eq!(directory.path().read_dir().unwrap().count(), 0);
}

async fn wait_terminal(coordinator: &AnalysisCoordinator, id: &AnalysisJobId) -> JobState {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let state = coordinator.job(id).await.unwrap().state;
            if !matches!(state, JobState::Queued | JobState::Analyzing) {
                break state;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}

async fn wait_for_state(
    coordinator: &AnalysisCoordinator,
    id: &AnalysisJobId,
    matches: impl Fn(&JobState) -> bool,
) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let state = coordinator.job(id).await.unwrap().state;
            if matches(&state) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

fn coordinator(analyzer: CountingAnalyzer) -> (tempfile::TempDir, AnalysisCoordinator) {
    let directory = tempfile::tempdir().unwrap();
    let cache = ReviewMapCache::new(
        directory.path(),
        CacheLimits {
            max_bytes: 1024 * 1024,
            max_age: Duration::from_secs(3600),
        },
    )
    .unwrap();
    let coordinator = AnalysisCoordinator::new(
        Arc::new(FakeProvider),
        Arc::new(analyzer),
        cache,
        CoordinatorConfig::default(),
    );
    (directory, coordinator)
}

fn request() -> ResolveRequest {
    ResolveRequest {
        key: PullRequestKey {
            repository: "owner/repo".into(),
            number: 7,
        },
        expected_head_sha: None,
    }
}

fn input(key: &PullRequestKey, head_sha: &str) -> ReviewMapInput {
    ReviewMapInput {
        identity: ReviewMapIdentity {
            repository: key.repository.clone(),
            pull_request: key.number,
            base_sha: "base".into(),
            head_sha: head_sha.into(),
        },
        files: vec![ReviewMapInputFile {
            path: "src/lib.rs".into(),
            previous_path: None,
            status: "modified".into(),
            additions: 3,
            deletions: 1,
            patch: Some("@@ -1 +1 @@\n-old\n+new".into()),
            binary: false,
        }],
        codeowners: None,
    }
}
