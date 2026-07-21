mod file_pair;
mod patch;

use std::error::Error;
use std::fmt;
use std::io::Read;
use std::path::PathBuf;

use crate::config::ResolvedConfig;
use crate::core::changeset::Changeset;
use crate::core::input::{InputKind, ReviewInput};
use crate::vcs::{CommandRunner, SystemCommandRunner};

pub use patch::normalize_patch_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadPlan {
    None,
    Files {
        left: PathBuf,
        right: PathBuf,
    },
    Vcs {
        input: ReviewInput,
        repo_root: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub struct LoadedReview {
    pub changeset: Changeset,
    pub reload_plan: ReloadPlan,
}

#[derive(Debug, Default)]
pub struct ReviewLoader;

pub struct LoadContext<'a> {
    pub cwd: &'a std::path::Path,
    pub config: &'a ResolvedConfig,
    pub runner: &'a dyn CommandRunner,
}

impl ReviewLoader {
    pub fn load(
        &self,
        input: &ReviewInput,
        stdin: &mut dyn Read,
    ) -> Result<LoadedReview, LoadError> {
        let cwd = std::env::current_dir().map_err(|source| LoadError::Io {
            path: PathBuf::from("."),
            source,
        })?;
        let config = ResolvedConfig::default();
        let runner = SystemCommandRunner;
        self.load_with_context(
            input,
            stdin,
            &LoadContext {
                cwd: &cwd,
                config: &config,
                runner: &runner,
            },
        )
    }

    pub fn load_with_context(
        &self,
        input: &ReviewInput,
        stdin: &mut dyn Read,
        _context: &LoadContext<'_>,
    ) -> Result<LoadedReview, LoadError> {
        match input {
            ReviewInput::Patch { source, .. } => patch::load(source, stdin),
            ReviewInput::FilePair {
                left,
                right,
                display_path,
                ..
            } => file_pair::load(left, right, display_path.as_deref()),
            input => Err(LoadError::UnsupportedInput(input.kind())),
        }
    }
}

#[derive(Debug)]
pub enum LoadError {
    Stdin(std::io::Error),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    EmptyInput,
    InvalidPatch {
        source_label: String,
    },
    NonUtf8 {
        path: PathBuf,
    },
    UnsupportedInput(InputKind),
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdin(source) => write!(formatter, "failed to read patch from stdin: {source}"),
            Self::Io { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::EmptyInput => formatter.write_str("no diff input received"),
            Self::InvalidPatch { source_label } => {
                write!(formatter, "no parseable diff found in {source_label}")
            }
            Self::NonUtf8 { path } => {
                write!(formatter, "{} is not valid UTF-8 text", path.display())
            }
            Self::UnsupportedInput(kind) => {
                write!(formatter, "input loader for {kind:?} is not available")
            }
        }
    }
}

impl Error for LoadError {}
