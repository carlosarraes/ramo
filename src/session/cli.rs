use std::path::PathBuf;

use crate::core::input::ReviewInput;

use super::SessionSelector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOutput {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSide {
    Old,
    New,
}

impl DiffSide {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::New => "new",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentDirection {
    Next,
    Prev,
}

impl CommentDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Next => "next",
            Self::Prev => "prev",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentRevealMode {
    None,
    First,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentListType {
    Live,
    All,
    Ai,
    Agent,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
    List {
        output: SessionOutput,
    },
    Get {
        selector: SessionSelector,
        output: SessionOutput,
    },
    Context {
        selector: SessionSelector,
        output: SessionOutput,
    },
    Review {
        selector: SessionSelector,
        include_patch: bool,
        include_notes: bool,
        output: SessionOutput,
    },
    Navigate {
        selector: SessionSelector,
        file_path: Option<String>,
        hunk_number: Option<usize>,
        side: Option<DiffSide>,
        line: Option<u32>,
        comment_direction: Option<CommentDirection>,
        output: SessionOutput,
    },
    Reload {
        selector: SessionSelector,
        next_input: ReviewInput,
        source_path: Option<PathBuf>,
        output: SessionOutput,
    },
    CommentAdd {
        selector: SessionSelector,
        file_path: String,
        side: DiffSide,
        line: u32,
        summary: String,
        rationale: Option<String>,
        markup: Option<String>,
        author: Option<String>,
        reveal: bool,
        output: SessionOutput,
    },
    CommentApply {
        selector: SessionSelector,
        read_stdin: bool,
        reveal_mode: CommentRevealMode,
        output: SessionOutput,
    },
    CommentList {
        selector: SessionSelector,
        file_path: Option<String>,
        note_type: Option<CommentListType>,
        output: SessionOutput,
    },
    CommentRemove {
        selector: SessionSelector,
        comment_id: String,
        output: SessionOutput,
    },
    CommentClear {
        selector: SessionSelector,
        file_path: Option<String>,
        include_user: bool,
        output: SessionOutput,
    },
}
