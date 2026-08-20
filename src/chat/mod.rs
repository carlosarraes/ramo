//! The `C` chat pane: a persistent conversation about the pull request under review.
//!
//! Unlike Ask — which is one stateless question per hunk — chat keeps a pi session so the model
//! remembers both the thread and the files it has already read. That session is a real file
//! under `~/.pi/agent/sessions/`, which is a deliberate divergence from Ask's `--no-session`
//! and is documented as such.

pub mod session;
pub mod store;

use std::collections::BTreeMap;
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTurn {
    pub question: String,
    pub state: ChatState,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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

/// What the reviewer has written, as chat should see it. Gathered fresh each turn; the watermark
/// decides how much of it is actually new.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewerWork {
    /// `(id, anchor, body)` for each inline note.
    pub notes: Vec<(String, String, String)>,
    /// `(id, question, answer)` for each answered Ask.
    pub asks: Vec<(String, String, String)>,
    pub overall: Option<String>,
}

/// How much of the conversation's context this process has already sent.
///
/// This replaces the old "is this the first turn?" test. That test asked whether the transcript
/// was empty, which is the wrong question the moment a transcript can be restored from disk: a
/// restored conversation would look like a continuation and silently skip the header, leaving the
/// model with a bare question. Tracking what was *dispatched* rather than what is *displayed*
/// keeps the two apart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatSent {
    header: bool,
    notes: BTreeMap<String, u64>,
    asks: BTreeMap<String, u64>,
    overall: Option<u64>,
}

/// Caps. A long review can hold hundreds of notes and Ask answers are model prose, so without
/// these the prompt grows without bound and the request times out for reasons nobody can see.
const MAX_ENTRIES: usize = 40;
const MAX_NOTE_CHARS: usize = 2_000;
const MAX_ANSWER_CHARS: usize = 800;
const MAX_REPLAY_TURNS: usize = 8;
const MAX_REPLAY_CHARS: usize = 6_000;

fn digest(value: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn clamp(value: &str, limit: usize) -> String {
    let value = crate::input::sanitize_terminal_text(value, false);
    let value = value.trim();
    if value.chars().count() <= limit {
        return value.to_owned();
    }
    let kept: String = value.chars().take(limit).collect();
    format!("{kept}… (truncated)")
}

fn section(out: &mut String, label: &str, lines: &[String], dropped: usize) {
    if lines.is_empty() {
        return;
    }
    out.push_str(label);
    out.push('\n');
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    if dropped > 0 {
        out.push_str(&format!(
            "… {dropped} older not shown
"
        ));
    }
    out.push('\n');
}

impl ChatContext {
    /// The standing facts about what is being reviewed. Sent once per process — see `ChatSent`.
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

/// Builds the prompt and the watermark that would follow from sending it. The caller commits the
/// new watermark only once the request is actually dispatched, so a refused turn does not swallow
/// the delta it never sent.
pub fn compose_prompt(
    context: &ChatContext,
    work: &ReviewerWork,
    sent: &ChatSent,
    replay: &[ChatTurn],
    question: &str,
) -> (String, ChatSent) {
    let question = crate::input::sanitize_terminal_text(question, false);
    let mut out = String::new();
    let mut next = sent.clone();

    if !sent.header {
        out.push_str(&context.render());
        next.header = true;
    }
    // Only ever non-empty when pi's own session could not be resumed, so the thread has to be
    // carried in the prompt instead. Newest turns first: the tail of a conversation is the part
    // a follow-up actually depends on.
    if !replay.is_empty() {
        let mut lines = Vec::new();
        let mut budget = MAX_REPLAY_CHARS;
        for turn in replay.iter().rev().take(MAX_REPLAY_TURNS) {
            let ChatState::Answered(answer) = &turn.state else {
                continue;
            };
            let entry = format!(
                "- asked: {}\n  answered: {}",
                clamp(&turn.question, MAX_NOTE_CHARS),
                clamp(answer, MAX_ANSWER_CHARS)
            );
            if entry.chars().count() > budget {
                break;
            }
            budget -= entry.chars().count();
            lines.push(entry);
        }
        lines.reverse();
        section(&mut out, "PREVIOUS CONVERSATION", &lines, 0);
    }

    let mut fresh = Vec::new();
    let mut changed = Vec::new();
    for (id, anchor, body) in &work.notes {
        let body = clamp(body, MAX_NOTE_CHARS);
        let mark = digest(&body);
        let line = format!("- {anchor}: {body}");
        match sent.notes.get(id) {
            Some(seen) if *seen == mark => {}
            Some(_) => changed.push(line),
            None => fresh.push(line),
        }
        next.notes.insert(id.clone(), mark);
    }
    let removed: Vec<String> = sent
        .notes
        .keys()
        .filter(|id| !work.notes.iter().any(|(note, _, _)| note == *id))
        .map(|id| format!("- {id}"))
        .collect();
    for id in sent.notes.keys() {
        if !work.notes.iter().any(|(note, _, _)| note == id) {
            next.notes.remove(id);
        }
    }

    let mut answers = Vec::new();
    for (id, ask_question, answer) in &work.asks {
        let answer = clamp(answer, MAX_ANSWER_CHARS);
        let mark = digest(&answer);
        if sent.asks.get(id) != Some(&mark) {
            answers.push(format!(
                "- asked: {}\n  answered: {answer}",
                clamp(ask_question, MAX_NOTE_CHARS)
            ));
        }
        next.asks.insert(id.clone(), mark);
    }

    for (label, mut lines) in [
        ("NEW REVIEW NOTES", fresh),
        ("UPDATED REVIEW NOTES", changed),
        ("REMOVED REVIEW NOTES", removed),
        ("NEW AI ANSWERS", answers),
    ] {
        let dropped = lines.len().saturating_sub(MAX_ENTRIES);
        lines.truncate(MAX_ENTRIES);
        section(&mut out, label, &lines, dropped);
    }

    if let Some(overall) = work.overall.as_deref().filter(|v| !v.trim().is_empty()) {
        let overall = clamp(overall, MAX_NOTE_CHARS);
        let mark = digest(&overall);
        if sent.overall != Some(mark) {
            out.push_str("OVERALL COMMENT\n");
            out.push_str(&overall);
            out.push_str("\n\n");
        }
        next.overall = Some(mark);
    }

    out.push_str("QUESTION\n");
    out.push_str(question.trim());
    out.push('\n');
    (out, next)
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

    fn note(id: &str, body: &str) -> (String, String, String) {
        (id.into(), "src/retry.rs:12".into(), body.into())
    }

    #[test]
    fn the_first_dispatch_carries_the_context_and_later_ones_do_not() {
        let work = ReviewerWork::default();
        let (first, sent) = compose_prompt(&context(), &work, &ChatSent::default(), &[], "why?");
        assert!(first.contains("PULL REQUEST"), "{first}");
        assert!(first.contains("MON-2799"), "{first}");
        assert!(first.contains("src/retry.rs"), "{first}");
        assert!(first.contains("QUESTION\nwhy?"), "{first}");

        // The session carries the thread, so repeating the context would only burn tokens.
        let (second, _) = compose_prompt(&context(), &work, &sent, &[], "and the cap?");
        assert!(!second.contains("PULL REQUEST"), "{second}");
        assert_eq!(second, "QUESTION\nand the cap?\n");
    }

    #[test]
    fn only_work_added_since_the_previous_dispatch_is_resent() {
        let mut work = ReviewerWork {
            notes: vec![note("n1", "this retry loop never terminates")],
            ..ReviewerWork::default()
        };
        let (first, sent) = compose_prompt(&context(), &work, &ChatSent::default(), &[], "why?");
        assert!(first.contains("never terminates"), "{first}");

        // Unchanged work must not ride along again.
        let (second, sent) = compose_prompt(&context(), &work, &sent, &[], "and?");
        assert!(!second.contains("never terminates"), "{second}");

        // A note written mid-conversation is new information and must reach the model.
        work.notes.push(note("n2", "this cap looks arbitrary"));
        let (third, sent) = compose_prompt(&context(), &work, &sent, &[], "what about the cap?");
        assert!(third.contains("NEW REVIEW NOTES"), "{third}");
        assert!(third.contains("arbitrary"), "{third}");
        assert!(!third.contains("never terminates"), "{third}");

        // Editing resends; deleting is named so the model stops relying on it.
        work.notes[0].2 = "actually it terminates on the third attempt".into();
        work.notes.remove(1);
        let (fourth, _) = compose_prompt(&context(), &work, &sent, &[], "so?");
        assert!(fourth.contains("UPDATED REVIEW NOTES"), "{fourth}");
        assert!(fourth.contains("third attempt"), "{fourth}");
        assert!(fourth.contains("REMOVED REVIEW NOTES"), "{fourth}");
        assert!(fourth.contains("n2"), "{fourth}");
    }

    #[test]
    fn answered_asks_ride_along_and_long_answers_are_truncated() {
        let work = ReviewerWork {
            asks: vec![("a1".into(), "why 429?".into(), "x".repeat(5_000))],
            ..ReviewerWork::default()
        };
        let (prompt, _) = compose_prompt(&context(), &work, &ChatSent::default(), &[], "and?");
        assert!(prompt.contains("NEW AI ANSWERS"), "{prompt}");
        assert!(prompt.contains("why 429?"), "{prompt}");
        assert!(prompt.contains("(truncated)"), "{prompt}");
        assert!(
            prompt.len() < 4_000,
            "answer was not capped: {}",
            prompt.len()
        );
    }

    #[test]
    fn a_long_review_caps_the_number_of_entries_and_says_so() {
        let work = ReviewerWork {
            notes: (0..60)
                .map(|n| note(&format!("n{n}"), &format!("finding {n}")))
                .collect(),
            ..ReviewerWork::default()
        };
        let (prompt, _) = compose_prompt(&context(), &work, &ChatSent::default(), &[], "summary?");
        assert!(prompt.contains("older not shown"), "{prompt}");
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
