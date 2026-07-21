use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSelector {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDescriptor {
    pub session_id: String,
    pub pid: u32,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
    pub launched_at: String,
    pub input_kind: String,
    pub title: String,
    pub source_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFileSummary {
    pub id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub additions: usize,
    pub deletions: usize,
    pub hunk_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReviewHunk {
    pub index: usize,
    pub header: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_range: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_range: Option<[u32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRegistrationFile {
    #[serde(flatten)]
    pub summary: SessionFileSummary,
    pub patch: String,
    pub hunks: Vec<SessionReviewHunk>,
}

impl std::ops::Deref for SessionRegistrationFile {
    type Target = SessionFileSummary;

    fn deref(&self) -> &Self::Target {
        &self.summary
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRegistration {
    pub registration_version: u32,
    #[serde(flatten)]
    pub descriptor: SessionDescriptor,
    pub files: Vec<SessionRegistrationFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedHunkSummary {
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_range: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_range: Option<[u32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLiveCommentSummary {
    pub comment_id: String,
    pub file_path: String,
    pub hunk_index: usize,
    pub side: String,
    pub line: u32,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReviewNoteSummary {
    pub note_id: String,
    pub source: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hunk_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_range: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_range: Option<[u32; 2]>,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    pub editable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshotState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_file_path: Option<String>,
    pub selected_hunk_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_hunk_old_range: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_hunk_new_range: Option<[u32; 2]>,
    pub show_agent_notes: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_markup_width: Option<u16>,
    pub live_comment_count: usize,
    pub live_comments: Vec<SessionLiveCommentSummary>,
    pub review_note_count: usize,
    pub review_notes: Vec<SessionReviewNoteSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub updated_at: String,
    pub state: SessionSnapshotState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedSessionContext {
    pub session_id: String,
    pub title: String,
    pub source_label: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
    pub input_kind: String,
    pub selected_file: Option<SessionFileSummary>,
    pub selected_hunk: Option<SelectedHunkSummary>,
    pub show_agent_notes: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_markup_width: Option<u16>,
    pub live_comment_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReviewFile {
    #[serde(flatten)]
    pub summary: SessionFileSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    pub hunks: Vec<SessionReviewHunk>,
}

impl std::ops::Deref for SessionReviewFile {
    type Target = SessionFileSummary;

    fn deref(&self) -> &Self::Target {
        &self.summary
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReview {
    pub session_id: String,
    pub title: String,
    pub source_label: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
    pub input_kind: String,
    pub selected_file: Option<SessionReviewFile>,
    pub selected_hunk: Option<SessionReviewHunk>,
    pub show_agent_notes: bool,
    pub live_comment_count: usize,
    pub review_note_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_notes: Option<Vec<SessionReviewNoteSummary>>,
    pub files: Vec<SessionReviewFile>,
}
