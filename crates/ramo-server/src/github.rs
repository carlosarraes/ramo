use async_trait::async_trait;
use ramo_core::github::{PullRequestKey, PullRequestSnapshot};
use ramo_core::review_map::{
    ReviewMapFailureCode, ReviewMapIdentity, ReviewMapInput, ReviewMapInputFile,
};
use ramo_github::{GithubClient, GithubError, GithubErrorKind};
use zeroize::Zeroizing;

use crate::ReviewMapFailure;

const CODEOWNERS_PATHS: [&str; 3] = [".github/CODEOWNERS", "CODEOWNERS", "docs/CODEOWNERS"];

#[async_trait]
pub trait PullRequestProvider: Send + Sync {
    async fn load(&self, key: &PullRequestKey) -> Result<ReviewMapInput, ReviewMapFailure>;
}

#[async_trait]
pub trait GhCredentialSource: Send + Sync {
    async fn auth_token(&self) -> Result<Zeroizing<String>, ReviewMapFailure>;
}

#[async_trait]
pub trait GithubRepositoryApi: Send + Sync {
    async fn load_snapshot(
        &self,
        key: &PullRequestKey,
        token: &str,
    ) -> Result<PullRequestSnapshot, GithubError>;

    async fn load_source(
        &self,
        repository: &str,
        revision: &str,
        path: &str,
        token: &str,
    ) -> Result<String, GithubError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemGh;

#[async_trait]
impl GhCredentialSource for SystemGh {
    async fn auth_token(&self) -> Result<Zeroizing<String>, ReviewMapFailure> {
        let output = tokio::process::Command::new("gh")
            .args(["auth", "token"])
            .output()
            .await
            .map_err(|error| {
                ReviewMapFailure::with_source(
                    ReviewMapFailureCode::GithubAuthUnavailable,
                    "GitHub CLI is unavailable; install gh and sign in",
                    error,
                )
            })?;
        if !output.status.success() {
            return Err(ReviewMapFailure::new(
                ReviewMapFailureCode::GithubAuthUnavailable,
                "GitHub CLI is not authenticated for this repository",
            ));
        }
        let token = String::from_utf8(output.stdout).map_err(|error| {
            ReviewMapFailure::with_source(
                ReviewMapFailureCode::GithubAuthUnavailable,
                "GitHub CLI returned an invalid authentication token",
                error,
            )
        })?;
        let token = token.trim().to_owned();
        if token.is_empty() {
            return Err(ReviewMapFailure::new(
                ReviewMapFailureCode::GithubAuthUnavailable,
                "GitHub CLI returned an empty authentication token",
            ));
        }
        Ok(Zeroizing::new(token))
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GithubApiClient;

#[async_trait]
impl GithubRepositoryApi for GithubApiClient {
    async fn load_snapshot(
        &self,
        key: &PullRequestKey,
        token: &str,
    ) -> Result<PullRequestSnapshot, GithubError> {
        let key = key.clone();
        let token = Zeroizing::new(token.to_owned());
        tokio::task::spawn_blocking(move || {
            GithubClient::new(token.to_string())?.load_snapshot(&key)
        })
        .await
        .map_err(join_error)?
    }

    async fn load_source(
        &self,
        repository: &str,
        revision: &str,
        path: &str,
        token: &str,
    ) -> Result<String, GithubError> {
        let repository = repository.to_owned();
        let revision = revision.to_owned();
        let path = path.to_owned();
        let token = Zeroizing::new(token.to_owned());
        tokio::task::spawn_blocking(move || {
            GithubClient::new(token.to_string())?.load_source(&repository, &revision, &path)
        })
        .await
        .map_err(join_error)?
    }
}

pub struct GithubPullRequestProvider<G = SystemGh, A = GithubApiClient> {
    credentials: G,
    api: A,
}

impl GithubPullRequestProvider {
    pub fn new() -> Self {
        Self::with_clients(SystemGh, GithubApiClient)
    }
}

impl Default for GithubPullRequestProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl<G, A> GithubPullRequestProvider<G, A> {
    pub fn with_clients(credentials: G, api: A) -> Self {
        Self { credentials, api }
    }

    pub fn debug_string(&self) -> &'static str {
        "GithubPullRequestProvider { credentials: [REDACTED] }"
    }
}

impl<G, A> std::fmt::Debug for GithubPullRequestProvider<G, A> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.debug_string())
    }
}

#[async_trait]
impl<G, A> PullRequestProvider for GithubPullRequestProvider<G, A>
where
    G: GhCredentialSource,
    A: GithubRepositoryApi,
{
    async fn load(&self, key: &PullRequestKey) -> Result<ReviewMapInput, ReviewMapFailure> {
        let token = self.credentials.auth_token().await?;
        let snapshot = self
            .api
            .load_snapshot(key, token.as_str())
            .await
            .map_err(map_github_error)?;
        let codeowners = self.load_codeowners(&snapshot, token.as_str()).await?;

        Ok(snapshot_to_input(snapshot, codeowners))
    }
}

impl<G, A> GithubPullRequestProvider<G, A>
where
    A: GithubRepositoryApi,
{
    async fn load_codeowners(
        &self,
        snapshot: &PullRequestSnapshot,
        token: &str,
    ) -> Result<Option<String>, ReviewMapFailure> {
        for path in CODEOWNERS_PATHS {
            match self
                .api
                .load_source(
                    &snapshot.context.repository,
                    &snapshot.context.captured_revision,
                    path,
                    token,
                )
                .await
            {
                Ok(contents) => return Ok(Some(contents)),
                Err(error) if matches!(error.kind(), GithubErrorKind::NotFound) => {}
                Err(error) => return Err(map_github_error(error)),
            }
        }
        Ok(None)
    }
}

fn snapshot_to_input(snapshot: PullRequestSnapshot, codeowners: Option<String>) -> ReviewMapInput {
    let identity = ReviewMapIdentity {
        repository: snapshot.context.repository,
        pull_request: snapshot.context.number,
        base_sha: snapshot.context.base_revision,
        head_sha: snapshot.context.captured_revision,
    };
    let files = snapshot
        .files
        .into_iter()
        .map(|file| ReviewMapInputFile {
            path: file.path,
            previous_path: file.previous_path,
            status: file.status,
            additions: file.additions,
            deletions: file.deletions,
            patch: file.patch,
            binary: file.binary,
        })
        .collect();

    ReviewMapInput {
        identity,
        files,
        codeowners,
    }
}

fn map_github_error(error: GithubError) -> ReviewMapFailure {
    let (code, message) = match error.kind() {
        GithubErrorKind::InvalidCredentials => (
            ReviewMapFailureCode::GithubAuthUnavailable,
            "GitHub authentication is unavailable or expired",
        ),
        GithubErrorKind::Forbidden | GithubErrorKind::NotFound => (
            ReviewMapFailureCode::PullRequestUnavailable,
            "The GitHub pull request is unavailable or inaccessible",
        ),
        _ => (
            ReviewMapFailureCode::GithubRequestFailed,
            "GitHub could not load the pull request",
        ),
    };
    ReviewMapFailure::with_source(code, message, error)
}

fn join_error(error: tokio::task::JoinError) -> GithubError {
    GithubError::new(
        GithubErrorKind::Transport,
        format!("GitHub worker failed: {error}"),
    )
}
