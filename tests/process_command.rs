#![cfg(unix)]

use std::ffi::OsString;
use std::time::{Duration, Instant};

use ramo::process::command::{
    CaptureLimits, CommandExecutor, CommandRequest, SystemCommandExecutor,
};

#[test]
fn captured_streams_are_drained_but_retained_only_to_the_requested_limit() {
    let mut executor = SystemCommandExecutor;
    let result = executor
        .execute(CommandRequest {
            argv: [
                "sh",
                "-c",
                "head -c 131072 /dev/zero; head -c 131072 /dev/zero >&2",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            stdin: None,
            inherit_stdio: false,
            limits: Some(CaptureLimits::new(1024, 1024, Duration::from_secs(2))),
        })
        .unwrap();

    assert_eq!(result.code, Some(0));
    assert_eq!(result.stdout.len(), 1024);
    assert_eq!(result.stderr.len(), 1024);
    assert!(result.stdout_truncated);
    assert!(result.stderr_truncated);
    assert!(!result.timed_out);
}

#[test]
fn a_timed_out_child_is_killed_and_reported() {
    let started = Instant::now();
    let mut executor = SystemCommandExecutor;
    let result = executor
        .execute(CommandRequest {
            argv: ["sh", "-c", "exec sleep 10"]
                .into_iter()
                .map(OsString::from)
                .collect(),
            stdin: None,
            inherit_stdio: false,
            limits: Some(CaptureLimits::new(1024, 1024, Duration::from_millis(25))),
        })
        .unwrap();

    assert!(result.timed_out);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn stdin_reaches_the_child_without_appearing_in_argv() {
    let sentinel = b"secret-review-body".to_vec();
    let argv = vec![
        OsString::from("sh"),
        OsString::from("-c"),
        OsString::from("cat"),
    ];
    let mut executor = SystemCommandExecutor;
    let result = executor
        .execute(CommandRequest {
            argv: argv.clone(),
            stdin: Some(sentinel.clone()),
            inherit_stdio: false,
            limits: Some(CaptureLimits::new(1024, 1024, Duration::from_secs(1))),
        })
        .unwrap();

    assert_eq!(result.stdout, sentinel);
    assert!(!argv.iter().any(|argument| argument == "secret-review-body"));
}
