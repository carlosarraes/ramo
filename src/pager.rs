use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::{self, Write};
use std::process::{Command, Stdio};

use crate::input::sanitize_terminal_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub display: String,
}

#[derive(Debug)]
pub enum PagerError {
    InvalidSetting {
        variable: &'static str,
        message: String,
    },
    Spawn {
        program: String,
        source: io::Error,
    },
    Write(io::Error),
    Wait(io::Error),
}

impl fmt::Display for PagerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSetting { variable, message } => {
                write!(formatter, "invalid {variable}: {message}")
            }
            Self::Spawn { program, source } => {
                write!(formatter, "failed to start text pager {program}: {source}")
            }
            Self::Write(source) => write!(formatter, "failed to write to text pager: {source}"),
            Self::Wait(source) => write!(formatter, "failed to wait for text pager: {source}"),
        }
    }
}

impl Error for PagerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Spawn { source, .. } | Self::Write(source) | Self::Wait(source) => Some(source),
            Self::InvalidSetting { .. } => None,
        }
    }
}

pub fn resolve_text_pager(env: &BTreeMap<String, String>) -> Result<PagerSpec, PagerError> {
    let (variable, value) = env
        .get("RAMO_TEXT_PAGER")
        .map(|value| ("RAMO_TEXT_PAGER", value.as_str()))
        .or_else(|| env.get("PAGER").map(|value| ("PAGER", value.as_str())))
        .unwrap_or(("RAMO_TEXT_PAGER", "less -R"));

    let words = shell_words::split(value).map_err(|error| PagerError::InvalidSetting {
        variable,
        message: error.to_string(),
    })?;
    if words.is_empty() {
        return Ok(default_pager());
    }
    let spec = parse_words(variable, words)?;
    if is_ramo_program(&spec.program) {
        return Ok(default_pager());
    }
    Ok(spec)
}

fn parse_words(variable: &'static str, words: Vec<String>) -> Result<PagerSpec, PagerError> {
    let mut words = words.into_iter().peekable();
    let mut env = BTreeMap::new();
    if words.peek().is_some_and(|word| word == "env") {
        words.next();
    }
    while let Some(word) = words.peek() {
        let Some((name, value)) = assignment(word) else {
            break;
        };
        env.insert(name.to_string(), value.to_string());
        words.next();
    }

    let Some(program) = words.next() else {
        return Err(PagerError::InvalidSetting {
            variable,
            message: "expected a pager program".into(),
        });
    };
    let args: Vec<_> = words.collect();
    let display =
        shell_words::join(std::iter::once(program.as_str()).chain(args.iter().map(String::as_str)));
    Ok(PagerSpec {
        program,
        args,
        env,
        display,
    })
}

fn assignment(word: &str) -> Option<(&str, &str)> {
    let (name, value) = word.split_once('=')?;
    let mut characters = name.chars();
    let first = characters.next()?;
    if !(first.is_ascii_alphabetic() || first == '_')
        || !characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }
    Some((name, value))
}

fn is_ramo_program(program: &str) -> bool {
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_ascii_lowercase();
    name.strip_suffix(".exe")
        .or_else(|| name.strip_suffix(".cmd"))
        .unwrap_or(&name)
        == "ramo"
}

fn default_pager() -> PagerSpec {
    PagerSpec {
        program: "less".into(),
        args: vec!["-R".into()],
        env: BTreeMap::new(),
        display: "less -R".into(),
    }
}

pub fn page_plain_text(
    text: &str,
    spec: &PagerSpec,
    stdout_is_terminal: bool,
) -> Result<u8, PagerError> {
    if !stdout_is_terminal {
        print!("{}", sanitize_terminal_text(text, false));
        io::stdout().flush().map_err(PagerError::Write)?;
        return Ok(0);
    }

    let mut child = Command::new(&spec.program)
        .args(&spec.args)
        .envs(&spec.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|source| PagerError::Spawn {
            program: spec.display.clone(),
            source,
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        let result = stdin.write_all(sanitize_terminal_text(text, true).as_bytes());
        if let Err(source) = result
            && source.kind() != io::ErrorKind::BrokenPipe
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(PagerError::Write(source));
        }
    }
    let status = child.wait().map_err(PagerError::Wait)?;
    Ok(exit_status_code(status))
}

fn exit_status_code(status: std::process::ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return u8::try_from(code).unwrap_or(1);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return u8::try_from(128 + signal).unwrap_or(1);
        }
    }
    1
}
