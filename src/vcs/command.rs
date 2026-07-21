use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::VcsError;

pub const MAX_CAPTURE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub accepted_exit_codes: Vec<i32>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, cwd: &Path) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env: BTreeMap::new(),
            accepted_exit_codes: vec![0],
        }
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn accepted_exit_codes(mut self, codes: impl IntoIterator<Item = i32>) -> Self {
        self.accepted_exit_codes = codes.into_iter().collect();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, VcsError>;
}

#[derive(Debug, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, VcsError> {
        let output = Command::new(&spec.program)
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .envs(&spec.env)
            .stdin(Stdio::null())
            .output()
            .map_err(|source| VcsError::Spawn {
                program: spec.program.clone(),
                source,
            })?;

        if output.stdout.len() > MAX_CAPTURE_BYTES || output.stderr.len() > MAX_CAPTURE_BYTES {
            return Err(VcsError::OutputTooLarge {
                program: spec.program.clone(),
                limit: MAX_CAPTURE_BYTES,
            });
        }

        let code = output.status.code().unwrap_or(128);
        if !spec.accepted_exit_codes.contains(&code) {
            return Err(VcsError::Exit {
                program: spec.program.clone(),
                args: spec.args.clone(),
                code,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        Ok(CommandOutput {
            code,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}
