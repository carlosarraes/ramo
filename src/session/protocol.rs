pub const SESSION_API_VERSION: u32 = 1;
pub const SESSION_DAEMON_VERSION: u32 = 1;
pub const SESSION_REGISTRATION_VERSION: u32 = 1;

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
