use std::path::PathBuf;

use crate::config::ResolvedConfig;
use crate::core::input::{ReviewInput, VcsId};
use crate::diff::model::DiffFile;

use super::VcsError;
use super::command::CommandRunner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcsOperation {
    WorkingTree,
    RevisionShow,
    StashShow,
}

impl VcsOperation {
    pub fn kind_name(self) -> &'static str {
        match self {
            Self::WorkingTree => "working-tree diff",
            Self::RevisionShow => "revision show",
            Self::StashShow => "stash show",
        }
    }
}

#[derive(Debug, Clone)]
pub struct VcsPatch {
    pub vcs: VcsId,
    pub repo_root: PathBuf,
    pub source_label: String,
    pub title: String,
    pub patch_text: String,
    pub extra_files: Vec<DiffFile>,
    pub source_endpoints: Option<SourceEndpoints>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceEndpoint {
    None,
    Worktree {
        repo_root: PathBuf,
    },
    GitBlob {
        repo_root: PathBuf,
        reference: String,
    },
    GitIndex {
        repo_root: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEndpoints {
    pub old: SourceEndpoint,
    pub new: SourceEndpoint,
}

pub struct VcsLoadContext<'a> {
    pub cwd: &'a std::path::Path,
    pub config: &'a ResolvedConfig,
    pub runner: &'a dyn CommandRunner,
    pub git_executable: &'a str,
    pub jj_executable: &'a str,
    pub sl_executable: &'a str,
}

pub trait VcsAdapter {
    fn id(&self) -> VcsId;

    fn detect(&self, cwd: &std::path::Path) -> Option<PathBuf>;

    fn load(&self, input: &ReviewInput, context: &VcsLoadContext<'_>)
    -> Result<VcsPatch, VcsError>;
}
