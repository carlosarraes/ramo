use std::time::Duration;

use serde::Deserialize;

use super::{GithubCli, GithubError, GithubOperation, STDERR_LIMIT, parse_json};
use crate::input::sanitize_terminal_text;
use crate::process::command::{CaptureLimits, CommandExecutor};
use crate::remote_review::{
    GithubReviewThread, GithubThreadComment, GithubThreadSubject, PullRequestReviewContext,
    RemoteLineSide,
};

const THREAD_PAGE_SIZE: usize = 100;
const MAX_THREADS: usize = 500;
const MAX_COMMENTS_PER_THREAD: usize = 100;
const MAX_COMMENT_BODY_BYTES: usize = 64 * 1024;
const THREAD_PAGE_STDOUT_LIMIT: usize = 4 * 1024 * 1024;
const MAX_RETAINED_THREAD_BYTES: usize = 8 * 1024 * 1024;
const THREAD_TIMEOUT: Duration = Duration::from_secs(20);

const REVIEW_THREADS_QUERY: &str = r#"
query($owner:String!,$name:String!,$number:Int!,$first:Int!,$after:String) {
  repository(owner:$owner,name:$name) {
    pullRequest(number:$number) {
      reviewThreads(first:$first,after:$after) {
        nodes {
          id isResolved isOutdated subjectType path diffSide
          startDiffSide startLine line
          comments(first:100) {
            nodes { id bodyText createdAt url author { login } }
            pageInfo { hasNextPage endCursor }
          }
        }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}
"#;

impl<E: CommandExecutor> GithubCli<E> {
    pub fn load_review_threads(
        &mut self,
        context: &PullRequestReviewContext,
    ) -> Result<Vec<GithubReviewThread>, GithubError> {
        let (owner, name) = context
            .repository
            .split_once('/')
            .filter(|(owner, name)| !owner.is_empty() && !name.is_empty())
            .ok_or_else(|| invalid_json("repository must use the owner/name form"))?;
        let mut cursor: Option<String> = None;
        let mut output = Vec::new();
        let mut retained = 0usize;

        loop {
            let text = self.review_threads_page(context.number, owner, name, cursor.as_deref())?;
            let page: RawEnvelope = parse_json(GithubOperation::LoadReviewThreads, &text)?;
            let connection = page.connection()?;
            let RawThreadConnection { nodes, page_info } = connection;

            for raw in nodes {
                if raw.is_resolved || raw.is_outdated {
                    continue;
                }
                if output.len() == MAX_THREADS {
                    return Err(invalid_json("more than 500 eligible review threads"));
                }
                let thread = decode_thread(raw)?;
                retained = retained.saturating_add(retained_bytes(&thread));
                if retained > MAX_RETAINED_THREAD_BYTES {
                    return Err(invalid_json("review threads exceed 8 MiB"));
                }
                output.push(thread);
            }

            if !page_info.has_next_page {
                break;
            }
            cursor = page_info.end_cursor.filter(|value| !value.is_empty());
            if cursor.is_none() {
                return Err(invalid_json("review thread page has no end cursor"));
            }
        }

        Ok(output)
    }

    fn review_threads_page(
        &mut self,
        number: u64,
        owner: &str,
        name: &str,
        cursor: Option<&str>,
    ) -> Result<String, GithubError> {
        let owner = format!("owner={owner}");
        let name = format!("name={name}");
        let number = format!("number={number}");
        let first = format!("first={THREAD_PAGE_SIZE}");
        let after = format!("after={}", cursor.unwrap_or("null"));
        let query = format!("query={REVIEW_THREADS_QUERY}");
        self.execute_text(
            GithubOperation::LoadReviewThreads,
            &[
                "api", "graphql", "-f", &query, "-F", &owner, "-F", &name, "-F", &number, "-F",
                &first, "-F", &after,
            ],
            CaptureLimits::new(THREAD_PAGE_STDOUT_LIMIT, STDERR_LIMIT, THREAD_TIMEOUT),
            None,
        )
    }
}

fn decode_thread(raw: RawThread) -> Result<GithubReviewThread, GithubError> {
    if raw.comments.page_info.has_next_page || raw.comments.nodes.len() > MAX_COMMENTS_PER_THREAD {
        return Err(invalid_json("review thread has more than 100 comments"));
    }
    if raw.comments.nodes.is_empty() {
        return Err(invalid_json("eligible review thread has no comments"));
    }

    let subject = match raw.subject_type.as_str() {
        "FILE" => GithubThreadSubject::File,
        "LINE" => GithubThreadSubject::Line {
            side: decode_side(raw.diff_side.as_deref()),
            start_side: decode_side(raw.start_diff_side.as_deref()),
            start_line: raw.start_line,
            end_line: raw.line,
        },
        value => GithubThreadSubject::Unsupported(clean(value)),
    };
    let mut comments = Vec::with_capacity(raw.comments.nodes.len());
    for comment in raw.comments.nodes {
        if comment.body_text.len() > MAX_COMMENT_BODY_BYTES {
            return Err(invalid_json("review comment body exceeds 64 KiB"));
        }
        comments.push(GithubThreadComment {
            id: clean(&comment.id),
            author: comment
                .author
                .map(|author| clean(&author.login))
                .unwrap_or_else(|| "[deleted]".into()),
            body: clean(&comment.body_text),
            created_at: clean(&comment.created_at),
            url: clean(&comment.url),
        });
    }
    let url = comments
        .first()
        .map(|comment| comment.url.clone())
        .unwrap_or_default();
    Ok(GithubReviewThread {
        id: clean(&raw.id),
        path: clean(&raw.path),
        is_resolved: raw.is_resolved,
        is_outdated: raw.is_outdated,
        subject,
        comments,
        url,
    })
}

fn decode_side(side: Option<&str>) -> Option<RemoteLineSide> {
    match side {
        Some("LEFT") => Some(RemoteLineSide::Left),
        Some("RIGHT") => Some(RemoteLineSide::Right),
        _ => None,
    }
}

fn clean(value: &str) -> String {
    sanitize_terminal_text(value, false)
}

fn invalid_json(detail: impl Into<String>) -> GithubError {
    GithubError::InvalidJson {
        operation: GithubOperation::LoadReviewThreads,
        detail: detail.into(),
    }
}

fn retained_bytes(thread: &GithubReviewThread) -> usize {
    thread.id.len()
        + thread.path.len()
        + thread.url.len()
        + thread
            .comments
            .iter()
            .map(|comment| {
                comment.id.len()
                    + comment.author.len()
                    + comment.body.len()
                    + comment.created_at.len()
                    + comment.url.len()
            })
            .sum::<usize>()
}

#[derive(Deserialize)]
struct RawEnvelope {
    data: RawData,
}

impl RawEnvelope {
    fn connection(self) -> Result<RawThreadConnection, GithubError> {
        self.data
            .repository
            .and_then(|repository| repository.pull_request)
            .map(|pull_request| pull_request.review_threads)
            .ok_or_else(|| invalid_json("missing repository or pull request"))
    }
}

#[derive(Deserialize)]
struct RawData {
    repository: Option<RawRepositoryThreads>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRepositoryThreads {
    pull_request: Option<RawPullRequestThreads>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPullRequestThreads {
    review_threads: RawThreadConnection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawThreadConnection {
    nodes: Vec<RawThread>,
    page_info: RawPageInfo,
}

#[derive(Deserialize)]
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
    comments: RawCommentConnection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCommentConnection {
    nodes: Vec<RawComment>,
    page_info: RawPageInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawComment {
    id: String,
    body_text: String,
    created_at: String,
    url: String,
    author: Option<RawThreadAuthor>,
}

#[derive(Deserialize)]
struct RawThreadAuthor {
    login: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}
