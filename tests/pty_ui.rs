use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const DEADLINE: Duration = Duration::from_secs(5);

struct PtyProcess {
    session: ramo::session::SessionClient,
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Option<Box<dyn portable_pty::Child + Send + Sync>>,
    writer: Option<Box<dyn Write + Send>>,
    chunks: Receiver<Vec<u8>>,
    raw: Vec<u8>,
}

impl PtyProcess {
    fn spawn(cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Self {
        Self::spawn_sized(cwd, args, env, 80, 24)
    }

    fn spawn_sized(cwd: &Path, args: &[&str], env: &[(&str, &str)], cols: u16, rows: u16) -> Self {
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let session = ramo::session::SessionClient::new(reservation.local_addr().unwrap());
        drop(reservation);
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
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
        command.env("RAMO_SESSION_HOST", session.address().ip().to_string());
        command.env("RAMO_SESSION_PORT", session.address().port().to_string());
        for (key, value) in env {
            command.env(key, value);
        }
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let master = pair.master;
        let writer = master.take_writer().unwrap();
        let mut reader = master.try_clone_reader().unwrap();
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
            session,
            master,
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

    fn resize(&self, cols: u16, rows: u16) {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
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
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(remaining) {
                Ok(chunk) => self.raw.extend(chunk),
                Err(RecvTimeoutError::Timeout) => {
                    let clean = ramo::input::sanitize_terminal_text(
                        &String::from_utf8_lossy(&self.raw),
                        false,
                    );
                    panic!("PTY deadline waiting for {needle}; output: {clean:?}")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("PTY ended before rendering {needle}")
                }
            }
            let clean = ramo::input::sanitize_terminal_text(
                &String::from_utf8_lossy(&self.raw[start.min(self.raw.len())..]),
                false,
            );
            if clean.contains(needle) {
                return clean;
            }
        }
    }

    fn read_raw_until(&mut self, needle: &[u8]) {
        let deadline = Instant::now() + DEADLINE;
        while !self
            .raw
            .windows(needle.len())
            .any(|window| window == needle)
        {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(remaining) {
                Ok(chunk) => self.raw.extend(chunk),
                Err(error) => panic!("PTY raw deadline waiting for {needle:?}: {error}"),
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
        let _ = self.session.shutdown();
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

fn write_multi_file_patch(path: &Path) {
    let mut patch = String::new();
    for name in ["alpha", "beta"] {
        patch.push_str(&format!(
            "diff --git a/src/{name}.rs b/src/{name}.rs\n--- a/src/{name}.rs\n+++ b/src/{name}.rs\n@@ -1,12 +1,12 @@\n"
        ));
        for line in 1..=5 {
            patch.push_str(&format!(" {name} context {line}\n"));
        }
        patch.push_str(&format!("-{name} old\n+{name} new\n"));
        for line in 7..=12 {
            patch.push_str(&format!(" {name} context {line}\n"));
        }
    }
    std::fs::write(path, patch).unwrap();
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

    let saved = std::fs::read_to_string(config_home.join("ramo/config.toml")).unwrap();
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
    assert!(!config_home.join("ramo/config.toml").exists());
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
fn resize_thresholds_keep_the_selected_file_anchor() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    disable_save_prompt(&config_home);
    let patch = temp.path().join("multi.patch");
    write_multi_file_patch(&patch);
    let patch_text = patch.display().to_string();
    let mut process = PtyProcess::spawn_sized(
        temp.path(),
        &["patch", &patch_text],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
        220,
        18,
    );
    let initial = process.read_until("alpha new");
    assert!(initial.contains("beta.rs"), "wide sidebar was not rendered");
    process.send(".");
    let selected_mark = process.mark();
    process.read_since_until(selected_mark, "beta new");

    let medium_mark = process.mark();
    process.resize(160, 18);
    process.read_since_until(medium_mark, "beta new");
    let tight_mark = process.mark();
    process.resize(159, 18);
    let tight = process.read_since_until(tight_mark, "beta new");
    assert!(!tight.contains("F10 menu"));
    process.send("q");
    assert_eq!(process.wait(), 0);
}

#[test]
fn filter_owns_literal_keys_and_tab_returns_to_help_and_review() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    let fixture = fixture();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    process.read_until("println!");
    process.send("/");
    process.read_until("Filter:");
    let literal_mark = process.mark();
    process.send("q");
    let filter = process.read_since_until(literal_mark, "q");
    assert!(!filter.contains("Save view preferences?"));
    process.send("\t?");
    let help = process.read_until("Controls help");
    assert!(!help.contains("F10 menu"));
    assert!(!help.contains("File  View"));
    process.send("?q");
    assert_eq!(process.wait(), 0);
}

#[test]
fn direct_controls_and_context_expansion_remain_native_across_layout_changes() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    disable_save_prompt(&config_home);
    let old = temp.path().join("old.rs");
    let new = temp.path().join("new.rs");
    let old_text = (1..=16)
        .map(|line| {
            if line == 10 {
                "let value = \"old\";\n".into()
            } else {
                format!("source {line}\n")
            }
        })
        .collect::<String>();
    let new_text = old_text.replace("\"old\"", "\"new\"");
    std::fs::write(&old, old_text).unwrap();
    std::fs::write(&new, new_text).unwrap();
    let old_arg = old.display().to_string();
    let new_arg = new.display().to_string();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["diff", &old_arg, &new_arg],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    process.read_until("let value");
    process.send("\x1b[<0;5;1M\x1b[<0;5;1m");
    process.read_until("source 1");
    let changed_mark = process.mark();
    process.send("12");
    process.read_since_until(changed_mark, "source 1");
    process.send("0slwm][],.gGfbduq");
    assert_eq!(process.wait(), 0);
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
fn cjk_mouse_selection_copies_whole_terminal_cells_through_osc52() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    disable_save_prompt(&config_home);
    let old = temp.path().join("old.txt");
    let new = temp.path().join("new.txt");
    std::fs::write(&old, "界 old\n").unwrap();
    std::fs::write(&new, "界 new\n").unwrap();
    let old_arg = old.display().to_string();
    let new_arg = new.display().to_string();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["diff", &old_arg, &new_arg],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    process.read_until("界 old");
    process.send("\x1b[<0;9;2M\x1b[<32;14;2M\x1b[<0;14;2m");
    process.read_raw_until(b"\x1b]52;c;55WMIG9sZA==\x07");
    process.send("q");
    assert_eq!(process.wait(), 0);
}

#[test]
fn direct_agent_skill_dialog_copies_native_guidance_and_closes() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    disable_save_prompt(&config_home);
    let fixture = fixture();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );
    process.read_until("println!");
    process.send("A");
    let dialog = process.read_until("Agent skill");
    assert!(dialog.contains("ramo skill path"));
    let mut sequence = Vec::new();
    ramo::clipboard::write_osc52(&mut sequence, ramo::ui::dialogs::AGENT_SKILL_PROMPT).unwrap();
    process.send("y");
    process.read_raw_until(&sequence);
    process.send("\x1bqq");
    assert_eq!(process.wait(), 0);
}

#[test]
fn deprecated_theme_syntax_surfaces_a_native_startup_notice() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    let path = config_home.join("ramo/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        concat!(
            "theme = \"custom\"\n",
            "prompt_save_view_preferences = false\n",
            "[custom_theme.syntax]\n",
            "keyword = \"#112233\"\n",
        ),
    )
    .unwrap();
    let fixture = fixture();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture],
        &[("XDG_CONFIG_HOME", config_home.to_str().unwrap())],
    );

    process.read_until("Deprecated [custom_theme.syntax]");
    process.send("q");
    assert_eq!(process.wait(), 0);
}

#[test]
fn installed_version_change_surfaces_a_local_copied_skill_notice_once() {
    let temp = tempfile::tempdir().unwrap();
    let config_home = temp.path().join("config");
    let state = config_home.join("ramo/state.json");
    std::fs::create_dir_all(state.parent().unwrap()).unwrap();
    std::fs::write(&state, "{\"version\":1,\"lastSeenCliVersion\":\"0.0.5\"}\n").unwrap();
    let fixture = fixture();
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture],
        &[
            ("XDG_CONFIG_HOME", config_home.to_str().unwrap()),
            ("RAMO_DISABLE_UPDATE_NOTICE", "0"),
        ],
    );

    process.read_until("If your agent copied ramo's skill");
    process.send("q");
    assert_eq!(process.wait(), 0);

    let state = std::fs::read_to_string(state).unwrap();
    assert!(state.contains(env!("CARGO_PKG_VERSION")));
}

#[cfg(unix)]
#[test]
fn remote_update_notice_uses_an_optional_nonblocking_git_query() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    let git = bin.join("git");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(&git, "#!/bin/sh\nprintf 'abc\\trefs/tags/v0.0.7\\n'\n").unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config_home = temp.path().join("config");
    let fixture = fixture();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture],
        &[
            ("PATH", &path),
            ("XDG_CONFIG_HOME", config_home.to_str().unwrap()),
            ("RAMO_DISABLE_UPDATE_NOTICE", "0"),
            ("RAMO_TEST_UPDATE_NOTICE_DELAY_MS", "1"),
        ],
    );

    process.read_until("Update available: 0.0.7");
    process.send("q");
    assert_eq!(process.wait(), 0);
}

#[cfg(unix)]
#[test]
fn slow_remote_update_query_is_killed_without_blocking_the_review() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    let git = bin.join("git");
    let pid_path = temp.path().join("git.pid");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(
        &git,
        "#!/bin/sh\nprintf '%s' \"$$\" > \"$RAMO_TEST_GIT_PID_PATH\"\nexec sleep 10\n",
    )
    .unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config_home = temp.path().join("config");
    let fixture = fixture();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture],
        &[
            ("PATH", &path),
            ("XDG_CONFIG_HOME", config_home.to_str().unwrap()),
            ("RAMO_DISABLE_UPDATE_NOTICE", "0"),
            ("RAMO_TEST_UPDATE_NOTICE_DELAY_MS", "1"),
            ("RAMO_TEST_UPDATE_NOTICE_TIMEOUT_MS", "50"),
            ("RAMO_TEST_GIT_PID_PATH", pid_path.to_str().unwrap()),
        ],
    );

    process.read_until("println!");
    let deadline = Instant::now() + Duration::from_secs(1);
    while !pid_path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let pid = std::fs::read_to_string(&pid_path)
        .unwrap()
        .parse::<i32>()
        .unwrap();
    while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_ne!(
        unsafe { libc::kill(pid, 0) },
        0,
        "update child survived its timeout"
    );

    process.send("q");
    assert_eq!(process.wait(), 0);
}

#[cfg(unix)]
#[test]
fn local_and_remote_startup_notices_are_shown_in_order() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    let git = bin.join("git");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(&git, "#!/bin/sh\nprintf 'abc\\trefs/tags/v0.0.7\\n'\n").unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config_home = temp.path().join("config");
    let config = config_home.join("ramo/config.toml");
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(
        config,
        concat!(
            "theme = \"custom\"\n",
            "prompt_save_view_preferences = false\n",
            "[custom_theme.syntax]\n",
            "keyword = \"#112233\"\n",
        ),
    )
    .unwrap();
    let fixture = fixture();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let mut process = PtyProcess::spawn(
        temp.path(),
        &["patch", &fixture],
        &[
            ("PATH", &path),
            ("XDG_CONFIG_HOME", config_home.to_str().unwrap()),
            ("RAMO_DISABLE_UPDATE_NOTICE", "0"),
            ("RAMO_TEST_UPDATE_NOTICE_DELAY_MS", "1"),
            ("RAMO_TEST_STARTUP_NOTICE_DURATION_MS", "80"),
        ],
    );

    process.read_until("Deprecated [custom_theme.syntax]");
    let after_local = process.mark();
    process.read_since_until(after_local, "Update available: 0.0.7");
    process.send("q");
    assert_eq!(process.wait(), 0);
}
