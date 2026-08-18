//! Review Map enrichment through the `pi` CLI.
//!
//! Swapped in for `OllamaAnalyzer` behind the same two-method `Analyzer` trait, so the cache,
//! batching, validation, repair pass, quality gate, and the Android path are all untouched.
//!
//! The one thing pi cannot do that Ollama could is constrain assistant text to a JSON Schema.
//! That capability is restored with a first-party extension exposing exactly one tool whose
//! parameter schema *is* the enrichment schema; `--no-builtin-tools` makes it the only tool the
//! model has, so a validated object is the only way for it to answer. See
//! `assets/review-map-extension.ts`.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use ramo_core::pi::{PiCli, PiError, PiRequest, PiSession, PiTools};
use ramo_core::process::SystemCommandExecutor;
use ramo_core::review_map::{EnrichmentProposal, EnrichmentRequest, ReviewMapFailureCode};

use crate::ReviewMapFailure;
use crate::analysis::{AnalysisBudget, AnalyzerIdentity, budget_batches};
use crate::ollama::client::{AnalysisResult, Analyzer};
use crate::ollama::prompt::{PROMPT_VERSION, repair_prompt, system_prompt, user_prompt};
use crate::ollama::schema::enrichment_schema;

/// Shipped beside the binary rather than fetched, so the tool contract is versioned with ramo.
const EXTENSION_SOURCE: &str = include_str!("../../assets/review-map-extension.ts");

#[derive(Debug, Clone)]
pub struct PiAnalyzer {
    provider: String,
    model: String,
    effort: String,
    timeout: Duration,
    budget: AnalysisBudget,
}

impl PiAnalyzer {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        effort: impl Into<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            effort: effort.into(),
            timeout,
            budget: AnalysisBudget::default(),
        }
    }

    /// pi exposes no model digest the way Ollama's `/api/tags` does, so the cache key is
    /// synthesized from everything that changes the output. It is coarser: a provider silently
    /// re-pointing a model name is invisible here, where a re-pulled Ollama tag was not.
    fn synthetic_digest(&self) -> String {
        format!(
            "pi:{}/{}/{}@{}",
            self.provider, self.model, self.effort, PROMPT_VERSION
        )
    }

    fn request(&self, prompt: String, workspace: &Workspace) -> PiRequest {
        PiRequest {
            provider: self.provider.clone(),
            model: self.model.clone(),
            thinking: self.effort.clone(),
            timeout: self.timeout,
            prompt,
            system_prompt: system_prompt().to_owned(),
            tools: PiTools::ExtensionsOnly(vec![workspace.extension.clone()]),
            session: PiSession::Ephemeral,
            env: vec![
                (
                    OsString::from("RAMO_REVIEW_MAP_SCHEMA"),
                    workspace.schema.clone().into_os_string(),
                ),
                (
                    OsString::from("RAMO_REVIEW_MAP_OUTPUT"),
                    workspace.output.clone().into_os_string(),
                ),
            ],
        }
    }

    /// One pi invocation: write the per-request schema, run, read back the tool arguments.
    fn invoke(
        &self,
        request: &EnrichmentRequest,
        batch_results: Option<&[EnrichmentProposal]>,
        repair: Option<&str>,
    ) -> Result<EnrichmentProposal, ReviewMapFailure> {
        let workspace = Workspace::new(&enrichment_schema(request))?;
        let mut prompt = user_prompt(request, batch_results).map_err(|error| {
            ReviewMapFailure::new(ReviewMapFailureCode::AnalysisFailed, error.to_string())
        })?;
        if let Some(category) = repair {
            prompt.push_str("\n\n");
            prompt.push_str(&repair_prompt(category));
        }
        let outcome = PiCli::new(SystemCommandExecutor).run(&self.request(prompt, &workspace));
        match outcome {
            Ok(_) => {}
            // pi printing nothing is expected: the answer went to the output file, not stdout.
            Err(PiError::EmptyAnswer) => {}
            Err(error) => return Err(classify(error)),
        }
        workspace.read_proposal()
    }
}

#[async_trait]
impl Analyzer for PiAnalyzer {
    async fn identity(&self) -> Result<AnalyzerIdentity, ReviewMapFailure> {
        // Mirrors Ollama's pre-flight: fail before queueing work when the backend is absent.
        let available = tokio::task::spawn_blocking(probe_pi)
            .await
            .map_err(|error| {
                ReviewMapFailure::new(ReviewMapFailureCode::AnalysisFailed, error.to_string())
            })?;
        available?;
        Ok(AnalyzerIdentity {
            model: format!("{}/{}", self.provider, self.model),
            model_digest: self.synthetic_digest(),
            prompt_version: PROMPT_VERSION,
            generation_parameters: vec![("effort".to_owned(), self.effort.clone())],
        })
    }

    async fn analyze(
        &self,
        request: EnrichmentRequest,
    ) -> Result<AnalysisResult, ReviewMapFailure> {
        let analyzer = self.clone();
        let digest = self.synthetic_digest();
        let model = format!("{}/{}", self.provider, self.model);
        tokio::task::spawn_blocking(move || analyzer.analyze_blocking(request, model, digest))
            .await
            .map_err(|error| {
                ReviewMapFailure::new(ReviewMapFailureCode::AnalysisFailed, error.to_string())
            })?
    }
}

impl PiAnalyzer {
    fn analyze_blocking(
        &self,
        request: EnrichmentRequest,
        model: String,
        digest: String,
    ) -> Result<AnalysisResult, ReviewMapFailure> {
        let batches = budget_batches(&request, &self.budget, |batch| {
            crate::ollama::estimate_prompt_tokens(batch, None).unwrap_or(usize::MAX)
        });
        let (proposal, repair_count) = if batches.len() == 1 {
            self.attempt(&batches[0], None)?
        } else {
            let mut results = Vec::with_capacity(batches.len());
            let mut repairs = 0;
            for batch in &batches {
                let (proposal, repaired) = self.attempt(batch, None)?;
                repairs = repairs.max(repaired);
                results.push(proposal);
            }
            let mut synthesis = request.clone();
            for file in &mut synthesis.files {
                file.patch = None;
            }
            let (proposal, repaired) = self.attempt(&synthesis, Some(&results))?;
            (proposal, repairs.max(repaired))
        };
        Ok(AnalysisResult {
            proposal,
            model,
            model_digest: digest,
            // pi reports no token accounting, so the benchmark metrics read zero here.
            prompt_eval_count: 0,
            eval_count: 0,
            total_duration_ns: 0,
            repair_count,
        })
    }

    /// First attempt, then one repair. The tool schema already rejects malformed shapes, so a
    /// repair here is almost always a semantic rejection from the quality gate.
    fn attempt(
        &self,
        request: &EnrichmentRequest,
        batch_results: Option<&[EnrichmentProposal]>,
    ) -> Result<(EnrichmentProposal, u8), ReviewMapFailure> {
        let first = self.invoke(request, batch_results, None)?;
        match validate(request, first) {
            Ok(proposal) => Ok((proposal, 0)),
            Err(category) => {
                let second = self.invoke(request, batch_results, Some(&category))?;
                validate(request, second)
                    .map(|proposal| (proposal, 1))
                    .map_err(|category| {
                        ReviewMapFailure::new(
                            ReviewMapFailureCode::AnalysisInvalid,
                            format!("The model could not produce a valid review map ({category})"),
                        )
                    })
            }
        }
    }
}

fn validate(
    request: &EnrichmentRequest,
    mut proposal: EnrichmentProposal,
) -> Result<EnrichmentProposal, String> {
    // Coverage is the server's own truth and is never taken from the model.
    proposal.coverage = request.coverage.clone();
    ramo_core::review_map::validate_enrichment(&validation_map(request), &proposal)
        .map_err(|error| format!("{error:?}"))?;
    ramo_core::review_map::validate_enrichment_quality(request, &proposal)
        .map_err(|issue| format!("{issue:?}"))?;
    Ok(proposal)
}

fn classify(error: PiError) -> ReviewMapFailure {
    let (code, message) = match error {
        // Reuses the existing wire codes rather than renaming them: they are part of the
        // versioned contract the Android client already understands.
        PiError::MissingCli => (
            ReviewMapFailureCode::OllamaUnavailable,
            "The pi CLI was not found on PATH".to_owned(),
        ),
        PiError::ModelRejected { .. } => (ReviewMapFailureCode::ModelMissing, error.to_string()),
        PiError::TimedOut { .. } => (ReviewMapFailureCode::AnalysisTimedOut, error.to_string()),
        other => (ReviewMapFailureCode::AnalysisFailed, other.to_string()),
    };
    ReviewMapFailure::new(code, message)
}

fn probe_pi() -> Result<(), ReviewMapFailure> {
    use ramo_core::process::{CaptureLimits, CommandExecutor, CommandRequest};
    let result = SystemCommandExecutor
        .execute(CommandRequest {
            argv: vec![OsString::from("pi"), OsString::from("--version")],
            env: Vec::new(),
            stdin: None,
            inherit_stdio: false,
            limits: Some(CaptureLimits::new(4096, 4096, Duration::from_secs(10))),
        })
        .map_err(|error| {
            ReviewMapFailure::new(
                ReviewMapFailureCode::OllamaUnavailable,
                format!("The pi CLI was not found on PATH: {error}"),
            )
        })?;
    if result.code == Some(0) {
        Ok(())
    } else {
        Err(ReviewMapFailure::new(
            ReviewMapFailureCode::OllamaUnavailable,
            "The pi CLI is installed but did not run",
        ))
    }
}

/// Per-invocation scratch files. Dropped with the struct, so a crashed run leaves nothing
/// behind containing patch text.
struct Workspace {
    _dir: tempfile::TempDir,
    extension: PathBuf,
    schema: PathBuf,
    output: PathBuf,
}

impl Workspace {
    fn new(schema: &serde_json::Value) -> Result<Self, ReviewMapFailure> {
        let io = |error: std::io::Error| {
            ReviewMapFailure::new(ReviewMapFailureCode::AnalysisFailed, error.to_string())
        };
        let dir = tempfile::tempdir().map_err(io)?;
        let extension = dir.path().join("review-map-extension.ts");
        let schema_path = dir.path().join("schema.json");
        let output = dir.path().join("proposal.json");
        std::fs::write(&extension, EXTENSION_SOURCE).map_err(io)?;
        std::fs::write(
            &schema_path,
            serde_json::to_vec(schema).map_err(|error| {
                ReviewMapFailure::new(ReviewMapFailureCode::AnalysisFailed, error.to_string())
            })?,
        )
        .map_err(io)?;
        Ok(Self {
            _dir: dir,
            extension,
            schema: schema_path,
            output,
        })
    }

    fn read_proposal(&self) -> Result<EnrichmentProposal, ReviewMapFailure> {
        let raw = std::fs::read_to_string(&self.output).map_err(|_| {
            ReviewMapFailure::new(
                ReviewMapFailureCode::AnalysisInvalid,
                "The model answered without calling submit_review_map",
            )
        })?;
        serde_json::from_str(&raw).map_err(|error| {
            ReviewMapFailure::new(
                ReviewMapFailureCode::AnalysisInvalid,
                format!("The submitted review map did not match the schema: {error}"),
            )
        })
    }
}

fn validation_map(request: &EnrichmentRequest) -> ramo_core::review_map::ReviewMap {
    crate::ollama::client::validation_map(request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analyzer() -> PiAnalyzer {
        PiAnalyzer::new(
            "openai-codex",
            "gpt-5.6-luna",
            "max",
            Duration::from_secs(180),
        )
    }

    #[test]
    fn the_synthetic_digest_covers_everything_that_changes_the_output() {
        let base = analyzer().synthetic_digest();
        assert!(base.contains("openai-codex"), "{base}");
        assert!(base.contains("gpt-5.6-luna"), "{base}");
        assert!(base.ends_with(&format!("@{PROMPT_VERSION}")), "{base}");

        // Each dimension must move the cache key, or a stale map would be served.
        for changed in [
            PiAnalyzer::new(
                "cursor-agent",
                "gpt-5.6-luna",
                "max",
                Duration::from_secs(1),
            ),
            PiAnalyzer::new("openai-codex", "gpt-5.6-sol", "max", Duration::from_secs(1)),
            PiAnalyzer::new(
                "openai-codex",
                "gpt-5.6-luna",
                "low",
                Duration::from_secs(1),
            ),
        ] {
            assert_ne!(base, changed.synthetic_digest());
        }
    }

    #[test]
    fn the_extension_is_the_only_tool_and_the_session_is_ephemeral() {
        let schema = serde_json::json!({"type": "object"});
        let workspace = Workspace::new(&schema).unwrap();
        let request = analyzer().request("prompt".into(), &workspace);

        // `--no-builtin-tools` rather than `--no-tools`: the extension must survive.
        match &request.tools {
            PiTools::ExtensionsOnly(paths) => {
                assert_eq!(paths, std::slice::from_ref(&workspace.extension));
            }
            other => panic!("expected an extension-only tool set, got {other:?}"),
        }
        assert_eq!(request.session, PiSession::Ephemeral);

        let env: Vec<_> = request
            .env
            .iter()
            .map(|(key, _)| key.to_string_lossy().into_owned())
            .collect();
        assert!(
            env.contains(&"RAMO_REVIEW_MAP_SCHEMA".to_owned()),
            "{env:?}"
        );
        assert!(
            env.contains(&"RAMO_REVIEW_MAP_OUTPUT".to_owned()),
            "{env:?}"
        );
    }

    #[test]
    fn the_workspace_writes_the_extension_and_schema_and_reads_the_result_back() {
        let schema = serde_json::json!({"type": "object", "properties": {}});
        let workspace = Workspace::new(&schema).unwrap();

        let extension = std::fs::read_to_string(&workspace.extension).unwrap();
        assert!(extension.contains("submit_review_map"), "{extension}");
        assert_eq!(
            std::fs::read_to_string(&workspace.schema).unwrap(),
            schema.to_string()
        );

        // Nothing written means the model answered with prose instead of calling the tool.
        let failure = workspace.read_proposal().unwrap_err();
        assert_eq!(failure.code, ReviewMapFailureCode::AnalysisInvalid);
        assert!(
            failure.message.contains("submit_review_map"),
            "{}",
            failure.message
        );
    }

    #[test]
    fn pi_failures_map_onto_the_existing_wire_codes() {
        // Renaming these would bump REVIEW_MAP_SCHEMA_VERSION and break the Android client.
        assert_eq!(
            classify(PiError::MissingCli).code,
            ReviewMapFailureCode::OllamaUnavailable
        );
        assert_eq!(
            classify(PiError::ModelRejected {
                provider: "openai-codex".into(),
                model: "bogus".into(),
                stderr: "not found".into(),
            })
            .code,
            ReviewMapFailureCode::ModelMissing
        );
        assert_eq!(
            classify(PiError::TimedOut { seconds: 180 }).code,
            ReviewMapFailureCode::AnalysisTimedOut
        );
        assert_eq!(
            classify(PiError::Truncated).code,
            ReviewMapFailureCode::AnalysisFailed
        );
    }
}
