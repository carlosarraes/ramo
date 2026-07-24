use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteLineSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineCommentTarget {
    pub path: String,
    pub side: RemoteLineSide,
    pub start_line: u32,
    pub end_line: u32,
}

impl InlineCommentTarget {
    pub fn range(&self) -> RangeInclusive<u32> {
        self.start_line..=self.end_line
    }

    pub fn display_label(&self) -> String {
        let side = match self.side {
            RemoteLineSide::Left => "LEFT",
            RemoteLineSide::Right => "RIGHT",
        };
        if self.start_line == self.end_line {
            format!("{} {side}:{}", self.path, self.end_line)
        } else {
            format!("{} {side}:{}-{}", self.path, self.start_line, self.end_line)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    Comment,
    Approve,
    RequestChanges,
}

impl ReviewVerdict {
    pub const fn event_name(self) -> &'static str {
        match self {
            Self::Comment => "COMMENT",
            Self::Approve => "APPROVE",
            Self::RequestChanges => "REQUEST_CHANGES",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteReviewComment {
    pub target: InlineCommentTarget,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteReviewRequest {
    pub commit_id: String,
    pub body: String,
    pub verdict: ReviewVerdict,
    pub comments: Vec<RemoteReviewComment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteReviewError {
    pub message: String,
}

impl std::fmt::Display for RemoteReviewError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RemoteReviewError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestReviewContext {
    pub repository: String,
    pub repository_url: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub base_ref: String,
    pub head_ref: String,
    pub captured_revision: String,
    pub author_login: String,
    pub viewer_login: String,
}

impl PullRequestReviewContext {
    pub fn is_self_authored(&self) -> bool {
        self.author_login.eq_ignore_ascii_case(&self.viewer_login)
    }

    pub fn status_label(&self) -> String {
        format!(
            "GitHub PR #{} · {} · {} ← {}",
            self.number, self.title, self.base_ref, self.head_ref
        )
    }
}

pub trait RemoteReviewPublisher {
    fn current_revision(
        &mut self,
        context: &PullRequestReviewContext,
    ) -> Result<String, RemoteReviewError>;

    fn submit_review(
        &mut self,
        context: &PullRequestReviewContext,
        request: &RemoteReviewRequest,
    ) -> Result<(), RemoteReviewError>;
}
