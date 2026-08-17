use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ramo_core::github::{ChangedFile, PullRequestKey, PullRequestSnapshot};
use ramo_core::remote_review::PullRequestReviewContext;
use ramo_core::review_map::ReviewMapFailureCode;
use ramo_github::{GithubError, GithubErrorKind};
use ramo_server::github::{
    GhCredentialSource, GithubPullRequestProvider, GithubRepositoryApi, PullRequestProvider,
};
use zeroize::Zeroizing;

#[derive(Clone)]
struct FakeGh {
    result: Result<String, &'static str>,
    calls: Arc<Mutex<Vec<Vec<String>>>>,
}

impl FakeGh {
    fn success(token: &str) -> Self {
        Self {
            result: Ok(token.into()),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn missing() -> Self {
        Self {
            result: Err("missing"),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl GhCredentialSource for FakeGh {
    async fn auth_token(&self) -> Result<Zeroizing<String>, ramo_server::ReviewMapFailure> {
        self.calls
            .lock()
            .unwrap()
            .push(vec!["auth".into(), "token".into()]);
        self.result
            .as_ref()
            .map(|token| Zeroizing::new(token.clone()))
            .map_err(|_| {
                ramo_server::ReviewMapFailure::new(
                    ReviewMapFailureCode::GithubAuthUnavailable,
                    "GitHub CLI is unavailable or not authenticated",
                )
            })
    }
}

#[derive(Clone)]
struct FakeGithubApi {
    snapshot: Result<PullRequestSnapshot, GithubError>,
    sources: Arc<Mutex<HashMap<String, Result<String, GithubError>>>>,
    seen_tokens: Arc<Mutex<Vec<String>>>,
    source_calls: Arc<Mutex<Vec<String>>>,
}

impl FakeGithubApi {
    fn snapshot(snapshot: PullRequestSnapshot) -> Self {
        Self {
            snapshot: Ok(snapshot),
            sources: Arc::new(Mutex::new(HashMap::new())),
            seen_tokens: Arc::new(Mutex::new(Vec::new())),
            source_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn with_source(self, path: &str, result: Result<&str, GithubError>) -> Self {
        self.sources
            .lock()
            .unwrap()
            .insert(path.into(), result.map(str::to_owned));
        self
    }
}

#[async_trait]
impl GithubRepositoryApi for FakeGithubApi {
    async fn load_snapshot(
        &self,
        _key: &PullRequestKey,
        token: &str,
    ) -> Result<PullRequestSnapshot, GithubError> {
        self.seen_tokens.lock().unwrap().push(token.into());
        self.snapshot.clone()
    }

    async fn load_source(
        &self,
        _repository: &str,
        _revision: &str,
        path: &str,
        _token: &str,
    ) -> Result<String, GithubError> {
        self.source_calls.lock().unwrap().push(path.into());
        self.sources
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or_else(|| Err(GithubError::new(GithubErrorKind::NotFound, "not found")))
    }
}

#[tokio::test]
async fn provider_uses_gh_token_without_persisting_it() {
    let gh = FakeGh::success("secret-token");
    let api =
        FakeGithubApi::snapshot(snapshot_fixture()).with_source("CODEOWNERS", Ok("src/ @backend"));
    let provider = GithubPullRequestProvider::with_clients(gh.clone(), api.clone());

    let input = provider.load(&key()).await.unwrap();

    assert_eq!(input.identity.head_sha, "head-sha");
    assert_eq!(input.files.len(), 2);
    assert_eq!(input.codeowners.as_deref(), Some("src/ @backend"));
    assert_eq!(gh.calls(), vec![vec!["auth", "token"]]);
    assert_eq!(
        api.source_calls.lock().unwrap().as_slice(),
        &[".github/CODEOWNERS", "CODEOWNERS"]
    );
    assert!(!provider.debug_string().contains("secret-token"));
    assert!(!format!("{provider:?}").contains("secret-token"));
}

#[tokio::test]
async fn missing_gh_and_github_errors_are_typed() {
    let provider = GithubPullRequestProvider::with_clients(
        FakeGh::missing(),
        FakeGithubApi::snapshot(snapshot_fixture()),
    );
    assert_eq!(
        provider.load(&key()).await.unwrap_err().code,
        ReviewMapFailureCode::GithubAuthUnavailable
    );

    for (kind, expected) in [
        (
            GithubErrorKind::InvalidCredentials,
            ReviewMapFailureCode::GithubAuthUnavailable,
        ),
        (
            GithubErrorKind::Forbidden,
            ReviewMapFailureCode::PullRequestUnavailable,
        ),
        (
            GithubErrorKind::NotFound,
            ReviewMapFailureCode::PullRequestUnavailable,
        ),
        (
            GithubErrorKind::Transport,
            ReviewMapFailureCode::GithubRequestFailed,
        ),
    ] {
        let mut api = FakeGithubApi::snapshot(snapshot_fixture());
        api.snapshot = Err(GithubError::new(kind, "sensitive upstream detail"));
        let failure = GithubPullRequestProvider::with_clients(FakeGh::success("secret"), api)
            .load(&key())
            .await
            .unwrap_err();
        assert_eq!(failure.code, expected);
        assert!(!format!("{failure:?}").contains("sensitive upstream detail"));
    }
}

fn key() -> PullRequestKey {
    PullRequestKey {
        repository: "owner/repo".into(),
        number: 7,
    }
}

fn snapshot_fixture() -> PullRequestSnapshot {
    PullRequestSnapshot {
        node_id: "PR_node".into(),
        context: PullRequestReviewContext {
            repository: "owner/repo".into(),
            repository_url: "https://github.com/owner/repo".into(),
            number: 7,
            title: "Review map".into(),
            body: String::new(),
            url: "https://github.com/owner/repo/pull/7".into(),
            base_ref: "main".into(),
            base_revision: "base-sha".into(),
            head_ref: "feature".into(),
            captured_revision: "head-sha".into(),
            author_login: "author".into(),
            viewer_login: "reviewer".into(),
        },
        files: vec![
            ChangedFile {
                path: "src/lib.rs".into(),
                previous_path: None,
                status: "modified".into(),
                additions: 4,
                deletions: 2,
                patch: Some("@@ -1 +1 @@".into()),
                viewed: false,
                binary: false,
            },
            ChangedFile {
                path: "assets/logo.png".into(),
                previous_path: None,
                status: "added".into(),
                additions: 0,
                deletions: 0,
                patch: None,
                viewed: false,
                binary: true,
            },
        ],
    }
}
