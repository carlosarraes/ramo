mod budget;
mod coordinator;

pub use budget::{AnalysisBudget, budget_batches};
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
