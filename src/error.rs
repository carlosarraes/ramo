use std::error::Error;
use std::fmt;

use crate::cli::CliError;
use crate::config::ConfigError;
use crate::input::LoadError;
use crate::pager::PagerError;

#[derive(Debug)]
pub enum AppError {
    Cli(CliError),
    Config(ConfigError),
    Load(LoadError),
    Pager(PagerError),
    Io(std::io::Error),
}

impl AppError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Cli(_) | Self::Config(_) => 2,
            Self::Load(_) | Self::Pager(_) | Self::Io(_) => 1,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cli(error) => write!(formatter, "{error}"),
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Load(error) => write!(formatter, "{error}"),
            Self::Pager(error) => write!(formatter, "{error}"),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for AppError {}

impl From<CliError> for AppError {
    fn from(error: CliError) -> Self {
        Self::Cli(error)
    }
}

impl From<ConfigError> for AppError {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<LoadError> for AppError {
    fn from(error: LoadError) -> Self {
        Self::Load(error)
    }
}

impl From<PagerError> for AppError {
    fn from(error: PagerError) -> Self {
        Self::Pager(error)
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
