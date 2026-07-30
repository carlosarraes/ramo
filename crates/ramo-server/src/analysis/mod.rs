mod budget;
pub(crate) mod coordinator;

pub use budget::{
    AnalysisBudget, MAX_PROMPT_TOKENS, OLLAMA_CONTEXT_TOKENS, OLLAMA_OUTPUT_TOKENS,
    OLLAMA_SAFETY_TOKENS, budget_batches, estimate_tokens,
};
pub use coordinator::{
    AnalysisCoordinator, AnalysisJobId, CoordinatorConfig, JobSnapshot, JobState, ResolveRequest,
    ResolveResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerIdentity {
    pub model: String,
    pub model_digest: String,
    pub prompt_version: u32,
    pub generation_parameters: Vec<(String, String)>,
}
