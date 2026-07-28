use crate::remote_review::RemoteLineSide;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DraftAnchor {
    pub repository: String,
    pub number: u64,
    pub captured_revision: String,
    pub path: String,
    pub side: RemoteLineSide,
    pub start_line: u32,
    pub end_line: u32,
    pub context_before: Vec<String>,
    pub selected_text: Vec<String>,
    pub context_after: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DraftComment {
    pub id: String,
    pub anchor: DraftAnchor,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DraftReview {
    pub repository: String,
    pub number: u64,
    pub captured_revision: String,
    pub body: String,
    pub comments: Vec<DraftComment>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DraftError {
    #[error("Comment body cannot be blank")]
    BlankBody,
    #[error("A comment range must stay on one diff side")]
    CrossSide,
    #[error("A comment range must stay in one diff hunk")]
    CrossHunk,
    #[error("A comment range needs a valid line")]
    InvalidLine,
}

pub fn create_draft(
    id: String,
    mut anchor: DraftAnchor,
    body: String,
    end_side: RemoteLineSide,
    start_hunk: u64,
    end_hunk: u64,
) -> Result<DraftComment, DraftError> {
    if body.trim().is_empty() {
        return Err(DraftError::BlankBody);
    }
    if anchor.start_line == 0 || anchor.end_line == 0 {
        return Err(DraftError::InvalidLine);
    }
    if anchor.side != end_side {
        return Err(DraftError::CrossSide);
    }
    if start_hunk != end_hunk {
        return Err(DraftError::CrossHunk);
    }
    if anchor.start_line > anchor.end_line {
        std::mem::swap(&mut anchor.start_line, &mut anchor.end_line);
        anchor.selected_text.reverse();
    }
    Ok(DraftComment { id, anchor, body })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReanchorResult {
    Exact(DraftComment),
    Moved {
        draft: DraftComment,
        old_line: u32,
        new_line: u32,
    },
    NeedsAttention {
        draft: DraftComment,
        reason: String,
    },
}

pub fn reanchor(
    draft: DraftComment,
    path: &str,
    side: RemoteLineSide,
    lines: &[String],
) -> ReanchorResult {
    if draft.anchor.path != path || draft.anchor.side != side {
        return ReanchorResult::NeedsAttention {
            draft,
            reason: "The file path or diff side changed".into(),
        };
    }
    let selected = &draft.anchor.selected_text;
    if selected.is_empty() {
        return ReanchorResult::NeedsAttention {
            draft,
            reason: "The selected text is unavailable".into(),
        };
    }
    let old = draft.anchor.start_line;
    let old_index = old.saturating_sub(1) as usize;
    if lines.get(old_index..old_index + selected.len()) == Some(selected.as_slice()) {
        return ReanchorResult::Exact(draft);
    }
    let needle = draft
        .anchor
        .context_before
        .iter()
        .chain(selected)
        .chain(&draft.anchor.context_after)
        .cloned()
        .collect::<Vec<_>>();
    if needle.is_empty() || needle.len() > lines.len() {
        return ReanchorResult::NeedsAttention {
            draft,
            reason: "The selected text no longer exists".into(),
        };
    }
    let matches = lines
        .windows(needle.len())
        .enumerate()
        .filter_map(|(index, window)| (window == needle).then_some(index))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return ReanchorResult::NeedsAttention {
            draft,
            reason: if matches.is_empty() {
                "The selected text no longer exists"
            } else {
                "The selected text appears more than once"
            }
            .into(),
        };
    }
    let context = draft.anchor.context_before.len() as u32;
    let new_line = matches[0] as u32 + context + 1;
    let mut moved = draft.clone();
    let length = moved.anchor.end_line - moved.anchor.start_line;
    moved.anchor.start_line = new_line;
    moved.anchor.end_line = new_line + length;
    ReanchorResult::Moved {
        draft: moved,
        old_line: old,
        new_line,
    }
}
