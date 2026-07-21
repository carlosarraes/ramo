use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutMode {
    #[default]
    Auto,
    Split,
    Stack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Diff,
    Show,
    StashShow,
    Patch,
    Pager,
    Difftool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VcsId {
    Git,
    Jj,
    Sl,
}

impl VcsId {
    pub fn executable(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Jj => "jj",
            Self::Sl => "sl",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommonOptions {
    pub mode: Option<LayoutMode>,
    pub theme: Option<String>,
    pub agent_context: Option<PathBuf>,
    pub pager: Option<bool>,
    pub watch: Option<bool>,
    pub exclude_untracked: Option<bool>,
    pub line_numbers: Option<bool>,
    pub wrap_lines: Option<bool>,
    pub hunk_headers: Option<bool>,
    pub agent_notes: Option<bool>,
    pub transparent_background: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewOutput {
    pub markdown_path: Option<PathBuf>,
    pub stdout: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchSource {
    Stdin,
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewInput {
    VcsDiff {
        range: Option<String>,
        staged: bool,
        pathspecs: Vec<String>,
        options: CommonOptions,
    },
    Show {
        reference: Option<String>,
        pathspecs: Vec<String>,
        options: CommonOptions,
    },
    StashShow {
        reference: Option<String>,
        options: CommonOptions,
    },
    FilePair {
        left: PathBuf,
        right: PathBuf,
        display_path: Option<PathBuf>,
        options: CommonOptions,
    },
    Patch {
        source: PatchSource,
        options: CommonOptions,
    },
    Pager {
        options: CommonOptions,
    },
}

impl ReviewInput {
    pub fn kind(&self) -> InputKind {
        match self {
            Self::VcsDiff { .. } => InputKind::Diff,
            Self::Show { .. } => InputKind::Show,
            Self::StashShow { .. } => InputKind::StashShow,
            Self::FilePair {
                display_path: Some(_),
                ..
            } => InputKind::Difftool,
            Self::FilePair { .. } => InputKind::Diff,
            Self::Patch { .. } => InputKind::Patch,
            Self::Pager { .. } => InputKind::Pager,
        }
    }

    pub fn options(&self) -> &CommonOptions {
        match self {
            Self::VcsDiff { options, .. }
            | Self::Show { options, .. }
            | Self::StashShow { options, .. }
            | Self::FilePair { options, .. }
            | Self::Patch { options, .. }
            | Self::Pager { options } => options,
        }
    }
}
