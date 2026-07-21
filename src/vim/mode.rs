#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    VisualLine { anchor: usize },
    CommentInsert,
    CommentNormal,
    Command,
    TmuxPanePick,
}

impl Mode {
    pub fn label(&self) -> &str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::VisualLine { .. } => "V-LINE",
            Mode::CommentInsert => "COMMENT",
            Mode::CommentNormal => "COMMENT",
            Mode::Command => "COMMAND",
            Mode::TmuxPanePick => "TMUX",
        }
    }

    pub fn is_comment(&self) -> bool {
        matches!(self, Mode::CommentInsert | Mode::CommentNormal)
    }
}
