#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::sync::mpsc::{self, RecvTimeoutError};
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use ramo::ui::appearance::{
    RgbColor, appearance_for_background, appearance_from_colorfgbg, parse_osc11_background,
};
use ramo::ui::themes::TerminalAppearance;
use ramo::ui::themes::ThemeRegistry;

#[test]
fn osc11_parsing_classification_and_environment_fallback_match_hunk() {
    assert_eq!(
        parse_osc11_background(b"prefix\x1b]11;rgb:0000/1111/2222\x1b\\suffix"),
        Some(RgbColor {
            red: 0,
            green: 17,
            blue: 34,
        })
    );
    assert_eq!(
        parse_osc11_background(b"\x1b]11;#ffffff\x07"),
        Some(RgbColor {
            red: 255,
            green: 255,
            blue: 255,
        })
    );
    assert_eq!(
        appearance_for_background(RgbColor {
            red: 12,
            green: 12,
            blue: 12,
        }),
        TerminalAppearance::Dark
    );
    assert_eq!(
        appearance_for_background(RgbColor {
            red: 245,
            green: 245,
            blue: 245,
        }),
        TerminalAppearance::Light
    );
    assert_eq!(
        appearance_from_colorfgbg("15;0"),
        Some(TerminalAppearance::Dark)
    );
    assert_eq!(
        appearance_from_colorfgbg("0;15"),
        Some(TerminalAppearance::Light)
    );
    assert_eq!(appearance_from_colorfgbg("invalid"), None);
    let response = parse_osc11_background(b"\x1b]11;#ffffff\x07").unwrap();
    assert_eq!(
        ThemeRegistry::default()
            .resolve("auto", Some(appearance_for_background(response)), false,)
            .id,
        "github-light-default"
    );
}

#[cfg(unix)]
fn launch_and_capture(respond: bool) -> (Vec<u8>, Duration) {
    let daemon = support::TestSessionDaemon::spawn();
    let session = daemon.client();
    let temp = tempfile::tempdir().unwrap();
    let patch = temp.path().join("review.patch");
    std::fs::write(
        &patch,
        "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n",
    )
    .unwrap();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 18,
            cols: 90,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin!("ramo"));
    command.cwd(temp.path());
    command.args(["patch", patch.to_str().unwrap(), "--theme", "auto"]);
    command.env("RAMO_SESSION_HOST", session.address().ip().to_string());
    command.env("RAMO_SESSION_PORT", session.address().port().to_string());
    command.env("RAMO_DISABLE_UPDATE_NOTICE", "1");
    command.env("COLORFGBG", "");
    let start = Instant::now();
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut writer = pair.master.take_writer().unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut raw = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut answered = false;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining) {
            Ok(chunk) => raw.extend(chunk),
            Err(RecvTimeoutError::Timeout) => panic!(
                "terminal appearance PTY timed out: {:?}",
                String::from_utf8_lossy(&raw)
            ),
            Err(RecvTimeoutError::Disconnected) => panic!("terminal appearance PTY closed"),
        }
        if !answered
            && raw
                .windows(b"\x1b]11;?\x1b\\".len())
                .any(|window| window == b"\x1b]11;?\x1b\\")
        {
            answered = true;
            if respond {
                writer
                    .write_all(b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\")
                    .unwrap();
                writer.flush().unwrap();
            }
        }
        let clean = ramo::input::sanitize_terminal_text(&String::from_utf8_lossy(&raw), false);
        if clean.contains("old") && clean.contains("new") {
            break;
        }
    }
    writer.write_all(b"q").unwrap();
    writer.flush().unwrap();
    assert!(child.wait().unwrap().success());
    (raw, start.elapsed())
}

#[test]
#[cfg(unix)]
fn real_pty_query_accepts_a_response_and_timeout_still_starts() {
    let (light, answered_elapsed) = launch_and_capture(true);
    assert!(
        light
            .windows(b"\x1b]11;?\x1b\\".len())
            .any(|window| window == b"\x1b]11;?\x1b\\")
    );
    assert!(answered_elapsed < Duration::from_secs(2));
    let (_, elapsed) = launch_and_capture(false);
    assert!(elapsed < Duration::from_secs(2));
}
#[cfg(unix)]
mod support;
