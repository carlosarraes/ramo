use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use super::{
    MAX_HTTP_BODY_BYTES, MAX_HTTP_HEADER_BYTES, MAX_HTTP_RESPONSE_BYTES, SESSION_API_PATH,
    SESSION_CAPABILITIES_PATH, SessionApiError, SessionCapabilities, SessionRegistry,
    SessionSelector, build_session_context, build_session_review, validate_request_authority,
};

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    body: Value,
}

pub(crate) fn serve_http_connection(
    mut stream: TcpStream,
    port: u16,
    registry: &Arc<Mutex<SessionRegistry>>,
    stop: &Arc<AtomicBool>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let (response, shutdown) = match read_request(&mut stream) {
        Ok(request) => {
            let shutdown = request.method == "POST" && request.path == "/daemon/shutdown";
            let response = route(request, port, registry);
            let shutdown = shutdown && response.status == 200;
            (response, shutdown)
        }
        Err(error) => (error, false),
    };
    let _ = write_response(&mut stream, response);
    if shutdown {
        stop.store(true, Ordering::Release);
    }
}

fn read_request(stream: &mut TcpStream) -> Result<HttpRequest, HttpResponse> {
    let mut bytes = Vec::with_capacity(1024);
    let header_end = loop {
        if bytes.len() >= MAX_HTTP_HEADER_BYTES {
            drain_header_tail(stream);
            return Err(error_response(
                431,
                "headers-too-large",
                "HTTP headers are too large",
            ));
        }
        let mut buffer = [0_u8; 4096];
        let count = stream
            .read(&mut buffer)
            .map_err(|_| error_response(400, "invalid-request", "Could not read HTTP request"))?;
        if count == 0 {
            return Err(error_response(
                400,
                "incomplete-request",
                "Expected a complete HTTP request",
            ));
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = find_bytes(&bytes, b"\r\n\r\n") {
            break index + 4;
        }
    };
    if header_end - 4 > MAX_HTTP_HEADER_BYTES {
        return Err(error_response(
            431,
            "headers-too-large",
            "HTTP headers are too large",
        ));
    }
    let head = std::str::from_utf8(&bytes[..header_end - 4])
        .map_err(|_| error_response(400, "invalid-headers", "HTTP headers must be valid UTF-8"))?;
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next().unwrap_or_default().split_whitespace();
    let method = request_line.next().unwrap_or_default().to_owned();
    let path = request_line.next().unwrap_or_default().to_owned();
    let version = request_line.next().unwrap_or_default();
    if method.is_empty()
        || !path.starts_with('/')
        || version != "HTTP/1.1"
        || request_line.next().is_some()
    {
        return Err(error_response(
            400,
            "invalid-request-line",
            "Expected one HTTP/1.1 request line",
        ));
    }
    let mut headers = BTreeMap::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(|| {
            error_response(400, "invalid-header", "Expected name: value HTTP headers")
        })?;
        let name = name.trim().to_ascii_lowercase();
        if name.is_empty() || headers.contains_key(&name) {
            return Err(error_response(
                400,
                "invalid-header",
                "Duplicate or empty HTTP header",
            ));
        }
        headers.insert(name, value.trim().to_owned());
    }
    if headers.contains_key("transfer-encoding") {
        return Err(error_response(
            400,
            "unsupported-transfer-encoding",
            "Chunked request bodies are not supported",
        ));
    }
    let content_length = match headers.get("content-length") {
        Some(value) => value
            .parse::<usize>()
            .map_err(|_| error_response(400, "invalid-content-length", "Invalid Content-Length"))?,
        None => 0,
    };
    if content_length > MAX_HTTP_BODY_BYTES {
        return Err(error_response(
            413,
            "body-too-large",
            "Session API request body exceeds 256 KiB",
        ));
    }
    let already_read = bytes.len() - header_end;
    let total = header_end + content_length;
    if already_read > content_length {
        bytes.truncate(total);
    }
    while bytes.len() < total {
        let remaining = total - bytes.len();
        let mut buffer = [0_u8; 4096];
        let read_length = remaining.min(buffer.len());
        let count = stream.read(&mut buffer[..read_length]).map_err(|_| {
            error_response(
                400,
                "incomplete-body",
                "Could not read complete request body",
            )
        })?;
        if count == 0 {
            return Err(error_response(
                400,
                "incomplete-body",
                "Expected the complete request body",
            ));
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(HttpRequest {
        method,
        path: path.split('?').next().unwrap_or(&path).to_owned(),
        headers,
        body: bytes[header_end..total].to_vec(),
    })
}

fn drain_header_tail(stream: &mut TcpStream) {
    let mut retained = [0_u8; 3];
    let mut retained_len = 0;
    let mut remaining = 4096_usize;
    while remaining > 0 {
        let mut buffer = [0_u8; 512];
        let read_length = remaining.min(buffer.len());
        let Ok(count) = stream.read(&mut buffer[..read_length]) else {
            return;
        };
        if count == 0 {
            return;
        }
        let mut combined = Vec::with_capacity(retained_len + count);
        combined.extend_from_slice(&retained[..retained_len]);
        combined.extend_from_slice(&buffer[..count]);
        if find_bytes(&combined, b"\r\n\r\n").is_some() {
            return;
        }
        retained_len = combined.len().min(3);
        retained[..retained_len]
            .copy_from_slice(&combined[combined.len() - retained_len..combined.len()]);
        remaining -= count;
    }
}

fn route(request: HttpRequest, port: u16, registry: &Arc<Mutex<SessionRegistry>>) -> HttpResponse {
    let Some(host) = request.headers.get("host") else {
        return error_response(400, "missing-host", "Expected Host header");
    };
    if let Err(error) = validate_request_authority(
        host,
        request.headers.get("origin").map(String::as_str),
        port,
    ) {
        return error_response(403, "forbidden-authority", &error.to_string());
    }
    match request.path.as_str() {
        "/health" => {
            if request.method != "GET" {
                return method_not_allowed("Health requests must use GET");
            }
            HttpResponse {
                status: 200,
                body: json!({
                    "ok": true,
                    "name": "ramo-session-broker",
                    "version": super::SESSION_DAEMON_VERSION,
                    "sessionApi": SESSION_API_PATH,
                    "sessionCapabilities": SESSION_CAPABILITIES_PATH,
                }),
            }
        }
        SESSION_CAPABILITIES_PATH => {
            if request.method != "GET" {
                return method_not_allowed("Capability requests must use GET");
            }
            json_response(200, &SessionCapabilities::default())
        }
        "/mcp" => error_response(
            410,
            "mcp-removed",
            "This app no longer exposes agent-facing MCP tools. Use the session CLI instead.",
        ),
        "/daemon/shutdown" => {
            if request.method != "POST" {
                return method_not_allowed("Daemon shutdown requests must use POST");
            }
            HttpResponse {
                status: 200,
                body: json!({"ok": true}),
            }
        }
        SESSION_API_PATH => route_session_api(request, registry),
        _ => error_response(
            404,
            "not-found",
            "No ramo session route exists at this path",
        ),
    }
}

fn route_session_api(request: HttpRequest, registry: &Arc<Mutex<SessionRegistry>>) -> HttpResponse {
    if request.method != "POST" {
        return method_not_allowed("Session API requests must use POST");
    }
    let content_type = request
        .headers
        .get("content-type")
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !content_type.is_some_and(|value| value.eq_ignore_ascii_case("application/json")) {
        return error_response(
            415,
            "unsupported-content-type",
            "Expected Content-Type application/json",
        );
    }
    let input: Value = match serde_json::from_slice(&request.body) {
        Ok(input) => input,
        Err(_) => {
            return error_response(400, "invalid-json", "Expected one JSON request body");
        }
    };
    let Some(action) = input
        .get("action")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        return error_response(400, "missing-action", "Expected a session action");
    };
    if action == "list" {
        let registry = match registry.lock() {
            Ok(registry) => registry,
            Err(_) => {
                return error_response(500, "registry-poisoned", "Session registry unavailable");
            }
        };
        let sessions: Vec<_> = registry
            .list()
            .into_iter()
            .map(|session| session.registration.descriptor)
            .collect();
        return HttpResponse {
            status: 200,
            body: json!({"sessions": sessions}),
        };
    }
    let selector = match input.get("selector").cloned() {
        Some(value) => match serde_json::from_value::<SessionSelector>(value) {
            Ok(selector) => selector,
            Err(_) => {
                return error_response(400, "invalid-selector", "Expected one session selector");
            }
        },
        None => return error_response(400, "missing-selector", "Expected one session selector"),
    };
    let selector_count = usize::from(selector.session_id.is_some())
        + usize::from(selector.repo_root.is_some())
        + usize::from(selector.session_path.is_some());
    if selector_count != 1 {
        return error_response(
            400,
            "invalid-selector",
            "Select exactly one session id, repository, or session path",
        );
    }
    let mutation = matches!(
        action.as_str(),
        "navigate"
            | "reload"
            | "comment-add"
            | "comment-apply"
            | "comment-list"
            | "comment-rm"
            | "comment-clear"
    );
    if mutation {
        let timeout = if matches!(action.as_str(), "reload" | "comment-apply") {
            Duration::from_secs(30)
        } else {
            Duration::from_secs(5)
        };
        return match super::dispatch_session_command(registry, &selector, input, timeout) {
            Ok(outcome) => match outcome.result {
                Ok(result) if action == "comment-list" => HttpResponse {
                    status: 200,
                    body: json!({"comments": result}),
                },
                Ok(result) => HttpResponse {
                    status: 200,
                    body: json!({"result": result}),
                },
                Err(error) => error_response(400, "command-rejected", &error),
            },
            Err(error) => error_response(503, "command-unavailable", &error),
        };
    }
    let session = match registry.lock() {
        Ok(registry) => match registry.select(&selector) {
            Ok(session) => session,
            Err(error) => return error_response(404, "session-not-found", &error),
        },
        Err(_) => return error_response(500, "registry-poisoned", "Session registry unavailable"),
    };
    match action.as_str() {
        "get" => HttpResponse {
            status: 200,
            body: json!({"session": session.registration.descriptor}),
        },
        "context" => HttpResponse {
            status: 200,
            body: json!({
                "context": build_session_context(&session.registration, &session.snapshot)
            }),
        },
        "review" => HttpResponse {
            status: 200,
            body: json!({
                "review": build_session_review(
                    &session.registration,
                    &session.snapshot,
                    input.get("includePatch").and_then(Value::as_bool).unwrap_or(false),
                    input.get("includeNotes").and_then(Value::as_bool).unwrap_or(false),
                )
            }),
        },
        _ => error_response(400, "unsupported-action", "Unsupported ramo session action"),
    }
}

fn method_not_allowed(message: &str) -> HttpResponse {
    error_response(405, "method-not-allowed", message)
}

fn error_response(status: u16, code: &str, message: &str) -> HttpResponse {
    json_response(
        status,
        &SessionApiError {
            error: message.to_owned(),
            code: code.to_owned(),
        },
    )
}

fn json_response(status: u16, value: &impl serde::Serialize) -> HttpResponse {
    HttpResponse {
        status,
        body: serde_json::to_value(value).unwrap_or_else(
            |_| json!({"error":"Could not serialize session response","code":"serialization"}),
        ),
    }
}

fn write_response(stream: &mut TcpStream, response: HttpResponse) -> io::Result<()> {
    let mut status = response.status;
    let mut body = serde_json::to_vec(&response.body)?;
    if body.len() > MAX_HTTP_RESPONSE_BYTES {
        status = 500;
        body = serde_json::to_vec(&SessionApiError {
            error: "Session response exceeds the 1 MiB limit".into(),
            code: "response-too-large".into(),
        })?;
    }
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        410 => "Gone",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        status,
        reason,
        body.len()
    )?;
    stream.write_all(&body)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
