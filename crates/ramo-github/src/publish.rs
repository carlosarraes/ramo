use ramo_core::github::PullRequestKey;
use ramo_core::remote_review::{RemoteLineSide, RemoteReviewComment, RemoteReviewRequest};

use crate::pull_request::repository_parts;
use crate::{GithubClient, GithubError, GithubErrorKind};

const REST_ACCEPT: &str = "application/vnd.github+json";

impl GithubClient {
    pub fn current_revision(&self, key: &PullRequestKey) -> Result<String, GithubError> {
        repository_parts(key)?;
        let pull: CurrentPull = self.send_json(self.rest_request(
            reqwest::Method::GET,
            &format!("/repos/{}/pulls/{}", key.repository, key.number),
            REST_ACCEPT,
        ))?;
        if pull.head.sha.is_empty() {
            return Err(GithubError::new(
                GithubErrorKind::Decode,
                "GitHub pull request has no head revision",
            ));
        }
        Ok(pull.head.sha)
    }

    pub fn submit_review(
        &self,
        key: &PullRequestKey,
        expected_revision: &str,
        request: &RemoteReviewRequest,
    ) -> Result<(), GithubError> {
        let actual = self.current_revision(key)?;
        if actual != expected_revision {
            return Err(GithubError::new(
                GithubErrorKind::StaleRevision {
                    expected: expected_revision.into(),
                    actual,
                },
                "Pull request changed while you were reviewing; refresh before publishing",
            ));
        }
        if request.commit_id != expected_revision {
            return Err(GithubError::new(
                GithubErrorKind::Validation,
                "Review commit does not match the captured pull request revision",
            ));
        }
        let payload = ReviewPayload::from(request);
        let response = self
            .rest_request(
                reqwest::Method::POST,
                &format!("/repos/{}/pulls/{}/reviews", key.repository, key.number),
                REST_ACCEPT,
            )
            .json(&payload)
            .send()
            .map_err(GithubError::transport)?;
        Self::ensure_success(response)?;
        Ok(())
    }
}

#[derive(serde::Deserialize)]
struct CurrentPull {
    head: CurrentHead,
}

#[derive(serde::Deserialize)]
struct CurrentHead {
    sha: String,
}

#[derive(serde::Serialize)]
struct ReviewPayload<'a> {
    commit_id: &'a str,
    body: &'a str,
    event: &'a str,
    comments: Vec<ReviewCommentPayload<'a>>,
}

impl<'a> From<&'a RemoteReviewRequest> for ReviewPayload<'a> {
    fn from(request: &'a RemoteReviewRequest) -> Self {
        Self {
            commit_id: &request.commit_id,
            body: &request.body,
            event: request.verdict.event_name(),
            comments: request.comments.iter().map(Into::into).collect(),
        }
    }
}

#[derive(serde::Serialize)]
struct ReviewCommentPayload<'a> {
    path: &'a str,
    body: &'a str,
    line: u32,
    side: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start_side: Option<&'static str>,
}

impl<'a> From<&'a RemoteReviewComment> for ReviewCommentPayload<'a> {
    fn from(comment: &'a RemoteReviewComment) -> Self {
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
