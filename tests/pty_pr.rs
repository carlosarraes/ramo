#![cfg(unix)]

mod support;

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const DEADLINE: Duration = Duration::from_secs(5);

struct PtyProcess {
    _daemon: support::TestSessionDaemon,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    writer: Option<Box<dyn Write + Send>>,
    chunks: Receiver<Vec<u8>>,
    raw: Vec<u8>,
}

impl PtyProcess {
    fn spawn(cwd: &Path, path: &str, payload: &Path, source_log: &Path) -> Self {
        let daemon = support::TestSessionDaemon::spawn();
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin("ramo"));
        command.cwd(cwd);
        command.arg("pr");
        command.arg("123");
        command.env("TERM", "xterm-256color");
        command.env("PATH", path);
        command.env("FAKE_GH_PAYLOAD", payload);
        command.env("FAKE_GH_SOURCE_LOG", source_log);
        command.env("RAMO_DISABLE_UPDATE_NOTICE", "1");
        command.env(
            "RAMO_SESSION_HOST",
            daemon.client().address().ip().to_string(),
        );
        command.env(
            "RAMO_SESSION_PORT",
            daemon.client().address().port().to_string(),
        );
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let writer = pair.master.take_writer().unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let (sender, chunks) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                    break;
                }
            }
        });
        Self {
            _daemon: daemon,
            child: Some(child),
            writer: Some(writer),
            chunks,
            raw: Vec::new(),
        }
    }

    fn send(&mut self, text: &str) {
        self.writer
            .as_mut()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
        self.writer.as_mut().unwrap().flush().unwrap();
    }

    fn mark(&self) -> usize {
        self.raw.len()
    }

    fn read_until(&mut self, needle: &str) -> String {
        self.read_since_until(0, needle)
    }

    fn read_since_until(&mut self, start: usize, needle: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let clean = ramo::input::sanitize_terminal_text(
                &String::from_utf8_lossy(&self.raw[start.min(self.raw.len())..]),
                false,
            );
            if clean.contains(needle) {
                return clean;
            }
            match self
                .chunks
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(chunk) => self.raw.extend(chunk),
                Err(RecvTimeoutError::Timeout) => {
                    let clean = ramo::input::sanitize_terminal_text(
                        &String::from_utf8_lossy(&self.raw),
                        false,
                    );
                    panic!("PTY deadline waiting for {needle:?}; output: {clean:?}")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let clean = ramo::input::sanitize_terminal_text(
                        &String::from_utf8_lossy(&self.raw),
                        false,
                    );
                    panic!("PTY exited before {needle:?}; output: {clean:?}")
                }
            }
        }
    }

    fn wait(&mut self) -> u32 {
        self.writer.take();
        let mut child = self.child.take().unwrap();
        child.wait().unwrap().exit_code()
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        self.writer.take();
        if let Some(child) = self.child.as_mut()
            && child.try_wait().ok().flatten().is_none()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn fake_gh(temp: &Path) -> (String, PathBuf, PathBuf) {
    let bin = temp.join("bin");
    let gh = bin.join("gh");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(
        &gh,
        r#"#!/bin/sh
case "$*" in
  "api user --jq .login")
    printf 'reviewer\n'
    ;;
  "repo view --json nameWithOwner,url")
    printf '%s\n' '{"nameWithOwner":"owner/repo","url":"https://github.com/owner/repo"}'
    ;;
  "pr view 123 --json number,title,url,author,baseRefName,baseRefOid,headRefName,headRefOid")
    printf '%s\n' '{"number":123,"title":"Improve review flow","url":"https://github.com/owner/repo/pull/123","author":{"login":"author"},"baseRefName":"main","baseRefOid":"base123","headRefName":"feature","headRefOid":"head123"}'
    ;;
  "pr diff 123 --color=never")
    printf '%s\n' 'diff --git a/src/lib.rs b/src/lib.rs' '--- a/src/lib.rs' '+++ b/src/lib.rs' '@@ -0,0 +1,2 @@' '+FIRST_PR_LINE' '+SECOND_PR_LINE'
    ;;
  "api --method GET repos/owner/repo/contents/src/lib.rs -f ref=head123 -H Accept:application/vnd.github.raw+json")
    printf 'fetch\n' >> "$FAKE_GH_SOURCE_LOG"
    printf '%s\n' 'FIRST_PR_LINE' 'SECOND_PR_LINE' 'CONTEXT_THREE' 'CONTEXT_FOUR'
    ;;
  "pr view 123 --json headRefOid --jq .headRefOid")
    printf 'head123\n'
    ;;
  "api --method POST repos/owner/repo/pulls/123/reviews --input -")
    cat > "$FAKE_GH_PAYLOAD"
    ;;
  *)
    printf 'unexpected gh args: %s\n' "$*" >&2
    exit 2
    ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    (
        path,
        temp.join("review.json"),
        temp.join("source-requests.log"),
    )
}

#[test]
fn public_pr_command_creates_and_publishes_one_github_review() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join(".ramo/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, "prompt_save_view_preferences = false\n").unwrap();
    let (path, payload, source_log) = fake_gh(temp.path());
    let mut process = PtyProcess::spawn(temp.path(), &path, &payload, &source_log);

    let screen = process.read_until("GitHub PR #123");
    assert!(screen.contains("FIRST_PR_LINE"), "{screen}");
    process.send("cInline feedback\r");
    process.read_until("Your note");
    process.send("qy");
    process.read_until("Submit GitHub review");
    process.send("c");
    assert_eq!(process.wait(), 0);

    let payload: serde_json::Value =
        serde_json::from_slice(&std::fs::read(payload).unwrap()).unwrap();
    assert_eq!(payload["commit_id"], "head123");
    assert_eq!(payload["event"], "COMMENT");
    assert_eq!(payload["comments"][0]["path"], "src/lib.rs");
    assert_eq!(payload["comments"][0]["side"], "RIGHT");
    assert_eq!(payload["comments"][0]["body"], "Inline feedback");
}

#[test]
fn public_pr_context_is_fetched_lazily_on_expansion() {
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join(".ramo/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, "prompt_save_view_preferences = false\n").unwrap();
    let (path, payload, source_log) = fake_gh(temp.path());
    let mut process = PtyProcess::spawn(temp.path(), &path, &payload, &source_log);

    process.read_until("GitHub PR #123");
    assert!(
        !source_log.exists(),
        "opening a pull request must not fetch unchanged source"
    );

    let expanded = process.mark();
    process.send("z");
    let expanded_screen = process.read_since_until(expanded, "CONTEXT_THREE");
    assert!(
        expanded_screen.contains("CONTEXT_THREE"),
        "{expanded_screen}"
    );

    let requests = std::fs::read_to_string(&source_log).unwrap();
    assert_eq!(requests.lines().count(), 1, "{requests}");

    process.send("q");
    process.read_until("no inline comments");
    process.send("d");
    assert_eq!(process.wait(), 0);
}
