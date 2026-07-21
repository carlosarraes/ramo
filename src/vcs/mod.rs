mod command;
pub mod detect;
pub mod git;
pub mod jj;
pub mod source;
mod types;

use std::error::Error;
use std::fmt;

use crate::core::input::{InputKind, VcsId};

pub use command::{CommandOutput, CommandRunner, CommandSpec, SystemCommandRunner};
pub use types::{
    SourceEndpoint, SourceEndpoints, VcsAdapter, VcsLoadContext, VcsOperation, VcsPatch,
};

#[derive(Debug)]
pub enum VcsError {
    Spawn {
        program: String,
        source: std::io::Error,
    },
    Capture {
        program: String,
        source: std::io::Error,
    },
    Exit {
        program: String,
        args: Vec<String>,
        code: i32,
        stderr: String,
    },
    OutputTooLarge {
        program: String,
        limit: usize,
    },
    UnsupportedOperation {
        vcs: VcsId,
        operation: InputKind,
    },
    User {
        message: String,
        help: Vec<String>,
    },
}

impl fmt::Display for VcsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn { program, source } => {
                write!(formatter, "could not run {program}: {source}")
            }
            Self::Capture { program, source } => {
                write!(
                    formatter,
                    "failed to collect output from {program}: {source}"
                )
            }
            Self::Exit {
                program,
                args,
                code,
                stderr,
            } => write!(
                formatter,
                "{program} {} exited with {code}: {}",
                args.join(" "),
                stderr.trim()
            ),
            Self::OutputTooLarge { program, limit } => {
                write!(formatter, "{program} output exceeded {limit} bytes")
            }
            Self::UnsupportedOperation { vcs, operation } => {
                write!(formatter, "{vcs:?} does not support {operation:?}")
            }
            Self::User { message, help } => {
                write!(formatter, "{message}")?;
                for line in help {
                    write!(formatter, "\n{line}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for VcsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Spawn { source, .. } | Self::Capture { source, .. } => Some(source),
            _ => None,
        }
    }
}
