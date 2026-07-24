use std::collections::VecDeque;
use std::ffi::OsString;
use std::io;

#[cfg(unix)]
use ramo::process::command::SystemCommandExecutor;
use ramo::process::command::{CommandExecutor, CommandRequest, CommandResult};
use ramo::tmux::{PasteMode, TmuxClient};

fn argv(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn success(stdout: &str) -> io::Result<CommandResult> {
    Ok(CommandResult {
        code: Some(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: false,
    })
}

struct RecordingExecutor {
    requests: Vec<CommandRequest>,
    results: VecDeque<io::Result<CommandResult>>,
}

impl RecordingExecutor {
    fn successful(count: usize) -> Self {
        Self {
            requests: Vec::new(),
            results: (0..count).map(|_| success("")).collect(),
        }
    }
}

impl CommandExecutor for RecordingExecutor {
    fn execute(&mut self, request: CommandRequest) -> io::Result<CommandResult> {
        self.requests.push(request);
        self.results.pop_front().unwrap()
    }
}

#[test]
fn tmux_send_uses_literal_stdin_and_never_a_shell() {
    let executor = RecordingExecutor::successful(2);
    let mut tmux = TmuxClient::new(executor);
    tmux.send_to_pane("%7", "line one\n$(touch /tmp/nope)", PasteMode::Bracketed)
        .unwrap();
    let executor = tmux.into_executor();

    assert_eq!(
        executor.requests[0].argv,
        argv(&["tmux", "load-buffer", "-b", "ramo-send", "-"])
    );
    assert_eq!(
        executor.requests[0].stdin.as_deref(),
        Some(b"line one\n$(touch /tmp/nope)".as_slice())
    );
    assert!(!executor.requests[0].inherit_stdio);
    assert_eq!(
        executor.requests[1].argv,
        argv(&[
            "tmux",
            "paste-buffer",
            "-p",
            "-r",
            "-b",
            "ramo-send",
            "-t",
            "%7",
            "-d",
        ])
    );
}

#[test]
fn tmux_list_filters_the_current_pane_and_preserves_target_metadata() {
    let executor = RecordingExecutor {
        requests: Vec::new(),
        results: VecDeque::from([success(
            "%1\twork:0.0\teditor\tnvim\n%2\twork:0.1\tagent\tpi\n",
        )]),
    };
    let mut tmux = TmuxClient::with_self_pane(executor, Some("%1".into()));
    let panes = tmux.list_panes().unwrap();

    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].id, "%2");
    assert_eq!(panes[0].current_command, "pi");
    assert!(panes[0].label.contains("work:0.1"));
}

#[test]
fn osc52_encodes_wide_character_selection_exactly() {
    let mut output = Vec::new();
    ramo::clipboard::write_osc52(&mut output, "界 old").unwrap();
    assert_eq!(output, b"\x1b]52;c;55WMIG9sZA==\x07");
}

#[test]
fn tmux_plain_paste_and_failures_are_operation_specific() {
    let mut tmux = TmuxClient::new(RecordingExecutor::successful(2));
    tmux.send_to_pane("%2", "review", PasteMode::Plain).unwrap();
    let executor = tmux.into_executor();
    assert_eq!(
        executor.requests[1].argv,
        argv(&["tmux", "paste-buffer", "-b", "ramo-send", "-t", "%2", "-d",])
    );

    let failure = CommandResult {
        code: Some(9),
        stdout: Vec::new(),
        stderr: b"permission denied".to_vec(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: false,
    };
    let mut tmux = TmuxClient::new(RecordingExecutor {
        requests: Vec::new(),
        results: VecDeque::from([Ok(failure)]),
    });
    let error = tmux
        .send_to_pane("%2", "review", PasteMode::Plain)
        .unwrap_err()
        .to_string();
    assert!(error.contains("tmux load buffer failed with status 9"));
    assert!(error.contains("permission denied"));
}

#[cfg(unix)]
#[test]
fn real_tmux_server_receives_the_exact_native_buffer() {
    if std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .is_err()
    {
        return;
    }
    let socket = format!("ramo-test-{}", std::process::id());
    let session = "ramo-native-smoke";
    let start = std::process::Command::new("tmux")
        .args(["-L", &socket, "new-session", "-d", "-s", session, "cat"])
        .output()
        .unwrap();
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );

    let result = (|| {
        let mut tmux = TmuxClient::with_server(SystemCommandExecutor, socket.clone());
        let pane = tmux.list_panes()?.into_iter().next().unwrap();
        tmux.send_to_pane(&pane.id, "native tmux smoke", PasteMode::Plain)?;
        for _ in 0..50 {
            let output = std::process::Command::new("tmux")
                .args(["-L", &socket, "capture-pane", "-p", "-t", &pane.id])
                .output()?;
            let text = String::from_utf8_lossy(&output.stdout);
            if text.contains("native tmux smoke") {
                return Ok::<_, io::Error>(());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Err(io::Error::other(
            "tmux pane did not receive the native buffer",
        ))
    })();
    let _ = std::process::Command::new("tmux")
        .args(["-L", &socket, "kill-server"])
        .status();
    result.unwrap();
}

#[test]
fn pi_install_writes_a_markdown_prompt_and_no_typescript() {
    let home = tempfile::tempdir().unwrap();
    let installed = ramo::pi_extension::install_at(home.path()).unwrap();
    assert_eq!(installed, home.path().join(".pi/agent/prompts/ramo.md"));
    let text = std::fs::read_to_string(&installed).unwrap();
    assert!(text.contains("ramo diff --staged"));
    assert!(text.contains("ramo show"));
    assert!(text.contains("--output"));
    assert!(!text.contains("registerCommand"));
    assert!(
        !home
            .path()
            .join(".pi/agent/extensions/ramo/index.ts")
            .exists()
    );

    ramo::pi_extension::uninstall_at(home.path()).unwrap();
    assert!(!installed.exists());
}

#[test]
fn pi_uninstall_preserves_unrelated_prompt_files() {
    let home = tempfile::tempdir().unwrap();
    let installed = ramo::pi_extension::install_at(home.path()).unwrap();
    let other = installed.parent().unwrap().join("other.md");
    std::fs::write(&other, "keep me").unwrap();

    ramo::pi_extension::uninstall_at(home.path()).unwrap();
    assert!(!installed.exists());
    assert_eq!(std::fs::read_to_string(other).unwrap(), "keep me");
}
