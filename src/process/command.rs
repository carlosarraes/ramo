use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureLimits {
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub timeout: Duration,
}

impl CaptureLimits {
    pub const fn new(stdout_bytes: usize, stderr_bytes: usize, timeout: Duration) -> Self {
        Self {
            stdout_bytes,
            stderr_bytes,
            timeout,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRequest {
    pub argv: Vec<OsString>,
    pub stdin: Option<Vec<u8>>,
    pub inherit_stdio: bool,
    pub limits: Option<CaptureLimits>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub timed_out: bool,
}

pub trait CommandExecutor {
    fn execute(&mut self, request: CommandRequest) -> io::Result<CommandResult>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandExecutor;

impl CommandExecutor for SystemCommandExecutor {
    fn execute(&mut self, request: CommandRequest) -> io::Result<CommandResult> {
        if request.inherit_stdio && request.limits.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "capture limits require captured stdio",
            ));
        }
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
        let Some(limits) = request.limits else {
            if let Some(input) = request.stdin
                && let Some(mut writer) = child.stdin.take()
            {
                writer.write_all(&input)?;
            }
            let output = child.wait_with_output()?;
            return Ok(CommandResult {
                code: output.status.code(),
                stdout: output.stdout,
                stderr: output.stderr,
                stdout_truncated: false,
                stderr_truncated: false,
                timed_out: false,
            });
        };

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("captured child stdout is unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("captured child stderr is unavailable"))?;
        let stdout_reader = std::thread::spawn(move || read_bounded(stdout, limits.stdout_bytes));
        let stderr_reader = std::thread::spawn(move || read_bounded(stderr, limits.stderr_bytes));

        if let Some(input) = request.stdin
            && let Some(mut writer) = child.stdin.take()
            && let Err(error) = writer.write_all(&input)
        {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(error);
        }

        let deadline = Instant::now() + limits.timeout;
        let (status, timed_out) = loop {
            if let Some(status) = child.try_wait()? {
                break (status, false);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                break (child.wait()?, true);
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let stdout = join_reader(stdout_reader)?;
        let stderr = join_reader(stderr_reader)?;
        Ok(CommandResult {
            code: status.code(),
            stdout: stdout.bytes,
            stderr: stderr.bytes,
            stdout_truncated: stdout.truncated,
            stderr_truncated: stderr.truncated,
            timed_out,
        })
    }
}

struct BoundedCapture {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<BoundedCapture> {
    let mut bytes = Vec::with_capacity(limit.min(8192));
    let mut chunk = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let retained = count.min(remaining);
        bytes.extend_from_slice(&chunk[..retained]);
        truncated |= retained < count;
    }
    Ok(BoundedCapture { bytes, truncated })
}

fn join_reader(
    reader: std::thread::JoinHandle<io::Result<BoundedCapture>>,
) -> io::Result<BoundedCapture> {
    reader
        .join()
        .map_err(|_| io::Error::other("captured output reader panicked"))?
}
