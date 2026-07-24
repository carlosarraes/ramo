use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use ramo::diff::model::SourceSpec;
use ramo::github::GithubContextSourceLoader;
use ramo::process::command::{CommandExecutor, CommandRequest, CommandResult};
use ramo::review::{ContextSourceLoader, SourceFailure};

#[derive(Default)]
struct FakeExecutor {
    requests: Vec<CommandRequest>,
    results: VecDeque<io::Result<CommandResult>>,
}

impl FakeExecutor {
    fn with_results(results: impl IntoIterator<Item = io::Result<CommandResult>>) -> Self {
        Self {
            results: results.into_iter().collect(),
            ..Self::default()
        }
    }
}

impl CommandExecutor for FakeExecutor {
    fn execute(&mut self, request: CommandRequest) -> io::Result<CommandResult> {
        self.requests.push(request);
        self.results.pop_front().expect("scripted result")
    }
}

fn result(code: i32, stdout: &[u8], stderr: &[u8]) -> io::Result<CommandResult> {
    Ok(CommandResult {
        code: Some(code),
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: false,
    })
}

fn remote(path: &str) -> SourceSpec {
    SourceSpec::RemoteBlob {
        repository: "owner/repo".into(),
        revision: "abc123".into(),
        path: path.into(),
    }
}

fn argv(request: &CommandRequest) -> Vec<String> {
    request
        .argv
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn remote_source_is_percent_encoded_fetched_at_its_revision_and_cached() {
    let executor = FakeExecutor::with_results([result(0, b"one\ntwo\n", b"")]);
    let mut loader = GithubContextSourceLoader::new(executor);
    let source = remote("src/space # unicode-\u{e7}.rs");

    assert_eq!(loader.load(&source).unwrap().as_deref(), Some("one\ntwo\n"));
    assert_eq!(loader.load(&source).unwrap().as_deref(), Some("one\ntwo\n"));

    let executor = loader.into_executor();
    assert_eq!(executor.requests.len(), 1);
    assert_eq!(
        argv(&executor.requests[0]),
        [
            "gh",
            "api",
            "--method",
            "GET",
            "repos/owner/repo/contents/src/space%20%23%20unicode-%C3%A7.rs",
            "-f",
            "ref=abc123",
            "-H",
            "Accept:application/vnd.github.raw+json",
        ]
    );
    let limits = executor.requests[0].limits.as_ref().unwrap();
    assert_eq!(limits.stdout_bytes, 8 * 1024 * 1024);
    assert_eq!(limits.stderr_bytes, 8 * 1024);
    assert_eq!(limits.timeout, Duration::from_secs(15));
}

#[test]
fn unrelated_source_specs_are_unavailable_without_spawning() {
    let mut loader = GithubContextSourceLoader::new(FakeExecutor::default());
    assert_eq!(
        loader.load(&SourceSpec::None),
        Err(SourceFailure::Unavailable)
    );
    assert!(loader.into_executor().requests.is_empty());
}

#[test]
fn source_failures_are_specific_sanitized_and_cached() {
    let truncated = CommandResult {
        code: Some(0),
        stdout: vec![b'x'],
        stderr: Vec::new(),
        stdout_truncated: true,
        stderr_truncated: false,
        timed_out: false,
    };
    let source = remote("src/large.rs");
    let mut loader = GithubContextSourceLoader::new(FakeExecutor::with_results([Ok(truncated)]));
    let expected = SourceFailure::TooLarge {
        limit: 8 * 1024 * 1024,
    };
    assert_eq!(loader.load(&source), Err(expected.clone()));
    assert_eq!(loader.load(&source), Err(expected));
    assert_eq!(loader.into_executor().requests.len(), 1);

    let invalid_utf8 = result(0, &[0xff], b"");
    let mut loader = GithubContextSourceLoader::new(FakeExecutor::with_results([invalid_utf8]));
    assert_eq!(
        loader.load(&remote("src/non-utf8.rs")),
        Err(SourceFailure::NonUtf8)
    );

    let timed_out = CommandResult {
        code: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        timed_out: true,
    };
    let mut loader = GithubContextSourceLoader::new(FakeExecutor::with_results([Ok(timed_out)]));
    let error = loader.load(&remote("src/slow.rs")).unwrap_err();
    assert!(matches!(
        error,
        SourceFailure::Command(message) if message.contains("timed out")
    ));

    let mut loader = GithubContextSourceLoader::new(FakeExecutor::with_results([result(
        1,
        b"",
        b"gh: Not Found (HTTP 404)\n",
    )]));
    assert_eq!(
        loader.load(&remote("src/missing.rs")),
        Err(SourceFailure::Missing)
    );

    let mut loader = GithubContextSourceLoader::new(FakeExecutor::with_results([result(
        1,
        b"",
        b"\x1b[31mrate limited\x1b[0m\n",
    )]));
    let error = loader.load(&remote("src/rate.rs")).unwrap_err();
    assert!(matches!(
        error,
        SourceFailure::Command(message)
            if message.contains("rate limited") && !message.contains('\u{1b}')
    ));
}

#[test]
fn invalidation_reloads_the_remote_source() {
    let executor =
        FakeExecutor::with_results([result(0, b"first\n", b""), result(0, b"second\n", b"")]);
    let mut loader = GithubContextSourceLoader::new(executor);
    let source = remote("src/lib.rs");

    assert_eq!(loader.load(&source).unwrap().as_deref(), Some("first\n"));
    assert_eq!(loader.load(&source).unwrap().as_deref(), Some("first\n"));
    loader.invalidate();
    assert_eq!(loader.load(&source).unwrap().as_deref(), Some("second\n"));
    assert_eq!(loader.into_executor().requests.len(), 2);
}
