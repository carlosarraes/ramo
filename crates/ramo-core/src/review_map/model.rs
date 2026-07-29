pub const REVIEW_MAP_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewMapStatus {
    Ready,
    Analyzing,
    Enriched,
    Stale,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewMapFailureCode {
    ServerUnreachable,
    PairingRejected,
    ClientUnauthorized,
    GithubAuthUnavailable,
    GithubRequestFailed,
    PullRequestUnavailable,
    OllamaUnavailable,
    ModelMissing,
    AnalysisTimedOut,
    AnalysisInvalid,
    AnalysisFailed,
    ResultStale,
    CacheUnavailable,
    ServerIncompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewFileKind {
    Authored,
    Test,
    Generated,
    Migration,
    Documentation,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchCoverage {
    Full,
    Truncated,
    MetadataOnly,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewMapIdentity {
    pub repository: String,
    pub pull_request: u64,
    pub base_sha: String,
    pub head_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewMapInputFile {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
    pub patch: Option<String>,
    pub binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewMapInput {
    pub identity: ReviewMapIdentity,
    pub files: Vec<ReviewMapInputFile>,
    pub codeowners: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewMapTotals {
    pub files: usize,
    pub additions: usize,
    pub deletions: usize,
    pub authored: usize,
    pub tests: usize,
    pub generated: usize,
    pub migrations: usize,
    pub documentation: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileInsight {
    pub summary: String,
    pub risk: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GroupInsight {
    pub summary: String,
    pub risk: Option<String>,
    pub review_priority: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewMapFile {
    pub id: String,
    pub path: String,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
    pub kind: ReviewFileKind,
    pub owner: Option<String>,
    pub coverage: PatchCoverage,
    pub insight: Option<FileInsight>,
    pub recommended_order: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewMapGroup {
    pub id: String,
    pub label: String,
    pub kind: ReviewFileKind,
    pub file_ids: Vec<String>,
    pub additions: usize,
    pub deletions: usize,
    pub collapsed_by_default: bool,
    pub insight: Option<GroupInsight>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewMapAnalysis {
    pub model: String,
    pub prompt_version: u32,
    pub completed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewMap {
    pub schema_version: u16,
    pub identity: ReviewMapIdentity,
    pub status: ReviewMapStatus,
    pub totals: ReviewMapTotals,
    pub groups: Vec<ReviewMapGroup>,
    pub files: Vec<ReviewMapFile>,
    pub analysis: Option<ReviewMapAnalysis>,
}
