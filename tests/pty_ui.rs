use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const DEADLINE: Duration = Duration::from_secs(5);

struct PtyProcess {
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    writer: Option<Box<dyn Write + Send>>,
    chunks: Receiver<Vec<u8>>,
    raw: Vec<u8>,
}

impl PtyProcess {
    fn spawn(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin("pdiff"));
        command.cwd(cwd);
        for argument in args {
            command.arg(argument);
        }
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
                Err(RecvTimeoutError::Timeout) => panic!("PTY deadline waiting for {needle}"),
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("PTY ended before rendering {needle}")
                }
            }
            let clean =
                pdiff::input::sanitize_terminal_text(&String::from_utf8_lossy(&self.raw), false);
            if clean.contains(needle) {
                return clean;
            }
        }
    }

    fn wait(&mut self) -> u32 {
        self.writer.take();
        let mut child = self.child.take().unwrap();
        let mut killer = child.clone_killer();
        let (sender, status) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(child.wait());
        });
        let exit = match status.recv_timeout(DEADLINE) {
            Ok(result) => result.unwrap().exit_code(),
            Err(error) => {
                let _ = killer.kill();
                panic!("PTY child exit deadline: {error}");
            }
        };
        while let Ok(chunk) = self.chunks.recv_timeout(Duration::from_millis(100)) {
            self.raw.extend(chunk);
        }
        exit
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

#[test]
fn changed_layout_and_theme_can_be_saved_from_the_centered_prompt() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    let fixture = fixture();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    process.read_until("println!");
    process.send("1t");
    process.read_until("Theme");
    process.send("\x1b[B\rq");
    let prompt = process.read_until("Save view preferences?");
    assert!(!prompt.contains("F10 menu"));
    assert!(!prompt.contains("File  View"));
    process.send("\r");
    assert_eq!(process.wait(), 0);

    let saved = std::fs::read_to_string(config_home.join("pdiff/config.toml")).unwrap();
    assert!(saved.contains("mode = \"split\""));
    assert!(saved.contains("theme = "));
    assert_eq!(
        process
            .raw
            .windows(8)
            .filter(|bytes| *bytes == b"\x1b[?1049l")
            .count(),
        1
    );
}

#[test]
fn cancel_returns_to_review_and_repeated_quit_discards_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    let fixture = fixture();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    process.read_until("println!");
    process.send("2q");
    process.read_until("Save view preferences?");
    process.send("\x1bqq");
    assert_eq!(process.wait(), 0);
    assert!(!config_home.join("pdiff/config.toml").exists());
    assert_eq!(
        process
            .raw
            .windows(8)
            .filter(|bytes| *bytes == b"\x1b[?1049l")
            .count(),
        1
    );
}
