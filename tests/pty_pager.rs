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
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin("ramo"));
        command.cwd(cwd);
        for argument in args {
            command.arg(argument);
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
        let writer = self.writer.as_mut().expect("PTY writer is open");
        writer.write_all(text.as_bytes()).unwrap();
        writer.flush().unwrap();
    }

    fn send_eof(&mut self) {
        self.send("\u{4}");
    }

    fn raw(&self) -> &[u8] {
        &self.raw
    }

    fn read_until(&mut self, needle: &str) -> String {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(remaining) {
                Ok(chunk) => self.raw.extend(chunk),
                Err(RecvTimeoutError::Timeout) => {
                    panic!("PTY output deadline waiting for {needle}")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("PTY output ended before {needle}")
                }
            }
            let clean =
                ramo::input::sanitize_terminal_text(&String::from_utf8_lossy(&self.raw), false);
            if clean.contains(needle) {
                return clean;
            }
        }
    }

    fn read_until_raw(&mut self, needle: &[u8]) {
        let deadline = Instant::now() + DEADLINE;
        while !self
            .raw
            .windows(needle.len())
            .any(|window| window == needle)
        {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(remaining) {
                Ok(chunk) => self.raw.extend(chunk),
                Err(error) => panic!("PTY raw-output deadline waiting for {needle:?}: {error}"),
            }
        }
    }

    fn wait(&mut self) -> u32 {
        self.writer.take();
        let mut child = self.child.take().expect("PTY child is running");
        let mut killer = child.clone_killer();
        let (sender, status) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(child.wait());
        });
        let exit = match status.recv_timeout(DEADLINE) {
            Ok(result) => result.unwrap().exit_code(),
            Err(error) => {
                let _ = killer.kill();
                while let Ok(chunk) = self.chunks.try_recv() {
                    self.raw.extend(chunk);
                }
                let clean =
                    ramo::input::sanitize_terminal_text(&String::from_utf8_lossy(&self.raw), false);
                panic!("PTY child exit deadline: {error}; output: {clean:?}");
            }
        };
        self.drain_output();
        exit
    }

    fn drain_output(&mut self) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(remaining) {
                Ok(chunk) => self.raw.extend(chunk),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => panic!("PTY reader did not close"),
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

#[cfg(unix)]
fn write_helper(directory: &Path, name: &str, source: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = directory.join(name);
    std::fs::write(&path, source).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn patch_pager_enters_review_ui_and_quits_cleanly() {
    let cwd = std::env::current_dir().unwrap();
    let mut process = PtyProcess::spawn(&cwd, &["pager"], &[]);
    process.send(include_str!("fixtures/simple.patch"));
    process.send_eof();
    process.read_until_raw(b"\x1b[?1049h");
    let rendered = process.read_until("println!");
    assert!(rendered.contains("fn main"));
    assert!(!rendered.contains("NORMAL"));
    assert!(!rendered.contains("F10 menu"));
    process.send("q");
    assert_eq!(process.wait(), 0);
    assert_eq!(
        process
            .raw()
            .windows(8)
            .filter(|bytes| *bytes == b"\x1b[?1049h")
            .count(),
        1
    );
    assert_eq!(
        process
            .raw()
            .windows(8)
            .filter(|bytes| *bytes == b"\x1b[?1049l")
            .count(),
        1
    );
    assert_eq!(
        process
            .raw()
            .windows(8)
            .filter(|bytes| *bytes == b"\x1b[?1000h")
            .count(),
        1
    );
    assert_eq!(
        process
            .raw()
            .windows(8)
            .filter(|bytes| *bytes == b"\x1b[?1000l")
            .count(),
        1
    );
}

#[test]
fn patch_pager_suppresses_application_startup_notices() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    let config = config_home.join("ramo/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        config,
        concat!(
            "theme = \"custom\"\n",
            "[custom_theme.syntax]\n",
            "keyword = \"#112233\"\n",
        ),
    )
    .unwrap();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["pager"],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    process.send(include_str!("fixtures/simple.patch"));
    process.send_eof();

    let rendered = process.read_until("println!");
    assert!(!rendered.contains("Deprecated [custom_theme.syntax]"));
    process.send("q");
    assert_eq!(process.wait(), 0);
}

#[cfg(unix)]
#[test]
fn plain_text_pager_does_not_enter_alternate_screen() {
    let temp = tempfile::tempdir().unwrap();
    let helper = write_helper(
        temp.path(),
        "capture",
        "#!/bin/sh\nprintf 'PAGER_START\\n'\ncat\n",
    );
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["pager"],
        &[("RAMO_TEXT_PAGER", helper.to_str().unwrap())],
    );
    process.send("safe\x1b]8;;https://bad\x1b\\text\x1b]8;;\x1b\\\n");
    process.send_eof();
    let output = process.read_until("safetext");
    assert_eq!(process.wait(), 0);
    let pager_output = output
        .split_once("PAGER_START")
        .expect("helper pager marker")
        .1;
    assert!(!pager_output.contains("https://bad"));
    assert!(
        !process
            .raw()
            .windows(8)
            .any(|bytes| bytes == b"\x1b[?1049h")
    );
}

#[cfg(unix)]
#[test]
fn pager_nonzero_exit_code_is_propagated() {
    let temp = tempfile::tempdir().unwrap();
    let helper = write_helper(temp.path(), "fail", "#!/bin/sh\ncat >/dev/null\nexit 23\n");
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["pager"],
        &[("RAMO_TEXT_PAGER", helper.to_str().unwrap())],
    );
    process.send("ordinary text\n");
    process.send_eof();
    assert_eq!(process.wait(), 23);
}

#[cfg(unix)]
#[test]
fn recursive_pager_setting_uses_fallback_without_spawning_ramo_again() {
    let temp = tempfile::tempdir().unwrap();
    write_helper(
        temp.path(),
        "less",
        "#!/bin/sh\ncat\nprintf 'LESS_CALLED\\n'\n",
    );
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["pager"],
        &[("RAMO_TEXT_PAGER", "ramo pager"), ("PATH", &path)],
    );
    process.send("ordinary text\n");
    process.send_eof();
    assert!(process.read_until("LESS_CALLED").contains("ordinary text"));
    assert_eq!(process.wait(), 0);
}

#[cfg(unix)]
#[test]
fn ctrl_c_terminated_pager_maps_to_130() {
    let temp = tempfile::tempdir().unwrap();
    let helper = write_helper(
        temp.path(),
        "interrupt",
        "#!/bin/sh\ncat >/dev/null\nkill -INT $$\n",
    );
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["pager"],
        &[("RAMO_TEXT_PAGER", helper.to_str().unwrap())],
    );
    process.send("ordinary text\n");
    process.send_eof();
    assert_eq!(process.wait(), 130);
}
