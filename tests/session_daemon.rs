use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ramo::core::input::LayoutMode;
use ramo::diff::parser::parse_unified_diff;
use ramo::review::{ReviewController, ReviewOptions, Viewport};
use ramo::session::{
    SESSION_API_VERSION, SESSION_DAEMON_VERSION, SessionAddress, SessionClient,
    SessionDaemonOptions, SessionDescriptor, SessionOutput, SessionSelector, build_registration,
    build_snapshot, spawn_session_daemon, supported_session_actions,
};

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";

fn start() -> ramo::session::SessionDaemonHandle {
    spawn_session_daemon(SessionDaemonOptions {
        address: SessionAddress::loopback_ephemeral(),
        idle_timeout: Duration::from_secs(5),
        stale_session_ttl: Duration::from_secs(5),
    })
    .unwrap()
}

fn raw(address: std::net::SocketAddr, request: &[u8]) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect(address).unwrap();
    stream.write_all(request).unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response_bytes = Vec::new();
    let _ = stream.read_to_end(&mut response_bytes);
    let response = String::from_utf8_lossy(&response_bytes);
    let mut parts = response.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap();
    let status = head
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let body = serde_json::from_str(parts.next().unwrap()).unwrap();
    (status, body)
}

fn serve_json(mut stream: TcpStream, status: u16, body: serde_json::Value) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut request = Vec::new();
    let mut byte = [0_u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).unwrap_or(0) == 0 {
            break;
        }
        request.push(byte[0]);
    }
    let body = serde_json::to_vec(&body).unwrap();
    write!(
        stream,
        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(&body).unwrap();
}

#[test]
fn health_capabilities_and_legacy_tombstone_are_bounded_json_routes() {
    let daemon = start();
    let client = SessionClient::new(daemon.address());
    let capabilities = client.capabilities().unwrap();
    assert_eq!(capabilities.version, SESSION_API_VERSION);
    assert_eq!(capabilities.daemon_version, SESSION_DAEMON_VERSION);
    assert_eq!(capabilities.actions, supported_session_actions());

    let health = client.get_json("/health").unwrap();
    assert_eq!(health["ok"], true);
    assert_eq!(health["name"], "ramo-session-broker");

    let (status, body) = raw(
        daemon.address(),
        format!(
            "POST /mcp HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\n\r\n",
            daemon.address()
        )
        .as_bytes(),
    );
    assert_eq!(status, 410);
    assert!(body["error"].as_str().unwrap().contains("session CLI"));
}

#[test]
fn session_api_enforces_method_content_type_body_limit_host_and_origin() {
    let daemon = start();
    let address = daemon.address();
    let host = address.to_string();
    let cases = [
        (
            format!("GET /session-api HTTP/1.1\r\nHost: {host}\r\n\r\n"),
            405,
        ),
        (
            format!(
                "POST /session-api HTTP/1.1\r\nHost: {host}\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\n{{}}"
            ),
            415,
        ),
        (
            "GET /health HTTP/1.1\r\nHost: evil.example:47657\r\n\r\n".into(),
            403,
        ),
        (
            format!(
                "GET /health HTTP/1.1\r\nHost: {host}\r\nOrigin: https://attacker.example\r\n\r\n"
            ),
            403,
        ),
    ];
    for (request, expected) in cases {
        assert_eq!(raw(address, request.as_bytes()).0, expected);
    }

    let oversized = format!(
        "POST /session-api HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        ramo::session::MAX_HTTP_BODY_BYTES + 1
    );
    assert_eq!(raw(address, oversized.as_bytes()).0, 413);

    let huge_header = format!(
        "GET /health HTTP/1.1\r\nHost: {host}\r\nX-Fill: {}\r\n\r\n",
        "x".repeat(ramo::session::MAX_HTTP_HEADER_BYTES)
    );
    assert_eq!(raw(address, huge_header.as_bytes()).0, 431);

    let uppercase_json = format!(
        "POST /session-api HTTP/1.1\r\nHost: {host}\r\nContent-Type: Application/JSON\r\nContent-Length: 17\r\n\r\n{{\"action\":\"list\"}}"
    );
    assert_eq!(raw(address, uppercase_json.as_bytes()).0, 200);

    let malformed = format!(
        "POST /session-api HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: 1\r\n\r\n{{"
    );
    let (status, error) = raw(address, malformed.as_bytes());
    assert_eq!(status, 400);
    assert_eq!(error["code"], "invalid-json");
}

#[test]
fn registry_list_get_context_review_and_selector_errors_are_structured() {
    let daemon = start();
    let mut controller = ReviewController::new(
        parse_unified_diff(PATCH),
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    let descriptor = SessionDescriptor {
        session_id: "session-a".into(),
        pid: 42,
        cwd: "/work/repo".into(),
        repo_root: Some("/work/repo".into()),
        launched_at: "2026-07-21T12:00:00Z".into(),
        input_kind: "diff".into(),
        title: "Working tree changes".into(),
        source_label: "/work/repo".into(),
    };
    let registration = build_registration(&descriptor, controller.files());
    let snapshot = build_snapshot(
        &mut controller,
        Viewport {
            width: 100,
            height: 18,
        },
        "2026-07-21T12:00:01Z",
    );
    daemon.registry().lock().unwrap().register(
        registration,
        snapshot,
        Some("/tmp/ramo.sock".into()),
    );

    let client = SessionClient::new(daemon.address());
    let listed = client
        .request(serde_json::json!({"action":"list"}), Duration::from_secs(1))
        .unwrap();
    assert_eq!(listed["sessions"][0]["sessionId"], "session-a");
    let context = client
        .request(
            serde_json::json!({"action":"context","selector":{"sessionId":"session-a"}}),
            Duration::from_secs(1),
        )
        .unwrap();
    assert_eq!(context["context"]["selectedFile"]["path"], "src/lib.rs");
    let review = client
        .request(
            serde_json::json!({"action":"review","selector":{"repoRoot":"/work/repo"},"includePatch":true,"includeNotes":false}),
            Duration::from_secs(1),
        )
        .unwrap();
    assert_eq!(review["review"]["files"][0]["patch"], PATCH);

    let error = client
        .request(
            serde_json::json!({"action":"get","selector":{"sessionId":"missing"}}),
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert!(error.to_string().contains("No live ramo session"));

    let invalid = client
        .request(
            serde_json::json!({"action":"get","selector":{"sessionId":"session-a","repoRoot":"/work/repo"}}),
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert!(invalid.to_string().contains("exactly one"));
}

#[test]
fn address_conflicts_are_explicit_and_idle_daemons_stop() {
    let daemon = start();
    let error = spawn_session_daemon(SessionDaemonOptions {
        address: SessionAddress::from_socket_addr(daemon.address()).unwrap(),
        idle_timeout: Duration::from_secs(5),
        stale_session_ttl: Duration::from_secs(5),
    })
    .unwrap_err();
    assert!(error.to_string().contains("already in use"));

    let idle = spawn_session_daemon(SessionDaemonOptions {
        address: SessionAddress::loopback_ephemeral(),
        idle_timeout: Duration::from_millis(80),
        stale_session_ttl: Duration::from_secs(5),
    })
    .unwrap();
    assert!(idle.wait_timeout(Duration::from_secs(1)));
}

#[test]
fn output_marker_remains_part_of_the_native_client_model() {
    assert_eq!(SessionOutput::Json, SessionOutput::Json);
    assert_eq!(
        SessionSelector {
            session_id: Some("x".into()),
            ..SessionSelector::default()
        }
        .session_id
        .as_deref(),
        Some("x")
    );
}

#[test]
fn installed_binary_serves_and_cli_commands_use_the_native_daemon_without_a_tui() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let binary = assert_cmd::cargo::cargo_bin!("ramo");
    let mut daemon = Command::new(binary)
        .args(["daemon", "serve"])
        .env("RAMO_SESSION_HOST", "127.0.0.1")
        .env("RAMO_SESSION_PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let address = format!("127.0.0.1:{port}").parse().unwrap();
    let client = SessionClient::new(address);
    for _ in 0..100 {
        if client.capabilities().is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(client.capabilities().is_ok());

    let output = Command::new(binary)
        .args(["session", "list", "--json"])
        .env("RAMO_SESSION_HOST", "127.0.0.1")
        .env("RAMO_SESSION_PORT", port.to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({"sessions": []})
    );
    client.shutdown().unwrap();
    assert!(daemon.wait().unwrap().success());
}

#[test]
fn cli_replaces_a_stale_compatible_ramo_daemon_with_the_same_binary() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let stale = std::thread::spawn(move || {
        let (capabilities, _) = listener.accept().unwrap();
        serve_json(
            capabilities,
            200,
            serde_json::json!({"version":SESSION_API_VERSION,"daemonVersion":0,"actions":[]}),
        );
        let (shutdown, _) = listener.accept().unwrap();
        serve_json(shutdown, 200, serde_json::json!({"ok":true}));
    });
    let binary = assert_cmd::cargo::cargo_bin!("ramo");
    let started = Instant::now();
    let output = Command::new(binary)
        .args(["session", "list", "--json"])
        .env("RAMO_SESSION_HOST", "127.0.0.1")
        .env("RAMO_SESSION_PORT", port.to_string())
        .output()
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "session command waited for the detached replacement daemon"
    );
    stale.join().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({"sessions": []})
    );
    let client = SessionClient::new(format!("127.0.0.1:{port}").parse().unwrap());
    client.shutdown().unwrap();
}

#[test]
fn cli_does_not_replace_a_foreign_service_on_the_configured_port() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let foreign = std::thread::spawn(move || {
        let (capabilities, _) = listener.accept().unwrap();
        serve_json(capabilities, 404, serde_json::json!({"error":"not ramo"}));
        let _ = listener.accept();
    });
    let output = Command::new(assert_cmd::cargo::cargo_bin!("ramo"))
        .args(["session", "list", "--json"])
        .env("RAMO_SESSION_HOST", "127.0.0.1")
        .env("RAMO_SESSION_PORT", port.to_string())
        .output()
        .unwrap();
    foreign.join().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("foreign or incompatible service"));
}

#[test]
fn registry_generations_pending_ids_ambiguity_and_stale_pruning_are_bounded() {
    let daemon = start();
    let mut controller = ReviewController::new(parse_unified_diff(PATCH), ReviewOptions::default());
    let snapshot = build_snapshot(
        &mut controller,
        Viewport {
            width: 80,
            height: 12,
        },
        "now",
    );
    let make = |id: &str| {
        build_registration(
            &SessionDescriptor {
                session_id: id.into(),
                pid: 1,
                cwd: "/same".into(),
                repo_root: Some("/same".into()),
                launched_at: "now".into(),
                input_kind: "diff".into(),
                title: id.into(),
                source_label: id.into(),
            },
            controller.files(),
        )
    };
    let registry = daemon.registry();
    let mut registry = registry.lock().unwrap();
    let first = registry.register(make("one"), snapshot.clone(), Some("/one".into()));
    let second = registry.register(make("one"), snapshot.clone(), Some("/one".into()));
    assert!(second > first);
    assert!(!registry.unregister_generation("one", first));
    registry.register(make("two"), snapshot, Some("/two".into()));
    assert!(
        registry
            .select(&SessionSelector {
                repo_root: Some("/same".into()),
                ..SessionSelector::default()
            })
            .unwrap_err()
            .contains("2 live")
    );
    let (id, generation) = registry
        .begin_request(
            &SessionSelector {
                session_id: Some("one".into()),
                ..SessionSelector::default()
            },
            "request-1",
        )
        .unwrap();
    assert!(registry.complete_request(&id, generation, "request-1"));
    std::thread::sleep(Duration::from_millis(2));
    assert_eq!(registry.prune_stale(Duration::ZERO), 2);
}
