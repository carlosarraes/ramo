use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};

use ramo::session::{
    DEFAULT_SESSION_PORT, SessionAddress, resolve_session_address, validate_request_authority,
};

#[test]
fn address_resolution_accepts_only_explicit_loopback_endpoints() {
    for host in ["127.0.0.1", "127.9.8.7", "localhost", "::1", "[::1]"] {
        let address = SessionAddress::parse(host, 47657).unwrap();
        assert!(address.ip().is_loopback(), "{host}");
    }

    for host in [
        "0.0.0.0",
        "::",
        "example.com",
        "192.168.1.2",
        "::ffff:192.168.1.2",
        "",
    ] {
        assert!(SessionAddress::parse(host, 47657).is_err(), "{host}");
    }
    assert!(SessionAddress::parse("127.0.0.1", 0).is_err());
}

#[test]
fn environment_resolution_prefers_ramo_names_and_validates_ports() {
    let env = HashMap::from([
        ("RAMO_SESSION_HOST".to_owned(), "localhost".to_owned()),
        ("RAMO_SESSION_PORT".to_owned(), "48123".to_owned()),
        ("HUNK_MCP_HOST".to_owned(), "127.0.0.2".to_owned()),
        ("HUNK_MCP_PORT".to_owned(), "48124".to_owned()),
    ]);
    let address = resolve_session_address(&env).unwrap();
    assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!(address.port(), 48123);

    let bad = HashMap::from([("RAMO_SESSION_PORT".to_owned(), "nope".to_owned())]);
    assert!(resolve_session_address(&bad).is_err());
    let defaults = resolve_session_address(&HashMap::new()).unwrap();
    assert_eq!(defaults.port(), DEFAULT_SESSION_PORT);
}

#[test]
fn host_and_origin_policy_blocks_dns_rebinding_and_cross_origin_browsers() {
    assert!(validate_request_authority("127.0.0.1:47657", None, 47657).is_ok());
    assert!(
        validate_request_authority("localhost:47657", Some("http://localhost:47657"), 47657,)
            .is_ok()
    );
    for (host, origin) in [
        ("evil.example:47657", None),
        ("127.0.0.1:1", None),
        ("127.0.0.1:47657", Some("https://attacker.example")),
        ("127.0.0.1:47657", Some("file://localhost")),
        ("127.0.0.1:47657", Some("not a url")),
    ] {
        assert!(
            validate_request_authority(host, origin, 47657).is_err(),
            "{host:?} {origin:?}"
        );
    }
}
