use std::io::{self, Cursor, Read};
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use ramo::core::input::LayoutMode;
use ramo::diff::parser::parse_unified_diff;
use ramo::review::{ReviewController, ReviewOptions, Viewport};
use ramo::session::{
    ClientSessionFrame, MAX_SESSION_FRAME_BYTES, ServerSessionFrame, SessionAddress,
    SessionDaemonOptions, SessionDescriptor, SessionRegistrationClient, build_registration,
    build_snapshot, read_session_frame, spawn_session_daemon, write_session_frame,
};

const PATCH: &str = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";

fn fixture() -> (
    ramo::session::SessionRegistration,
    ramo::session::SessionSnapshot,
) {
    let mut controller = ReviewController::new(
        parse_unified_diff(PATCH),
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    let registration = build_registration(
        &SessionDescriptor {
            session_id: "live-one".into(),
            pid: 7,
            cwd: "/repo".into(),
            repo_root: Some("/repo".into()),
            launched_at: "start".into(),
            input_kind: "diff".into(),
            title: "review".into(),
            source_label: "/repo".into(),
        },
        controller.files(),
    );
    let snapshot = build_snapshot(
        &mut controller,
        Viewport {
            width: 80,
            height: 12,
        },
        "first",
    );
    (registration, snapshot)
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !predicate() {
        assert!(Instant::now() < deadline, "condition did not become true");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn frames_are_versioned_big_endian_json_and_strictly_bounded() {
    let frame = ClientSessionFrame::Ping;
    let mut bytes = Vec::new();
    write_session_frame(&mut bytes, &frame).unwrap();
    let payload_len = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
    assert_eq!(payload_len, bytes.len() - 4);
    assert_eq!(
        read_session_frame::<_, ClientSessionFrame>(&mut Cursor::new(bytes)).unwrap(),
        frame
    );

    let mut oversized = Cursor::new(((MAX_SESSION_FRAME_BYTES + 1) as u32).to_be_bytes());
    assert!(read_session_frame::<_, ClientSessionFrame>(&mut oversized).is_err());
    let incompatible =
        serde_json::to_vec(&serde_json::json!({"type":"registered","version":99,"generation":1}))
            .unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(incompatible.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&incompatible);
    assert!(read_session_frame::<_, ServerSessionFrame>(&mut Cursor::new(bytes)).is_err());
}

#[test]
fn fragmented_frames_survive_a_transient_read_timeout() {
    struct TimeoutAfterPartialRead {
        bytes: Cursor<Vec<u8>>,
        calls: usize,
        timeout_call: usize,
    }

    impl Read for TimeoutAfterPartialRead {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.calls == self.timeout_call {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "transient"));
            }
            let limit = buffer.len().min(2);
            self.bytes.read(&mut buffer[..limit])
        }
    }

    let frame = ServerSessionFrame::Pong;
    let mut bytes = Vec::new();
    write_session_frame(&mut bytes, &frame).unwrap();
    for timeout_call in [2, 3] {
        let mut reader = TimeoutAfterPartialRead {
            bytes: Cursor::new(bytes.clone()),
            calls: 0,
            timeout_call,
        };
        assert_eq!(
            read_session_frame::<_, ServerSessionFrame>(&mut reader).unwrap(),
            frame
        );
    }
}

#[test]
fn registration_snapshot_disconnect_and_reconnect_update_the_daemon_registry() {
    let first_daemon = spawn_session_daemon(SessionDaemonOptions {
        address: SessionAddress::loopback_ephemeral(),
        idle_timeout: Duration::from_secs(5),
        stale_session_ttl: Duration::from_secs(5),
    })
    .unwrap();
    let address = first_daemon.address();
    let (registration, mut snapshot) = fixture();
    let client = SessionRegistrationClient::start(
        address,
        registration,
        snapshot.clone(),
        Some("/dev/pts/7".into()),
    )
    .unwrap();
    wait_until(|| first_daemon.registry().lock().unwrap().list().len() == 1);
    snapshot.updated_at = "second".into();
    client.publish_snapshot(snapshot);
    wait_until(|| {
        first_daemon.registry().lock().unwrap().list()[0]
            .snapshot
            .updated_at
            == "second"
    });

    first_daemon.stop();
    assert!(first_daemon.wait_timeout(Duration::from_secs(1)));
    drop(first_daemon);
    let second_daemon = spawn_session_daemon(SessionDaemonOptions {
        address: SessionAddress::from_socket_addr(address).unwrap(),
        idle_timeout: Duration::from_secs(5),
        stale_session_ttl: Duration::from_secs(5),
    })
    .unwrap();
    wait_until(|| second_daemon.registry().lock().unwrap().list().len() == 1);
    drop(client);
    wait_until(|| second_daemon.registry().lock().unwrap().list().is_empty());
}

#[test]
fn real_review_auto_launches_registers_and_cleanly_unregisters_before_exit() {
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reserved.local_addr().unwrap().port();
    drop(reserved);
    let temp = tempfile::tempdir().unwrap();
    let patch = temp.path().join("review.patch");
    std::fs::write(&patch, PATCH).unwrap();
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin!("ramo"));
    command.cwd(temp.path());
    command.args(["patch", patch.to_str().unwrap()]);
    command.env("RAMO_SESSION_HOST", "127.0.0.1");
    command.env("RAMO_SESSION_PORT", port.to_string());
    command.env("RAMO_DISABLE_UPDATE_NOTICE", "1");
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut writer = pair.master.take_writer().unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    #[cfg(unix)]
    let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    #[cfg(unix)]
    let reader_output = std::sync::Arc::clone(&output);
    #[cfg(unix)]
    let drain = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(&mut reader, &mut bytes);
        *reader_output.lock().unwrap() = bytes;
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
    let mut output = Vec::new();
    #[cfg(windows)]
    let mut cursor_queries_answered = 0;
    let address = format!("127.0.0.1:{port}").parse().unwrap();
    let client = ramo::session::SessionClient::new(address);
    let deadline = Instant::now() + Duration::from_secs(3);
    while client
        .request(
            serde_json::json!({"action":"list"}),
            Duration::from_millis(200),
        )
        .ok()
        .and_then(|value| value["sessions"].as_array().map(Vec::len))
        != Some(1)
    {
        #[cfg(windows)]
        {
            while let Ok(chunk) = chunks.try_recv() {
                output.extend(chunk);
            }
            let query_count = output
                .windows(b"\x1b[6n".len())
                .filter(|bytes| *bytes == b"\x1b[6n")
                .count();
            while cursor_queries_answered < query_count {
                std::io::Write::write_all(&mut writer, b"\x1b[1;1R").unwrap();
                std::io::Write::flush(&mut writer).unwrap();
                cursor_queries_answered += 1;
            }
        }
        #[cfg(unix)]
        let captured = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
        #[cfg(windows)]
        let captured = String::from_utf8_lossy(&output).into_owned();
        assert!(
            Instant::now() < deadline,
            "review did not register; output: {captured}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let listed = client
        .request(serde_json::json!({"action":"list"}), Duration::from_secs(1))
        .unwrap();
    assert_eq!(listed["sessions"][0]["inputKind"], "patch");
    let expected_cwd = temp.path().canonicalize().unwrap();
    assert_eq!(
        listed["sessions"][0]["cwd"],
        expected_cwd.to_string_lossy().as_ref()
    );
    std::io::Write::write_all(&mut writer, b"q").unwrap();
    std::io::Write::flush(&mut writer).unwrap();
    assert!(child.wait().unwrap().success());
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let response = client.request(
            serde_json::json!({"action":"list"}),
            Duration::from_millis(200),
        );
        if response
            .as_ref()
            .ok()
            .and_then(|value| value["sessions"].as_array().map(Vec::len))
            == Some(0)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "review did not unregister: {response:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    client.shutdown().unwrap();
    drop(writer);
    drop(pair.master);
    #[cfg(unix)]
    drain.join().unwrap();
    #[cfg(windows)]
    drop(chunks);
    #[cfg(windows)]
    drop(drain);
}
