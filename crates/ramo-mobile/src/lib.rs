uniffi::setup_scaffolding!();

use ramo_github::{GithubClient, GithubError, GithubErrorKind};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileViewer {
    pub login: String,
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
}
