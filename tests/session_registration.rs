use std::io::Cursor;
use std::time::{Duration, Instant};

use pdiff::core::input::LayoutMode;
use pdiff::diff::parser::parse_unified_diff;
use pdiff::review::{ReviewController, ReviewOptions, Viewport};
use pdiff::session::{
    ClientSessionFrame, MAX_SESSION_FRAME_BYTES, ServerSessionFrame, SessionAddress,
    SessionDaemonOptions, SessionDescriptor, SessionRegistrationClient, build_registration,
    build_snapshot, read_session_frame, spawn_session_daemon, write_session_frame,
};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

const PATCH: &str = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";

fn fixture() -> (
    pdiff::session::SessionRegistration,
    pdiff::session::SessionSnapshot,
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
    let mut command = CommandBuilder::new(assert_cmd::cargo::cargo_bin!("pdiff"));
    command.cwd(temp.path());
    command.args(["patch", patch.to_str().unwrap()]);
    command.env("PDIFF_SESSION_HOST", "127.0.0.1");
    command.env("PDIFF_SESSION_PORT", port.to_string());
    command.env("PDIFF_DISABLE_UPDATE_NOTICE", "1");
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut writer = pair.master.take_writer().unwrap();
    let mut reader = pair.master.try_clone_reader().unwrap();
    let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let reader_output = std::sync::Arc::clone(&output);
    let drain = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(&mut reader, &mut bytes);
        *reader_output.lock().unwrap() = bytes;
    });
    let address = format!("127.0.0.1:{port}").parse().unwrap();
    let client = pdiff::session::SessionClient::new(address);
    let deadline = Instant::now() + Duration::from_secs(2);
    while client
        .request(
            serde_json::json!({"action":"list"}),
            Duration::from_millis(200),
        )
        .ok()
        .and_then(|value| value["sessions"].as_array().map(Vec::len))
        != Some(1)
    {
        assert!(
            Instant::now() < deadline,
            "review did not register; output: {}",
            String::from_utf8_lossy(&output.lock().unwrap())
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let listed = client
        .request(serde_json::json!({"action":"list"}), Duration::from_secs(1))
        .unwrap();
    assert_eq!(listed["sessions"][0]["inputKind"], "patch");
    assert_eq!(
        listed["sessions"][0]["cwd"],
        temp.path().to_string_lossy().as_ref()
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
    drain.join().unwrap();
}
