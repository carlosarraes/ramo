use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ramo_core::github::PullRequestKey;
use ramo_core::review_map::{
    ClassifierConfig, EnrichmentCoverage, EnrichmentExactGroup, EnrichmentInputFile,
    EnrichmentRequest, PatchCoverage, REVIEW_MAP_CLASSIFIER_VERSION, REVIEW_MAP_SCHEMA_VERSION,
    ReviewMap, ReviewMapAnalysis, ReviewMapCacheIdentity, ReviewMapFailureCode, ReviewMapInput,
    ReviewMapStatus, build_review_map, merge_enrichment, review_map_cache_key,
};
use tokio::sync::{Mutex, mpsc};

use crate::ReviewMapFailure;
use crate::cache::ReviewMapCache;
use crate::github::PullRequestProvider;
use crate::ollama::Analyzer;

use super::AnalyzerIdentity;

static JOB_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AnalysisJobId(pub String);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "failure")]
pub enum JobState {
    Queued,
    Analyzing,
    Enriched,
    Unavailable(ReviewMapFailure),
    Failed(ReviewMapFailure),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobSnapshot {
    pub job_id: AnalysisJobId,
    pub state: JobState,
    pub map: ReviewMap,
}

#[derive(Debug, Clone)]
pub struct ResolveRequest {
    pub key: PullRequestKey,
    pub expected_head_sha: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolveResult {
    pub job_id: AnalysisJobId,
    pub state: JobState,
    pub map: ReviewMap,
}

#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub classifier: ClassifierConfig,
    pub classifier_version: u32,
    pub schema_version: u16,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            classifier: ClassifierConfig::default(),
            classifier_version: REVIEW_MAP_CLASSIFIER_VERSION,
            schema_version: REVIEW_MAP_SCHEMA_VERSION,
        }
    }
}

#[derive(Clone)]
pub struct AnalysisCoordinator {
    inner: Arc<CoordinatorInner>,
}

struct CoordinatorInner {
    provider: Arc<dyn PullRequestProvider>,
    analyzer: Arc<dyn Analyzer>,
    cache: ReviewMapCache,
    config: CoordinatorConfig,
    state: Mutex<CoordinatorState>,
    sender: mpsc::Sender<AnalysisWork>,
}

#[derive(Default)]
struct CoordinatorState {
    jobs: HashMap<AnalysisJobId, JobRecord>,
    in_flight: HashMap<String, AnalysisJobId>,
}

struct JobRecord {
    snapshot: JobSnapshot,
    cache_key: Option<String>,
    repository: String,
    pull_request: u64,
    head_sha: String,
}

struct AnalysisWork {
    job_id: AnalysisJobId,
    cache_identity: ReviewMapCacheIdentity,
    exact_map: ReviewMap,
    request: EnrichmentRequest,
}

impl AnalysisCoordinator {
    pub fn new(
        provider: Arc<dyn PullRequestProvider>,
        analyzer: Arc<dyn Analyzer>,
        cache: ReviewMapCache,
        config: CoordinatorConfig,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(64);
        let inner = Arc::new(CoordinatorInner {
            provider,
            analyzer,
            cache,
            config,
            state: Mutex::new(CoordinatorState::default()),
            sender,
        });
        tokio::spawn(worker(inner.clone(), receiver));
        Self { inner }
    }

    pub async fn resolve(
        &self,
        request: ResolveRequest,
    ) -> Result<ResolveResult, ReviewMapFailure> {
        let input = self.inner.provider.load(&request.key).await?;
        let exact_map =
            build_review_map(&input, &self.inner.config.classifier).map_err(|error| {
                ReviewMapFailure::with_source(
                    ReviewMapFailureCode::AnalysisFailed,
                    "Could not build the exact Review Map",
                    error,
                )
            })?;
        if request
            .expected_head_sha
            .as_deref()
            .is_some_and(|expected| expected != exact_map.identity.head_sha)
        {
            return Err(stale_failure());
        }
        let enrichment_request = enrichment_request(&input, &exact_map);
        let analyzer_identity = match self.inner.analyzer.identity().await {
            Ok(identity) => identity,
            Err(failure) => {
                return Ok(self
                    .record_terminal(exact_map, JobState::Unavailable(failure), None)
                    .await);
            }
        };
        let cache_identity = cache_identity(&exact_map, &analyzer_identity, &self.inner.config);
        if let Some(map) = self.inner.cache.get(&cache_identity)? {
            return Ok(self.record_terminal(map, JobState::Enriched, None).await);
        }
        let key = review_map_cache_key(&cache_identity);
        let mut state = self.inner.state.lock().await;
        if let Some(job_id) = state.in_flight.get(&key) {
            return Ok(resolve_result(&state.jobs[job_id].snapshot));
        }

        cancel_stale_queued(&mut state, &exact_map);
        let job_id = next_job_id();
        let mut visible_map = exact_map.clone();
        visible_map.status = ReviewMapStatus::Analyzing;
        let snapshot = JobSnapshot {
            job_id: job_id.clone(),
            state: JobState::Queued,
            map: visible_map,
        };
        state.in_flight.insert(key.clone(), job_id.clone());
        state.jobs.insert(
            job_id.clone(),
            JobRecord {
                snapshot: snapshot.clone(),
                cache_key: Some(key),
                repository: exact_map.identity.repository.clone(),
                pull_request: exact_map.identity.pull_request,
                head_sha: exact_map.identity.head_sha.clone(),
            },
        );
        drop(state);
        let work = AnalysisWork {
            job_id,
            cache_identity,
            exact_map,
            request: enrichment_request,
        };
        if self.inner.sender.send(work).await.is_err() {
            return Err(ReviewMapFailure::new(
                ReviewMapFailureCode::AnalysisFailed,
                "The local analysis worker is unavailable",
            ));
        }
        Ok(resolve_result(&snapshot))
    }

    pub async fn job(&self, id: &AnalysisJobId) -> Option<JobSnapshot> {
        self.inner
            .state
            .lock()
            .await
            .jobs
            .get(id)
            .map(|record| record.snapshot.clone())
    }

    pub async fn retry(&self, id: &AnalysisJobId) -> Result<ResolveResult, ReviewMapFailure> {
        let key = {
            let state = self.inner.state.lock().await;
            let record = state.jobs.get(id).ok_or_else(|| {
                ReviewMapFailure::new(
                    ReviewMapFailureCode::PullRequestUnavailable,
                    "The Review Map job was not found",
                )
            })?;
            if !matches!(
                record.snapshot.state,
                JobState::Unavailable(_) | JobState::Failed(_)
            ) {
                return Err(ReviewMapFailure::new(
                    ReviewMapFailureCode::AnalysisFailed,
                    "Only unavailable or failed Review Map jobs can be retried",
                ));
            }
            PullRequestKey {
                repository: record.repository.clone(),
                number: record.pull_request,
            }
        };
        self.resolve(ResolveRequest {
            key,
            expected_head_sha: None,
        })
        .await
    }

    async fn record_terminal(
        &self,
        mut map: ReviewMap,
        state_value: JobState,
        cache_key: Option<String>,
    ) -> ResolveResult {
        map.status = status_for(&state_value);
        let job_id = next_job_id();
        let snapshot = JobSnapshot {
            job_id: job_id.clone(),
            state: state_value,
            map,
        };
        let record = JobRecord {
            repository: snapshot.map.identity.repository.clone(),
            pull_request: snapshot.map.identity.pull_request,
            head_sha: snapshot.map.identity.head_sha.clone(),
            cache_key,
            snapshot: snapshot.clone(),
        };
        self.inner.state.lock().await.jobs.insert(job_id, record);
        resolve_result(&snapshot)
    }
}

async fn worker(inner: Arc<CoordinatorInner>, mut receiver: mpsc::Receiver<AnalysisWork>) {
    while let Some(work) = receiver.recv().await {
        {
            let mut state = inner.state.lock().await;
            let Some(record) = state.jobs.get_mut(&work.job_id) else {
                continue;
            };
            if !matches!(record.snapshot.state, JobState::Queued) {
                continue;
            }
            record.snapshot.state = JobState::Analyzing;
        }
        let result = inner.analyzer.analyze(work.request).await;
        let outcome = match result {
            Ok(result) if result.model_digest != work.cache_identity.model_digest => Err((
                JobState::Unavailable(stale_failure()),
                ReviewMapStatus::Stale,
            )),
            Ok(result) => match merge_enrichment(
                &work.exact_map,
                &result.proposal,
                ReviewMapAnalysis {
                    model: result.model,
                    prompt_version: work.cache_identity.prompt_version,
                    completed_at: completed_at(),
                },
            ) {
                Ok(map) => {
                    let _ = inner.cache.put(&work.cache_identity, &map);
                    Ok(map)
                }
                Err(error) => Err((
                    JobState::Failed(ReviewMapFailure::with_source(
                        ReviewMapFailureCode::AnalysisInvalid,
                        "Validated analysis could not be merged",
                        error,
                    )),
                    ReviewMapStatus::Failed,
                )),
            },
            Err(failure) => failure_outcome(failure),
        };

        let mut state = inner.state.lock().await;
        let cache_key = state
            .jobs
            .get(&work.job_id)
            .and_then(|record| record.cache_key.clone());
        if let Some(record) = state.jobs.get_mut(&work.job_id) {
            match outcome {
                Ok(map) => {
                    record.snapshot.map = map;
                    record.snapshot.state = JobState::Enriched;
                }
                Err((job_state, map_status)) => {
                    record.snapshot.map.status = map_status;
                    record.snapshot.state = job_state;
                }
            }
        }
        if let Some(cache_key) = cache_key {
            state.in_flight.remove(&cache_key);
        }
    }
}

fn failure_outcome(failure: ReviewMapFailure) -> Result<ReviewMap, (JobState, ReviewMapStatus)> {
    let unavailable = matches!(
        failure.code,
        ReviewMapFailureCode::OllamaUnavailable
            | ReviewMapFailureCode::ModelMissing
            | ReviewMapFailureCode::AnalysisTimedOut
    );
    if unavailable {
        Err((JobState::Unavailable(failure), ReviewMapStatus::Unavailable))
    } else {
        Err((JobState::Failed(failure), ReviewMapStatus::Failed))
    }
}

pub(crate) fn enrichment_request(input: &ReviewMapInput, map: &ReviewMap) -> EnrichmentRequest {
    let input_by_path = input
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<HashMap<_, _>>();
    let files_by_id = map
        .files
        .iter()
        .map(|file| (file.id.as_str(), file))
        .collect::<HashMap<_, _>>();
    let groups = map
        .groups
        .iter()
        .map(|group| EnrichmentExactGroup {
            id: group.id.clone(),
            label: group.label.clone(),
            kind: group.kind,
            paths: group
                .file_ids
                .iter()
                .map(|id| files_by_id[id.as_str()].path.clone())
                .collect(),
        })
        .collect();
    let files = map
        .files
        .iter()
        .map(|file| EnrichmentInputFile {
            path: file.path.clone(),
            kind: file.kind,
            additions: file.additions,
            deletions: file.deletions,
            coverage: file.coverage,
            patch: input_by_path
                .get(file.path.as_str())
                .and_then(|input| input.patch.clone()),
        })
        .collect::<Vec<_>>();
    let mut coverage = EnrichmentCoverage::default();
    for file in &files {
        match file.coverage {
            PatchCoverage::Full => coverage.analyzed_paths.push(file.path.clone()),
            PatchCoverage::Truncated => coverage.truncated_paths.push(file.path.clone()),
            PatchCoverage::MetadataOnly => coverage.metadata_only_paths.push(file.path.clone()),
            PatchCoverage::Binary => coverage.binary_paths.push(file.path.clone()),
        }
    }
    EnrichmentRequest {
        schema_version: map.schema_version,
        identity: map.identity.clone(),
        groups,
        files,
        coverage,
    }
}

fn cache_identity(
    map: &ReviewMap,
    analyzer: &AnalyzerIdentity,
    config: &CoordinatorConfig,
) -> ReviewMapCacheIdentity {
    ReviewMapCacheIdentity {
        repository: map.identity.repository.clone(),
        pull_request: map.identity.pull_request,
        head_sha: map.identity.head_sha.clone(),
        model: analyzer.model.clone(),
        model_digest: analyzer.model_digest.clone(),
        prompt_version: analyzer.prompt_version,
        schema_version: config.schema_version,
        classifier_version: config.classifier_version,
        generation_parameters: analyzer.generation_parameters.clone(),
    }
}

fn cancel_stale_queued(state: &mut CoordinatorState, map: &ReviewMap) {
    let stale_ids = state
        .jobs
        .iter()
        .filter_map(|(id, record)| {
            (record.repository == map.identity.repository
                && record.pull_request == map.identity.pull_request
                && record.head_sha != map.identity.head_sha
                && matches!(record.snapshot.state, JobState::Queued))
            .then_some(id.clone())
        })
        .collect::<Vec<_>>();
    for id in stale_ids {
        let key = state
            .jobs
            .get(&id)
            .and_then(|record| record.cache_key.clone());
        if let Some(record) = state.jobs.get_mut(&id) {
            record.snapshot.state = JobState::Unavailable(stale_failure());
            record.snapshot.map.status = ReviewMapStatus::Stale;
        }
        if let Some(key) = key {
            state.in_flight.remove(&key);
        }
    }
}

fn stale_failure() -> ReviewMapFailure {
    ReviewMapFailure::new(
        ReviewMapFailureCode::ResultStale,
        "A newer pull request revision superseded this analysis",
    )
}

fn status_for(state: &JobState) -> ReviewMapStatus {
    match state {
        JobState::Queued | JobState::Analyzing => ReviewMapStatus::Analyzing,
        JobState::Enriched => ReviewMapStatus::Enriched,
        JobState::Unavailable(_) => ReviewMapStatus::Unavailable,
        JobState::Failed(_) => ReviewMapStatus::Failed,
    }
}

fn resolve_result(snapshot: &JobSnapshot) -> ResolveResult {
    ResolveResult {
        job_id: snapshot.job_id.clone(),
        state: snapshot.state.clone(),
        map: snapshot.map.clone(),
    }
}

fn next_job_id() -> AnalysisJobId {
    AnalysisJobId(format!(
        "job-{}-{}",
        unix_seconds(),
        JOB_ID.fetch_add(1, Ordering::Relaxed)
    ))
}

fn completed_at() -> String {
    let seconds = unix_seconds();
    let days = seconds / 86_400;
    let day_seconds = seconds % 86_400;
    let (year, month, day) = civil_date(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        day_seconds / 3_600,
        day_seconds % 3_600 / 60,
        day_seconds % 60
    )
}

fn civil_date(days_since_epoch: i64) -> (i64, i64, i64) {
    let days = days_since_epoch + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
