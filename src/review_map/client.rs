use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::Duration;

use ramo_core::review_map::{
    REVIEW_MAP_SCHEMA_VERSION, ReviewMap, ReviewMapFailureCode, ReviewMapStatus,
};

const MAX_REVIEW_MAP_HEADERS: usize = 32 * 1024;
pub const MAX_REVIEW_MAP_RESPONSE: usize = 1024 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReviewMapResolveRequest {
    pub schema_version: u16,
    pub repository: String,
    pub pull_request: u64,
    pub expected_head_sha: String,
}

impl ReviewMapResolveRequest {
    pub fn new(
        repository: impl Into<String>,
        pull_request: u64,
        expected_head_sha: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: REVIEW_MAP_SCHEMA_VERSION,
            repository: repository.into(),
            pull_request,
            expected_head_sha: expected_head_sha.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewMapPoll {
    pub job_id: String,
    pub state: ReviewMapStatus,
    pub map: ReviewMap,
    pub failure: Option<ReviewMapClientError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewMapClientError {
    code: ReviewMapFailureCode,
    message: String,
}

impl ReviewMapClientError {
    pub fn new(code: ReviewMapFailureCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> ReviewMapFailureCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ReviewMapClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReviewMapClientError {}

pub trait ReviewMapService: Send + 'static {
    fn resolve(
        &self,
        request: &ReviewMapResolveRequest,
    ) -> Result<ReviewMapPoll, ReviewMapClientError>;

    fn poll(&self, job_id: &str) -> Result<ReviewMapPoll, ReviewMapClientError>;
}

#[derive(Clone)]
pub struct ReviewMapClient {
    endpoint: String,
    address: SocketAddr,
    token: String,
    timeout: Duration,
}

impl ReviewMapClient {
    pub fn new(
        endpoint: impl Into<String>,
        token: impl Into<String>,
    ) -> Result<Self, ReviewMapClientError> {
        Self::with_timeout(endpoint, token, DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(
        endpoint: impl Into<String>,
        token: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, ReviewMapClientError> {
        let endpoint = endpoint.into();
        let address = parse_loopback_endpoint(&endpoint)?;
        let token = token.into();
        if token.trim().is_empty() || token.contains(['\r', '\n']) {
            return Err(incompatible("Review Map client token is invalid"));
        }
        Ok(Self {
            endpoint,
            address,
            token,
            timeout,
        })
    }

    fn send(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Result<ReviewMapPoll, ReviewMapClientError> {
        let mut stream = TcpStream::connect_timeout(&self.address, self.timeout)
            .map_err(|_| unavailable("Could not connect to local ramo-server"))?;
        stream
            .set_read_timeout(Some(self.timeout))
            .and_then(|()| stream.set_write_timeout(Some(self.timeout)))
            .map_err(|_| unavailable("Could not configure the local ramo-server connection"))?;
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n",
            self.address, self.token
        )
        .map_err(|_| unavailable("Could not write to local ramo-server"))?;
        if !body.is_empty() {
            write!(
                stream,
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            )
            .map_err(|_| unavailable("Could not write to local ramo-server"))?;
        }
        stream
            .write_all(b"\r\n")
            .and_then(|()| stream.write_all(body))
            .and_then(|()| stream.flush())
            .map_err(|_| unavailable("Could not write to local ramo-server"))?;

        let response = read_response(&mut stream)?;
        decode_response(response)
    }
}

impl std::fmt::Debug for ReviewMapClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReviewMapClient")
            .field("endpoint", &self.endpoint)
            .field("token", &"[REDACTED]")
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl ReviewMapService for ReviewMapClient {
    fn resolve(
        &self,
        request: &ReviewMapResolveRequest,
    ) -> Result<ReviewMapPoll, ReviewMapClientError> {
        let body = serde_json::to_vec(request)
            .map_err(|_| incompatible("Could not encode Review Map request"))?;
        let response = self.send("POST", "/v1/review-maps", &body)?;
        if response.map.identity.head_sha != request.expected_head_sha {
            return Err(ReviewMapClientError::new(
                ReviewMapFailureCode::ResultStale,
                "ramo-server returned a map for a different PR revision",
            ));
        }
        Ok(response)
    }

    fn poll(&self, job_id: &str) -> Result<ReviewMapPoll, ReviewMapClientError> {
        if job_id.is_empty()
            || !job_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(incompatible("Review Map job id is invalid"));
        }
        self.send("GET", &format!("/v1/review-maps/{job_id}"), &[])
    }
}

pub fn validate_loopback_endpoint(endpoint: &str) -> Result<(), ReviewMapClientError> {
    parse_loopback_endpoint(endpoint).map(|_| ())
}

fn parse_loopback_endpoint(endpoint: &str) -> Result<SocketAddr, ReviewMapClientError> {
    let authority = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| incompatible("Review Map server must use loopback HTTP"))?;
    if authority.contains(['/', '?', '#', '@']) {
        return Err(incompatible(
            "Review Map server endpoint must contain only a loopback host and port",
        ));
    }
    let address = authority
        .parse::<SocketAddr>()
        .map_err(|_| incompatible("Review Map server endpoint must include a valid port"))?;
    if !address.ip().is_loopback()
        || !matches!(address.ip(), IpAddr::V4(_) | IpAddr::V6(_))
        || address.port() == 0
    {
        return Err(incompatible(
            "Review Map server must use a loopback address",
        ));
    }
    Ok(address)
}

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

fn read_response(stream: &mut TcpStream) -> Result<HttpResponse, ReviewMapClientError> {
    let mut bytes = Vec::with_capacity(4096);
    let header_end = loop {
        if bytes.len() > MAX_REVIEW_MAP_HEADERS {
            return Err(incompatible("ramo-server response headers are too large"));
        }
        let mut chunk = [0_u8; 4096];
        let count = stream
            .read(&mut chunk)
            .map_err(|_| unavailable("Could not read from local ramo-server"))?;
        if count == 0 {
            return Err(incompatible("ramo-server returned an incomplete response"));
        }
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(index) = find_bytes(&bytes, b"\r\n\r\n") {
            if index > MAX_REVIEW_MAP_HEADERS {
                return Err(incompatible("ramo-server response headers are too large"));
            }
            break index + 4;
        }
    };
    let head = std::str::from_utf8(&bytes[..header_end - 4])
        .map_err(|_| incompatible("ramo-server returned invalid HTTP headers"))?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| {
            let mut fields = line.split_whitespace();
            (fields.next() == Some("HTTP/1.1"))
                .then(|| fields.next())
                .flatten()
        })
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| incompatible("ramo-server returned an invalid HTTP status"))?;
    let mut headers = BTreeMap::new();
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| incompatible("ramo-server returned invalid HTTP headers"))?;
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || headers.insert(name, value.trim().to_owned()).is_some() {
            return Err(incompatible("ramo-server returned duplicate HTTP headers"));
        }
    }
    if headers.contains_key("transfer-encoding") {
        return Err(incompatible(
            "ramo-server returned unsupported chunked framing",
        ));
    }
    let content_length = headers
        .get("content-length")
        .ok_or_else(|| incompatible("ramo-server response is missing Content-Length"))?
        .parse::<usize>()
        .map_err(|_| incompatible("ramo-server returned invalid Content-Length"))?;
    if content_length > MAX_REVIEW_MAP_RESPONSE {
        return Err(incompatible("ramo-server response exceeds the 1 MiB limit"));
    }
    let target = header_end.saturating_add(content_length);
    if bytes.len() > target {
        bytes.truncate(target);
    }
    while bytes.len() < target {
        let remaining = target - bytes.len();
        let mut chunk = [0_u8; 4096];
        let count = stream
            .read(&mut chunk[..remaining.min(4096)])
            .map_err(|_| unavailable("Could not read from local ramo-server"))?;
        if count == 0 {
            return Err(incompatible("ramo-server returned an incomplete body"));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok(HttpResponse {
        status,
        body: bytes[header_end..target].to_vec(),
    })
}

#[derive(serde::Deserialize)]
struct WireResponse {
    schema_version: u16,
    job_id: String,
    state: ReviewMapStatus,
    map: ReviewMap,
    failure: Option<WireFailure>,
}

#[derive(serde::Deserialize)]
struct ErrorEnvelope {
    schema_version: u16,
    failure: WireFailure,
}

#[derive(serde::Deserialize)]
struct WireFailure {
    code: ReviewMapFailureCode,
    message: String,
}

fn decode_response(response: HttpResponse) -> Result<ReviewMapPoll, ReviewMapClientError> {
    if !(200..300).contains(&response.status) {
        let envelope = serde_json::from_slice::<ErrorEnvelope>(&response.body)
            .map_err(|_| incompatible("ramo-server returned a malformed error"))?;
        ensure_schema(envelope.schema_version)?;
        return Err(ReviewMapClientError::new(
            envelope.failure.code,
            envelope.failure.message,
        ));
    }
    let response = serde_json::from_slice::<WireResponse>(&response.body)
        .map_err(|_| incompatible("ramo-server returned malformed Review Map JSON"))?;
    ensure_schema(response.schema_version)?;
    ensure_schema(response.map.schema_version)?;
    Ok(ReviewMapPoll {
        job_id: response.job_id,
        state: response.state,
        map: response.map,
        failure: response
            .failure
            .map(|failure| ReviewMapClientError::new(failure.code, failure.message)),
    })
}

fn ensure_schema(version: u16) -> Result<(), ReviewMapClientError> {
    if version == REVIEW_MAP_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(incompatible(
            "ramo-server uses an incompatible Review Map schema",
        ))
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn unavailable(message: &str) -> ReviewMapClientError {
    ReviewMapClientError::new(ReviewMapFailureCode::ServerUnreachable, message)
}

fn incompatible(message: &str) -> ReviewMapClientError {
    ReviewMapClientError::new(ReviewMapFailureCode::ServerIncompatible, message)
}
