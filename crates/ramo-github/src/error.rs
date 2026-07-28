#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubErrorKind {
    InvalidCredentials,
    Forbidden,
    RateLimited { reset_at: Option<u64> },
    NotFound,
    Validation,
    Transport,
    Decode,
    Graphql,
    UnexpectedStatus { status: u16 },
    StaleRevision { expected: String, actual: String },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct GithubError {
    kind: GithubErrorKind,
    message: String,
}

impl GithubError {
    pub fn new(kind: GithubErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn invalid_credentials(message: impl Into<String>) -> Self {
        Self::new(GithubErrorKind::InvalidCredentials, message)
    }

    pub fn kind(&self) -> &GithubErrorKind {
        &self.kind
    }

    pub(crate) fn transport(error: reqwest::Error) -> Self {
        Self::new(
            GithubErrorKind::Transport,
            format!("GitHub request failed: {error}"),
        )
    }

    pub(crate) fn decode(error: impl std::fmt::Display) -> Self {
        Self::new(
            GithubErrorKind::Decode,
            format!("GitHub returned malformed data: {error}"),
        )
    }
}
