uniffi::setup_scaffolding!();

use ramo_core::github::{InboxKind, InboxPage, PullRequestSummary};
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

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("GitHub rejected this token")]
    InvalidCredentials,
    #[error("GitHub denied this operation")]
    Forbidden,
    #[error("GitHub rate limit exceeded")]
    RateLimited,
    #[error("Could not reach GitHub")]
    Network,
    #[error("GitHub returned an unexpected response")]
    Unexpected,
}

impl From<GithubError> for MobileError {
    fn from(error: GithubError) -> Self {
        match error.kind() {
            GithubErrorKind::InvalidCredentials => Self::InvalidCredentials,
            GithubErrorKind::Forbidden => Self::Forbidden,
            GithubErrorKind::RateLimited { .. } => Self::RateLimited,
            GithubErrorKind::Transport => Self::Network,
            _ => Self::Unexpected,
        }
    }
}

#[derive(uniffi::Object)]
pub struct MobileSession {
    client: GithubClient,
}

#[uniffi::export]
impl MobileSession {
    #[uniffi::constructor]
    pub fn new(token: String) -> Result<std::sync::Arc<Self>, MobileError> {
        Ok(std::sync::Arc::new(Self {
            client: GithubClient::new(token)?,
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
        assert_eq!(super::core_version(), "0.0.15");
    }

    #[test]
    fn rejects_empty_tokens_without_a_network_request() {
        assert!(matches!(
            super::MobileSession::new("  ".to_owned()),
            Err(super::MobileError::InvalidCredentials)
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
