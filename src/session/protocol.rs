use serde::{Deserialize, Serialize};

pub const SESSION_API_VERSION: u32 = 1;
pub const SESSION_DAEMON_VERSION: u32 = 1;
pub const SESSION_REGISTRATION_VERSION: u32 = 1;
pub const SESSION_API_PATH: &str = "/session-api";
pub const SESSION_CAPABILITIES_PATH: &str = "/session-api/capabilities";
pub const MAX_HTTP_BODY_BYTES: usize = 256 * 1024;
pub const MAX_HTTP_HEADER_BYTES: usize = 32 * 1024;
pub const MAX_HTTP_RESPONSE_BYTES: usize = 1024 * 1024;
pub const MAX_SESSION_COMMENT_BATCH: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCapabilities {
    pub version: u32,
    pub daemon_version: u32,
    pub actions: Vec<String>,
}

impl Default for SessionCapabilities {
    fn default() -> Self {
        Self {
            version: SESSION_API_VERSION,
            daemon_version: SESSION_DAEMON_VERSION,
            actions: supported_session_actions()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionApiError {
    pub error: String,
    pub code: String,
}

pub const fn supported_session_actions() -> [&'static str; 11] {
    [
        "list",
        "get",
        "context",
        "review",
        "navigate",
        "reload",
        "comment-add",
        "comment-apply",
        "comment-list",
        "comment-rm",
        "comment-clear",
    ]
}
