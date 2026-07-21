mod args;
mod normalize;

use std::error::Error;
use std::ffi::OsString;
use std::fmt;

use clap::{CommandFactory, Parser, error::ErrorKind};

use crate::core::input::{ReviewInput, ReviewOutput};
use crate::markup::MarkupRenderOptions;
use crate::session::SessionCommand;

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Print(String),
    Review(ReviewInput),
    InstallPi,
    UninstallPi,
    MarkupRender(MarkupRenderOptions),
    MarkupGuide,
    Session(SessionCommand),
    DaemonServe,
    SkillPath,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Invocation {
    pub action: Action,
    pub output: ReviewOutput,
}

#[derive(Debug)]
pub enum CliError {
    Parse(clap::Error),
    ConflictingInput,
    InvalidDiffTargets(Vec<String>),
    UnsupportedIntegration(String),
    Session(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "{error}"),
            Self::ConflictingInput => {
                formatter.write_str("--input cannot be combined with a review command")
            }
            Self::InvalidDiffTargets(targets) => write!(
                formatter,
                "ramo diff accepts one revision or two existing files; received {} target(s)",
                targets.len()
            ),
            Self::UnsupportedIntegration(target) => {
                write!(
                    formatter,
                    "unsupported integration target: {target}; expected pi"
                )
            }
            Self::Session(message) => formatter.write_str(message),
        }
    }
}

impl Error for CliError {}

pub fn parse_from<I, T>(args: I, stdin_is_terminal: bool) -> Result<Invocation, CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match args::Cli::try_parse_from(args) {
        Ok(cli) => normalize::normalize(cli, stdin_is_terminal),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Ok(Invocation {
                action: Action::Print(error.to_string()),
                output: ReviewOutput::default(),
            })
        }
        Err(error) => Err(CliError::Parse(error)),
    }
}

fn render_help() -> String {
    args::Cli::command().render_help().to_string()
}
