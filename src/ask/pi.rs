use std::ffi::OsString;
use std::fmt;
use std::io;
use std::time::Duration;

use crate::process::command::{CaptureLimits, CommandExecutor, CommandRequest, CommandResult};

/// The stored answer is capped well below this; the headroom exists so a runaway
/// reasoning dump is reported as `Truncated` instead of buffering without bound.
pub const PI_STDOUT_LIMIT: usize = 256 * 1024;
pub const PI_STDERR_LIMIT: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AskRequest {
    pub provider: String,
    pub model: String,
    pub thinking: String,
    pub timeout: Duration,
    pub prompt: String,
    pub system_prompt: String,
}

#[derive(Debug)]
pub enum AskError {
    MissingCli,
    ModelRejected {
        provider: String,
        model: String,
        stderr: String,
    },
    TimedOut {
        seconds: u64,
    },
    Truncated,
    InvalidUtf8,
    EmptyAnswer,
    Failed {
        code: Option<i32>,
        stderr: String,
    },
    Io(io::Error),
}

impl fmt::Display for AskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCli => write!(
                formatter,
                "pi was not found on PATH; install the pi CLI or set ask_enabled = false"
            ),
            Self::ModelRejected {
                provider,
                model,
                stderr,
            } => write!(
                formatter,
                "pi rejected model {provider}/{model}: {}; run `pi --list-models {model}` to see \
                 available ids, then set ask_model in your ramo config",
                first_line(stderr)
            ),
            Self::TimedOut { seconds } => write!(
                formatter,
                "the AI question timed out after {seconds}s; raise ask_timeout_secs or lower \
                 ask_thinking"
            ),
            Self::Truncated => write!(formatter, "the AI answer was too large to capture"),
            Self::InvalidUtf8 => write!(formatter, "pi returned output that is not valid UTF-8"),
            Self::EmptyAnswer => write!(
                formatter,
                "pi returned no answer text; try again or check `pi -p` manually"
            ),
            Self::Failed { code, stderr } => match code {
                Some(code) => write!(
                    formatter,
                    "pi exited with status {code}: {}",
                    first_line(stderr)
                ),
                None => write!(formatter, "pi was terminated: {}", first_line(stderr)),
            },
            Self::Io(source) => write!(formatter, "could not run pi: {source}"),
        }
    }
}

impl std::error::Error for AskError {}

fn first_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no error output")
}

pub struct PiCli<E> {
    executor: E,
}

impl<E> PiCli<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    pub fn into_executor(self) -> E {
        self.executor
    }
}

impl<E: CommandExecutor> PiCli<E> {
    pub fn ask(&mut self, request: &AskRequest) -> Result<String, AskError> {
        let argv = argv_for(request);
        let limits = CaptureLimits::new(PI_STDOUT_LIMIT, PI_STDERR_LIMIT, request.timeout);
        let result = self
            .executor
            .execute(CommandRequest {
                argv,
                stdin: Some(request.prompt.clone().into_bytes()),
                inherit_stdio: false,
                limits: Some(limits),
            })
            .map_err(|source| {
                if source.kind() == io::ErrorKind::NotFound {
                    AskError::MissingCli
                } else {
                    AskError::Io(source)
                }
            })?;
        validate_result(request, result)
    }
}

fn argv_for(request: &AskRequest) -> Vec<OsString> {
    [
        "pi",
        "-p",
        "--provider",
        &request.provider,
        "--model",
        &request.model,
        "--thinking",
        &request.thinking,
        // Nothing may execute and no transcript may be stored.
        "--no-session",
        "--no-tools",
        "--system-prompt",
        &request.system_prompt,
    ]
    .into_iter()
    .map(OsString::from)
    .collect()
}

fn validate_result(request: &AskRequest, result: CommandResult) -> Result<String, AskError> {
    if result.timed_out {
        return Err(AskError::TimedOut {
            seconds: request.timeout.as_secs(),
        });
    }
    if result.stdout_truncated {
        return Err(AskError::Truncated);
    }
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    if result.code != Some(0) {
        return Err(classify_failure(request, result.code, stderr));
    }
    let stdout = String::from_utf8(result.stdout).map_err(|_| AskError::InvalidUtf8)?;
    let answer = strip_pi_decoration(&stdout);
    if answer.is_empty() {
        return Err(AskError::EmptyAnswer);
    }
    Ok(answer)
}

pub(crate) fn classify_failure(
    request: &AskRequest,
    code: Option<i32>,
    stderr: String,
) -> AskError {
    let haystack = stderr.to_lowercase();
    let rejected = haystack.contains("model")
        && [
            "not found",
            "unknown",
            "no match",
            "unsupported",
            "not supported",
            "invalid",
            "unavailable",
        ]
        .iter()
        .any(|needle| haystack.contains(needle));
    if rejected {
        return AskError::ModelRejected {
            provider: request.provider.clone(),
            model: request.model.clone(),
            stderr,
        };
    }
    AskError::Failed { code, stderr }
}

/// `pi -p` writes the answer alone on stdout, but strip control sequences defensively
/// so a future banner or colored build cannot leak escape codes into a note card.
pub(crate) fn strip_pi_decoration(stdout: &str) -> String {
    let mut output = String::with_capacity(stdout.len());
    let mut characters = stdout.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            output.push(character);
            continue;
        }
        match characters.next() {
            // CSI: consume parameters and the final byte.
            Some('[') => {
                for escaped in characters.by_ref() {
                    if escaped.is_ascii_alphabetic() || escaped == '~' {
                        break;
                    }
                }
            }
            // OSC: consume through BEL or ST.
            Some(']') => {
                while let Some(escaped) = characters.next() {
                    if escaped == '\u{7}' {
                        break;
                    }
                    if escaped == '\u{1b}' && characters.peek() == Some(&'\\') {
                        characters.next();
                        break;
                    }
                }
            }
            Some(_) | None => {}
        }
    }
    output.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> AskRequest {
        AskRequest {
            provider: "opencode-go".into(),
            model: "deepseek-v4-flash".into(),
            thinking: "max".into(),
            timeout: Duration::from_secs(180),
            prompt: "QUESTION\nwhy?".into(),
            system_prompt: "be brief".into(),
        }
    }

    #[test]
    fn argv_disables_tools_and_sessions() {
        let argv = argv_for(&request())
            .into_iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            argv,
            vec![
                "pi",
                "-p",
                "--provider",
                "opencode-go",
                "--model",
                "deepseek-v4-flash",
                "--thinking",
                "max",
                "--no-session",
                "--no-tools",
                "--system-prompt",
                "be brief",
            ]
        );
    }

    #[test]
    fn a_rejected_model_is_classified_and_named() {
        let stderr =
            "Warning: Model \"deepseek-v4-bogus\" not found for provider \"opencode-go\".\n\
                      401: {\"type\":\"ModelError\",\"message\":\"Model is not supported\"}"
                .to_owned();
        let error = classify_failure(&request(), Some(1), stderr);

        assert!(matches!(error, AskError::ModelRejected { .. }));
        let message = error.to_string();
        assert!(message.contains("deepseek-v4-flash"), "{message}");
        assert!(message.contains("pi --list-models"), "{message}");
    }

    #[test]
    fn other_failures_stay_generic() {
        let error = classify_failure(&request(), Some(2), "network is unreachable".into());
        assert!(matches!(error, AskError::Failed { code: Some(2), .. }));
    }

    #[test]
    fn decoration_is_stripped_and_trimmed() {
        assert_eq!(
            strip_pi_decoration("\n\u{1b}[1mIt changes x.\u{1b}[0m\n\n"),
            "It changes x."
        );
        assert_eq!(strip_pi_decoration("plain answer\n"), "plain answer");
    }
}
