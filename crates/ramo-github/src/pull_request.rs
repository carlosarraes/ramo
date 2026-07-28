use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use ramo_core::github::{ChangedFile, PullRequestKey, PullRequestSnapshot};
use ramo_core::remote_review::{
    GithubReviewThread, GithubThreadComment, GithubThreadSubject, PullRequestReviewContext,
    RemoteLineSide,
};
use reqwest::header::LINK;

use crate::{GithubClient, GithubError, GithubErrorKind};

const REST_ACCEPT: &str = "application/vnd.github+json";
const DIFF_ACCEPT: &str = "application/vnd.github.diff";
const RAW_ACCEPT: &str = "application/vnd.github.raw+json";
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');
const THREADS_QUERY: &str = "query ReviewThreads($owner: String!, $name: String!, $number: Int!, $first: Int!, $after: String) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { reviewThreads(first: $first, after: $after) { nodes { id isResolved isOutdated subjectType path diffSide startDiffSide startLine line comments(first: 100) { nodes { id bodyText createdAt url author { login } } pageInfo { hasNextPage endCursor } } } pageInfo { hasNextPage endCursor } } } } }";

impl GithubClient {
    pub fn load_snapshot(&self, key: &PullRequestKey) -> Result<PullRequestSnapshot, GithubError> {
        repository_parts(key)?;
        let pull: RawPullRequest = self.send_json(self.rest_request(
            reqwest::Method::GET,
            &format!("/repos/{}/pulls/{}", key.repository, key.number),
            REST_ACCEPT,
        ))?;
        let viewer = self.viewer()?;
        let files = self.load_files(key)?;
        Ok(PullRequestSnapshot {
            node_id: pull.node_id,
            context: PullRequestReviewContext {
                repository: key.repository.clone(),
                repository_url: format!("https://github.com/{}", key.repository),
                number: key.number,
                title: pull.title,
                url: pull.html_url,
                base_ref: pull.base.reference,
                base_revision: pull.base.sha,
                head_ref: pull.head.reference,
                captured_revision: pull.head.sha,
                author_login: pull.user.login,
                viewer_login: viewer.login,
            },
            files,
        })
    }

    pub fn load_unified_diff(&self, key: &PullRequestKey) -> Result<String, GithubError> {
        repository_parts(key)?;
        self.send_text(self.rest_request(
            reqwest::Method::GET,
            &format!("/repos/{}/pulls/{}", key.repository, key.number),
            DIFF_ACCEPT,
        ))
    }

    pub fn load_source(
        &self,
        repository: &str,
        revision: &str,
        path: &str,
    ) -> Result<String, GithubError> {
        validate_repository(repository)?;
        let path = path
            .split('/')
            .map(|segment| utf8_percent_encode(segment, PATH_SEGMENT).to_string())
            .collect::<Vec<_>>()
            .join("/");
        let request = self
            .rest_request(
                reqwest::Method::GET,
                &format!("/repos/{repository}/contents/{path}"),
                RAW_ACCEPT,
            )
            .query(&[("ref", revision)]);
        self.send_text(request)
    }

    pub fn load_review_threads(
        &self,
        key: &PullRequestKey,
    ) -> Result<Vec<GithubReviewThread>, GithubError> {
        let (owner, name) = repository_parts(key)?;
        let mut after: Option<String> = None;
        let mut threads = Vec::new();
        loop {
            let data: ThreadsData = self.graphql(
                THREADS_QUERY,
                ThreadVariables {
                    owner,
                    name,
                    number: key.number,
                    first: 100,
                    after: after.as_deref(),
                },
            )?;
            let connection = data
                .repository
                .and_then(|repository| repository.pull_request)
                .map(|pull| pull.review_threads)
                .ok_or_else(|| {
                    GithubError::new(
                        GithubErrorKind::NotFound,
                        "GitHub pull request threads were not found",
                    )
                })?;
            for thread in connection.nodes.into_iter().flatten() {
                threads.push(map_thread(thread)?);
            }
            if !connection.page_info.has_next_page {
                break;
            }
            after = connection
                .page_info
                .end_cursor
                .filter(|cursor| !cursor.is_empty());
            if after.is_none() {
                return Err(GithubError::new(
                    GithubErrorKind::Decode,
                    "GitHub thread page had no end cursor",
                ));
            }
        }
        Ok(threads)
    }

    fn load_files(&self, key: &PullRequestKey) -> Result<Vec<ChangedFile>, GithubError> {
        let mut page = 1usize;
        let mut files = Vec::new();
        loop {
            let response = self
                .rest_request(
                    reqwest::Method::GET,
                    &format!("/repos/{}/pulls/{}/files", key.repository, key.number),
                    REST_ACCEPT,
                )
                .query(&[("per_page", 100usize), ("page", page)])
                .send()
                .map_err(GithubError::transport)?;
            let has_next = response
                .headers()
                .get(LINK)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.split(',').any(|link| link.contains("rel=\"next\"")));
            let response = Self::ensure_success(response)?;
            let raw: Vec<RawFile> = response.json().map_err(GithubError::decode)?;
            files.extend(raw.into_iter().map(map_file));
            if !has_next {
                break;
            }
            page += 1;
        }
        Ok(files)
    }

    fn send_text(&self, request: reqwest::blocking::RequestBuilder) -> Result<String, GithubError> {
        let response = request.send().map_err(GithubError::transport)?;
        Self::ensure_success(response)?
            .text()
            .map_err(GithubError::transport)
    }
}

fn validate_repository(repository: &str) -> Result<(&str, &str), GithubError> {
    repository
        .split_once('/')
        .filter(|(owner, name)| !owner.is_empty() && !name.is_empty() && !name.contains('/'))
        .ok_or_else(|| {
            GithubError::new(
                GithubErrorKind::Validation,
                "repository must use owner/name form",
            )
        })
}

pub(crate) fn repository_parts(key: &PullRequestKey) -> Result<(&str, &str), GithubError> {
    validate_repository(&key.repository)
}

#[derive(serde::Deserialize)]
struct RawPullRequest {
    node_id: String,
    title: String,
    html_url: String,
    user: Login,
    base: RawRevision,
    head: RawRevision,
}

#[derive(serde::Deserialize)]
struct RawRevision {
    #[serde(rename = "ref")]
    reference: String,
    sha: String,
}

#[derive(serde::Deserialize)]
struct RawFile {
    filename: String,
    previous_filename: Option<String>,
    status: String,
    additions: usize,
    deletions: usize,
    #[serde(default)]
    changes: usize,
    patch: Option<String>,
    #[serde(default)]
    viewer_viewed_state: String,
}

fn map_file(file: RawFile) -> ChangedFile {
    ChangedFile {
        path: file.filename,
        previous_path: file.previous_filename,
        status: file.status,
        additions: file.additions,
        deletions: file.deletions,
        binary: file.patch.is_none() && file.changes > 0,
        patch: file.patch,
        viewed: file.viewer_viewed_state == "VIEWED",
    }
}

#[derive(serde::Serialize)]
struct ThreadVariables<'a> {
    owner: &'a str,
    name: &'a str,
    number: u64,
    first: usize,
    after: Option<&'a str>,
}

#[derive(serde::Deserialize)]
struct ThreadsData {
    repository: Option<ThreadRepository>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadRepository {
    pull_request: Option<ThreadPullRequest>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadPullRequest {
    review_threads: ThreadConnection,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadConnection {
    #[serde(default)]
    nodes: Vec<Option<RawThread>>,
    page_info: PageInfo,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawThread {
    id: String,
    is_resolved: bool,
    is_outdated: bool,
    subject_type: String,
    path: String,
    diff_side: Option<String>,
    start_diff_side: Option<String>,
    start_line: Option<u32>,
    line: Option<u32>,
    comments: CommentConnection,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommentConnection {
    nodes: Vec<RawComment>,
    page_info: PageInfo,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawComment {
    id: String,
    body_text: String,
    created_at: String,
    url: String,
    author: Option<Login>,
}

#[derive(serde::Deserialize)]
struct Login {
    login: String,
}

fn map_thread(thread: RawThread) -> Result<GithubReviewThread, GithubError> {
    if thread.comments.page_info.has_next_page {
        return Err(GithubError::new(
            GithubErrorKind::Validation,
            "GitHub review thread has more than 100 comments",
        ));
    }
    let subject = match thread.subject_type.as_str() {
        "FILE" => GithubThreadSubject::File,
        "LINE" => GithubThreadSubject::Line {
            side: map_side(thread.diff_side.as_deref()),
            start_side: map_side(thread.start_diff_side.as_deref()),
            start_line: thread.start_line,
            end_line: thread.line,
        },
        other => GithubThreadSubject::Unsupported(other.to_owned()),
    };
    let comments = thread
        .comments
        .nodes
        .into_iter()
        .map(|comment| GithubThreadComment {
            id: comment.id,
            author: comment
                .author
                .map_or_else(|| "[deleted]".into(), |author| author.login),
            body: comment.body_text,
            created_at: comment.created_at,
            url: comment.url,
        })
        .collect::<Vec<_>>();
    let url = comments
        .first()
        .map_or_else(String::new, |comment| comment.url.clone());
    Ok(GithubReviewThread {
        id: thread.id,
        path: thread.path,
        is_resolved: thread.is_resolved,
        is_outdated: thread.is_outdated,
        subject,
        comments,
        url,
    })
}

fn map_side(side: Option<&str>) -> Option<RemoteLineSide> {
    match side {
        Some("LEFT") => Some(RemoteLineSide::Left),
        Some("RIGHT") => Some(RemoteLineSide::Right),
        _ => None,
    }
}
