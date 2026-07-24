use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::path::Path;

use super::command::{CommandExecutor, CommandRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorCommand {
    pub argv: Vec<OsString>,
    pub suspend_terminal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorError {
    NotConfigured,
    InvalidCommand(String),
}

impl fmt::Display for EditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured => formatter.write_str("$EDITOR is not set"),
            Self::InvalidCommand(message) => {
                write!(formatter, "invalid $EDITOR command: {message}")
            }
        }
    }
}

impl Error for EditorError {}

#[derive(Debug)]
pub enum EditorLaunchError {
    Spawn(std::io::Error),
    Exit(Option<i32>),
}

impl fmt::Display for EditorLaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "failed to launch editor: {error}"),
            Self::Exit(Some(code)) => write!(formatter, "editor exited with status {code}"),
            Self::Exit(None) => formatter.write_str("editor terminated without an exit status"),
        }
    }
}

impl Error for EditorLaunchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Spawn(error) => Some(error),
            Self::Exit(_) => None,
        }
    }
}

pub struct EditorLauncher<E> {
    executor: E,
}

impl<E> EditorLauncher<E> {
    pub fn new(executor: E) -> Self {
        Self { executor }
    }

    pub fn into_executor(self) -> E {
        self.executor
    }
}

impl<E: CommandExecutor> EditorLauncher<E> {
    pub fn launch(&mut self, command: &EditorCommand) -> Result<(), EditorLaunchError> {
        let result = self
            .executor
            .execute(CommandRequest {
                argv: command.argv.clone(),
                stdin: None,
                inherit_stdio: true,
                limits: None,
            })
            .map_err(EditorLaunchError::Spawn)?;
        if result.code == Some(0) {
            Ok(())
        } else {
            Err(EditorLaunchError::Exit(result.code))
        }
    }
}

pub fn build_editor_command(
    editor: &str,
    path: &Path,
    line: u32,
) -> Result<EditorCommand, EditorError> {
    let mut argv = split_editor(editor)?
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    let program = normalized_program(&argv[0]);
    let line = line.max(1);
    match program.as_str() {
        "vi" | "vim" | "nvim" => {
            argv.push(OsString::from(format!("+{line}")));
            argv.push(path.as_os_str().to_owned());
        }
        "code" | "code-insiders" | "cursor" => {
            argv.push(OsString::from("--goto"));
            argv.push(OsString::from(format!("{}:{line}", path.display())));
        }
        "hx" => argv.push(OsString::from(format!("{}:{line}", path.display()))),
        _ => argv.push(path.as_os_str().to_owned()),
    }
    Ok(EditorCommand {
        argv,
        suspend_terminal: !matches!(program.as_str(), "code" | "code-insiders" | "cursor"),
    })
}

pub fn should_suspend_for_editor(editor: &str) -> Result<bool, EditorError> {
    let argv = split_editor(editor)?;
    let program = normalized_program(argv.first().expect("split editor is nonempty"));
    Ok(!matches!(
        program.as_str(),
        "code" | "code-insiders" | "cursor"
    ))
}

fn split_editor(editor: &str) -> Result<Vec<String>, EditorError> {
    if editor.trim().is_empty() {
        return Err(EditorError::NotConfigured);
    }
    let parseable = if editor.contains(":\\") {
        editor.replace('\\', "\\\\")
    } else {
        editor.to_owned()
    };
    let argv = shell_words::split(&parseable)
        .map_err(|error| EditorError::InvalidCommand(error.to_string()))?;
    if argv.is_empty() {
        Err(EditorError::NotConfigured)
    } else {
        Ok(argv)
    }
}

fn normalized_program(value: impl AsRef<std::ffi::OsStr>) -> String {
    let basename = value
        .as_ref()
        .to_string_lossy()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    basename
        .strip_suffix(".exe")
        .or_else(|| basename.strip_suffix(".cmd"))
        .unwrap_or(&basename)
        .to_owned()
}
