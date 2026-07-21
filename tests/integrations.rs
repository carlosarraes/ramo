use std::collections::VecDeque;
use std::ffi::OsString;
use std::io;

use pdiff::process::command::{CommandExecutor, CommandRequest, CommandResult};
use pdiff::tmux::{PasteMode, TmuxClient};

fn argv(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn success(stdout: &str) -> io::Result<CommandResult> {
    Ok(CommandResult {
        code: Some(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
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
        argv(&["tmux", "load-buffer", "-b", "pdiff-send", "-"])
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
            "pdiff-send",
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
    pdiff::clipboard::write_osc52(&mut output, "界 old").unwrap();
    assert_eq!(output, b"\x1b]52;c;55WMIG9sZA==\x07");
}

#[test]
fn tmux_plain_paste_and_failures_are_operation_specific() {
    let mut tmux = TmuxClient::new(RecordingExecutor::successful(2));
    tmux.send_to_pane("%2", "review", PasteMode::Plain).unwrap();
    let executor = tmux.into_executor();
    assert_eq!(
        executor.requests[1].argv,
        argv(&["tmux", "paste-buffer", "-b", "pdiff-send", "-t", "%2", "-d",])
    );

    let failure = CommandResult {
        code: Some(9),
        stdout: Vec::new(),
        stderr: b"permission denied".to_vec(),
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
