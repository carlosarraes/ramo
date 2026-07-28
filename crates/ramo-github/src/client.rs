use reqwest::StatusCode;
use reqwest::blocking::{RequestBuilder, Response};
use reqwest::header::{ACCEPT, HeaderMap};
use serde::de::DeserializeOwned;

use crate::{GithubError, GithubErrorKind};

const REST_ACCEPT: &str = "application/vnd.github+json";
const API_VERSION: &str = "2026-03-10";

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct GithubViewer {
    pub login: String,
    pub id: u64,
}

pub struct GithubClient {
    pub(crate) http: reqwest::blocking::Client,
    pub(crate) rest_base: String,
    pub(crate) graphql_url: String,
    token: zeroize::Zeroizing<String>,
}

impl std::fmt::Debug for GithubClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GithubClient")
            .field("rest_base", &self.rest_base)
            .field("graphql_url", &self.graphql_url)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl GithubClient {
    pub fn new(token: String) -> Result<Self, GithubError> {
        Self::with_endpoints(
            token,
            "https://api.github.com",
            "https://api.github.com/graphql",
        )
    }

    pub fn with_endpoints(
        token: String,
        rest_base: impl Into<String>,
        graphql_url: impl Into<String>,
    ) -> Result<Self, GithubError> {
        if token.trim().is_empty() {
            return Err(GithubError::invalid_credentials("GitHub token is empty"));
        }
        let http = reqwest::blocking::Client::builder()
            .user_agent(concat!("ramo/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(GithubError::transport)?;
        Ok(Self {
            http,
            rest_base: rest_base.into().trim_end_matches('/').to_owned(),
            graphql_url: graphql_url.into(),
            token: zeroize::Zeroizing::new(token),
        })
    }

    pub fn viewer(&self) -> Result<GithubViewer, GithubError> {
        let request = self.rest_request(reqwest::Method::GET, "/user", REST_ACCEPT);
        self.send_json(request)
    }

    pub(crate) fn rest_request(
        &self,
        method: reqwest::Method,
        path: &str,
        accept: &str,
    ) -> RequestBuilder {
        self.authorize(
            self.http
                .request(method, format!("{}{path}", self.rest_base))
                .header(ACCEPT, accept)
                .header("X-GitHub-Api-Version", API_VERSION),
        )
    }

    pub(crate) fn send_json<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<T, GithubError> {
        let response = request.send().map_err(GithubError::transport)?;
        let response = Self::ensure_success(response)?;
        response.json().map_err(GithubError::decode)
    }

    pub(crate) fn ensure_success(response: Response) -> Result<Response, GithubError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        let headers = response.headers().clone();
        let body = response.text().unwrap_or_default();
        let message = api_message(&body).unwrap_or_else(|| status.to_string());
        let (kind, prefix) = match status {
            StatusCode::UNAUTHORIZED => (
                GithubErrorKind::InvalidCredentials,
                "GitHub rejected the token",
            ),
            StatusCode::FORBIDDEN if rate_limited(&headers) => (
                GithubErrorKind::RateLimited {
                    reset_at: header_u64(&headers, "x-ratelimit-reset"),
                },
                "GitHub rate limit exceeded",
            ),
            StatusCode::FORBIDDEN => (GithubErrorKind::Forbidden, "GitHub denied this operation"),
            StatusCode::NOT_FOUND => (GithubErrorKind::NotFound, "GitHub resource not found"),
            StatusCode::UNPROCESSABLE_ENTITY => {
                (GithubErrorKind::Validation, "GitHub rejected the request")
            }
            _ => (
                GithubErrorKind::UnexpectedStatus {
                    status: status.as_u16(),
                },
                "GitHub request failed",
            ),
        };
        Err(GithubError::new(kind, format!("{prefix}: {message}")))
    }

    fn authorize(&self, request: RequestBuilder) -> RequestBuilder {
        request.bearer_auth(self.token.as_str())
    }
}

fn rate_limited(headers: &HeaderMap) -> bool {
    headers
        .get("x-ratelimit-remaining")
        .and_then(|value| value.to_str().ok())
        == Some("0")
}

fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

fn api_message(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("message")?.as_str().map(str::to_owned))
        .filter(|message| !message.is_empty())
}
