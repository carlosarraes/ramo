uniffi::setup_scaffolding!();

#[cfg(target_os = "android")]
mod android;
mod models;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Mutex;

pub use models::*;
use ramo_core::drafts::{DraftAnchor, create_draft as validate_draft};
use ramo_core::github::{
    InboxKind, InboxPage, PullRequestKey, PullRequestSnapshot, PullRequestSummary,
};
use ramo_core::remote_review::GithubReviewThread;
use ramo_core::remote_review::{
    InlineCommentTarget, RemoteLineSide, RemoteReviewComment, RemoteReviewRequest, ReviewVerdict,
};
use ramo_github::{GithubClient, GithubError, GithubErrorKind};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileViewer {
    pub login: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileInboxKind {
    ReviewRequests,
    Authored,
}

impl From<MobileInboxKind> for InboxKind {
    fn from(value: MobileInboxKind) -> Self {
        match value {
            MobileInboxKind::ReviewRequests => Self::ReviewRequests,
            MobileInboxKind::Authored => Self::Authored,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct MobilePullRequest {
    pub node_id: String,
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author_login: String,
    pub updated_at: String,
    pub is_draft: bool,
    pub additions: u64,
    pub deletions: u64,
    pub changed_files: u64,
}

impl From<PullRequestSummary> for MobilePullRequest {
    fn from(value: PullRequestSummary) -> Self {
        Self {
            node_id: value.node_id,
            repository: value.key.repository,
            number: value.key.number,
            title: value.title,
            url: value.url,
            author_login: value.author_login,
            updated_at: value.updated_at,
            is_draft: value.is_draft,
            additions: value.additions as u64,
            deletions: value.deletions as u64,
            changed_files: value.changed_files as u64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct MobileInboxPage {
    pub items: Vec<MobilePullRequest>,
    pub end_cursor: Option<String>,
    pub has_next_page: bool,
    pub warnings: Vec<String>,
}

impl From<InboxPage> for MobileInboxPage {
    fn from(value: InboxPage) -> Self {
        Self {
            items: value.items.into_iter().map(Into::into).collect(),
            end_cursor: value.end_cursor,
            has_next_page: value.has_next_page,
            warnings: value.warnings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct MobileInboxCache {
    pub review_requests: MobileInboxPage,
    pub authored: MobileInboxPage,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileReviewNotification {
    pub id: String,
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileNotificationPage {
    pub notifications: Vec<MobileReviewNotification>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub not_modified: bool,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("GitHub rejected this token")]
    InvalidCredentials,
    #[error("Organization access is not active")]
    AccessUnavailable,
    #[error("GitHub rate limit exceeded")]
    RateLimited,
    #[error("Could not reach GitHub")]
    Network,
    #[error("GitHub returned an unexpected response")]
    Unexpected,
    #[error("The pull request changed while you were reviewing")]
    StaleRevision,
    #[error("This review is not valid")]
    Validation,
}

impl From<GithubError> for MobileError {
    fn from(error: GithubError) -> Self {
        match error.kind() {
            GithubErrorKind::InvalidCredentials => Self::InvalidCredentials,
            GithubErrorKind::Forbidden => Self::AccessUnavailable,
            GithubErrorKind::RateLimited { .. } => Self::RateLimited,
            GithubErrorKind::Transport => Self::Network,
            GithubErrorKind::StaleRevision { .. } => Self::StaleRevision,
            GithubErrorKind::Validation => Self::Validation,
            _ => Self::Unexpected,
        }
    }
}

#[derive(uniffi::Object)]
pub struct MobileSession {
    client: GithubClient,
    snapshots: Mutex<HashMap<String, PullRequestSnapshot>>,
    threads: Mutex<HashMap<String, Vec<GithubReviewThread>>>,
    expanded_rows: Mutex<HashMap<String, Vec<MobileDiffRow>>>,
}

#[uniffi::export]
impl MobileSession {
    #[uniffi::constructor]
    pub fn new(token: String) -> Result<std::sync::Arc<Self>, MobileError> {
        Ok(std::sync::Arc::new(Self {
            client: GithubClient::new(token)?,
            snapshots: Mutex::new(HashMap::new()),
            threads: Mutex::new(HashMap::new()),
            expanded_rows: Mutex::new(HashMap::new()),
        }))
    }

    pub fn viewer(&self) -> Result<MobileViewer, MobileError> {
        let viewer = self.client.viewer()?;
        Ok(MobileViewer {
            login: viewer.login,
        })
    }

    pub fn inbox(
        &self,
        kind: MobileInboxKind,
        after: Option<String>,
    ) -> Result<MobileInboxPage, MobileError> {
        Ok(self
            .client
            .list_inbox(kind.into(), after.as_deref())?
            .into())
    }

    pub fn review_notifications(
        &self,
        _etag: Option<String>,
        _last_modified: Option<String>,
    ) -> Result<MobileNotificationPage, MobileError> {
        let page = self.client.list_inbox(InboxKind::ReviewRequests, None)?;
        let last_modified = page.items.first().map(|item| item.updated_at.clone());
        Ok(MobileNotificationPage {
            notifications: page
                .items
                .into_iter()
                .map(|item| MobileReviewNotification {
                    id: item.node_id,
                    repository: item.key.repository,
                    number: item.key.number,
                    title: item.title,
                    updated_at: item.updated_at,
                })
                .collect(),
            etag: None,
            last_modified,
            not_modified: false,
        })
    }

    pub fn open_pull_request(
        &self,
        repository: String,
        number: u64,
    ) -> Result<MobilePullRequestDetail, MobileError> {
        let key = PullRequestKey { repository, number };
        let cache_key = pull_cache_key(&key);
        let snapshot = self.client.load_snapshot(&key)?;
        let threads = self.client.load_review_threads(&key)?;
        let detail = mobile_pull_request(&snapshot);
        self.snapshots
            .lock()
            .map_err(|_| MobileError::Unexpected)?
            .insert(cache_key.clone(), snapshot);
        self.threads
            .lock()
            .map_err(|_| MobileError::Unexpected)?
            .insert(cache_key, threads);
        Ok(detail)
    }

    pub fn file_screen(
        &self,
        repository: String,
        number: u64,
        file_index: u64,
        start_row: u64,
        row_limit: u64,
    ) -> Result<MobileFileScreen, MobileError> {
        if row_limit == 0 || row_limit > 500 {
            return Err(MobileError::Unexpected);
        }
        let key = PullRequestKey { repository, number };
        let cache_key = pull_cache_key(&key);
        if !self
            .snapshots
            .lock()
            .map_err(|_| MobileError::Unexpected)?
            .contains_key(&cache_key)
        {
            self.open_pull_request(key.repository.clone(), key.number)?;
        }
        let snapshots = self.snapshots.lock().map_err(|_| MobileError::Unexpected)?;
        let snapshot = snapshots.get(&cache_key).ok_or(MobileError::Unexpected)?;
        let threads = self.threads.lock().map_err(|_| MobileError::Unexpected)?;
        let expanded_key = format!("{cache_key}:{file_index}");
        let expanded_rows = self
            .expanded_rows
            .lock()
            .map_err(|_| MobileError::Unexpected)?;
        build_file_screen_with_rows(
            snapshot,
            threads
                .get(&cache_key)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            file_index as usize,
            start_row as usize,
            row_limit as usize,
            expanded_rows.get(&expanded_key).map(Vec::as_slice),
        )
    }

    pub fn expand_context(
        &self,
        repository: String,
        number: u64,
        file_index: u64,
        gap_key: String,
    ) -> Result<MobileFileScreen, MobileError> {
        let key = PullRequestKey { repository, number };
        let cache_key = pull_cache_key(&key);
        if !self
            .snapshots
            .lock()
            .map_err(|_| MobileError::Unexpected)?
            .contains_key(&cache_key)
        {
            self.open_pull_request(key.repository.clone(), key.number)?;
        }
        let snapshot = self
            .snapshots
            .lock()
            .map_err(|_| MobileError::Unexpected)?
            .get(&cache_key)
            .cloned()
            .ok_or(MobileError::Unexpected)?;
        let file = snapshot
            .files
            .get(file_index as usize)
            .ok_or(MobileError::Unexpected)?;
        let removed = file.status == "removed";
        let revision = if removed {
            &snapshot.context.base_revision
        } else {
            &snapshot.context.captured_revision
        };
        let path = if removed {
            file.previous_path.as_deref().unwrap_or(&file.path)
        } else {
            &file.path
        };
        let source = self.client.load_source(&key.repository, revision, path)?;
        let rows = expand_diff_gap(file, &gap_key, &source)?;
        self.expanded_rows
            .lock()
            .map_err(|_| MobileError::Unexpected)?
            .insert(format!("{cache_key}:{file_index}"), rows);
        self.file_screen(key.repository, key.number, file_index, 0, 400)
    }

    pub fn set_file_viewed(
        &self,
        pull_request_id: String,
        path: String,
        viewed: bool,
    ) -> Result<(), MobileError> {
        self.client
            .set_file_viewed(&pull_request_id, &path, viewed)?;
        for snapshot in self
            .snapshots
            .lock()
            .map_err(|_| MobileError::Unexpected)?
            .values_mut()
        {
            if snapshot.node_id == pull_request_id {
                if let Some(file) = snapshot.files.iter_mut().find(|file| file.path == path) {
                    file.viewed = viewed;
                }
            }
        }
        Ok(())
    }

    pub fn publish_review(
        &self,
        review: MobileDraftReview,
        verdict: MobileReviewVerdict,
    ) -> Result<(), MobileError> {
        let key = PullRequestKey {
            repository: review.repository.clone(),
            number: review.number,
        };
        let snapshot = self
            .snapshots
            .lock()
            .map_err(|_| MobileError::Unexpected)?
            .get(&pull_cache_key(&key))
            .cloned()
            .ok_or(MobileError::Validation)?;
        if snapshot.context.captured_revision != review.captured_revision {
            return Err(MobileError::StaleRevision);
        }
        let verdict = match verdict {
            MobileReviewVerdict::Comment => ReviewVerdict::Comment,
            MobileReviewVerdict::Approve => ReviewVerdict::Approve,
            MobileReviewVerdict::RequestChanges => ReviewVerdict::RequestChanges,
        };
        if snapshot.context.is_self_authored() && verdict != ReviewVerdict::Comment {
            return Err(MobileError::Validation);
        }
        if verdict == ReviewVerdict::RequestChanges && review.body.trim().is_empty() {
            return Err(MobileError::Validation);
        }
        let comments = review
            .comments
            .into_iter()
            .map(|comment| {
                if comment.body.trim().is_empty()
                    || comment.start_line == 0
                    || comment.end_line == 0
                {
                    return Err(MobileError::Validation);
                }
                Ok(RemoteReviewComment {
                    target: InlineCommentTarget {
                        path: comment.path,
                        side: remote_side(comment.side),
                        start_line: comment.start_line,
                        end_line: comment.end_line,
                    },
                    body: comment.body,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let request = RemoteReviewRequest {
            commit_id: review.captured_revision,
            body: review.body,
            verdict,
            comments,
        };
        self.client
            .submit_review(&key, &request.commit_id, &request)?;
        Ok(())
    }
}

fn pull_cache_key(key: &PullRequestKey) -> String {
    format!("{}#{}", key.repository, key.number)
}

#[uniffi::export]
pub fn create_mobile_draft(input: MobileDraftInput) -> Result<MobileDraftComment, MobileError> {
    let side = remote_side(input.side);
    let anchor = DraftAnchor {
        repository: input.repository,
        number: input.number,
        captured_revision: input.captured_revision,
        path: input.path,
        side,
        start_line: input.start_line,
        end_line: input.end_line,
        context_before: input.context_before,
        selected_text: input.selected_text,
        context_after: input.context_after,
    };
    let mut hasher = DefaultHasher::new();
    anchor.repository.hash(&mut hasher);
    anchor.number.hash(&mut hasher);
    anchor.path.hash(&mut hasher);
    anchor.start_line.hash(&mut hasher);
    input.body.hash(&mut hasher);
    let draft = validate_draft(
        format!("draft-{:016x}", hasher.finish()),
        anchor,
        input.body,
        remote_side(input.end_side),
        input.start_hunk,
        input.end_hunk,
    )
    .map_err(|_| MobileError::Validation)?;
    Ok(MobileDraftComment {
        id: draft.id,
        repository: draft.anchor.repository,
        number: draft.anchor.number,
        captured_revision: draft.anchor.captured_revision,
        path: draft.anchor.path,
        side: mobile_side(draft.anchor.side),
        start_line: draft.anchor.start_line,
        end_line: draft.anchor.end_line,
        context_before: draft.anchor.context_before,
        selected_text: draft.anchor.selected_text,
        context_after: draft.anchor.context_after,
        body: draft.body,
    })
}

#[uniffi::export]
pub fn encode_draft_review(review: MobileDraftReview) -> Result<Vec<u8>, MobileError> {
    serde_json::to_vec(&review).map_err(|_| MobileError::Unexpected)
}

#[uniffi::export]
pub fn decode_draft_review(bytes: Vec<u8>) -> Result<MobileDraftReview, MobileError> {
    serde_json::from_slice(&bytes).map_err(|_| MobileError::Validation)
}

fn remote_side(side: MobileDraftSide) -> RemoteLineSide {
    match side {
        MobileDraftSide::Left => RemoteLineSide::Left,
        MobileDraftSide::Right => RemoteLineSide::Right,
    }
}

fn mobile_side(side: RemoteLineSide) -> MobileDraftSide {
    match side {
        RemoteLineSide::Left => MobileDraftSide::Left,
        RemoteLineSide::Right => MobileDraftSide::Right,
    }
}

#[uniffi::export]
pub fn encode_inbox_cache(cache: MobileInboxCache) -> Result<String, MobileError> {
    serde_json::to_string(&cache).map_err(|_| MobileError::Unexpected)
}

#[uniffi::export]
pub fn decode_inbox_cache(value: String) -> Result<MobileInboxCache, MobileError> {
    serde_json::from_str(&value).map_err(|_| MobileError::Unexpected)
}

#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn reports_workspace_version() {
        assert_eq!(super::core_version(), "0.0.16");
    }

    #[test]
    fn rejects_empty_tokens_without_a_network_request() {
        assert!(matches!(
            super::MobileSession::new("  ".to_owned()),
            Err(super::MobileError::InvalidCredentials)
        ));
    }

    #[test]
    fn maps_forbidden_github_failures_to_unavailable_access() {
        let error = ramo_github::GithubError::new(
            ramo_github::GithubErrorKind::Forbidden,
            "private detail that must not cross FFI",
        );

        assert!(matches!(
            super::MobileError::from(error),
            super::MobileError::AccessUnavailable
        ));
    }

    #[test]
    fn inbox_cache_round_trips_through_the_bridge_contract() {
        let page = super::MobileInboxPage {
            items: Vec::new(),
            end_cursor: Some("cursor".to_owned()),
            has_next_page: true,
            warnings: vec!["warning".to_owned()],
        };
        let cache = super::MobileInboxCache {
            review_requests: page.clone(),
            authored: page,
        };
        let encoded = super::encode_inbox_cache(cache.clone()).unwrap();
        assert_eq!(super::decode_inbox_cache(encoded).unwrap(), cache);
    }
}
