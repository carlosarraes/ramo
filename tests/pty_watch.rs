#![cfg(unix)]

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
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
    fn spawn(cwd: &Path, args: &[&str]) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 18,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin("pdiff"));
        command.cwd(cwd);
        for argument in args {
            command.arg(argument);
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

    fn mark(&self) -> usize {
        self.raw.len()
    }

    fn read_until(&mut self, needle: &str) -> String {
        self.read_since_until(0, needle)
    }

    fn read_since_until(&mut self, _start: usize, needle: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(remaining) {
                Ok(chunk) => {
                    self.raw.extend(chunk);
                }
                Err(RecvTimeoutError::Timeout) => {
                    let clean = clean(&self.raw);
                    panic!("PTY deadline waiting for {needle:?}; output: {clean:?}")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("PTY ended before rendering {needle:?}")
                }
            }
            let clean = clean(&self.raw);
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
        match status.recv_timeout(DEADLINE) {
            Ok(result) => result.unwrap().exit_code(),
            Err(error) => {
                let _ = killer.kill();
                panic!("PTY child exit deadline: {error}");
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

fn clean(bytes: &[u8]) -> String {
    let mut parser = vt100::Parser::new(18, 120, 0);
    parser.process(bytes);
    parser.screen().contents()
}

struct WatchFixture {
    dir: tempfile::TempDir,
    before: PathBuf,
    after: PathBuf,
}

impl WatchFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let before = dir.path().join("before.rs");
        let after = dir.path().join("after.rs");
        fs::write(&before, "export const watchedValue = 'before';\n").unwrap();
        fs::write(&after, "export const watchedValue = 'initial change';\n").unwrap();
        Self { dir, before, after }
    }

    fn args(&self, watch: bool) -> Vec<String> {
        let mut args = vec![
            "diff".into(),
            self.before.display().to_string(),
            self.after.display().to_string(),
            "--mode".into(),
            "stack".into(),
        ];
        if watch {
            args.push("--watch".into());
        }
        args
    }

    fn replace_after(&self, value: &str) {
        let replacement = self.dir.path().join("after-replacement.rs");
        fs::write(
            &replacement,
            format!("export const watchedValue = '{value}';\n"),
        )
        .unwrap();
        fs::rename(replacement, &self.after).unwrap();
    }
}

fn launch(fixture: &WatchFixture, watch: bool) -> PtyProcess {
    let args = fixture.args(watch);
    PtyProcess::spawn(
        fixture.dir.path(),
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
}

#[test]
fn manual_r_reloads_a_direct_file_without_watch_mode() {
    let fixture = WatchFixture::new();
    let mut session = launch(&fixture, false);
    session.read_until("initial change");
    fixture.replace_after("manual replacement");
    let mark = session.mark();
    session.send("r");
    session.read_since_until(mark, "manual replacement");
    session.send("q");
    assert_eq!(session.wait(), 0);
}

#[test]
fn watch_mode_refreshes_after_an_atomic_save() {
    let fixture = WatchFixture::new();
    let mut session = launch(&fixture, true);
    session.read_until("initial change");
    let mark = session.mark();
    fixture.replace_after("passive replacement");
    let screen = session.read_since_until(mark, "passive replacement");
    assert!(!screen.contains("initial change"));
    session.send("q");
    assert_eq!(session.wait(), 0);
}

#[test]
fn reload_error_keeps_the_last_valid_review_visible() {
    let fixture = WatchFixture::new();
    let mut session = launch(&fixture, false);
    session.read_until("initial change");
    fs::remove_file(&fixture.after).unwrap();
    let mark = session.mark();
    session.send("r");
    let screen = session.read_since_until(mark, "Reload failed:");
    assert!(screen.contains("initial change"));
    session.send("q");
    assert_eq!(session.wait(), 0);
}
