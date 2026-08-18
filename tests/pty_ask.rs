#![cfg(unix)]

mod support;

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const DEADLINE: Duration = Duration::from_secs(10);

struct PtyProcess {
    _daemon: support::TestSessionDaemon,
    _master: Box<dyn portable_pty::MasterPty + Send>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    writer: Option<Box<dyn Write + Send>>,
    chunks: Receiver<Vec<u8>>,
    raw: Vec<u8>,
}

impl PtyProcess {
    fn spawn(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Self {
        let daemon = support::TestSessionDaemon::spawn();
        let session = daemon.client();
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin("ramo"));
        command.cwd(cwd);
        for arg in args {
            command.arg(arg);
        }
        command.env("RAMO_DISABLE_UPDATE_NOTICE", "1");
        command.env("RAMO_SESSION_HOST", session.address().ip().to_string());
        command.env("RAMO_SESSION_PORT", session.address().port().to_string());
        for (key, value) in env {
            command.env(key, value);
        }
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
            _master: pair.master,
            child: Some(child),
            writer: Some(writer),
            chunks,
            raw: Vec::new(),
        }
    }

    fn send(&mut self, text: &str) {
        let writer = self.writer.as_mut().unwrap();
        writer.write_all(text.as_bytes()).unwrap();
        writer.flush().unwrap();
    }

    fn screen_text(&mut self) -> String {
        while let Ok(chunk) = self.chunks.try_recv() {
            self.raw.extend(chunk);
        }
        let mut parser = vt100::Parser::new(30, 120, 0);
        parser.process(&self.raw);
        parser.screen().contents()
    }

    fn read_screen_until_absent(&mut self, needle: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let screen = self.screen_text();
            if !screen.contains(needle) {
                return screen;
            }
            if Instant::now() >= deadline {
                panic!("PTY screen deadline waiting for {needle:?} to clear; screen: {screen:?}");
            }
            match self.chunks.recv_timeout(Duration::from_millis(50)) {
                Ok(chunk) => self.raw.extend(chunk),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("PTY ended while {needle:?} was still on screen")
                }
            }
        }
    }

    fn read_screen_until(&mut self, needle: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let screen = self.screen_text();
            if screen.contains(needle) {
                return screen;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self
                .chunks
                .recv_timeout(remaining.min(Duration::from_millis(50)))
            {
                Ok(chunk) => self.raw.extend(chunk),
                Err(RecvTimeoutError::Timeout) if Instant::now() < deadline => {}
                Err(RecvTimeoutError::Timeout) => {
                    panic!("PTY screen deadline waiting for {needle:?}; screen: {screen:?}")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("PTY ended before screen contained {needle:?}")
                }
            }
        }
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

fn fixture() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/simple.patch")
        .display()
        .to_string()
}

/// Installs a fake `pi` ahead of the real one on PATH and returns `(PATH, call log)`.
fn fake_pi(root: &Path, body: &str) -> (String, std::path::PathBuf) {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = root.join("pi-calls.log");
    let pi = bin.join("pi");
    std::fs::write(
        &pi,
        format!(
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >> {log}\ncat > /dev/null\n{body}\n",
            log = log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o755)).unwrap();
    (
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        log,
    )
}

fn config_home(root: &Path, ask_enabled: bool) -> std::path::PathBuf {
    let home = root.join("config");
    let path = home.join("ramo/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        format!(
            "[general]\nprompt_save_view_preferences = false\n\n[ask]\nenabled = {ask_enabled}\n"
        ),
    )
    .unwrap();
    home
}

#[test]
fn asking_shows_a_pending_card_then_the_answer_and_o_jumps_to_it() {
    let temp = tempfile::tempdir().unwrap();
    let (path, log) = fake_pi(temp.path(), "printf 'It renames the helper.\\n'");
    let home = config_home(temp.path(), true);
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture(), "--mode", "stack"],
        &[("PATH", &path), ("XDG_CONFIG_HOME", home.to_str().unwrap())],
    );
    process.read_screen_until("Reviewed");

    process.send("a");
    process.read_screen_until("Ask AI");
    process.send("what changed?");
    process.read_screen_until("what changed?");
    process.send("\r");

    let answered = process.read_screen_until("It renames the helper.");
    assert!(answered.contains("Q: what changed?"), "{answered}");
    // The badge persists past a navigation key.
    process.send("j");
    let badge = process.read_screen_until("AI 1");
    assert!(badge.contains("· o"), "{badge}");

    process.send("o");
    let jumped = process.read_screen_until_absent("AI 1");
    assert!(
        jumped.contains("It renames the helper."),
        "the answer stays on screen after jumping: {jumped}"
    );

    let calls = std::fs::read_to_string(&log).unwrap();
    assert!(calls.contains("--no-tools"), "{calls}");
    assert!(calls.contains("--no-session"), "{calls}");
    assert!(calls.contains("--provider openai-codex"), "{calls}");
}

#[test]
fn a_rejected_model_names_the_model_on_the_card() {
    let temp = tempfile::tempdir().unwrap();
    let (path, _log) = fake_pi(
        temp.path(),
        "printf 'Model \"deepseek-v4-flash\" not found for provider\\n' >&2\nexit 1",
    );
    let home = config_home(temp.path(), true);
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture(), "--mode", "stack"],
        &[("PATH", &path), ("XDG_CONFIG_HOME", home.to_str().unwrap())],
    );
    process.read_screen_until("Reviewed");

    process.send("a");
    process.read_screen_until("Ask AI");
    process.send("why?");
    process.send("\r");

    let failed = process.read_screen_until("deepseek-v4-flash");
    assert!(failed.contains("failed"), "{failed}");
}

#[test]
fn the_kill_switch_stops_any_provider_call() {
    let temp = tempfile::tempdir().unwrap();
    let (path, log) = fake_pi(temp.path(), "printf 'should never run\\n'");
    // Config says enabled; --no-ask must still win.
    let home = config_home(temp.path(), true);
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture(), "--mode", "stack", "--no-ask"],
        &[("PATH", &path), ("XDG_CONFIG_HOME", home.to_str().unwrap())],
    );
    process.read_screen_until("Reviewed");

    process.send("a");
    let screen = process.read_screen_until("ask_enabled");
    assert!(!screen.contains("Q: "), "no question box opens: {screen}");

    process.send("q");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !log.exists(),
        "the provider must never be invoked: {:?}",
        std::fs::read_to_string(&log).ok()
    );
}
