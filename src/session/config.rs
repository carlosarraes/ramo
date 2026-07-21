use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

pub const DEFAULT_SESSION_HOST: &str = "127.0.0.1";
pub const DEFAULT_SESSION_PORT: u16 = 47_657;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConfigError(String);

impl fmt::Display for SessionConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SessionConfigError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionAddress {
    socket: SocketAddr,
}

impl SessionAddress {
    pub fn parse(host: &str, port: u16) -> Result<Self, SessionConfigError> {
        if port == 0 {
            return Err(SessionConfigError(
                "ramo session port must be between 1 and 65535".into(),
            ));
        }
        let ip = parse_loopback_host(host)?;
        Ok(Self {
            socket: SocketAddr::new(ip, port),
        })
    }

    pub const fn loopback_ephemeral() -> Self {
        Self {
            socket: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        }
    }

    pub fn from_socket_addr(socket: SocketAddr) -> Result<Self, SessionConfigError> {
        if socket.port() == 0 || !is_loopback_ip(socket.ip()) {
            return Err(SessionConfigError(
                "ramo sessions require a nonzero loopback socket address".into(),
            ));
        }
        Ok(Self { socket })
    }

    pub const fn ip(self) -> IpAddr {
        self.socket.ip()
    }

    pub const fn port(self) -> u16 {
        self.socket.port()
    }

    pub const fn socket_addr(self) -> SocketAddr {
        self.socket
    }
}

pub fn resolve_session_address(
    env: &HashMap<String, String>,
) -> Result<SessionAddress, SessionConfigError> {
    let host =
        first_nonempty(env, "RAMO_SESSION_HOST", "HUNK_MCP_HOST").unwrap_or(DEFAULT_SESSION_HOST);
    let port = match first_nonempty(env, "RAMO_SESSION_PORT", "HUNK_MCP_PORT") {
        Some(value) => value.parse::<u16>().map_err(|_| {
            SessionConfigError(format!(
                "invalid ramo session port {value:?}; expected an integer from 1 to 65535"
            ))
        })?,
        None => DEFAULT_SESSION_PORT,
    };
    SessionAddress::parse(host, port)
}

pub fn validate_request_authority(
    host: &str,
    origin: Option<&str>,
    expected_port: u16,
) -> Result<(), SessionConfigError> {
    validate_authority(host, expected_port).map_err(|_| {
        SessionConfigError("Host header is not allowed for the local ramo session broker".into())
    })?;
    if let Some(origin) = origin {
        let authority = origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"))
            .ok_or_else(|| {
                SessionConfigError(
                    "Origin is not allowed for the local ramo session broker".into(),
                )
            })?;
        let authority = authority.split('/').next().unwrap_or_default();
        if authority.contains('@') || validate_authority(authority, expected_port).is_err() {
            return Err(SessionConfigError(
                "Origin is not allowed for the local ramo session broker".into(),
            ));
        }
    }
    Ok(())
}

fn first_nonempty<'a>(
    env: &'a HashMap<String, String>,
    preferred: &str,
    compatible: &str,
) -> Option<&'a str> {
    [preferred, compatible]
        .into_iter()
        .filter_map(|key| env.get(key))
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
}

fn validate_authority(authority: &str, expected_port: u16) -> Result<(), SessionConfigError> {
    let (host, port) = split_authority(authority)?;
    if port != expected_port {
        return Err(SessionConfigError("unexpected port".into()));
    }
    parse_loopback_host(host).map(|_| ())
}

fn split_authority(authority: &str) -> Result<(&str, u16), SessionConfigError> {
    let authority = authority.trim();
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, port) = rest
            .split_once("]:")
            .ok_or_else(|| SessionConfigError("invalid bracketed authority".into()))?;
        return Ok((host, parse_port(port)?));
    }
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| SessionConfigError("authority must include the broker port".into()))?;
    if host.contains(':') {
        return Err(SessionConfigError(
            "IPv6 Host headers must use brackets".into(),
        ));
    }
    Ok((host, parse_port(port)?))
}

fn parse_port(value: &str) -> Result<u16, SessionConfigError> {
    value
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| SessionConfigError("invalid authority port".into()))
}

fn parse_loopback_host(host: &str) -> Result<IpAddr, SessionConfigError> {
    let normalized = host.trim().trim_start_matches('[').trim_end_matches(']');
    let ip = if normalized.eq_ignore_ascii_case("localhost") {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    } else {
        normalized.parse::<IpAddr>().map_err(|_| {
            SessionConfigError(format!(
                "ramo session host {host:?} is not a loopback IP or localhost"
            ))
        })?
    };
    if !is_loopback_ip(ip) {
        return Err(SessionConfigError(format!(
            "ramo session host {host:?} is not loopback; remote binding is forbidden"
        )));
    }
    Ok(ip)
}

fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.to_ipv4().is_some_and(|ip| ip.is_loopback()),
    }
}
