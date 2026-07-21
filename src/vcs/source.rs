use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::diff::model::SourceSpec;

use super::{CommandRunner, CommandSpec, VcsError};

#[derive(Debug)]
pub enum SourceError {
    TooLarge {
        limit: usize,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    NonUtf8,
    Command(VcsError),
    Git {
        object: String,
        stderr: String,
    },
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { limit } => write!(formatter, "source text exceeded {limit} bytes"),
            Self::Io { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::NonUtf8 => formatter.write_str("source text is not valid UTF-8"),
            Self::Command(error) => write!(formatter, "{error}"),
            Self::Git { object, stderr } => {
                write!(
                    formatter,
                    "failed to read Git source {object}: {}",
                    stderr.trim()
                )
            }
        }
    }
}

impl Error for SourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Command(source) => Some(source),
            _ => None,
        }
    }
}

pub struct SourceReader<'a> {
    runner: &'a dyn CommandRunner,
    git_executable: &'a str,
    max_bytes: usize,
    cache: HashMap<SourceSpec, Option<String>>,
}

impl<'a> SourceReader<'a> {
    pub fn new(runner: &'a dyn CommandRunner, git_executable: &'a str, max_bytes: usize) -> Self {
        Self {
            runner,
            git_executable,
            max_bytes,
            cache: HashMap::new(),
        }
    }

    pub fn read(&mut self, spec: &SourceSpec) -> Result<Option<String>, SourceError> {
        if let Some(cached) = self.cache.get(spec) {
            return Ok(cached.clone());
        }
        let text = match spec {
            SourceSpec::None => None,
            SourceSpec::File(path) => Some(read_file(path, self.max_bytes)?),
            SourceSpec::GitBlob {
                repo_root,
                reference,
                path,
            } => self.read_git(repo_root, format!("{reference}:{path}"))?,
            SourceSpec::GitIndex { repo_root, path } => {
                self.read_git(repo_root, format!(":{path}"))?
            }
        };
        self.cache.insert(spec.clone(), text.clone());
        Ok(text)
    }

    fn read_git(
        &self,
        repo_root: &std::path::Path,
        object: String,
    ) -> Result<Option<String>, SourceError> {
        let size_output = self
            .runner
            .run(
                &CommandSpec::new(self.git_executable, repo_root)
                    .args(["cat-file", "-s", object.as_str()])
                    .accepted_exit_codes([0, 1, 128]),
            )
            .map_err(SourceError::Command)?;
        if size_output.code != 0 {
            let stderr = String::from_utf8_lossy(&size_output.stderr);
            if is_expected_missing(&stderr) {
                return Ok(None);
            }
            return Err(SourceError::Git {
                object,
                stderr: stderr.into_owned(),
            });
        }
        let size = String::from_utf8(size_output.stdout)
            .map_err(|_| SourceError::NonUtf8)?
            .trim()
            .parse::<usize>()
            .map_err(|_| SourceError::Git {
                object: object.clone(),
                stderr: "Git returned an invalid object size.".into(),
            })?;
        if size > self.max_bytes {
            return Err(SourceError::TooLarge {
                limit: self.max_bytes,
            });
        }
        let output = self
            .runner
            .run(
                &CommandSpec::new(self.git_executable, repo_root)
                    .args(["show", object.as_str()])
                    .capture_limit(self.max_bytes.saturating_add(1)),
            )
            .map_err(SourceError::Command)?;
        decode_bounded(output.stdout, self.max_bytes).map(Some)
    }
}

fn read_file(path: &std::path::Path, max_bytes: usize) -> Result<String, SourceError> {
    let mut file = File::open(path).map_err(|source| SourceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| SourceError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    decode_bounded(bytes, max_bytes)
}

fn decode_bounded(bytes: Vec<u8>, max_bytes: usize) -> Result<String, SourceError> {
    if bytes.len() > max_bytes {
        return Err(SourceError::TooLarge { limit: max_bytes });
    }
    String::from_utf8(bytes).map_err(|_| SourceError::NonUtf8)
}

fn is_expected_missing(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    [
        "exists on disk, but not in",
        "does not exist in",
        "invalid object name",
        "needed a single revision",
        "unknown revision or path not in the working tree",
    ]
    .iter()
    .any(|fragment| stderr.contains(fragment))
}
