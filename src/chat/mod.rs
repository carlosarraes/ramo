//! The `C` chat pane: a persistent conversation about the pull request under review.
//!
//! Unlike Ask — which is one stateless question per hunk — chat keeps a pi session so the model
//! remembers both the thread and the files it has already read. That session is a real file
//! under `~/.pi/agent/sessions/`, which is a deliberate divergence from Ask's `--no-session`
//! and is documented as such.

use std::path::PathBuf;

use ramo_core::pi::{PiError, PiRequest, PiSession, PiTools};

pub const SYSTEM_PROMPT: &str = "You are helping a reviewer understand a pull request they are \
reading in a terminal. You have read-only access to the repository through the read tool; use it \
to follow code beyond the diff when that answers the question. You cannot edit, write, or run \
anything, and must never claim to have done so. Answer in plain text, concise and concrete. Cite \
paths and line numbers when you refer to code. Say plainly when something cannot be determined \
from what you can see.";

/// One exchange in the pane. A turn is `Pending` until its reply lands, so the pane can show
/// the question immediately while the reviewer keeps reading the diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurn {
    pub question: String,
    pub state: ChatState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatState {
    Pending,
    Answered(String),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSettings {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub effort: String,
    pub timeout: std::time::Duration,
}

/// Everything ramo knows that the model does not: the PR, the ticket, and where the cursor is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatContext {
    pub pull_request: Option<String>,
    pub ticket: Option<String>,
    pub file: Option<String>,
}

impl ChatContext {
    /// Rendered once, on the first turn only. Later turns ride the pi session, so repeating it
    /// would just burn context.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (label, value) in [
            ("PULL REQUEST", self.pull_request.as_deref()),
            ("LINEAR TICKET", self.ticket.as_deref()),
            ("CURRENTLY READING", self.file.as_deref()),
        ] {
            let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
                continue;
            };
            out.push_str(label);
            out.push('\n');
            out.push_str(crate::input::sanitize_terminal_text(value, false).trim());
            out.push_str("\n\n");
        }
        out
    }
}

pub fn compose_prompt(context: &ChatContext, question: &str, first_turn: bool) -> String {
    let question = crate::input::sanitize_terminal_text(question, false);
    if first_turn {
        format!("{}QUESTION\n{}\n", context.render(), question.trim())
    } else {
        format!("QUESTION\n{}\n", question.trim())
    }
}

/// `--tools read` is read-only by construction: no write, no edit, no bash.
pub fn request(settings: &ChatSettings, session_id: &str, prompt: String) -> PiRequest {
    PiRequest {
        provider: settings.provider.clone(),
        model: settings.model.clone(),
        thinking: settings.effort.clone(),
        timeout: settings.timeout,
        prompt,
        system_prompt: SYSTEM_PROMPT.to_owned(),
        tools: PiTools::Allow(vec!["read".to_owned()]),
        session: PiSession::Id(session_id.to_owned()),
        env: Vec::new(),
    }
}

/// A session id ramo mints itself, so nothing has to be scraped from pi's output.
pub fn new_session_id(seed: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in seed.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("ramo-{hash:016x}")
}

/// Where pi keeps the transcript, so it can be named in the docs and removed by hand.
pub fn session_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".pi/agent/sessions"))
}

pub type ChatError = PiError;

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ChatContext {
        ChatContext {
            pull_request: Some("PR #482 · Add retry backoff".into()),
            ticket: Some("MON-2799 · Reject quote-only filters".into()),
            file: Some("src/retry.rs".into()),
        }
    }

    #[test]
    fn the_first_turn_carries_the_context_and_later_turns_do_not() {
        let first = compose_prompt(&context(), "why the backoff?", true);
        assert!(first.contains("PULL REQUEST"), "{first}");
        assert!(first.contains("MON-2799"), "{first}");
        assert!(first.contains("src/retry.rs"), "{first}");
        assert!(first.contains("QUESTION\nwhy the backoff?"), "{first}");

        // The session carries the thread, so repeating the context would only burn tokens.
        let second = compose_prompt(&context(), "and the cap?", false);
        assert!(!second.contains("PULL REQUEST"), "{second}");
        assert_eq!(second, "QUESTION\nand the cap?\n");
    }

    #[test]
    fn absent_context_sections_are_omitted_rather_than_left_empty() {
        let rendered = ChatContext {
            pull_request: Some("PR #1".into()),
            ticket: None,
            file: Some("   ".into()),
        }
        .render();
        assert!(rendered.contains("PULL REQUEST"), "{rendered}");
        assert!(!rendered.contains("LINEAR TICKET"), "{rendered}");
        assert!(!rendered.contains("CURRENTLY READING"), "{rendered}");
    }

    #[test]
    fn the_request_is_read_only_and_keeps_one_named_session() {
        let settings = ChatSettings {
            enabled: true,
            provider: "openai-codex".into(),
            model: "gpt-5.6-luna".into(),
            effort: "max".into(),
            timeout: std::time::Duration::from_secs(300),
        };
        let request = request(&settings, "ramo-abc", "QUESTION\nwhy?".into());

        assert_eq!(request.tools, PiTools::Allow(vec!["read".to_owned()]));
        assert_eq!(request.session, PiSession::Id("ramo-abc".into()));
        assert!(request.system_prompt.contains("read-only"));
    }

    #[test]
    fn a_session_id_is_stable_per_review_and_distinct_across_reviews() {
        assert_eq!(
            new_session_id("owner/repo#482"),
            new_session_id("owner/repo#482")
        );
        assert_ne!(
            new_session_id("owner/repo#482"),
            new_session_id("owner/repo#483")
        );
        assert!(new_session_id("x").starts_with("ramo-"));
    }
}
