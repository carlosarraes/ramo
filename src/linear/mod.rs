//! Reading a Linear ticket through the `linear` CLI.
//!
//! Mirrors `crate::github`: generic over `CommandExecutor` so tests never spawn a process, with
//! a typed error whose `Display` carries the remediation for each way this realistically fails.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::time::Duration;

use serde::{Deserialize, Deserializer};

use crate::process::command::{CaptureLimits, CommandExecutor, CommandRequest};

const STDOUT_LIMIT: usize = 256 * 1024;
const STDERR_LIMIT: usize = 8 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);

/// A ticket identifier such as `MON-2799`. Real branches spell it lowercase, so the key is
/// upper-cased before it reaches the CLI.
pub fn infer_ticket(body: &str, head_ref: &str, title: &str) -> Option<String> {
    // A Linear-authored URL in the description is the only signal that cannot be a coincidence.
    if let Some(id) = body
        .split("linear.app/")
        .skip(1)
        .filter_map(|rest| rest.split("/issue/").nth(1))
        .find_map(match_key)
    {
        return Some(id);
    }
    [head_ref, title, body].into_iter().find_map(match_key)
}

/// Words that form `word-123` in ordinary branch and commit names. `carraes/patch-1` is
/// GitHub's own default branch name, so without this every such branch would look like a ticket.
const NOT_TICKET_KEYS: &[&str] = &[
    "patch", "release", "fix", "fixes", "feat", "chore", "docs", "test", "tests", "part", "step",
    "rev", "pr", "issue", "wip", "utf", "iso", "sha", "rfc", "http", "https", "base", "gpt", "v",
    "version", "phase", "day", "week", "top", "no", "id", "pt", "ref",
];

fn match_key(text: &str) -> Option<String> {
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_alphabetic() {
            index += 1;
            continue;
        }
        // A key is not a key when it is glued to a preceding word character.
        if index > 0 && (bytes[index - 1].is_alphanumeric() || bytes[index - 1] == '_') {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphanumeric() {
            index += 1;
        }
        let letters = index - start;
        // Real Linear team keys are short; a long run is a word, not a key.
        if !(2..=6).contains(&letters) || index >= bytes.len() || bytes[index] != '-' {
            continue;
        }
        let digits_start = index + 1;
        let mut digits_end = digits_start;
        while digits_end < bytes.len() && bytes[digits_end].is_ascii_digit() {
            digits_end += 1;
        }
        if digits_end == digits_start {
            continue;
        }
        // A trailing word character means this was part of something longer.
        if bytes.get(digits_end).is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        let key: String = bytes[start..index].iter().collect();
        if NOT_TICKET_KEYS.contains(&key.to_ascii_lowercase().as_str()) {
            continue;
        }
        let number: String = bytes[digits_start..digits_end].iter().collect();
        if key.chars().any(|c| c.is_ascii_digit()) && !key.chars().next()?.is_ascii_alphabetic() {
            continue;
        }
        return Some(format!("{}-{number}", key.to_ascii_uppercase()));
    }
    None
}

#[derive(Debug)]
pub enum LinearError {
    MissingCli { command: String },
    Unauthenticated,
    NotFound { id: String },
    TimedOut,
    Truncated,
    InvalidJson(String),
    Failed { code: Option<i32>, stderr: String },
    Io(io::Error),
}

impl fmt::Display for LinearError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCli { command } => write!(
                formatter,
                "`{command}` was not found on PATH; install the Linear CLI or set enabled = false \
                 under [linear]"
            ),
            Self::Unauthenticated => formatter.write_str(
                "the Linear CLI is not authenticated; run `linear auth login` and retry",
            ),
            Self::NotFound { id } => write!(
                formatter,
                "{id} was not found; check that the authenticated workspace is the one that owns it"
            ),
            Self::TimedOut => formatter.write_str("the Linear CLI timed out"),
            Self::Truncated => formatter.write_str("the Linear ticket was too large to capture"),
            Self::InvalidJson(detail) => {
                write!(formatter, "the Linear CLI returned invalid JSON: {detail}")
            }
            Self::Failed { code, stderr } => match code {
                Some(code) => write!(
                    formatter,
                    "the Linear CLI exited with status {code}: {}",
                    first_line(stderr)
                ),
                None => write!(
                    formatter,
                    "the Linear CLI was terminated: {}",
                    first_line(stderr)
                ),
            },
            Self::Io(source) => write!(formatter, "could not run the Linear CLI: {source}"),
        }
    }
}

impl std::error::Error for LinearError {}

fn first_line(text: &str) -> &str {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no error output")
}

/// `#[serde(default)]` covers only *absent* keys. Linear sends an explicit `null` for every unset
/// relation — half a real board is unassigned — and a null is a type error against the field's own
/// type. This collapses both cases to the default, which is what the struct already promises.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Sampled from a real `linear issue view MON-2799 --json`. Everything is defaulted so an
/// unexpected or absent field degrades the card rather than failing the fetch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LinearTicket {
    #[serde(deserialize_with = "null_as_default")]
    pub identifier: String,
    #[serde(deserialize_with = "null_as_default")]
    pub title: String,
    #[serde(deserialize_with = "null_as_default")]
    pub description: String,
    #[serde(deserialize_with = "null_as_default")]
    pub url: String,
    #[serde(deserialize_with = "null_as_default")]
    pub branch_name: String,
    #[serde(deserialize_with = "null_as_default")]
    pub state: NamedField,
    #[serde(deserialize_with = "null_as_default")]
    pub assignee: Assignee,
    #[serde(deserialize_with = "null_as_default")]
    pub project: NamedField,
    #[serde(deserialize_with = "null_as_default")]
    pub attachments: Attachments,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NamedField {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Assignee {
    pub name: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Attachments {
    #[serde(deserialize_with = "null_as_default")]
    pub nodes: Vec<Attachment>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Attachment {
    pub source_type: Option<String>,
    #[serde(deserialize_with = "null_as_default")]
    pub metadata: AttachmentMetadata,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AttachmentMetadata {
    pub number: Option<u64>,
    pub branch: Option<String>,
    pub status: Option<String>,
}

impl LinearTicket {
    /// The pull request Linear itself associates with this ticket, when its GitHub integration
    /// recorded one. Lets a mismatch be reported instead of quietly showing the wrong ticket.
    pub fn linked_pull_request(&self) -> Option<u64> {
        self.attachments
            .nodes
            .iter()
            .find(|attachment| attachment.source_type.as_deref() == Some("github"))
            .and_then(|attachment| attachment.metadata.number)
    }

    pub fn subtitle(&self) -> String {
        let mut parts = Vec::new();
        if let Some(state) = self.state.name.as_deref() {
            parts.push(state.to_owned());
        }
        if let Some(assignee) = self
            .assignee
            .display_name
            .as_deref()
            .or(self.assignee.name.as_deref())
        {
            parts.push(format!("@{assignee}"));
        }
        if let Some(project) = self.project.name.as_deref() {
            parts.push(project.to_owned());
        }
        parts.join(" · ")
    }
}

pub struct LinearCli<E> {
    executor: E,
    command: String,
}

impl<E> LinearCli<E> {
    pub fn new(executor: E, command: impl Into<String>) -> Self {
        Self {
            executor,
            command: command.into(),
        }
    }

    pub fn into_executor(self) -> E {
        self.executor
    }
}

impl<E: CommandExecutor> LinearCli<E> {
    pub fn view(&mut self, id: &str) -> Result<LinearTicket, LinearError> {
        // `--no-pager` is mandatory: a pager would block forever on a captured stdout.
        let argv = [&self.command, "issue", "view", id, "--json", "--no-pager"]
            .into_iter()
            .map(OsString::from)
            .collect();
        let result = self
            .executor
            .execute(CommandRequest {
                argv,
                env: Vec::new(),
                stdin: None,
                inherit_stdio: false,
                limits: Some(CaptureLimits::new(STDOUT_LIMIT, STDERR_LIMIT, TIMEOUT)),
            })
            .map_err(|source| {
                if source.kind() == io::ErrorKind::NotFound {
                    LinearError::MissingCli {
                        command: self.command.clone(),
                    }
                } else {
                    LinearError::Io(source)
                }
            })?;
        if result.timed_out {
            return Err(LinearError::TimedOut);
        }
        if result.stdout_truncated {
            return Err(LinearError::Truncated);
        }
        let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
        if result.code != Some(0) {
            return Err(classify(id, &stdout, &stderr, result.code));
        }
        serde_json::from_str(strip_ansi(&stdout).trim())
            .map_err(|error| LinearError::InvalidJson(error.to_string()))
    }
}

fn classify(id: &str, stdout: &str, stderr: &str, code: Option<i32>) -> LinearError {
    // This CLI reports both conditions on stdout, so both streams are searched.
    let haystack = format!("{stdout}\n{stderr}").to_lowercase();
    if haystack.contains("could not find referenced issue") || haystack.contains("not found") {
        return LinearError::NotFound { id: id.to_owned() };
    }
    if haystack.contains("not authenticated") || haystack.contains("auth login") {
        return LinearError::Unauthenticated;
    }
    LinearError::Failed {
        code,
        stderr: if stderr.trim().is_empty() {
            stdout.to_owned()
        } else {
            stderr.to_owned()
        },
    }
}

/// The CLI colours its output even when piped, which would otherwise break JSON parsing.
fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            output.push(character);
            continue;
        }
        if characters.peek() == Some(&'[') {
            characters.next();
            for escaped in characters.by_ref() {
                if escaped.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trimmed from a real `linear issue view MON-2799 --json`.
    const SAMPLE: &str = r###"{
        "identifier": "MON-2799",
        "title": "fix(deal-hub): reject quote-only filters",
        "description": "## Problem\n\nThe HTTP quote-version list ...",
        "url": "https://linear.app/mondrio/issue/MON-2799/fix",
        "branchName": "feature/mon-2799-fixdeal-hub-reject-quot",
        "state": { "name": "Done", "color": "#5e6ad2" },
        "assignee": { "name": "carlos@mondrio.io", "displayName": "carlos" },
        "priority": 2,
        "project": { "name": "Deal Hub port" },
        "projectMilestone": null,
        "parent": null,
        "children": { "nodes": [] },
        "comments": { "nodes": [] },
        "attachments": { "nodes": [ {
            "title": "fix(deal-hub)",
            "sourceType": "github",
            "metadata": { "number": 2289, "branch": "feature/mon-2799-x", "status": "merged" }
        } ] }
    }"###;

    #[test]
    fn the_sampled_payload_deserializes_including_the_linked_pull_request() {
        let ticket: LinearTicket = serde_json::from_str(SAMPLE).unwrap();

        assert_eq!(ticket.identifier, "MON-2799");
        assert!(ticket.description.starts_with("## Problem"));
        assert_eq!(
            ticket.branch_name,
            "feature/mon-2799-fixdeal-hub-reject-quot"
        );
        assert_eq!(ticket.state.name.as_deref(), Some("Done"));
        assert_eq!(ticket.assignee.display_name.as_deref(), Some("carlos"));
        assert_eq!(ticket.linked_pull_request(), Some(2289));
        assert_eq!(ticket.subtitle(), "Done · @carlos · Deal Hub port");
    }

    #[test]
    fn explicit_nulls_degrade_the_card_instead_of_failing_the_fetch() {
        // Linear sends `null`, not an absent key, for every unset relation. Half the tickets on a
        // real board are unassigned, so this is the common case rather than an edge one.
        let ticket: LinearTicket = serde_json::from_str(
            r##"{
                "identifier": "MON-2822",
                "title": "t",
                "description": null,
                "url": null,
                "branchName": null,
                "state": null,
                "assignee": null,
                "project": null,
                "attachments": null
            }"##,
        )
        .unwrap();

        assert_eq!(ticket.identifier, "MON-2822");
        assert!(ticket.description.is_empty());
        assert_eq!(ticket.assignee.display_name, None);
        assert_eq!(ticket.project.name, None);
        assert_eq!(ticket.linked_pull_request(), None);
        assert_eq!(ticket.subtitle(), "");
    }

    #[test]
    fn unknown_and_missing_fields_never_fail_the_fetch() {
        let ticket: LinearTicket =
            serde_json::from_str(r##"{"identifier":"ABC-1","somethingNew":{"a":1}}"##).unwrap();
        assert_eq!(ticket.identifier, "ABC-1");
        assert!(ticket.description.is_empty());
        assert_eq!(ticket.linked_pull_request(), None);
        assert_eq!(ticket.subtitle(), "");
    }

    #[test]
    fn a_linear_url_in_the_body_wins_over_every_other_signal() {
        let id = infer_ticket(
            "Closes https://linear.app/mondrio/issue/MON-2799/fix-the-thing",
            "carraes/zex-1-other",
            "feat(ABC-2): title",
        );
        assert_eq!(id.as_deref(), Some("MON-2799"));
    }

    #[test]
    fn branch_names_are_matched_case_insensitively() {
        // Real branches spell the key lowercase.
        assert_eq!(
            infer_ticket("", "feature/mon-2799-fixdeal-hub", "").as_deref(),
            Some("MON-2799")
        );
        assert_eq!(
            infer_ticket("", "MON-2799", "").as_deref(),
            Some("MON-2799")
        );
    }

    #[test]
    fn the_title_is_used_when_the_branch_carries_nothing() {
        assert_eq!(
            infer_ticket("", "carraes/patch-1", "feat(ZEX-1234): add retry").as_deref(),
            Some("ZEX-1234")
        );
    }

    #[test]
    fn things_that_merely_look_like_keys_are_rejected() {
        for (body, head, title) in [
            ("", "carraes/patch-1", "bump utf-8 handling"),
            ("", "release/v2-final", "chore: release"),
            ("", "", "fixes 1234"),
            ("", "feature/abcdefghijkl-1", "too many letters"),
        ] {
            assert_eq!(infer_ticket(body, head, title), None, "{head:?} {title:?}");
        }
    }

    #[test]
    fn ansi_coloured_output_still_parses() {
        let coloured = format!("\u{1b}[36m{SAMPLE}\u{1b}[0m");
        assert_eq!(strip_ansi(&coloured).trim(), SAMPLE);
    }
}
