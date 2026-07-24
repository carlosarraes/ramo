use std::ffi::OsString;
use std::fmt;
use std::io;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::input::sanitize_terminal_text;
use crate::process::command::{CaptureLimits, CommandExecutor, CommandRequest, CommandResult};
use crate::remote_review::{
    PullRequestReviewContext, RemoteLineSide, RemoteReviewError, RemoteReviewPublisher,
    RemoteReviewRequest,
};

const METADATA_STDOUT_LIMIT: usize = 64 * 1024;
const DIFF_STDOUT_LIMIT: usize = 32 * 1024 * 1024;
const STDERR_LIMIT: usize = 8 * 1024;
const METADATA_TIMEOUT: Duration = Duration::from_secs(10);
const DIFF_TIMEOUT: Duration = Duration::from_secs(30);

pub trait GithubPullRequestSource {
    fn resolve_pr(&mut self, number: u64) -> Result<PullRequestReviewContext, GithubError>;
    fn load_diff(&mut self, number: u64) -> Result<String, GithubError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubOperation {
    Authenticate,
    ResolveRepository,
    ResolvePullRequest,
    LoadDiff,
    RefreshPullRequest,
    SubmitReview,
}

impl GithubOperation {
    fn description(self) -> &'static str {
        match self {
            Self::Authenticate => "authenticate with GitHub",
            Self::ResolveRepository => "resolve the GitHub repository",
            Self::ResolvePullRequest => "resolve the GitHub pull request",
            Self::LoadDiff => "load the GitHub pull request diff",
            Self::RefreshPullRequest => "refresh the GitHub pull request",
            Self::SubmitReview => "submit the GitHub pull request review",
        }
    }
}

#[derive(Debug)]
pub enum GithubError {
    MissingCli,
    TimedOut {
        operation: GithubOperation,
    },
    Truncated {
        operation: GithubOperation,
    },
    InvalidUtf8 {
        operation: GithubOperation,
    },
    InvalidJson {
        operation: GithubOperation,
        detail: String,
    },
    Failed {
        operation: GithubOperation,
        code: Option<i32>,
        stderr: String,
    },
    Io {
        operation: GithubOperation,
        source: io::Error,
    },
}

impl fmt::Display for GithubError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCli => formatter.write_str(
                "GitHub CLI (`gh`) was not found; install GitHub CLI and retry `ramo pr`",
            ),
            Self::TimedOut { operation } => {
                write!(formatter, "{} timed out", operation.description())
            }
            Self::Truncated { operation } => {
                write!(
                    formatter,
                    "{} returned too much output",
                    operation.description()
                )
            }
            Self::InvalidUtf8 { operation } => {
                write!(
                    formatter,
                    "{} returned invalid UTF-8",
                    operation.description()
                )
            }
            Self::InvalidJson { operation, detail } => {
                write!(
                    formatter,
                    "{} returned invalid JSON: {detail}",
                    operation.description()
                )
            }
            Self::Failed {
                operation,
                code,
                stderr,
            } => {
                write!(
                    formatter,
                    "{} failed with status {}",
                    operation.description(),
                    code.map_or_else(|| "signal".into(), |code| code.to_string())
                )?;
                if !stderr.is_empty() {
                    write!(formatter, ": {stderr}")?;
                }
                match operation {
                    GithubOperation::Authenticate => {
                        formatter.write_str("; run `gh auth login` and retry")
                    }
                    GithubOperation::ResolveRepository => formatter
                        .write_str("; run inside a GitHub repository or configure a GitHub remote"),
                    GithubOperation::ResolvePullRequest => formatter
                        .write_str("; verify the pull request number and your repository access"),
                    _ => Ok(()),
                }
            }
            Self::Io { operation, source } => {
                write!(formatter, "failed to {}: {source}", operation.description())
            }
        }
    }
}

impl std::error::Error for GithubError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub struct GithubCli<E> {
    executor: E,
}

impl<E> GithubCli<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    pub fn into_executor(self) -> E {
        self.executor
    }
}

impl<E: CommandExecutor> GithubCli<E> {
    fn execute_text(
        &mut self,
        operation: GithubOperation,
        arguments: &[&str],
        limits: CaptureLimits,
        stdin: Option<Vec<u8>>,
    ) -> Result<String, GithubError> {
        let mut argv = Vec::with_capacity(arguments.len() + 1);
        argv.push(OsString::from("gh"));
        argv.extend(arguments.iter().map(OsString::from));
        let result = self
            .executor
            .execute(CommandRequest {
                argv,
                stdin,
                inherit_stdio: false,
                limits: Some(limits),
            })
            .map_err(|source| {
                if source.kind() == io::ErrorKind::NotFound {
                    GithubError::MissingCli
                } else {
                    GithubError::Io { operation, source }
                }
            })?;
        validate_result(operation, result)
    }

    fn metadata_text(
        &mut self,
        operation: GithubOperation,
        arguments: &[&str],
    ) -> Result<String, GithubError> {
        self.execute_text(
            operation,
            arguments,
            CaptureLimits::new(METADATA_STDOUT_LIMIT, STDERR_LIMIT, METADATA_TIMEOUT),
            None,
        )
    }

    fn pull_request(&mut self, number: u64) -> Result<RawPullRequest, GithubError> {
        let number = number.to_string();
        let text = self.metadata_text(
            GithubOperation::ResolvePullRequest,
            &[
                "pr",
                "view",
                &number,
                "--json",
                "number,title,url,author,baseRefName,headRefName,headRefOid",
            ],
        )?;
        parse_json(GithubOperation::ResolvePullRequest, &text)
    }
}

impl<E: CommandExecutor> GithubPullRequestSource for GithubCli<E> {
    fn resolve_pr(&mut self, number: u64) -> Result<PullRequestReviewContext, GithubError> {
        let viewer = self
            .metadata_text(
                GithubOperation::Authenticate,
                &["api", "user", "--jq", ".login"],
            )?
            .trim()
            .to_owned();
        require_field(GithubOperation::Authenticate, "viewer login", &viewer)?;

        let repository: RawRepository = parse_json(
            GithubOperation::ResolveRepository,
            &self.metadata_text(
                GithubOperation::ResolveRepository,
                &["repo", "view", "--json", "nameWithOwner,url"],
            )?,
        )?;
        require_field(
            GithubOperation::ResolveRepository,
            "repository name",
            &repository.name_with_owner,
        )?;
        require_field(
            GithubOperation::ResolveRepository,
            "repository URL",
            &repository.url,
        )?;

        let pull_request = self.pull_request(number)?;
        for (name, value) in [
            ("title", pull_request.title.as_str()),
            ("URL", pull_request.url.as_str()),
            ("author login", pull_request.author.login.as_str()),
            ("base ref", pull_request.base_ref_name.as_str()),
            ("head ref", pull_request.head_ref_name.as_str()),
            ("head revision", pull_request.head_ref_oid.as_str()),
        ] {
            require_field(GithubOperation::ResolvePullRequest, name, value)?;
        }
        if pull_request.number != number {
            return Err(GithubError::InvalidJson {
                operation: GithubOperation::ResolvePullRequest,
                detail: format!(
                    "expected pull request #{number}, received #{}",
                    pull_request.number
                ),
            });
        }
        Ok(PullRequestReviewContext {
            repository: repository.name_with_owner,
            repository_url: repository.url,
            number,
            title: pull_request.title,
            url: pull_request.url,
            base_ref: pull_request.base_ref_name,
            head_ref: pull_request.head_ref_name,
            captured_revision: pull_request.head_ref_oid,
            author_login: pull_request.author.login,
            viewer_login: viewer,
        })
    }

    fn load_diff(&mut self, number: u64) -> Result<String, GithubError> {
        self.execute_text(
            GithubOperation::LoadDiff,
            &["pr", "diff", &number.to_string(), "--color=never"],
            CaptureLimits::new(DIFF_STDOUT_LIMIT, STDERR_LIMIT, DIFF_TIMEOUT),
            None,
        )
    }
}

impl<E: CommandExecutor> RemoteReviewPublisher for GithubCli<E> {
    fn current_revision(
        &mut self,
        context: &PullRequestReviewContext,
    ) -> Result<String, RemoteReviewError> {
        self.metadata_text(
            GithubOperation::RefreshPullRequest,
            &[
                "pr",
                "view",
                &context.number.to_string(),
                "--json",
                "headRefOid",
                "--jq",
                ".headRefOid",
            ],
        )
        .map(|revision| revision.trim().to_owned())
        .and_then(|revision| {
            require_field(
                GithubOperation::RefreshPullRequest,
                "head revision",
                &revision,
            )
            .map(|_| revision)
        })
        .map_err(remote_error)
    }

    fn submit_review(
        &mut self,
        context: &PullRequestReviewContext,
        request: &RemoteReviewRequest,
    ) -> Result<(), RemoteReviewError> {
        let payload = GithubReviewPayload::from_request(request);
        let input = serde_json::to_vec(&payload).map_err(|error| RemoteReviewError {
            message: format!("failed to serialize GitHub review: {error}"),
        })?;
        let endpoint = format!(
            "repos/{}/pulls/{}/reviews",
            context.repository, context.number
        );
        self.execute_text(
            GithubOperation::SubmitReview,
            &["api", "--method", "POST", &endpoint, "--input", "-"],
            CaptureLimits::new(METADATA_STDOUT_LIMIT, STDERR_LIMIT, METADATA_TIMEOUT),
            Some(input),
        )
        .map(|_| ())
        .map_err(remote_error)
    }
}

fn validate_result(
    operation: GithubOperation,
    result: CommandResult,
) -> Result<String, GithubError> {
    if result.timed_out {
        return Err(GithubError::TimedOut { operation });
    }
    if result.stdout_truncated {
        return Err(GithubError::Truncated { operation });
    }
    if result.code != Some(0) {
        return Err(GithubError::Failed {
            operation,
            code: result.code,
            stderr: sanitize_terminal_text(&String::from_utf8_lossy(&result.stderr), false)
                .trim()
                .to_owned(),
        });
    }
    String::from_utf8(result.stdout).map_err(|_| GithubError::InvalidUtf8 { operation })
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    operation: GithubOperation,
    text: &str,
) -> Result<T, GithubError> {
    serde_json::from_str(text).map_err(|error| GithubError::InvalidJson {
        operation,
        detail: error.to_string(),
    })
}

fn require_field(operation: GithubOperation, name: &str, value: &str) -> Result<(), GithubError> {
    if value.trim().is_empty() {
        Err(GithubError::InvalidJson {
            operation,
            detail: format!("missing {name}"),
        })
    } else {
        Ok(())
    }
}

fn remote_error(error: GithubError) -> RemoteReviewError {
    RemoteReviewError {
        message: error.to_string(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRepository {
    name_with_owner: String,
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPullRequest {
    number: u64,
    title: String,
    url: String,
    author: RawAuthor,
    base_ref_name: String,
    head_ref_name: String,
    head_ref_oid: String,
}

#[derive(Deserialize)]
struct RawAuthor {
    login: String,
}

#[derive(Serialize)]
struct GithubReviewPayload<'a> {
    commit_id: &'a str,
    body: &'a str,
    event: &'a str,
    comments: Vec<GithubReviewComment<'a>>,
}

impl<'a> GithubReviewPayload<'a> {
    fn from_request(request: &'a RemoteReviewRequest) -> Self {
        Self {
            commit_id: &request.commit_id,
            body: &request.body,
            event: request.verdict.event_name(),
            comments: request
                .comments
                .iter()
                .map(GithubReviewComment::from_comment)
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct GithubReviewComment<'a> {
    path: &'a str,
    body: &'a str,
    line: u32,
    side: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_side: Option<&'static str>,
}

impl<'a> GithubReviewComment<'a> {
    fn from_comment(comment: &'a crate::remote_review::RemoteReviewComment) -> Self {
        let side = match comment.target.side {
            RemoteLineSide::Left => "LEFT",
            RemoteLineSide::Right => "RIGHT",
        };
        let multiline = comment.target.start_line != comment.target.end_line;
        Self {
            path: &comment.target.path,
            body: &comment.body,
            line: comment.target.end_line,
            side,
            start_line: multiline.then_some(comment.target.start_line),
            start_side: multiline.then_some(side),
        }
    }
}
