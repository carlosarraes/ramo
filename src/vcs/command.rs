use std::collections::BTreeMap;
use std::io::Read;
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
    pub capture_limit: usize,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, cwd: &Path) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: cwd.into(),
            env: BTreeMap::new(),
            accepted_exit_codes: vec![0],
            capture_limit: MAX_CAPTURE_BYTES,
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

    pub fn capture_limit(mut self, bytes: usize) -> Self {
        self.capture_limit = bytes;
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
        let mut child = Command::new(&spec.program)
            .args(&spec.args)
            .current_dir(&spec.cwd)
            .envs(&spec.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| VcsError::Spawn {
                program: spec.program.clone(),
                source,
            })?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let limit = spec.capture_limit;
        let stdout_reader = std::thread::spawn(move || read_bounded(stdout, limit));
        let stderr_reader = std::thread::spawn(move || read_bounded(stderr, limit));
        let status = child.wait().map_err(|source| VcsError::Capture {
            program: spec.program.clone(),
            source,
        })?;
        let (stdout, stdout_exceeded) = join_reader(stdout_reader, &spec.program)?;
        let (stderr, stderr_exceeded) = join_reader(stderr_reader, &spec.program)?;
        if stdout_exceeded || stderr_exceeded {
            return Err(VcsError::OutputTooLarge {
                program: spec.program.clone(),
                limit,
            });
        }

        let code = status.code().unwrap_or(128);
        if !spec.accepted_exit_codes.contains(&code) {
            return Err(VcsError::Exit {
                program: spec.program.clone(),
                args: spec.args.clone(),
                code,
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
            });
        }

        Ok(CommandOutput {
            code,
            stdout,
            stderr,
        })
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> (Result<Vec<u8>, std::io::Error>, bool) {
    let mut captured = Vec::with_capacity(limit.min(64 * 1024));
    let mut exceeded = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => return (Ok(captured), exceeded),
            Ok(count) => count,
            Err(error) => return (Err(error), exceeded),
        };
        let remaining = limit.saturating_sub(captured.len());
        captured.extend_from_slice(&buffer[..count.min(remaining)]);
        exceeded |= count > remaining;
    }
}

fn join_reader(
    reader: std::thread::JoinHandle<(Result<Vec<u8>, std::io::Error>, bool)>,
    program: &str,
) -> Result<(Vec<u8>, bool), VcsError> {
    let (output, exceeded) = reader.join().map_err(|_| VcsError::User {
        message: format!("failed to collect output from {program}"),
        help: vec!["Retry the command.".into()],
    })?;
    output
        .map(|output| (output, exceeded))
        .map_err(|source| VcsError::Capture {
            program: program.into(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::read_bounded;

    #[test]
    fn bounded_reader_drains_but_retains_only_the_limit() {
        let (output, exceeded) = read_bounded(std::io::Cursor::new(b"12345"), 4);
        assert_eq!(output.unwrap(), b"1234");
        assert!(exceeded);
    }
}
