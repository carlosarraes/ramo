mod support;

use std::io::{Read, Write};
use std::process::Command;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n@@ -10,2 +10,2 @@\n-old ten\n+new ten\n end\n";
const RELOADED_PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+newer\n tail\n@@ -10,2 +10,2 @@\n-old ten\n+newer ten\n end\n";

fn cli(binary: &std::path::Path, port: u16, args: &[&str]) -> std::process::Output {
    Command::new(binary)
        .args(args)
        .env("RAMO_SESSION_HOST", "127.0.0.1")
        .env("RAMO_SESSION_PORT", port.to_string())
        .output()
        .unwrap()
}

#[test]
fn live_pty_routes_navigation_comments_failures_lists_and_clearing_on_the_ui_thread() {
    let daemon = support::TestSessionDaemon::spawn();
    let port = daemon.client().address().port();
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join(".git")).unwrap();
    let patch = temp.path().join("review.patch");
    std::fs::write(&patch, PATCH).unwrap();
    let binary = assert_cmd::cargo::cargo_bin!("ramo");
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut review = CommandBuilder::new(binary);
    review.cwd(temp.path());
    review.args(["patch", patch.to_str().unwrap()]);
    review.env("RAMO_SESSION_HOST", "127.0.0.1");
    review.env("RAMO_SESSION_PORT", port.to_string());
    review.env("RAMO_DISABLE_UPDATE_NOTICE", "1");
    let mut child = pair.slave.spawn_command(review).unwrap();
    drop(pair.slave);
    let mut writer = pair.master.take_writer().unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    #[cfg(unix)]
    let drain = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = reader.read_to_end(&mut bytes);
    });
    #[cfg(windows)]
    let (chunks, drain) = {
        let (sender, chunks) = std::sync::mpsc::channel();
        let drain = std::thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                    break;
                }
            }
        });
        (chunks, drain)
    };
    #[cfg(windows)]
    let mut raw = Vec::new();
    #[cfg(windows)]
    let mut cursor_queries_answered = 0;

    let deadline = Instant::now() + Duration::from_secs(3);
    let session_id = loop {
        #[cfg(windows)]
        {
            while let Ok(chunk) = chunks.try_recv() {
                raw.extend(chunk);
            }
            let query_count = raw
                .windows(b"\x1b[6n".len())
                .filter(|bytes| *bytes == b"\x1b[6n")
                .count();
            while cursor_queries_answered < query_count {
                writer.write_all(b"\x1b[1;1R").unwrap();
                writer.flush().unwrap();
                cursor_queries_answered += 1;
            }
        }
        let output = cli(binary, port, &["session", "list", "--json"]);
        if output.status.success()
            && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
            && let Some(id) = value["sessions"]
                .as_array()
                .and_then(|sessions| sessions.first())
                .and_then(|session| session["sessionId"].as_str())
        {
            break id.to_owned();
        }
        assert!(Instant::now() < deadline, "live review never registered");
        std::thread::sleep(Duration::from_millis(20));
    };

    let added = cli(
        binary,
        port,
        &[
            "session",
            "comment",
            "add",
            &session_id,
            "--file",
            "src/lib.rs",
            "--new-line",
            "2",
            "--summary",
            "agent finding",
            "--focus",
            "--json",
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let added: serde_json::Value = serde_json::from_slice(&added.stdout).unwrap();
    let comment_id = added["result"]["commentId"].as_str().unwrap();
    assert!(comment_id.starts_with("mcp:request-"));

    let failed = cli(
        binary,
        port,
        &[
            "session",
            "comment",
            "add",
            &session_id,
            "--file",
            "missing.rs",
            "--new-line",
            "1",
            "--summary",
            "bad",
            "--json",
        ],
    );
    assert!(!failed.status.success());
    let deadline = Instant::now() + Duration::from_secs(2);
    let listed = loop {
        let listed = cli(
            binary,
            port,
            &[
                "session",
                "comment",
                "list",
                &session_id,
                "--type",
                "live",
                "--json",
            ],
        );
        if listed.status.success() {
            break listed;
        }
        assert!(
            Instant::now() < deadline,
            "{}",
            String::from_utf8_lossy(&listed.stderr)
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(listed["comments"].as_array().unwrap().len(), 1);

    std::fs::write(&patch, RELOADED_PATCH).unwrap();
    let reloaded = cli(
        binary,
        port,
        &[
            "session",
            "reload",
            &session_id,
            "--source",
            temp.path().to_str().unwrap(),
            "--json",
            "--",
            "patch",
            patch.to_str().unwrap(),
        ],
    );
    assert!(
        reloaded.status.success(),
        "{}",
        String::from_utf8_lossy(&reloaded.stderr)
    );
    let reloaded: serde_json::Value = serde_json::from_slice(&reloaded.stdout).unwrap();
    assert_eq!(reloaded["result"]["sessionId"], session_id);
    assert_eq!(reloaded["result"]["fileCount"], 1);
    let exported = cli(
        binary,
        port,
        &[
            "session",
            "review",
            &session_id,
            "--include-patch",
            "--include-notes",
            "--json",
        ],
    );
    assert!(exported.status.success());
    let exported: serde_json::Value = serde_json::from_slice(&exported.stdout).unwrap();
    assert!(
        exported["review"]["files"][0]["patch"]
            .as_str()
            .unwrap()
            .contains("+newer")
    );
    assert_eq!(
        exported["review"]["reviewNotes"].as_array().unwrap().len(),
        1
    );

    let navigated = cli(
        binary,
        port,
        &[
            "session",
            "navigate",
            &session_id,
            "--file",
            "src/lib.rs",
            "--hunk",
            "2",
            "--json",
        ],
    );
    assert!(navigated.status.success());
    let context = cli(binary, port, &["session", "context", &session_id, "--json"]);
    assert!(
        context.status.success(),
        "{}",
        String::from_utf8_lossy(&context.stderr)
    );
    let context: serde_json::Value = serde_json::from_slice(&context.stdout).unwrap();
    assert_eq!(context["context"]["selectedHunk"]["index"], 1);

    let cleared = cli(
        binary,
        port,
        &[
            "session",
            "comment",
            "clear",
            &session_id,
            "--yes",
            "--json",
        ],
    );
    assert!(
        cleared.status.success(),
        "{}",
        String::from_utf8_lossy(&cleared.stderr)
    );
    let cleared: serde_json::Value = serde_json::from_slice(&cleared.stdout).unwrap();
    assert_eq!(cleared["result"]["removedLiveCommentCount"], 1);

    writer.write_all(b"qq").unwrap();
    writer.flush().unwrap();
    assert!(child.wait().unwrap().success());
    drop(writer);
    drop(pair.master);
    #[cfg(windows)]
    drop(chunks);
    #[cfg(unix)]
    drain.join().unwrap();
    #[cfg(windows)]
    drop(drain);
    drop(daemon);
}
