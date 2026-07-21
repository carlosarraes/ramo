mod file_pair;
mod pager;
mod patch;
mod vcs;

use std::error::Error;
use std::fmt;
use std::io::Read;
use std::path::PathBuf;

use crate::config::ResolvedConfig;
use crate::core::changeset::Changeset;
use crate::core::input::{InputKind, ReviewInput};
use crate::vcs::VcsError;
use crate::vcs::{CommandRunner, SystemCommandRunner};

pub use pager::{looks_like_patch, sanitize_terminal_text};
pub use patch::normalize_patch_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadPlan {
    None,
    Files {
        left: PathBuf,
        right: PathBuf,
        display_path: Option<PathBuf>,
    },
    PatchFile {
        path: PathBuf,
    },
    Vcs {
        input: ReviewInput,
        repo_root: PathBuf,
        vcs: crate::core::input::VcsId,
    },
}

#[derive(Debug, Clone)]
pub struct LoadedReview {
    pub changeset: Changeset,
    pub reload_plan: ReloadPlan,
    pub agent_context: crate::notes::AgentContextSource,
}

#[derive(Debug, Clone)]
pub enum LoadOutcome {
    Review(Box<LoadedReview>),
    PlainText(String),
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
        context: &LoadContext<'_>,
    ) -> Result<LoadedReview, LoadError> {
        let review_uses_stdin = matches!(
            input,
            ReviewInput::Patch {
                source: crate::core::input::PatchSource::Stdin,
                ..
            } | ReviewInput::Pager { .. }
        );
        let (agent_context, agent_source) = crate::notes::context::resolve_agent_context(
            input.options().agent_context.as_deref(),
            context.cwd,
            stdin,
            review_uses_stdin,
        )?;
        let mut loaded = match input {
            ReviewInput::Patch { source, .. } => patch::load(source, stdin),
            ReviewInput::FilePair {
                left,
                right,
                display_path,
                ..
            } => file_pair::load(left, right, display_path.as_deref()),
            ReviewInput::VcsDiff { .. }
            | ReviewInput::Show { .. }
            | ReviewInput::StashShow { .. } => vcs::load(input, context),
            input => Err(LoadError::UnsupportedInput(input.kind())),
        }?;
        if let Some(agent_context) = &agent_context {
            loaded.changeset.apply_agent_context(agent_context);
        }
        loaded.agent_context = agent_source;
        Ok(loaded)
    }

    pub fn load_outcome_with_context(
        &self,
        input: &ReviewInput,
        stdin: &mut dyn Read,
        context: &LoadContext<'_>,
    ) -> Result<LoadOutcome, LoadError> {
        if matches!(input, ReviewInput::Pager { .. }) {
            let (agent_context, agent_source) = crate::notes::context::resolve_agent_context(
                input.options().agent_context.as_deref(),
                context.cwd,
                stdin,
                true,
            )?;
            return match pager::load(stdin)? {
                LoadOutcome::Review(mut loaded) => {
                    if let Some(agent_context) = &agent_context {
                        loaded.changeset.apply_agent_context(agent_context);
                    }
                    loaded.agent_context = agent_source;
                    Ok(LoadOutcome::Review(loaded))
                }
                plain => Ok(plain),
            };
        }
        self.load_with_context(input, stdin, context)
            .map(Box::new)
            .map(LoadOutcome::Review)
    }

    pub fn reload(
        &self,
        plan: &ReloadPlan,
        context: &LoadContext<'_>,
    ) -> Result<LoadedReview, LoadError> {
        self.reload_with_agent(plan, &crate::notes::AgentContextSource::None, context)
    }

    pub fn reload_with_agent(
        &self,
        plan: &ReloadPlan,
        agent_source: &crate::notes::AgentContextSource,
        context: &LoadContext<'_>,
    ) -> Result<LoadedReview, LoadError> {
        let mut loaded = match plan {
            ReloadPlan::None => Err(LoadError::NotReloadable),
            ReloadPlan::Files {
                left,
                right,
                display_path,
            } => file_pair::load(left, right, display_path.as_deref()),
            ReloadPlan::PatchFile { path } => patch::load(
                &crate::core::input::PatchSource::File(path.clone()),
                &mut std::io::empty(),
            ),
            ReloadPlan::Vcs { input, .. } => vcs::load(input, context),
        }?;
        if let Some(agent_context) = crate::notes::context::reload_agent_context(agent_source)? {
            loaded.changeset.apply_agent_context(&agent_context);
        }
        loaded.agent_context = agent_source.clone();
        Ok(loaded)
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
    NotReloadable,
    AgentContext(crate::notes::AgentContextError),
    UnsupportedInput(InputKind),
    Vcs(VcsError),
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
            Self::NotReloadable => formatter
                .write_str("this review input cannot be reloaded because it came from stdin"),
            Self::AgentContext(error) => write!(formatter, "{error}"),
            Self::UnsupportedInput(kind) => {
                write!(formatter, "input loader for {kind:?} is not available")
            }
            Self::Vcs(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Stdin(source) | Self::Io { source, .. } => Some(source),
            Self::AgentContext(source) => Some(source),
            Self::Vcs(source) => Some(source),
            _ => None,
        }
    }
}

impl From<VcsError> for LoadError {
    fn from(error: VcsError) -> Self {
        Self::Vcs(error)
    }
}

impl From<crate::notes::AgentContextError> for LoadError {
    fn from(error: crate::notes::AgentContextError) -> Self {
        Self::AgentContext(error)
    }
}
