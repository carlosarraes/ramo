use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ramo_core::github::PullRequestKey;
use ramo_core::review_map::{
    EnrichmentCoverage, EnrichmentProposal, ProposedGroup, ReviewFileKind, ReviewMapIdentity,
    ReviewMapInput, ReviewMapInputFile,
};
use ramo_server::ReviewMapFailure;
use ramo_server::analysis::{AnalysisCoordinator, AnalyzerIdentity, CoordinatorConfig};
use ramo_server::api::{
    ClientCredential, HealthStatus, PairingState, ReviewMapClientTokenStore, ServerState,
};
use ramo_server::cache::{CacheLimits, ReviewMapCache};
use ramo_server::github::PullRequestProvider;
use ramo_server::ollama::{AnalysisResult, Analyzer};

struct Provider;

#[async_trait]
impl PullRequestProvider for Provider {
    async fn load(&self, key: &PullRequestKey) -> Result<ReviewMapInput, ReviewMapFailure> {
        Ok(ReviewMapInput {
            identity: ReviewMapIdentity {
                repository: key.repository.clone(),
                pull_request: key.number,
                base_sha: "base".into(),
                head_sha: "head".into(),
            },
            files: vec![ReviewMapInputFile {
                path: "src/lib.rs".into(),
                previous_path: None,
                status: "modified".into(),
                additions: 3,
                deletions: 1,
                patch: Some("@@ secret patch body".into()),
                binary: false,
            }],
            codeowners: None,
        })
    }
}

struct ImmediateAnalyzer;

#[async_trait]
impl Analyzer for ImmediateAnalyzer {
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
        request: ramo_core::review_map::EnrichmentRequest,
    ) -> Result<AnalysisResult, ReviewMapFailure> {
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

pub fn state() -> (tempfile::TempDir, ServerState, ClientCredential) {
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
        Arc::new(Provider),
        Arc::new(ImmediateAnalyzer),
        cache,
        CoordinatorConfig::default(),
    );
    let tokens = ReviewMapClientTokenStore::default();
    let credential = tokens.issue("test phone").unwrap();
    let pairing = PairingState::new(tokens.clone());
    (
        directory,
        ServerState {
            coordinator,
            tokens,
            pairing,
            health: HealthStatus::healthy("qwen3:8b"),
        },
        credential,
    )
}

pub fn create_body() -> serde_json::Value {
    serde_json::json!({
        "repository": "owner/repo",
        "pull_request": 7,
        "expected_head_sha": "head"
    })
}
