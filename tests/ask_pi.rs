use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use ramo::ask::{AskError, AskRequest, PiCli};
use ramo::process::command::{CommandExecutor, CommandRequest, CommandResult};

#[derive(Default)]
struct FakeExecutor {
    requests: Vec<CommandRequest>,
    results: VecDeque<io::Result<CommandResult>>,
}

impl CommandExecutor for FakeExecutor {
    fn execute(&mut self, request: CommandRequest) -> io::Result<CommandResult> {
        self.requests.push(request);
        self.results.pop_front().expect("scripted result")
    }
}

fn result(code: i32, stdout: &str, stderr: &str) -> io::Result<CommandResult> {
    Ok(CommandResult {
        code: Some(code),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: false,
    })
}

fn request() -> AskRequest {
    AskRequest {
        provider: "opencode-go".into(),
        model: "deepseek-v4-flash".into(),
        thinking: "max".into(),
        timeout: Duration::from_secs(180),
        prompt: "QUESTION\nWhat does this do?".into(),
        system_prompt: "Answer briefly.".into(),
    }
}

fn ask(results: Vec<io::Result<CommandResult>>) -> (Result<String, AskError>, Vec<CommandRequest>) {
    let executor = FakeExecutor {
        requests: Vec::new(),
        results: results.into_iter().collect(),
    };
    let mut cli = PiCli::new(executor);
    let outcome = cli.ask(&request());
    (outcome, cli.into_executor().requests)
}

#[test]
fn a_question_runs_pi_without_tools_or_sessions_and_returns_the_answer() {
    let (answer, requests) = ask(vec![result(0, "It renames the abort helper.\n", "")]);

    assert_eq!(answer.unwrap(), "It renames the abort helper.");
    assert_eq!(requests.len(), 1);
    let argv = requests[0]
        .argv
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
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
            "Answer briefly.",
        ]
    );
    assert_eq!(
        requests[0].stdin.as_deref(),
        Some("QUESTION\nWhat does this do?".as_bytes())
    );
    assert!(!requests[0].inherit_stdio);
    let limits = requests[0].limits.expect("bounded capture");
    assert_eq!(limits.timeout, Duration::from_secs(180));
}

#[test]
fn a_rejected_model_names_the_model_and_the_remedy() {
    let stderr = "Warning: Model \"deepseek-v4-flash\" not found for provider \"opencode-go\". \
                  Using custom model id.\n\
                  401: {\"type\":\"ModelError\",\"message\":\"Model is not supported\"}\n";
    let (answer, _) = ask(vec![result(1, "", stderr)]);

    let error = answer.unwrap_err();
    assert!(matches!(error, AskError::ModelRejected { .. }));
    let message = error.to_string();
    assert!(message.contains("deepseek-v4-flash"), "{message}");
    assert!(message.contains("pi --list-models"), "{message}");
}

#[test]
fn a_missing_binary_is_reported_as_a_missing_cli() {
    let (answer, _) = ask(vec![Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no such file",
    ))]);

    let error = answer.unwrap_err();
    assert!(matches!(error, AskError::MissingCli));
    assert!(error.to_string().contains("ask_enabled = false"));
}

#[test]
fn timeouts_truncation_and_empty_answers_are_distinct_failures() {
    let timed_out = Ok(CommandResult {
        code: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: true,
    });
    let (answer, _) = ask(vec![timed_out]);
    let error = answer.unwrap_err();
    assert!(matches!(error, AskError::TimedOut { seconds: 180 }));
    assert!(error.to_string().contains("ask_timeout_secs"));

    let truncated = Ok(CommandResult {
        code: Some(0),
        stdout: b"partial".to_vec(),
        stderr: Vec::new(),
        stdout_truncated: true,
        stderr_truncated: false,
        timed_out: false,
    });
    let (answer, _) = ask(vec![truncated]);
    assert!(matches!(answer.unwrap_err(), AskError::Truncated));

    let (answer, _) = ask(vec![result(0, "   \n\n", "")]);
    assert!(matches!(answer.unwrap_err(), AskError::EmptyAnswer));
}

#[test]
fn non_utf8_and_generic_failures_are_reported_verbatim() {
    let invalid = Ok(CommandResult {
        code: Some(0),
        stdout: vec![0xff, 0xfe],
        stderr: Vec::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: false,
    });
    let (answer, _) = ask(vec![invalid]);
    assert!(matches!(answer.unwrap_err(), AskError::InvalidUtf8));

    let (answer, _) = ask(vec![result(2, "", "network is unreachable\n")]);
    let error = answer.unwrap_err();
    assert!(matches!(error, AskError::Failed { code: Some(2), .. }));
    assert!(error.to_string().contains("network is unreachable"));
}
