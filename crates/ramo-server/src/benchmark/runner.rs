use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use ramo_core::github::PullRequestKey;
use ramo_core::review_map::{
    ClassifierConfig, EnrichmentProposal, REVIEW_MAP_SCHEMA_VERSION, ReviewMap, ReviewMapInput,
    build_review_map, validate_enrichment,
};
use sha2::{Digest, Sha256};

use crate::ReviewMapFailure;
use crate::analysis::{AnalysisBudget, AnalyzerIdentity};
use crate::github::PullRequestProvider;
use crate::ollama::{Analyzer, OllamaAnalyzer};

use super::corpus::write_private;
use super::{
    BenchmarkCase, BenchmarkManifest, BenchmarkRun, CandidateMeasurement, CompletionState,
    benchmark_io, invalid, resources,
};

#[async_trait]
pub trait BenchmarkAnalyzerFactory: Send + Sync {
    async fn create(
        &self,
        model: &str,
        budget: AnalysisBudget,
    ) -> Result<Arc<dyn Analyzer>, ReviewMapFailure>;
}

#[derive(Debug, Clone)]
pub struct OllamaBenchmarkAnalyzerFactory {
    base_url: String,
    timeout: std::time::Duration,
}

impl OllamaBenchmarkAnalyzerFactory {
    pub fn new(base_url: impl Into<String>, timeout: std::time::Duration) -> Self {
        Self {
            base_url: base_url.into(),
            timeout,
        }
    }
}

#[async_trait]
impl BenchmarkAnalyzerFactory for OllamaBenchmarkAnalyzerFactory {
    async fn create(
        &self,
        model: &str,
        budget: AnalysisBudget,
    ) -> Result<Arc<dyn Analyzer>, ReviewMapFailure> {
        Ok(Arc::new(
            OllamaAnalyzer::new(&self.base_url, model, self.timeout).with_budget(budget),
        ))
    }
}

pub struct BenchmarkRunner {
    provider: Arc<dyn PullRequestProvider>,
    factory: Arc<dyn BenchmarkAnalyzerFactory>,
    run_directory: PathBuf,
    classifier: ClassifierConfig,
}

impl BenchmarkRunner {
    pub fn new(
        provider: Arc<dyn PullRequestProvider>,
        factory: Arc<dyn BenchmarkAnalyzerFactory>,
        run_directory: PathBuf,
    ) -> Self {
        Self {
            provider,
            factory,
            run_directory,
            classifier: ClassifierConfig::default(),
        }
    }

    pub async fn run(
        &self,
        manifest: &BenchmarkManifest,
        run: &mut BenchmarkRun,
    ) -> Result<(), ReviewMapFailure> {
        let candidates = self.prepare_candidates(manifest).await;
        let measurements_path = self.run_directory.join("measurements.jsonl");
        let run_path = self.run_directory.join("run.json");

        for pull_request in &manifest.pull_requests {
            let input = self
                .provider
                .load(&PullRequestKey {
                    repository: manifest.repository.clone(),
                    number: *pull_request,
                })
                .await?;
            let exact_map = build_review_map(&input, &self.classifier).map_err(|error| {
                ReviewMapFailure::with_source(
                    ramo_core::review_map::ReviewMapFailureCode::AnalysisFailed,
                    "Could not build benchmark Review Map input",
                    error,
                )
            })?;
            let request = crate::analysis::coordinator::enrichment_request(&input, &exact_map);
            let request_digest = digest_request(&request)?;

            for candidate in &candidates {
                let Some(identity) = candidate.identity.as_ref().ok() else {
                    self.record_failure(
                        manifest,
                        run,
                        &measurements_path,
                        *pull_request,
                        candidate,
                        &request_digest,
                    )?;
                    continue;
                };
                if run.is_completed(
                    *pull_request,
                    &candidate.model,
                    &identity.model_digest,
                    manifest.prompt_version,
                ) {
                    continue;
                }

                let started = Instant::now();
                let result = candidate
                    .analyzer
                    .as_ref()
                    .expect("identity requires analyzer")
                    .analyze(request.clone())
                    .await;
                let wall_time_ms = elapsed_millis(started);
                let measurement = match result {
                    Ok(result) => {
                        let semantic_valid =
                            validate_enrichment(&exact_map, &result.proposal).is_ok();
                        if semantic_valid {
                            self.save_private(
                                *pull_request,
                                &candidate.id,
                                &input,
                                &exact_map,
                                &result.proposal,
                            )?;
                        }
                        CandidateMeasurement {
                            case: BenchmarkCase::new(*pull_request),
                            candidate_id: candidate.id.clone(),
                            model: candidate.model.clone(),
                            model_digest: result.model_digest,
                            prompt_version: manifest.prompt_version,
                            request_digest: request_digest.clone(),
                            wall_time_ms,
                            ollama_total_duration_ns: result.total_duration_ns,
                            prompt_eval_count: result.prompt_eval_count,
                            eval_count: result.eval_count,
                            schema_valid: true,
                            semantic_valid,
                            repair_count: result.repair_count,
                            unknown_reference_count: usize::from(!semantic_valid),
                            peak_rss_bytes: resources::peak_rss_bytes(),
                            completion: if semantic_valid {
                                CompletionState::Completed
                            } else {
                                CompletionState::Failed
                            },
                            failure_code: (!semantic_valid).then_some(
                                ramo_core::review_map::ReviewMapFailureCode::AnalysisInvalid,
                            ),
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "Benchmark PR #{} with {} failed ({:?}): {}",
                            pull_request, candidate.model, error.code, error.message
                        );
                        failed_measurement(
                            manifest,
                            *pull_request,
                            candidate,
                            identity,
                            &request_digest,
                            wall_time_ms,
                            error.code,
                        )
                    }
                };
                record(run, &run_path, &measurements_path, measurement)?;
            }
        }
        Ok(())
    }

    async fn prepare_candidates(&self, manifest: &BenchmarkManifest) -> Vec<PreparedCandidate> {
        let mut prepared = Vec::with_capacity(manifest.candidates.len());
        for (index, model) in manifest.candidates.iter().enumerate() {
            let analyzer = self.factory.create(model, manifest.budget.into()).await;
            let (analyzer, identity) = match analyzer {
                Ok(analyzer) => {
                    let identity = analyzer.identity().await;
                    (Some(analyzer), identity)
                }
                Err(error) => (None, Err(error)),
            };
            prepared.push(PreparedCandidate {
                id: format!("candidate-{}", index + 1),
                model: model.clone(),
                analyzer,
                identity,
            });
        }
        prepared
    }

    fn record_failure(
        &self,
        manifest: &BenchmarkManifest,
        run: &mut BenchmarkRun,
        measurements_path: &std::path::Path,
        pull_request: u64,
        candidate: &PreparedCandidate,
        request_digest: &str,
    ) -> Result<(), ReviewMapFailure> {
        let measurement = CandidateMeasurement {
            case: BenchmarkCase::new(pull_request),
            candidate_id: candidate.id.clone(),
            model: candidate.model.clone(),
            model_digest: String::new(),
            prompt_version: manifest.prompt_version,
            request_digest: request_digest.into(),
            wall_time_ms: 0,
            ollama_total_duration_ns: 0,
            prompt_eval_count: 0,
            eval_count: 0,
            schema_valid: false,
            semantic_valid: false,
            repair_count: 0,
            unknown_reference_count: 0,
            peak_rss_bytes: resources::peak_rss_bytes(),
            completion: CompletionState::Failed,
            failure_code: candidate.identity.as_ref().err().map(|error| error.code),
        };
        record(
            run,
            &self.run_directory.join("run.json"),
            measurements_path,
            measurement,
        )
    }

    fn save_private(
        &self,
        pull_request: u64,
        candidate_id: &str,
        input: &ReviewMapInput,
        exact_map: &ReviewMap,
        proposal: &EnrichmentProposal,
    ) -> Result<(), ReviewMapFailure> {
        let artifact = PrivateCandidateOutput {
            input,
            exact_map,
            proposal,
        };
        let bytes = serde_json::to_vec_pretty(&artifact)
            .map_err(|error| benchmark_io("Could not serialize private benchmark output", error))?;
        write_private(
            &self
                .run_directory
                .join("private")
                .join(pull_request.to_string())
                .join(format!("{candidate_id}.json")),
            &bytes,
        )
    }
}

struct PreparedCandidate {
    id: String,
    model: String,
    analyzer: Option<Arc<dyn Analyzer>>,
    identity: Result<AnalyzerIdentity, ReviewMapFailure>,
}

#[derive(serde::Serialize)]
struct PrivateCandidateOutput<'a> {
    input: &'a ReviewMapInput,
    exact_map: &'a ReviewMap,
    proposal: &'a EnrichmentProposal,
}

fn failed_measurement(
    manifest: &BenchmarkManifest,
    pull_request: u64,
    candidate: &PreparedCandidate,
    identity: &AnalyzerIdentity,
    request_digest: &str,
    wall_time_ms: u64,
    failure_code: ramo_core::review_map::ReviewMapFailureCode,
) -> CandidateMeasurement {
    CandidateMeasurement {
        case: BenchmarkCase::new(pull_request),
        candidate_id: candidate.id.clone(),
        model: candidate.model.clone(),
        model_digest: identity.model_digest.clone(),
        prompt_version: manifest.prompt_version,
        request_digest: request_digest.into(),
        wall_time_ms,
        ollama_total_duration_ns: 0,
        prompt_eval_count: 0,
        eval_count: 0,
        schema_valid: false,
        semantic_valid: false,
        repair_count: 0,
        unknown_reference_count: 0,
        peak_rss_bytes: resources::peak_rss_bytes(),
        completion: CompletionState::Failed,
        failure_code: Some(failure_code),
    }
}

fn record(
    run: &mut BenchmarkRun,
    run_path: &std::path::Path,
    measurements_path: &std::path::Path,
    measurement: CandidateMeasurement,
) -> Result<(), ReviewMapFailure> {
    BenchmarkRun::append_measurement(measurements_path, &measurement)?;
    run.record(measurement);
    run.save(run_path)
}

fn digest_request(
    request: &ramo_core::review_map::EnrichmentRequest,
) -> Result<String, ReviewMapFailure> {
    if request.schema_version != REVIEW_MAP_SCHEMA_VERSION {
        return Err(invalid(
            "Benchmark request schema does not match this Ramo build",
        ));
    }
    let bytes = serde_json::to_vec(request)
        .map_err(|error| benchmark_io("Could not hash benchmark request", error))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
