#![cfg(unix)]

mod support;

use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEADLINE: Duration = Duration::from_secs(5);
static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);

struct TmuxServer {
    socket: String,
}

impl TmuxServer {
    fn new() -> Option<Self> {
        if Command::new("tmux").arg("-V").output().is_err() {
            return None;
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Some(Self {
            socket: format!(
                "ramo-test-{}-{nonce}-{}",
                std::process::id(),
                NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
            ),
        })
    }

    fn run(&self, args: &[&str]) -> Output {
        let output = Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "tmux {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn panes(&self, session: &str) -> Vec<(String, String)> {
        let output = self.run(&[
            "list-panes",
            "-t",
            session,
            "-F",
            "#{pane_id}\t#{pane_current_command}",
        ]);
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let (id, command) = line.split_once('\t')?;
                Some((id.to_owned(), command.to_owned()))
            })
            .collect()
    }

    fn send_key(&self, pane: &str, key: &str) {
        self.run(&["send-keys", "-t", pane, key]);
    }

    fn send_literal(&self, pane: &str, text: &str) {
        self.run(&["send-keys", "-l", "-t", pane, text]);
    }

    fn capture(&self, pane: &str) -> String {
        let output = self.run(&["capture-pane", "-p", "-t", pane]);
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn capture_until(&self, pane: &str, needle: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let screen = self.capture(pane);
            if screen.contains(needle) {
                return screen;
            }
            assert!(
                Instant::now() < deadline,
                "tmux pane deadline waiting for {needle:?}; screen: {screen:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn capture_until_absent(&self, pane: &str, needle: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let screen = self.capture(pane);
            if !screen.contains(needle) {
                return screen;
            }
            assert!(
                Instant::now() < deadline,
                "tmux pane deadline waiting for {needle:?} to disappear; screen: {screen:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_for_pane_exit(&self, session: &str, pane: &str) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            if !self.panes(session).iter().any(|(id, _)| id == pane) {
                return;
            }
            assert!(Instant::now() < deadline, "tmux pane {pane} did not exit");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for TmuxServer {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .arg("kill-server")
            .output();
    }
}

fn shell_quote(value: &Path) -> String {
    format!("'{}'", value.display().to_string().replace('\'', "'\\''"))
}

fn write_fixture(root: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let config_home = root.join("config");
    let config = config_home.join("ramo/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&config, "prompt_save_view_preferences = false\n").unwrap();
    let patch = root.join("range.patch");
    std::fs::write(
        &patch,
        concat!(
            "diff --git a/src/range.rs b/src/range.rs\n",
            "new file mode 100644\n",
            "--- /dev/null\n",
            "+++ b/src/range.rs\n",
            "@@ -0,0 +1,5 @@\n",
            "+one\n+two\n+three\n+four\n+five\n",
        ),
    )
    .unwrap();
    (config_home, patch)
}

#[test]
fn picker_is_visible_and_native_note_saves_only_after_tmux_delivery() {
    let Some(server) = TmuxServer::new() else {
        eprintln!("skipping: tmux is unavailable");
        return;
    };
    let daemon = support::TestSessionDaemon::spawn();
    let temp = tempfile::tempdir().unwrap();
    let (config_home, patch) = write_fixture(temp.path());
    let output = temp.path().join("review.md");
    let received = temp.path().join("received.txt");
    let binary = assert_cmd::cargo::cargo_bin("ramo");
    let session = "review";
    let client = daemon.client();
    let command = format!(
        "exec env XDG_CONFIG_HOME={} RAMO_DISABLE_UPDATE_NOTICE=1 \
         RAMO_SESSION_HOST={} RAMO_SESSION_PORT={} {} --output {} patch {} --mode stack",
        shell_quote(&config_home),
        client.address().ip(),
        client.address().port(),
        shell_quote(&binary),
        shell_quote(&output),
        shell_quote(&patch),
    );
    let ramo_output = server.run(&[
        "new-session",
        "-d",
        "-s",
        session,
        "-x",
        "120",
        "-y",
        "30",
        "-P",
        "-F",
        "#{pane_id}",
        &command,
    ]);
    let ramo_pane = String::from_utf8_lossy(&ramo_output.stdout)
        .trim()
        .to_owned();
    let cat_command = format!("exec cat > {}", shell_quote(&received));
    let cat_output = server.run(&[
        "split-window",
        "-d",
        "-t",
        session,
        "-P",
        "-F",
        "#{pane_id}",
        &cat_command,
    ]);
    let cat_pane = String::from_utf8_lossy(&cat_output.stdout)
        .trim()
        .to_owned();

    server.capture_until(&ramo_pane, "one");
    server.send_literal(&ramo_pane, "VjjcExplain this range");
    server.send_key(&ramo_pane, "C-t");
    let picker = server.capture_until(&ramo_pane, "Send to tmux");
    assert!(picker.contains("[cat]"), "{picker}");
    assert!(picker.contains("Enter send"), "{picker}");

    server.send_key(&ramo_pane, "Escape");
    let draft = server.capture_until_absent(&ramo_pane, "Send to tmux");
    assert!(draft.contains("Draft note"), "{draft}");
    assert!(draft.contains("Explain this range"), "{draft}");

    server.send_key(&ramo_pane, "C-t");
    server.capture_until(&ramo_pane, "Send to tmux");
    server.send_key(&ramo_pane, "Enter");
    let target = server.capture_until(&cat_pane, "Explain this range");
    assert!(target.contains("src/range.rs"), "{target}");
    assert!(target.contains("one"), "{target}");
    assert!(target.contains("three"), "{target}");
    server.capture_until(&ramo_pane, "Your note");

    server.send_literal(&ramo_pane, "q");
    server.wait_for_pane_exit(session, &ramo_pane);
    let markdown = std::fs::read_to_string(output).unwrap();
    assert!(markdown.contains("Explain this range"), "{markdown}");
    assert!(markdown.contains("R1–R3"), "{markdown}");
}
