#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PullRequestKey {
    pub repository: String,
    pub number: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxKind {
    ReviewRequests,
    Authored,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PullRequestSummary {
    pub node_id: String,
    pub key: PullRequestKey,
    pub title: String,
    pub url: String,
    pub author_login: String,
    pub updated_at: String,
    pub is_draft: bool,
    pub additions: usize,
    pub deletions: usize,
    pub changed_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InboxPage {
    pub items: Vec<PullRequestSummary>,
    pub end_cursor: Option<String>,
    pub has_next_page: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: usize,
    pub deletions: usize,
    pub patch: Option<String>,
    pub viewed: bool,
    pub binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PullRequestSnapshot {
    pub node_id: String,
    pub context: crate::remote_review::PullRequestReviewContext,
    pub files: Vec<ChangedFile>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ConditionalCursor {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewNotification {
    pub id: String,
    pub key: PullRequestKey,
    pub title: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewNotificationPage {
    pub notifications: Vec<ReviewNotification>,
    pub cursor: ConditionalCursor,
    pub not_modified: bool,
}
