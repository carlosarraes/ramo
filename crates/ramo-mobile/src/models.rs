use ramo_core::diff::model::LineType;
use ramo_core::diff::parser::parse_unified_diff;
use ramo_core::github::{ChangedFile, PullRequestSnapshot};
use ramo_core::remote_review::{GithubReviewThread, GithubThreadSubject, RemoteLineSide};
use ramo_core::syntax::{SyntaxHighlighter, SyntaxSpan};

use crate::MobileError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum MobileDraftSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct MobileDraftInput {
    pub repository: String,
    pub number: u64,
    pub captured_revision: String,
    pub path: String,
    pub side: MobileDraftSide,
    pub end_side: MobileDraftSide,
    pub start_line: u32,
    pub end_line: u32,
    pub start_hunk: u64,
    pub end_hunk: u64,
    pub context_before: Vec<String>,
    pub selected_text: Vec<String>,
    pub context_after: Vec<String>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct MobileDraftComment {
    pub id: String,
    pub repository: String,
    pub number: u64,
    pub captured_revision: String,
    pub path: String,
    pub side: MobileDraftSide,
    pub start_line: u32,
    pub end_line: u32,
    pub context_before: Vec<String>,
    pub selected_text: Vec<String>,
    pub context_after: Vec<String>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct MobileDraftReview {
    pub repository: String,
    pub number: u64,
    pub captured_revision: String,
    pub body: String,
    pub comments: Vec<MobileDraftComment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileReviewVerdict {
    Comment,
    Approve,
    RequestChanges,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobilePullRequestDetail {
    pub node_id: String,
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub author_login: String,
    pub viewer_login: String,
    pub base_ref: String,
    pub head_ref: String,
    pub captured_revision: String,
    pub additions: u64,
    pub deletions: u64,
    pub files: Vec<MobileFileSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileFileSummary {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub viewed: bool,
    pub binary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum MobileReviewMapStatus {
    Ready,
    Analyzing,
    Enriched,
    Stale,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum MobileReviewFileKind {
    Authored,
    Test,
    Generated,
    Migration,
    Documentation,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Enum)]
pub enum MobilePatchCoverage {
    Full,
    Truncated,
    MetadataOnly,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct MobileReviewMapFile {
    pub id: String,
    pub path: String,
    pub previous_path: Option<String>,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
    pub kind: MobileReviewFileKind,
    pub owner: Option<String>,
    pub coverage: MobilePatchCoverage,
    pub summary: Option<String>,
    pub risk: Option<String>,
    pub recommended_order: Option<u64>,
    pub viewed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct MobileReviewMapGroup {
    pub id: String,
    pub label: String,
    pub kind: MobileReviewFileKind,
    pub file_ids: Vec<String>,
    pub additions: u64,
    pub deletions: u64,
    pub collapsed_by_default: bool,
    pub summary: Option<String>,
    pub risk: Option<String>,
    pub review_priority: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct MobileReviewMap {
    pub schema_version: u64,
    pub repository: String,
    pub number: u64,
    pub base_sha: String,
    pub head_sha: String,
    pub status: MobileReviewMapStatus,
    pub file_count: u64,
    pub additions: u64,
    pub deletions: u64,
    pub authored_count: u64,
    pub test_count: u64,
    pub generated_count: u64,
    pub migration_count: u64,
    pub documentation_count: u64,
    pub groups: Vec<MobileReviewMapGroup>,
    pub files: Vec<MobileReviewMapFile>,
    pub analysis_model: Option<String>,
    pub analysis_completed_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileLineKind {
    Context,
    Addition,
    Deletion,
    Hunk,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileSyntaxSpan {
    pub text: String,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileDiffRow {
    pub key: String,
    pub hunk_index: u64,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub kind: MobileLineKind,
    pub spans: Vec<MobileSyntaxSpan>,
    pub commentable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileThreadComment {
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileReviewThread {
    pub id: String,
    pub path: String,
    pub side: Option<MobileCommentSide>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub resolved: bool,
    pub outdated: bool,
    pub url: String,
    pub comments: Vec<MobileThreadComment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MobileCommentSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MobileFileScreen {
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub pull_request_id: String,
    pub additions: u64,
    pub deletions: u64,
    pub file_index: u64,
    pub file_count: u64,
    pub viewed_count: u64,
    pub file: MobileFileSummary,
    pub rows: Vec<MobileDiffRow>,
    pub next_row: Option<u64>,
    pub threads: Vec<MobileReviewThread>,
}

pub fn mobile_pull_request(snapshot: &PullRequestSnapshot) -> MobilePullRequestDetail {
    MobilePullRequestDetail {
        node_id: snapshot.node_id.clone(),
        repository: snapshot.context.repository.clone(),
        number: snapshot.context.number,
        title: snapshot.context.title.clone(),
        author_login: snapshot.context.author_login.clone(),
        viewer_login: snapshot.context.viewer_login.clone(),
        base_ref: snapshot.context.base_ref.clone(),
        head_ref: snapshot.context.head_ref.clone(),
        captured_revision: snapshot.context.captured_revision.clone(),
        additions: snapshot
            .files
            .iter()
            .map(|file| file.additions as u64)
            .sum(),
        deletions: snapshot
            .files
            .iter()
            .map(|file| file.deletions as u64)
            .sum(),
        files: snapshot.files.iter().map(file_summary).collect(),
    }
}

pub fn build_file_screen(
    snapshot: &PullRequestSnapshot,
    threads: &[GithubReviewThread],
    file_index: usize,
    start_row: usize,
    row_limit: usize,
) -> Result<MobileFileScreen, MobileError> {
    build_file_screen_with_rows(snapshot, threads, file_index, start_row, row_limit, None)
}

pub fn build_file_screen_with_rows(
    snapshot: &PullRequestSnapshot,
    threads: &[GithubReviewThread],
    file_index: usize,
    start_row: usize,
    row_limit: usize,
    supplied_rows: Option<&[MobileDiffRow]>,
) -> Result<MobileFileScreen, MobileError> {
    let file = snapshot
        .files
        .get(file_index)
        .ok_or(MobileError::Unexpected)?;
    let generated_rows;
    let all_rows = if let Some(rows) = supplied_rows {
        rows
    } else {
        generated_rows = diff_rows(file);
        &generated_rows
    };
    let end = start_row.saturating_add(row_limit).min(all_rows.len());
    if start_row > end {
        return Err(MobileError::Unexpected);
    }
    Ok(MobileFileScreen {
        repository: snapshot.context.repository.clone(),
        number: snapshot.context.number,
        title: snapshot.context.title.clone(),
        pull_request_id: snapshot.node_id.clone(),
        additions: snapshot
            .files
            .iter()
            .map(|file| file.additions as u64)
            .sum(),
        deletions: snapshot
            .files
            .iter()
            .map(|file| file.deletions as u64)
            .sum(),
        file_index: file_index as u64,
        file_count: snapshot.files.len() as u64,
        viewed_count: snapshot.files.iter().filter(|file| file.viewed).count() as u64,
        file: file_summary(file),
        rows: all_rows[start_row..end].to_vec(),
        next_row: (end < all_rows.len()).then_some(end as u64),
        threads: threads
            .iter()
            .filter(|thread| thread.path == file.path)
            .map(review_thread)
            .collect(),
    })
}

fn file_summary(file: &ChangedFile) -> MobileFileSummary {
    MobileFileSummary {
        path: file.path.clone(),
        previous_path: file.previous_path.clone(),
        status: file.status.clone(),
        additions: file.additions as u64,
        deletions: file.deletions as u64,
        viewed: file.viewed,
        binary: file.binary,
    }
}

pub(crate) fn diff_rows(file: &ChangedFile) -> Vec<MobileDiffRow> {
    let Some(patch) = &file.patch else {
        return vec![MobileDiffRow {
            key: format!("{}:unavailable", file.path),
            hunk_index: 0,
            old_line: None,
            new_line: None,
            kind: MobileLineKind::Hunk,
            spans: vec![plain_span(if file.binary {
                "Binary file"
            } else {
                "Diff unavailable"
            })],
            commentable: false,
        }];
    };
    let synthetic = format!(
        "diff --git a/{0} b/{0}\n--- a/{0}\n+++ b/{0}\n{patch}\n",
        file.path
    );
    let parsed = parse_unified_diff(&synthetic);
    let Some(parsed_file) = parsed.first() else {
        return vec![MobileDiffRow {
            key: format!("{}:invalid", file.path),
            hunk_index: 0,
            old_line: None,
            new_line: None,
            kind: MobileLineKind::Hunk,
            spans: vec![plain_span("Diff could not be parsed")],
            commentable: false,
        }];
    };
    let mut highlighter = SyntaxHighlighter::tokyo_night();
    let mut rows = Vec::new();
    let mut previous_new_end = 0u32;
    for (hunk_index, hunk) in parsed_file.hunks.iter().enumerate() {
        if hunk.new_start > previous_new_end.saturating_add(1) {
            let start = previous_new_end.saturating_add(1);
            rows.push(gap_row(&file.path, start, hunk.new_start - 1));
        }
        rows.push(MobileDiffRow {
            key: format!("{}:hunk:{hunk_index}", file.path),
            hunk_index: hunk_index as u64,
            old_line: None,
            new_line: None,
            kind: MobileLineKind::Hunk,
            spans: vec![plain_span(&hunk.header)],
            commentable: false,
        });
        for (line_index, line) in hunk.lines.iter().enumerate() {
            let kind = match line.kind {
                LineType::Context => MobileLineKind::Context,
                LineType::Addition => MobileLineKind::Addition,
                LineType::Deletion => MobileLineKind::Deletion,
            };
            rows.push(MobileDiffRow {
                key: format!("{}:{hunk_index}:{line_index}", file.path),
                hunk_index: hunk_index as u64,
                old_line: line.old_lineno,
                new_line: line.new_lineno,
                kind,
                spans: highlighter
                    .highlight_line(&file.path, None, &line.content)
                    .into_iter()
                    .map(mobile_span)
                    .collect(),
                commentable: line.old_lineno.is_some() || line.new_lineno.is_some(),
            });
        }
        previous_new_end = hunk
            .lines
            .iter()
            .filter_map(|line| line.new_lineno)
            .max()
            .unwrap_or(previous_new_end);
    }
    rows
}

pub(crate) fn expand_diff_gap(
    file: &ChangedFile,
    gap_key: &str,
    source: &str,
) -> Result<Vec<MobileDiffRow>, MobileError> {
    let mut parts = gap_key.rsplit(':');
    let end: u32 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or(MobileError::Unexpected)?;
    let start: u32 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .ok_or(MobileError::Unexpected)?;
    if parts.next() != Some("gap") || start == 0 || end < start {
        return Err(MobileError::Unexpected);
    }
    let mut rows = diff_rows(file);
    let position = rows
        .iter()
        .position(|row| row.key == gap_key)
        .ok_or(MobileError::Unexpected)?;
    let lines = source.lines().collect::<Vec<_>>();
    if end as usize > lines.len() {
        return Err(MobileError::Unexpected);
    }
    let mut highlighter = SyntaxHighlighter::tokyo_night();
    let expanded = (start..=end)
        .map(|line| MobileDiffRow {
            key: format!("{}:expanded:{line}", file.path),
            hunk_index: u64::MAX,
            old_line: None,
            new_line: Some(line),
            kind: MobileLineKind::Context,
            spans: highlighter
                .highlight_line(&file.path, None, lines[line as usize - 1])
                .into_iter()
                .map(mobile_span)
                .collect(),
            commentable: false,
        })
        .collect::<Vec<_>>();
    rows.splice(position..=position, expanded);
    Ok(rows)
}

fn gap_row(path: &str, start: u32, end: u32) -> MobileDiffRow {
    MobileDiffRow {
        key: format!("{path}:gap:{start}:{end}"),
        hunk_index: u64::MAX,
        old_line: None,
        new_line: None,
        kind: MobileLineKind::Hunk,
        spans: vec![plain_span(&format!(
            "⋯ {} unchanged lines · tap to expand",
            end - start + 1
        ))],
        commentable: false,
    }
}

fn mobile_span(span: SyntaxSpan) -> MobileSyntaxSpan {
    MobileSyntaxSpan {
        text: span.text,
        red: span.foreground.red,
        green: span.foreground.green,
        blue: span.foreground.blue,
        bold: span.bold,
        italic: span.italic,
        underline: span.underline,
    }
}

fn plain_span(text: &str) -> MobileSyntaxSpan {
    MobileSyntaxSpan {
        text: text.to_owned(),
        red: 0xc0,
        green: 0xca,
        blue: 0xf5,
        bold: false,
        italic: false,
        underline: false,
    }
}

fn review_thread(thread: &GithubReviewThread) -> MobileReviewThread {
    let (side, start_line, end_line) = match &thread.subject {
        GithubThreadSubject::Line {
            side,
            start_side: _,
            start_line,
            end_line,
        } => (side.map(comment_side), *start_line, *end_line),
        _ => (None, None, None),
    };
    MobileReviewThread {
        id: thread.id.clone(),
        path: thread.path.clone(),
        side,
        start_line,
        end_line,
        resolved: thread.is_resolved,
        outdated: thread.is_outdated,
        url: thread.url.clone(),
        comments: thread
            .comments
            .iter()
            .map(|comment| MobileThreadComment {
                author: comment.author.clone(),
                body: comment.body.clone(),
                created_at: comment.created_at.clone(),
                url: comment.url.clone(),
            })
            .collect(),
    }
}

fn comment_side(side: RemoteLineSide) -> MobileCommentSide {
    match side {
        RemoteLineSide::Left => MobileCommentSide::Left,
        RemoteLineSide::Right => MobileCommentSide::Right,
    }
}

#[cfg(test)]
mod tests {
    use ramo_core::github::{ChangedFile, PullRequestSnapshot};
    use ramo_core::remote_review::PullRequestReviewContext;

    #[test]
    fn file_screen_is_bounded_and_highlighted() {
        let snapshot = PullRequestSnapshot {
            node_id: "PR_node".into(),
            context: PullRequestReviewContext {
                repository: "ramo/ramo".into(),
                repository_url: "https://github.com/ramo/ramo".into(),
                number: 7,
                title: "Mobile".into(),
                body: String::new(),
                url: "https://github.com/ramo/ramo/pull/7".into(),
                base_ref: "main".into(),
                base_revision: "base".into(),
                head_ref: "feature".into(),
                captured_revision: "head".into(),
                author_login: "author".into(),
                viewer_login: "reviewer".into(),
            },
            files: vec![ChangedFile {
                path: "src/lib.rs".into(),
                previous_path: None,
                status: "modified".into(),
                additions: 1,
                deletions: 1,
                patch: Some("@@ -1 +1 @@\n-let old = 1;\n+let new = \"ramo\";".into()),
                viewed: false,
                binary: false,
            }],
        };
        let screen = super::build_file_screen(&snapshot, &[], 0, 0, 2).unwrap();
        assert_eq!(screen.rows.len(), 2);
        assert_eq!(screen.next_row, Some(2));
        assert_eq!(screen.file.path, "src/lib.rs");
    }
}
