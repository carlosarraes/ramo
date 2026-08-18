use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crate::process::{CaptureLimits, CommandExecutor, CommandRequest, CommandResult};

/// The stored answer is capped well below this; the headroom exists so a runaway
/// reasoning dump is reported as `Truncated` instead of buffering without bound.
pub const PI_STDOUT_LIMIT: usize = 256 * 1024;
pub const PI_STDERR_LIMIT: usize = 32 * 1024;

/// What the model is allowed to do. Every caller states this explicitly, because it is the
/// difference between "nothing executes" and "the model can read your repository".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PiTools {
    /// `--no-tools`. Nothing at all, which is the Ask guarantee.
    #[default]
    None,
    /// `--no-builtin-tools -e <path>…`. Built-ins off, but extension tools stay available —
    /// how a schema-constrained tool becomes the model's only way to answer.
    ExtensionsOnly(Vec<PathBuf>),
    /// `--tools a,b`. An explicit allowlist, e.g. read-only repository access.
    Allow(Vec<String>),
}

/// Whether pi persists a transcript. `Ephemeral` is `--no-session`; `Id` names a session that
/// pi creates if missing, which is what makes a multi-turn conversation possible.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PiSession {
    #[default]
    Ephemeral,
    Id(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiRequest {
    pub provider: String,
    pub model: String,
    pub thinking: String,
    pub timeout: Duration,
    pub prompt: String,
    pub system_prompt: String,
    pub tools: PiTools,
    pub session: PiSession,
    /// Extra environment for the child. The Review Map uses it to tell its extension where to
    /// find the response schema and where to write the validated result.
    pub env: Vec<(std::ffi::OsString, std::ffi::OsString)>,
}

#[derive(Debug)]
pub enum PiError {
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

impl fmt::Display for PiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCli => write!(
                formatter,
                "pi was not found on PATH; install the pi CLI or set enabled = false in your ramo config"
            ),
            Self::ModelRejected {
                provider,
                model,
                stderr,
            } => write!(
                formatter,
                "pi rejected model {provider}/{model}: {}; run `pi --list-models {model}` to see \
                 available ids, then set the model in your ramo config",
                first_line(stderr)
            ),
            Self::TimedOut { seconds } => write!(
                formatter,
                "the AI request timed out after {seconds}s; raise timeout_secs or lower effort"
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

impl std::error::Error for PiError {}

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
    pub fn run(&mut self, request: &PiRequest) -> Result<String, PiError> {
        let argv = argv_for(request);
        let limits = CaptureLimits::new(PI_STDOUT_LIMIT, PI_STDERR_LIMIT, request.timeout);
        let result = self
            .executor
            .execute(CommandRequest {
                argv,
                env: request.env.clone(),
                stdin: Some(request.prompt.clone().into_bytes()),
                inherit_stdio: false,
                limits: Some(limits),
            })
            .map_err(|source| {
                if source.kind() == io::ErrorKind::NotFound {
                    PiError::MissingCli
                } else {
                    PiError::Io(source)
                }
            })?;
        validate_result(request, result)
    }
}

fn argv_for(request: &PiRequest) -> Vec<OsString> {
    let mut argv: Vec<OsString> = ["pi", "-p"].into_iter().map(OsString::from).collect();
    for (flag, value) in [
        ("--provider", &request.provider),
        ("--model", &request.model),
        ("--thinking", &request.thinking),
    ] {
        argv.push(OsString::from(flag));
        argv.push(OsString::from(value));
    }
    match &request.session {
        PiSession::Ephemeral => argv.push(OsString::from("--no-session")),
        PiSession::Id(id) => {
            argv.push(OsString::from("--session-id"));
            argv.push(OsString::from(id));
        }
    }
    match &request.tools {
        PiTools::None => argv.push(OsString::from("--no-tools")),
        PiTools::ExtensionsOnly(extensions) => {
            argv.push(OsString::from("--no-builtin-tools"));
            for extension in extensions {
                argv.push(OsString::from("-e"));
                argv.push(extension.clone().into_os_string());
            }
        }
        PiTools::Allow(tools) => {
            argv.push(OsString::from("--tools"));
            argv.push(OsString::from(tools.join(",")));
        }
    }
    argv.push(OsString::from("--system-prompt"));
    argv.push(OsString::from(&request.system_prompt));
    argv
}

fn validate_result(request: &PiRequest, result: CommandResult) -> Result<String, PiError> {
    if result.timed_out {
        return Err(PiError::TimedOut {
            seconds: request.timeout.as_secs(),
        });
    }
    if result.stdout_truncated {
        return Err(PiError::Truncated);
    }
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    if result.code != Some(0) {
        return Err(classify_failure(request, result.code, stderr));
    }
    let stdout = String::from_utf8(result.stdout).map_err(|_| PiError::InvalidUtf8)?;
    let answer = strip_pi_decoration(&stdout);
    if answer.is_empty() {
        return Err(PiError::EmptyAnswer);
    }
    Ok(answer)
}

pub(crate) fn classify_failure(request: &PiRequest, code: Option<i32>, stderr: String) -> PiError {
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
        return PiError::ModelRejected {
            provider: request.provider.clone(),
            model: request.model.clone(),
            stderr,
        };
    }
    PiError::Failed { code, stderr }
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

    fn request() -> PiRequest {
        PiRequest {
            provider: "openai-codex".into(),
            model: "gpt-5.6-luna".into(),
            thinking: "max".into(),
            timeout: Duration::from_secs(180),
            prompt: "QUESTION\nwhy?".into(),
            system_prompt: "be brief".into(),
            tools: PiTools::None,
            session: PiSession::Ephemeral,
            env: Vec::new(),
        }
    }

    fn argv_of(request: &PiRequest) -> Vec<String> {
        argv_for(request)
            .into_iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn no_tools_and_no_session_is_the_ask_shape() {
        assert_eq!(
            argv_of(&request()),
            vec![
                "pi",
                "-p",
                "--provider",
                "openai-codex",
                "--model",
                "gpt-5.6-luna",
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
    fn extension_tools_disable_the_builtins_rather_than_all_tools() {
        let mut request = request();
        request.tools = PiTools::ExtensionsOnly(vec![std::path::PathBuf::from("/tmp/schema.js")]);
        let argv = argv_of(&request);

        assert!(argv.contains(&"--no-builtin-tools".to_owned()), "{argv:?}");
        assert!(
            !argv.contains(&"--no-tools".to_owned()),
            "--no-tools would disable the extension too: {argv:?}"
        );
        assert!(
            argv.windows(2).any(|w| w == ["-e", "/tmp/schema.js"]),
            "{argv:?}"
        );
    }

    #[test]
    fn an_allowlist_and_a_named_session_render_for_chat() {
        let mut request = request();
        request.tools = PiTools::Allow(vec!["read".into()]);
        request.session = PiSession::Id("abc-123".into());
        let argv = argv_of(&request);

        assert!(
            argv.windows(2).any(|w| w == ["--tools", "read"]),
            "{argv:?}"
        );
        assert!(
            argv.windows(2).any(|w| w == ["--session-id", "abc-123"]),
            "{argv:?}"
        );
        assert!(!argv.contains(&"--no-session".to_owned()), "{argv:?}");
    }

    #[test]
    fn a_rejected_model_is_classified_and_named() {
        let stderr = "Warning: Model \"bogus\" not found for provider \"openai-codex\".\n\
                      401: {\"type\":\"ModelError\",\"message\":\"Model is not supported\"}"
            .to_owned();
        let error = classify_failure(&request(), Some(1), stderr);

        assert!(matches!(error, PiError::ModelRejected { .. }));
        let message = error.to_string();
        assert!(message.contains("gpt-5.6-luna"), "{message}");
        assert!(message.contains("pi --list-models"), "{message}");
    }

    #[test]
    fn other_failures_stay_generic() {
        let error = classify_failure(&request(), Some(2), "network is unreachable".into());
        assert!(matches!(error, PiError::Failed { code: Some(2), .. }));
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
