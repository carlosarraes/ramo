#![cfg(unix)]

use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const DEADLINE: Duration = Duration::from_secs(5);

struct PtyProcess {
    _master: Box<dyn portable_pty::MasterPty + Send>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    writer: Option<Box<dyn Write + Send>>,
    chunks: Receiver<Vec<u8>>,
    raw: Vec<u8>,
}

impl PtyProcess {
    fn spawn(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 22,
                cols: 110,
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

    fn read_until(&mut self, needle: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(remaining) {
                Ok(chunk) => self.raw.extend(chunk),
                Err(RecvTimeoutError::Timeout) => {
                    let clean = ramo::input::sanitize_terminal_text(
                        &String::from_utf8_lossy(&self.raw),
                        false,
                    );
                    panic!("PTY deadline waiting for {needle:?}; output: {clean:?}")
                }
                Err(RecvTimeoutError::Disconnected) => panic!("PTY ended before {needle:?}"),
            }
            let clean =
                ramo::input::sanitize_terminal_text(&String::from_utf8_lossy(&self.raw), false);
            if clean.contains(needle) {
                return clean;
            }
        }
    }

    fn wait(&mut self) -> u32 {
        self.writer.take();
        let mut child = self.child.take().unwrap();
        let mut killer = child.clone_killer();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(child.wait());
        });
        let exit = match receiver.recv_timeout(DEADLINE) {
            Ok(status) => status.unwrap().exit_code(),
            Err(error) => {
                let _ = killer.kill();
                panic!("PTY child exit deadline: {error}")
            }
        };
        while let Ok(chunk) = self.chunks.recv_timeout(Duration::from_millis(100)) {
            self.raw.extend(chunk);
        }
        exit
    }

    fn screen_text(&mut self) -> String {
        while let Ok(chunk) = self.chunks.try_recv() {
            self.raw.extend(chunk);
        }
        let mut parser = vt100::Parser::new(22, 110, 0);
        parser.process(&self.raw);
        parser.screen().contents()
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

fn disable_save_prompt(config_home: &Path) {
    let path = config_home.join("ramo/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "prompt_save_view_preferences = false\n").unwrap();
}

#[test]
fn agent_notes_toggle_in_the_live_review() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    disable_save_prompt(&config_home);
    let context = temp.path().join("agent.json");
    std::fs::write(
        &context,
        r#"{"files":[{"path":"src/main.rs","annotations":[{
          "newRange":[2,2],"summary":"Agent finding visible"
        }]}]}"#,
    )
    .unwrap();
    let fixture = fixture();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &[
            "patch",
            &fixture,
            "--agent-context",
            context.to_str().unwrap(),
            "--mode",
            "stack",
        ],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    let initial = process.read_until("println!");
    assert!(!initial.contains("Agent finding visible"));
    process.send("a");
    process.read_until("Agent finding visible");
    process.send("q");
    assert_eq!(process.wait(), 0);
}

#[test]
fn human_note_draft_owns_keys_saves_inline_and_exports_markdown() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    disable_save_prompt(&config_home);
    let output = temp.path().join("review.md");
    let fixture = fixture();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &[
            "--output",
            output.to_str().unwrap(),
            "patch",
            &fixture,
            "--mode",
            "stack",
        ],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    process.read_until("println!");
    process.send("c");
    let draft = process.read_until("Draft note");
    assert!(draft.contains("Write a note"));
    process.send("Please keep ] literal.\rSecond line.\x13");
    process.read_until("Your note");
    std::thread::sleep(Duration::from_millis(100));
    let saved = process.screen_text();
    assert!(saved.contains("Please keep ] literal."), "{saved}");
    assert!(saved.contains("Second line."), "{saved}");
    process.send("q");
    assert_eq!(process.wait(), 0);

    let markdown = std::fs::read_to_string(output).unwrap();
    assert!(markdown.contains("Please keep ] literal."));
    assert!(markdown.contains("Second line."));
    assert!(markdown.contains("src/main.rs"));
}

#[test]
fn escape_cancels_a_fresh_inline_draft() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    disable_save_prompt(&config_home);
    let output = temp.path().join("review.md");
    let fixture = fixture();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["--output", output.to_str().unwrap(), "patch", &fixture],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    process.read_until("println!");
    process.send("cCancel me\x1b");
    std::thread::sleep(Duration::from_millis(100));
    process.send("q");
    assert_eq!(process.wait(), 0);
    let markdown = std::fs::read_to_string(output).unwrap();
    assert!(markdown.contains("No comments."));
    assert!(!markdown.contains("Cancel me"));
}

#[test]
fn stdout_export_is_printed_after_the_tui_restores_the_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    disable_save_prompt(&config_home);
    let fixture = fixture();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["--stdout", "patch", &fixture],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    process.read_until("println!");
    process.send("cstdout note\x13");
    process.read_until("Your note");
    process.send("q");
    assert_eq!(process.wait(), 0);
    let restored = process
        .raw
        .windows(b"\x1b[?1049l".len())
        .position(|window| window == b"\x1b[?1049l")
        .unwrap();
    let markdown = process
        .raw
        .windows(b"## Review Comments".len())
        .position(|window| window == b"## Review Comments")
        .unwrap();
    assert!(restored < markdown);
    let output = String::from_utf8_lossy(&process.raw);
    assert!(output.contains("stdout note"));
}
