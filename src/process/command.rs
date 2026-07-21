use std::ffi::OsString;
use std::io::{self, Write};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRequest {
    pub argv: Vec<OsString>,
    pub stdin: Option<Vec<u8>>,
    pub inherit_stdio: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait CommandExecutor {
    fn execute(&mut self, request: CommandRequest) -> io::Result<CommandResult>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandExecutor;

impl CommandExecutor for SystemCommandExecutor {
    fn execute(&mut self, request: CommandRequest) -> io::Result<CommandResult> {
        let (program, arguments) = request
            .argv
            .split_first()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty command argv"))?;
        let mut command = Command::new(program);
        command.args(arguments);
        if request.stdin.is_some() {
            command.stdin(Stdio::piped());
        } else if request.inherit_stdio {
            command.stdin(Stdio::inherit());
        } else {
            command.stdin(Stdio::null());
        }
        if request.inherit_stdio {
            command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        } else {
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
        }
        let mut child = command.spawn()?;
        if let Some(input) = request.stdin
            && let Some(mut writer) = child.stdin.take()
        {
            writer.write_all(&input)?;
        }
        let output = child.wait_with_output()?;
        Ok(CommandResult {
            code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}
