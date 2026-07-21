#![cfg(unix)]

use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ramo::session::{SessionAddress, SessionClient, SessionDaemonOptions, spawn_session_daemon};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde_json::{Value, json};

const PATCH_ONE: &str = "diff --git a/src/one.rs b/src/one.rs\n--- a/src/one.rs\n+++ b/src/one.rs\n@@ -1 +1 @@\n-old one\n+new one\n@@ -10 +10 @@\n-old ten\n+new ten\n";
const PATCH_TWO: &str = "diff --git a/src/two.rs b/src/two.rs\n--- a/src/two.rs\n+++ b/src/two.rs\n@@ -1 +1 @@\n-old two\n+new two\n@@ -20 +20 @@\n-old twenty\n+new twenty\n";

struct ReviewPty {
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    writer: Option<Box<dyn Write + Send>>,
}

impl ReviewPty {
    fn spawn(cwd: &Path, patch: &Path, address: std::net::SocketAddr, session_path: &str) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin!("ramo"));
        command.cwd(cwd);
        command.args(["patch", patch.to_str().unwrap()]);
        command.env("RAMO_SESSION_HOST", address.ip().to_string());
        command.env("RAMO_SESSION_PORT", address.port().to_string());
        command.env("RAMO_SESSION_PATH", session_path);
        command.env("RAMO_DISABLE_UPDATE_NOTICE", "1");
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let writer = pair.master.take_writer().unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            while reader.read(&mut buffer).is_ok_and(|count| count > 0) {}
        });
        Self {
            child: Some(child),
            writer: Some(writer),
        }
    }

    fn quit(mut self) {
        let writer = self.writer.as_mut().unwrap();
        writer.write_all(b"qq").unwrap();
        writer.flush().unwrap();
        let mut child = self.child.take().unwrap();
        let mut killer = child.clone_killer();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(child.wait());
        });
        let status = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        if !status.as_ref().is_ok_and(|status| status.success()) {
            let _ = killer.kill();
            panic!("review PTY did not exit successfully: {status:?}");
        }
        self.writer.take();
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool, message: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "{message}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn selector(session_id: &str) -> Value {
    json!({"sessionId":session_id})
}

#[test]
fn two_live_terminals_route_isolated_commands_and_reconnect_before_idle_exit() {
    let first_daemon = spawn_session_daemon(SessionDaemonOptions {
        address: SessionAddress::loopback_ephemeral(),
        idle_timeout: Duration::from_secs(1),
        stale_session_ttl: Duration::from_secs(5),
    })
    .unwrap();
    let address = first_daemon.address();
    let repo = tempfile::tempdir().unwrap();
    std::fs::create_dir(repo.path().join(".git")).unwrap();
    let first_patch = repo.path().join("first.patch");
    let second_patch = repo.path().join("second.patch");
    std::fs::write(&first_patch, PATCH_ONE).unwrap();
    std::fs::write(&second_patch, PATCH_TWO).unwrap();
    let first_path = "/dev/pts/ramo-one";
    let second_path = "/dev/pts/ramo-two";
    let first = ReviewPty::spawn(repo.path(), &first_patch, address, first_path);
    let second = ReviewPty::spawn(repo.path(), &second_patch, address, second_path);
    let client = SessionClient::new(address);
    wait_until(
        || {
            client
                .request(json!({"action":"list"}), Duration::from_millis(250))
                .ok()
                .and_then(|value| value["sessions"].as_array().map(Vec::len))
                == Some(2)
        },
        "two reviews did not register",
    );
    let listed = client
        .request(json!({"action":"list"}), Duration::from_secs(1))
        .unwrap();
    let sessions = listed["sessions"].as_array().unwrap();
    let id_for = |title: &str| {
        sessions
            .iter()
            .find(|session| session["title"] == title)
            .and_then(|session| session["sessionId"].as_str())
            .unwrap()
            .to_owned()
    };
    let first_id = id_for("first.patch");
    let second_id = id_for("second.patch");
    assert_ne!(first_id, second_id);

    let repo_selector = json!({"repoRoot":repo.path().canonicalize().unwrap()});
    let ambiguous = client
        .request(
            json!({"action":"get","selector":repo_selector}),
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert!(ambiguous.to_string().contains("2 live ramo sessions"));
    let by_path = client
        .request(
            json!({"action":"get","selector":{"sessionPath":second_path}}),
            Duration::from_secs(1),
        )
        .unwrap();
    assert_eq!(by_path["session"]["sessionId"], second_id);

    client
        .request(
            json!({
                "action":"navigate","selector":selector(&first_id),
                "filePath":"src/one.rs","hunkNumber":2
            }),
            Duration::from_secs(1),
        )
        .unwrap();
    client
        .request(
            json!({
                "action":"comment-add","selector":selector(&first_id),
                "filePath":"src/one.rs","side":"new","line":10,
                "summary":"only first","reveal":true
            }),
            Duration::from_secs(1),
        )
        .unwrap();
    let first_context = client
        .request(
            json!({"action":"context","selector":selector(&first_id)}),
            Duration::from_secs(1),
        )
        .unwrap();
    let second_context = client
        .request(
            json!({"action":"context","selector":selector(&second_id)}),
            Duration::from_secs(1),
        )
        .unwrap();
    assert_eq!(first_context["context"]["selectedHunk"]["index"], 1);
    assert_eq!(first_context["context"]["liveCommentCount"], 1);
    assert_eq!(second_context["context"]["selectedHunk"]["index"], 0);
    assert_eq!(second_context["context"]["liveCommentCount"], 0);

    client
        .request(
            json!({
                "action":"navigate","selector":{"sessionPath":second_path},
                "filePath":"src/two.rs","hunkNumber":2
            }),
            Duration::from_secs(1),
        )
        .unwrap();
    let second_context = client
        .request(
            json!({"action":"context","selector":selector(&second_id)}),
            Duration::from_secs(1),
        )
        .unwrap();
    assert_eq!(second_context["context"]["selectedHunk"]["index"], 1);

    first_daemon.stop();
    assert!(first_daemon.wait_timeout(Duration::from_secs(2)));
    drop(first_daemon);
    let second_daemon = spawn_session_daemon(SessionDaemonOptions {
        address: SessionAddress::from_socket_addr(address).unwrap(),
        idle_timeout: Duration::from_secs(1),
        stale_session_ttl: Duration::from_secs(5),
    })
    .unwrap();
    wait_until(
        || second_daemon.registry().lock().unwrap().list().len() == 2,
        "live reviews did not reconnect to the restarted daemon",
    );

    first.quit();
    second.quit();
    wait_until(
        || second_daemon.registry().lock().unwrap().is_empty(),
        "reviews did not unregister cleanly",
    );
    assert!(second_daemon.wait_timeout(Duration::from_secs(2)));
}
