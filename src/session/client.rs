use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use super::{
    CommentListType, CommentRevealMode, MAX_HTTP_BODY_BYTES, MAX_HTTP_RESPONSE_BYTES,
    MAX_SESSION_COMMENT_BATCH, SESSION_API_PATH, SESSION_API_VERSION, SESSION_CAPABILITIES_PATH,
    SESSION_DAEMON_VERSION, SessionCapabilities, SessionCommand, SessionDaemonOptions,
    SessionOutput, resolve_session_address, spawn_session_daemon,
};
use crate::core::input::{CommonOptions, PatchSource, ReviewInput};

#[derive(Debug, Clone, Copy)]
pub struct SessionClient {
    address: SocketAddr,
}

pub fn run_daemon_from_environment() -> io::Result<()> {
    let env: HashMap<_, _> = std::env::vars().collect();
    let address = resolve_session_address(&env)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let daemon = spawn_session_daemon(SessionDaemonOptions {
        address,
        ..SessionDaemonOptions::default()
    })?;
    eprintln!(
        "ramo session broker listening on http://{}{SESSION_API_PATH}",
        daemon.address()
    );
    daemon.wait();
    Ok(())
}

pub fn run_session_command(command: SessionCommand) -> io::Result<()> {
    let output = command_output(&command);
    let (input, timeout) = command_request(command)?;
    let client = ensure_session_daemon()?;
    let result = client.request(input, timeout)?;
    print_session_output(&result, output)
}

pub fn ensure_session_daemon() -> io::Result<SessionClient> {
    let env: HashMap<_, _> = std::env::vars().collect();
    let address = resolve_session_address(&env)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?
        .socket_addr();
    let client = SessionClient::new(address);
    match client.capabilities() {
        Ok(capabilities) if compatible(&capabilities) => return Ok(client),
        Ok(_) => {
            client.shutdown().map_err(|error| {
                io::Error::other(format!(
                    "an incompatible ramo session daemon is using {address} and could not be stopped: {error}"
                ))
            })?;
            wait_for_port_to_close(address)?;
        }
        Err(capability_error) => {
            if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!(
                        "port {address} is occupied by a foreign or incompatible service: {capability_error}"
                    ),
                ));
            }
        }
    }
    launch_daemon(address)?;
    Ok(client)
}

fn compatible(capabilities: &SessionCapabilities) -> bool {
    capabilities.version == SESSION_API_VERSION
        && capabilities.daemon_version == SESSION_DAEMON_VERSION
}

fn wait_for_port_to_close(address: SocketAddr) -> io::Result<()> {
    for _ in 0..100 {
        if TcpStream::connect_timeout(&address, Duration::from_millis(20)).is_err() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("timed out waiting for stale ramo daemon at {address} to stop"),
    ))
}

fn launch_daemon(address: SocketAddr) -> io::Result<()> {
    let executable = std::env::current_exe().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("could not locate the current ramo executable: {error}"),
        )
    })?;
    let mut daemon = Command::new(executable);
    daemon
        .args(["daemon", "serve"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach_daemon(&mut daemon);
    let mut child = daemon.spawn().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("could not launch the native ramo session daemon: {error}"),
        )
    })?;
    let client = SessionClient::new(address);
    for _ in 0..150 {
        if client.capabilities().is_ok_and(|value| compatible(&value)) {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!(
                "ramo session daemon exited before becoming ready: {status}"
            )));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("timed out waiting for the ramo session daemon at {address}"),
    ))
}

#[cfg(unix)]
fn detach_daemon(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}

#[cfg(windows)]
fn detach_daemon(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

fn command_output(command: &SessionCommand) -> SessionOutput {
    match command {
        SessionCommand::List { output }
        | SessionCommand::Get { output, .. }
        | SessionCommand::Context { output, .. }
        | SessionCommand::Review { output, .. }
        | SessionCommand::Navigate { output, .. }
        | SessionCommand::Reload { output, .. }
        | SessionCommand::CommentAdd { output, .. }
        | SessionCommand::CommentApply { output, .. }
        | SessionCommand::CommentList { output, .. }
        | SessionCommand::CommentRemove { output, .. }
        | SessionCommand::CommentClear { output, .. } => *output,
    }
}

fn command_request(command: SessionCommand) -> io::Result<(Value, Duration)> {
    let ordinary = Duration::from_secs(5);
    let long = Duration::from_secs(30);
    Ok(match command {
        SessionCommand::List { .. } => (json!({"action":"list"}), ordinary),
        SessionCommand::Get { selector, .. } => {
            (json!({"action":"get","selector":selector}), ordinary)
        }
        SessionCommand::Context { selector, .. } => {
            (json!({"action":"context","selector":selector}), ordinary)
        }
        SessionCommand::Review {
            selector,
            include_patch,
            include_notes,
            ..
        } => (
            json!({"action":"review","selector":selector,"includePatch":include_patch,"includeNotes":include_notes}),
            ordinary,
        ),
        SessionCommand::Navigate {
            selector,
            file_path,
            hunk_number,
            side,
            line,
            comment_direction,
            ..
        } => (
            json!({
                "action":"navigate",
                "selector":selector,
                "filePath":file_path,
                "hunkNumber":hunk_number,
                "side":side.map(|side| side.as_str()),
                "line":line,
                "commentDirection":comment_direction.map(|direction| direction.as_str()),
            }),
            ordinary,
        ),
        SessionCommand::Reload {
            selector,
            next_input,
            source_path,
            ..
        } => (
            json!({
                "action":"reload",
                "selector":selector,
                "nextInput":review_input_json(&next_input),
                "sourcePath":source_path.map(|path| path.to_string_lossy().into_owned()),
            }),
            long,
        ),
        SessionCommand::CommentAdd {
            selector,
            file_path,
            side,
            line,
            summary,
            rationale,
            markup,
            author,
            reveal,
            ..
        } => (
            json!({
                "action":"comment-add","selector":selector,"filePath":file_path,
                "side":side.as_str(),"line":line,"summary":summary,"rationale":rationale,
                "markup":markup,"author":author,"reveal":reveal,
            }),
            ordinary,
        ),
        SessionCommand::CommentApply {
            selector,
            read_stdin,
            reveal_mode,
            ..
        } => {
            if !read_stdin {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "comment batches require --stdin",
                ));
            }
            let batch = read_comment_batch()?;
            (
                json!({
                    "action":"comment-apply","selector":selector,"comments":batch,
                    "revealMode":match reveal_mode { CommentRevealMode::None => "none", CommentRevealMode::First => "first" },
                }),
                long,
            )
        }
        SessionCommand::CommentList {
            selector,
            file_path,
            note_type,
            ..
        } => (
            json!({
                "action":"comment-list","selector":selector,"filePath":file_path,
                "type":note_type.map(comment_list_type),
            }),
            ordinary,
        ),
        SessionCommand::CommentRemove {
            selector,
            comment_id,
            ..
        } => (
            json!({"action":"comment-rm","selector":selector,"commentId":comment_id}),
            ordinary,
        ),
        SessionCommand::CommentClear {
            selector,
            file_path,
            include_user,
            ..
        } => (
            json!({"action":"comment-clear","selector":selector,"filePath":file_path,"includeUser":include_user}),
            ordinary,
        ),
    })
}

fn comment_list_type(note_type: CommentListType) -> &'static str {
    match note_type {
        CommentListType::Live => "live",
        CommentListType::All => "all",
        CommentListType::Ai => "ai",
        CommentListType::Agent => "agent",
        CommentListType::User => "user",
    }
}

fn read_comment_batch() -> io::Result<Value> {
    let mut bytes = Vec::new();
    io::stdin()
        .take((MAX_HTTP_BODY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_HTTP_BODY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "comment batch exceeds 256 KiB",
        ));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let comments = value
        .as_array()
        .cloned()
        .or_else(|| value.get("comments").and_then(Value::as_array).cloned())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "comment batch must be a JSON array or an object with a comments array",
            )
        })?;
    if comments.len() > MAX_SESSION_COMMENT_BATCH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("comment batch exceeds {MAX_SESSION_COMMENT_BATCH} comments"),
        ));
    }
    Ok(Value::Array(comments))
}

fn review_input_json(input: &ReviewInput) -> Value {
    match input {
        ReviewInput::VcsDiff {
            range,
            staged,
            pathspecs,
            options,
        } => {
            json!({"kind":"diff","range":range,"staged":staged,"pathspecs":pathspecs,"options":options_json(options)})
        }
        ReviewInput::Show {
            reference,
            pathspecs,
            options,
        } => {
            json!({"kind":"show","reference":reference,"pathspecs":pathspecs,"options":options_json(options)})
        }
        ReviewInput::StashShow { reference, options } => {
            json!({"kind":"stash","reference":reference,"options":options_json(options)})
        }
        ReviewInput::FilePair {
            left,
            right,
            display_path,
            options,
        } => json!({
            "kind":"files","left":left.to_string_lossy(),"right":right.to_string_lossy(),
            "displayPath":display_path.as_ref().map(|path| path.to_string_lossy()),"options":options_json(options),
        }),
        ReviewInput::Patch {
            source: PatchSource::File(path),
            options,
        } => json!({"kind":"patch","path":path.to_string_lossy(),"options":options_json(options)}),
        ReviewInput::Patch {
            source: PatchSource::Stdin,
            ..
        }
        | ReviewInput::Pager { .. } => Value::Null,
    }
}

fn options_json(options: &CommonOptions) -> Value {
    json!({
        "mode":options.mode.map(|mode| format!("{mode:?}").to_ascii_lowercase()),
        "theme":options.theme,"agentContext":options.agent_context.as_ref().map(|path| path.to_string_lossy()),
        "pager":options.pager,"watch":options.watch,"excludeUntracked":options.exclude_untracked,
        "lineNumbers":options.line_numbers,"wrapLines":options.wrap_lines,"hunkHeaders":options.hunk_headers,
        "agentNotes":options.agent_notes,"transparentBackground":options.transparent_background,
    })
}

fn print_session_output(value: &Value, output: SessionOutput) -> io::Result<()> {
    match output {
        SessionOutput::Json => println!("{}", serde_json::to_string(value)?),
        SessionOutput::Text => print_text_output(value),
    }
    Ok(())
}

fn print_text_output(value: &Value) {
    if let Some(sessions) = value.get("sessions").and_then(Value::as_array) {
        if sessions.is_empty() {
            println!("No live ramo sessions.");
        } else {
            for session in sessions {
                println!(
                    "{}  {}  {}",
                    string_field(session, "sessionId"),
                    string_field(session, "title"),
                    string_field(session, "sourceLabel")
                );
            }
        }
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        );
    }
}

fn string_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("-")
}

impl SessionClient {
    pub const fn new(address: SocketAddr) -> Self {
        Self { address }
    }

    pub const fn address(self) -> SocketAddr {
        self.address
    }

    pub fn capabilities(self) -> io::Result<SessionCapabilities> {
        serde_json::from_value(self.get_json(SESSION_CAPABILITIES_PATH)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    pub fn get_json(self, path: &str) -> io::Result<Value> {
        self.send("GET", path, &[], Duration::from_secs(5))
    }

    pub fn request(self, input: Value, timeout: Duration) -> io::Result<Value> {
        let body = serde_json::to_vec(&input)?;
        self.send("POST", SESSION_API_PATH, &body, timeout)
    }

    pub fn shutdown(self) -> io::Result<()> {
        self.send("POST", "/daemon/shutdown", &[], Duration::from_secs(5))?;
        Ok(())
    }

    fn send(self, method: &str, path: &str, body: &[u8], timeout: Duration) -> io::Result<Value> {
        let mut stream = TcpStream::connect_timeout(&self.address, timeout).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "could not connect to the ramo session broker at {}: {error}",
                    self.address
                ),
            )
        })?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n",
            self.address
        )?;
        if method == "POST" {
            write!(
                stream,
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            )?;
        }
        stream.write_all(b"\r\n")?;
        stream.write_all(body)?;
        stream.flush()?;

        let mut response = Vec::new();
        stream
            .take((MAX_HTTP_RESPONSE_BYTES + 1) as u64)
            .read_to_end(&mut response)?;
        if response.len() > MAX_HTTP_RESPONSE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "ramo session response exceeds 1 MiB",
            ));
        }
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid HTTP response from broker",
                )
            })?;
        let head = std::str::from_utf8(&response[..header_end])
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse::<u16>().ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid HTTP status from broker",
                )
            })?;
        let value: Value = serde_json::from_slice(&response[header_end + 4..])
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if !(200..300).contains(&status) {
            let message = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown ramo session broker error");
            return Err(io::Error::other(message.to_owned()));
        }
        Ok(value)
    }
}
