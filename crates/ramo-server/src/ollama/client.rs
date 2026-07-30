use std::time::Duration;

use async_trait::async_trait;
use ramo_core::review_map::{
    EnrichmentProposal, EnrichmentQualityIssue, EnrichmentRequest, ReviewMap, ReviewMapFailureCode,
    ReviewMapFile, ReviewMapStatus, ReviewMapTotals, validate_enrichment,
    validate_enrichment_quality,
};
use reqwest::{StatusCode, Url};

use crate::ReviewMapFailure;
use crate::analysis::{
    AnalysisBudget, AnalyzerIdentity, OLLAMA_CONTEXT_TOKENS, OLLAMA_OUTPUT_TOKENS, budget_batches,
    estimate_tokens,
};

use super::prompt::{repair_prompt, system_prompt, user_prompt};
use super::schema::enrichment_schema;

#[async_trait]
pub trait Analyzer: Send + Sync {
    async fn identity(&self) -> Result<AnalyzerIdentity, ReviewMapFailure>;

    async fn analyze(&self, request: EnrichmentRequest)
    -> Result<AnalysisResult, ReviewMapFailure>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisResult {
    pub proposal: EnrichmentProposal,
    pub model: String,
    pub model_digest: String,
    pub prompt_eval_count: u64,
    pub eval_count: u64,
    pub total_duration_ns: u64,
    pub repair_count: u8,
}

#[derive(Debug, Clone)]
pub struct OllamaAnalyzer {
    base_url: String,
    model: String,
    timeout: Duration,
    budget: AnalysisBudget,
    http: Result<reqwest::Client, String>,
}

impl OllamaAnalyzer {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>, timeout: Duration) -> Self {
        Self {
            base_url: base_url.into(),
            model: model.into(),
            timeout,
            budget: AnalysisBudget::default(),
            http: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|error| error.to_string()),
        }
    }

    pub fn with_budget(mut self, budget: AnalysisBudget) -> Self {
        self.budget = budget;
        self
    }

    async fn analyze_with_digest(
        &self,
        request: EnrichmentRequest,
        model_digest: &str,
    ) -> Result<AnalysisResult, ReviewMapFailure> {
        let batches = budget_batches(&request, &self.budget, |batch| {
            estimate_prompt_tokens(batch, None).unwrap_or(usize::MAX)
        });
        if batches.len() == 1 {
            return self.analyze_request(&batches[0], None, model_digest).await;
        }

        let mut results = Vec::with_capacity(batches.len());
        let mut totals = Metrics::default();
        for batch in batches {
            let result = self.analyze_request(&batch, None, model_digest).await?;
            totals.add_result(&result);
            results.push(result.proposal);
        }

        let mut synthesis_request = request;
        for file in &mut synthesis_request.files {
            file.patch = None;
        }
        if estimate_prompt_tokens(&synthesis_request, Some(&results))
            .map_or(true, |tokens| tokens > self.budget.max_prompt_tokens)
        {
            return Err(ReviewMapFailure::new(
                ReviewMapFailureCode::AnalysisInvalid,
                "The local analysis result is too large to synthesize safely",
            ));
        }
        let mut result = self
            .analyze_request(&synthesis_request, Some(&results), model_digest)
            .await?;
        result.prompt_eval_count += totals.prompt_eval_count;
        result.eval_count += totals.eval_count;
        result.total_duration_ns += totals.total_duration_ns;
        Ok(result)
    }

    async fn analyze_request(
        &self,
        request: &EnrichmentRequest,
        batch_results: Option<&[EnrichmentProposal]>,
        model_digest: &str,
    ) -> Result<AnalysisResult, ReviewMapFailure> {
        let first = self
            .request_once(request, batch_results, None, model_digest)
            .await?;
        match parse_and_validate(request, first) {
            Ok(result) => Ok(result),
            Err((rejection, first_metrics)) => {
                let category = rejection.repair_category();
                let second = self
                    .request_once(request, batch_results, Some(&category), model_digest)
                    .await?;
                parse_and_validate(request, second)
                    .map(|mut result| {
                        result.prompt_eval_count += first_metrics.prompt_eval_count;
                        result.eval_count += first_metrics.eval_count;
                        result.total_duration_ns += first_metrics.total_duration_ns;
                        result.repair_count = 1;
                        result
                    })
                    .map_err(|(rejection, _)| rejection.into_failure())
            }
        }
    }

    async fn request_once(
        &self,
        request: &EnrichmentRequest,
        batch_results: Option<&[EnrichmentProposal]>,
        repair_category: Option<&str>,
        model_digest: &str,
    ) -> Result<RawAnalysis, ReviewMapFailure> {
        let endpoint = local_chat_endpoint(&self.base_url)?;
        let mut messages = vec![
            serde_json::json!({ "role": "system", "content": system_prompt() }),
            serde_json::json!({
                "role": "user",
                "content": user_prompt(request, batch_results).map_err(|error| {
                    ReviewMapFailure::with_source(
                        ReviewMapFailureCode::AnalysisFailed,
                        "Could not prepare the local analysis request",
                        error,
                    )
                })?
            }),
        ];
        if let Some(category) = repair_category {
            messages.push(serde_json::json!({
                "role": "user",
                "content": repair_prompt(category),
            }));
        }
        let payload = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": false,
            "format": enrichment_schema(request),
            "options": {
                "temperature": 0,
                "seed": 42,
                "num_ctx": OLLAMA_CONTEXT_TOKENS,
                "num_predict": OLLAMA_OUTPUT_TOKENS,
            }
        });
        let http = self.http.as_ref().map_err(|_| {
            ReviewMapFailure::new(
                ReviewMapFailureCode::OllamaUnavailable,
                "Could not initialize the local Ollama client",
            )
        })?;
        let response = http
            .post(endpoint)
            .timeout(self.timeout)
            .json(&payload)
            .send()
            .await
            .map_err(map_transport_error)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(ReviewMapFailure::new(
                ReviewMapFailureCode::ModelMissing,
                format!("The local Ollama model '{}' is not installed", self.model),
            ));
        }
        if !response.status().is_success() {
            return Err(ReviewMapFailure::new(
                ReviewMapFailureCode::AnalysisFailed,
                format!("Ollama returned HTTP {}", response.status().as_u16()),
            ));
        }
        let response = response.json::<ChatResponse>().await.map_err(|error| {
            ReviewMapFailure::with_source(
                ReviewMapFailureCode::AnalysisInvalid,
                "Ollama returned a malformed response envelope",
                error,
            )
        })?;
        Ok(RawAnalysis {
            content: response.message.content,
            model: response.model,
            model_digest: model_digest.to_owned(),
            prompt_eval_count: response.prompt_eval_count,
            eval_count: response.eval_count,
            total_duration_ns: response.total_duration,
        })
    }

    async fn model_digest(&self) -> Result<String, ReviewMapFailure> {
        let endpoint = local_endpoint(&self.base_url, "/api/tags")?;
        let http = self.http.as_ref().map_err(|_| {
            ReviewMapFailure::new(
                ReviewMapFailureCode::OllamaUnavailable,
                "Could not initialize the local Ollama client",
            )
        })?;
        let response = http
            .get(endpoint)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(map_transport_error)?;
        if !response.status().is_success() {
            return Err(ReviewMapFailure::new(
                ReviewMapFailureCode::OllamaUnavailable,
                "Ollama could not list installed models",
            ));
        }
        let tags = response.json::<TagsResponse>().await.map_err(|error| {
            ReviewMapFailure::with_source(
                ReviewMapFailureCode::OllamaUnavailable,
                "Ollama returned a malformed model list",
                error,
            )
        })?;
        tags.models
            .into_iter()
            .find(|model| model.name == self.model || model.model == self.model)
            .map(|model| model.digest)
            .filter(|digest| !digest.trim().is_empty())
            .ok_or_else(|| {
                ReviewMapFailure::new(
                    ReviewMapFailureCode::ModelMissing,
                    format!("The local Ollama model '{}' is not installed", self.model),
                )
            })
    }
}

#[async_trait]
impl Analyzer for OllamaAnalyzer {
    async fn identity(&self) -> Result<AnalyzerIdentity, ReviewMapFailure> {
        Ok(AnalyzerIdentity {
            model: self.model.clone(),
            model_digest: self.model_digest().await?,
            prompt_version: super::PROMPT_VERSION,
            generation_parameters: vec![
                ("seed".into(), "42".into()),
                ("temperature".into(), "0".into()),
                ("num_ctx".into(), OLLAMA_CONTEXT_TOKENS.to_string()),
                ("num_predict".into(), OLLAMA_OUTPUT_TOKENS.to_string()),
            ],
        })
    }

    async fn analyze(
        &self,
        request: EnrichmentRequest,
    ) -> Result<AnalysisResult, ReviewMapFailure> {
        tokio::time::timeout(self.timeout, async {
            let model_digest = self.model_digest().await?;
            self.analyze_with_digest(request, &model_digest).await
        })
        .await
        .unwrap_or_else(|_| {
            Err(ReviewMapFailure::new(
                ReviewMapFailureCode::AnalysisTimedOut,
                "Local Review Map analysis timed out",
            ))
        })
    }
}

pub fn estimate_prompt_tokens(
    request: &EnrichmentRequest,
    batch_results: Option<&[EnrichmentProposal]>,
) -> Result<usize, serde_json::Error> {
    let payload = serde_json::json!({
        "messages": [
            { "role": "system", "content": system_prompt() },
            { "role": "user", "content": user_prompt(request, batch_results)? },
        ],
        "format": enrichment_schema(request),
    });
    Ok(estimate_tokens(serde_json::to_vec(&payload)?.len()))
}

#[derive(serde::Deserialize)]
struct ChatResponse {
    model: String,
    message: ChatMessage,
    #[serde(default)]
    prompt_eval_count: u64,
    #[serde(default)]
    eval_count: u64,
    #[serde(default)]
    total_duration: u64,
}

#[derive(serde::Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(serde::Deserialize)]
struct TagsResponse {
    models: Vec<TaggedModel>,
}

#[derive(serde::Deserialize)]
struct TaggedModel {
    name: String,
    model: String,
    digest: String,
}

struct RawAnalysis {
    content: String,
    model: String,
    model_digest: String,
    prompt_eval_count: u64,
    eval_count: u64,
    total_duration_ns: u64,
}

#[derive(Default)]
struct Metrics {
    prompt_eval_count: u64,
    eval_count: u64,
    total_duration_ns: u64,
}

impl Metrics {
    fn add_result(&mut self, result: &AnalysisResult) {
        self.prompt_eval_count += result.prompt_eval_count;
        self.eval_count += result.eval_count;
        self.total_duration_ns += result.total_duration_ns;
    }
}

fn parse_and_validate(
    request: &EnrichmentRequest,
    raw: RawAnalysis,
) -> Result<AnalysisResult, (AnalysisRejection, Metrics)> {
    let metrics = Metrics {
        prompt_eval_count: raw.prompt_eval_count,
        eval_count: raw.eval_count,
        total_duration_ns: raw.total_duration_ns,
    };
    let mut proposal = match serde_json::from_str::<EnrichmentProposal>(&raw.content) {
        Ok(proposal) => proposal,
        Err(_) => {
            return Err((
                AnalysisRejection::Invalid("invalid JSON or schema".into()),
                metrics,
            ));
        }
    };
    proposal.coverage = request.coverage.clone();
    if let Err(error) = validate_enrichment(&validation_map(request), &proposal) {
        return Err((
            AnalysisRejection::Invalid(validation_category(&error)),
            metrics,
        ));
    }
    if let Err(issues) = validate_enrichment_quality(request, &proposal) {
        return Err((AnalysisRejection::LowQuality(issues), metrics));
    }
    Ok(AnalysisResult {
        proposal,
        model: raw.model,
        model_digest: raw.model_digest,
        prompt_eval_count: raw.prompt_eval_count,
        eval_count: raw.eval_count,
        total_duration_ns: raw.total_duration_ns,
        repair_count: 0,
    })
}

enum AnalysisRejection {
    Invalid(String),
    LowQuality(Vec<EnrichmentQualityIssue>),
}

impl AnalysisRejection {
    fn repair_category(&self) -> String {
        match self {
            Self::Invalid(category) => category.clone(),
            Self::LowQuality(issues) => issues
                .iter()
                .map(|issue| issue.category())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(","),
        }
    }

    fn into_failure(self) -> ReviewMapFailure {
        match self {
            Self::Invalid(_) => ReviewMapFailure::new(
                ReviewMapFailureCode::AnalysisInvalid,
                "Ollama returned invalid structured analysis",
            ),
            Self::LowQuality(_) => ReviewMapFailure::new(
                ReviewMapFailureCode::AnalysisLowQuality,
                "Local AI did not add reliable review guidance; the exact map is still ready",
            ),
        }
    }
}

fn validation_map(request: &EnrichmentRequest) -> ReviewMap {
    let files = request
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| ReviewMapFile {
            id: format!("validation:{index}"),
            path: file.path.clone(),
            previous_path: None,
            status: "modified".into(),
            additions: file.additions,
            deletions: file.deletions,
            kind: file.kind,
            owner: None,
            coverage: file.coverage,
            insight: None,
            recommended_order: None,
        })
        .collect::<Vec<_>>();
    ReviewMap {
        schema_version: request.schema_version,
        identity: request.identity.clone(),
        status: ReviewMapStatus::Analyzing,
        totals: ReviewMapTotals {
            files: files.len(),
            additions: files.iter().map(|file| file.additions).sum(),
            deletions: files.iter().map(|file| file.deletions).sum(),
            ..ReviewMapTotals::default()
        },
        groups: Vec::new(),
        files,
        analysis: None,
    }
}

fn validation_category(error: &ramo_core::review_map::EnrichmentError) -> String {
    use ramo_core::review_map::EnrichmentError;
    match error {
        EnrichmentError::UnknownFile(_) => "unknown path",
        EnrichmentError::DuplicateFile(_) => "duplicate group or file assignment",
        EnrichmentError::DuplicateOrder(_) => "duplicate review order",
        EnrichmentError::FixedClassification(_) => "fixed classification changed",
        EnrichmentError::MissingFile(_) | EnrichmentError::MissingOrder(_) => "missing path",
        EnrichmentError::InvalidText { .. } => "invalid text bounds",
        EnrichmentError::DuplicateCoverage(_) => "invalid coverage",
        EnrichmentError::InvalidMerge(_) => "invalid exact facts",
    }
    .into()
}

fn local_chat_endpoint(base_url: &str) -> Result<Url, ReviewMapFailure> {
    local_endpoint(base_url, "/api/chat")
}

fn local_endpoint(base_url: &str, path: &str) -> Result<Url, ReviewMapFailure> {
    let mut url = Url::parse(base_url).map_err(|error| {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::OllamaUnavailable,
            "The Ollama URL is invalid",
            error,
        )
    })?;
    let host_is_local = url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    if !host_is_local {
        return Err(ReviewMapFailure::new(
            ReviewMapFailureCode::OllamaUnavailable,
            "Ollama must use a loopback URL",
        ));
    }
    url.set_path(path);
    url.set_query(None);
    Ok(url)
}

fn map_transport_error(error: reqwest::Error) -> ReviewMapFailure {
    if error.is_timeout() {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::AnalysisTimedOut,
            "Local Ollama analysis timed out",
            error,
        )
    } else {
        ReviewMapFailure::with_source(
            ReviewMapFailureCode::OllamaUnavailable,
            "Could not connect to local Ollama",
            error,
        )
    }
}
