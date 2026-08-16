use ramo_core::diff::model::{DiffFile, Hunk};

use crate::input::sanitize_terminal_text;
use crate::notes::{NoteAnchorSide, NoteTarget};
use crate::review::state::target_diff_context;

pub const MAX_QUESTION_CHARS: usize = 1000;
const MAX_HUNK_LINES: usize = 300;
const MAX_HUNK_BYTES: usize = 16 * 1024;
const MAX_PROMPT_BYTES: usize = 24 * 1024;
const TRUNCATED_HUNK: &str = "… (hunk truncated)";

pub const SYSTEM_PROMPT: &str = "You answer a reviewer's question about one diff hunk shown in a \
terminal review tool. You can only see what is in this message. Do not ask to read other files and \
do not speculate about code you cannot see; say plainly what you cannot determine. Answer in plain \
text. At most 12 short lines. No markdown headings. No code fences unless quoting six lines or \
fewer.";

/// Builds the single user message sent to the provider. The payload is exactly the
/// question plus one anchored hunk — never the repository, other files, or environment.
pub fn compose_prompt(file: &DiffFile, target: &NoteTarget, question: &str) -> String {
    let question = clamp_chars(&sanitize_terminal_text(question, false), MAX_QUESTION_CHARS);
    let mut prompt = String::new();
    prompt.push_str("QUESTION\n");
    prompt.push_str(question.trim());
    prompt.push_str("\n\nFILE\n");
    prompt.push_str(&sanitize_terminal_text(&file.path, false));
    if let Some(previous) = &file.previous_path {
        prompt.push_str(&format!(
            " (renamed from {})",
            sanitize_terminal_text(previous, false)
        ));
    }
    prompt.push_str("\n\nLOCATION\n");
    prompt.push_str(&location(file, target));

    let selected = target_diff_context(file, target);
    let hunk = target
        .hunk_index
        .and_then(|index| file.hunks.get(index))
        .map(|hunk| render_hunk(hunk, target));

    // Trim the hunk first, then the selection, so the question always survives.
    let mut sections = Vec::new();
    if !selected.trim().is_empty() {
        sections.push(("SELECTED", selected));
    }
    if let Some(hunk) = hunk.filter(|hunk| !hunk.trim().is_empty()) {
        sections.push(("HUNK", hunk));
    }
    for (title, body) in sections {
        let candidate = format!("\n\n{title}\n{body}");
        if prompt.len().saturating_add(candidate.len()) > MAX_PROMPT_BYTES {
            let room = MAX_PROMPT_BYTES.saturating_sub(prompt.len() + title.len() + 4);
            if room > TRUNCATED_HUNK.len() {
                prompt.push_str(&format!(
                    "\n\n{title}\n{}\n{TRUNCATED_HUNK}",
                    clamp_bytes(&body, room - TRUNCATED_HUNK.len())
                ));
            }
            break;
        }
        prompt.push_str(&candidate);
    }
    prompt.push('\n');
    prompt
}

fn location(file: &DiffFile, target: &NoteTarget) -> String {
    let mut parts = Vec::new();
    if let Some(range) = target.new_range {
        parts.push(format!("new lines {}-{}", range.start, range.end));
    }
    if let Some(range) = target.old_range {
        parts.push(format!("old lines {}-{}", range.start, range.end));
    }
    if parts.is_empty()
        && let Some(line) = target.anchor_line
    {
        parts.push(match target.anchor_side {
            Some(NoteAnchorSide::Old) => format!("old line {line}"),
            _ => format!("new line {line}"),
        });
    }
    let mut location = parts.join(" / ");
    if let Some(index) = target.hunk_index {
        location.push_str(&format!(
            " (hunk {} of {})",
            index.saturating_add(1),
            file.hunks.len()
        ));
    }
    location
}

fn render_hunk(hunk: &Hunk, target: &NoteTarget) -> String {
    let mut rendered = Vec::with_capacity(hunk.lines.len().min(MAX_HUNK_LINES) + 1);
    rendered.push(sanitize_terminal_text(&hunk.header, false));
    let window = hunk_window(hunk, target);
    let truncated = hunk.lines.len() > window.len();
    for line in window.iter() {
        rendered.push(format!(
            "{}{}",
            line.kind.prefix(),
            sanitize_terminal_text(&line.content, false)
        ));
    }
    if truncated {
        rendered.push(TRUNCATED_HUNK.to_owned());
    }
    clamp_bytes(&rendered.join("\n"), MAX_HUNK_BYTES)
}

/// Centres an oversized hunk on the anchor so the question's own lines are never cut.
fn hunk_window<'a>(
    hunk: &'a Hunk,
    target: &NoteTarget,
) -> Vec<&'a ramo_core::diff::model::DiffLine> {
    if hunk.lines.len() <= MAX_HUNK_LINES {
        return hunk.lines.iter().collect();
    }
    let anchor = target
        .anchor_line
        .and_then(|anchor| {
            hunk.lines.iter().position(|line| {
                let number = match target.anchor_side {
                    Some(NoteAnchorSide::Old) => line.old_lineno,
                    _ => line.new_lineno,
                };
                number == Some(anchor)
            })
        })
        .unwrap_or(0);
    let half = MAX_HUNK_LINES / 2;
    let start = anchor
        .saturating_sub(half)
        .min(hunk.lines.len().saturating_sub(MAX_HUNK_LINES));
    hunk.lines.iter().skip(start).take(MAX_HUNK_LINES).collect()
}

fn clamp_chars(text: &str, maximum: usize) -> String {
    text.chars().take(maximum).collect()
}

fn clamp_bytes(text: &str, maximum: usize) -> String {
    if text.len() <= maximum {
        return text.to_owned();
    }
    let mut end = maximum;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ramo_core::agent::LineRange;
    use ramo_core::diff::model::{DiffLine, FileChangeKind, FileStats, LineType, SourceSpec};
    use std::path::PathBuf;

    fn line(kind: LineType, content: &str, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine {
            kind,
            content: content.into(),
            old_lineno: old,
            new_lineno: new,
            moved: None,
        }
    }

    fn file(previous_path: Option<&str>, lines: Vec<DiffLine>) -> DiffFile {
        DiffFile {
            id: "file:src/lib.rs".into(),
            path: "src/lib.rs".into(),
            previous_path: previous_path.map(str::to_owned),
            summary: None,
            agent: None,
            patch: String::new(),
            hunks: vec![Hunk {
                old_start: 1,
                new_start: 1,
                header: "@@ -1,3 +1,3 @@ fn demo()".into(),
                lines,
            }],
            change_kind: FileChangeKind::Modified,
            is_binary: false,
            is_untracked: false,
            is_too_large: false,
            stats_truncated: false,
            language: Some("rs".into()),
            stats: FileStats {
                additions: 1,
                deletions: 1,
            },
            old_source: SourceSpec::File(PathBuf::from("old")),
            new_source: SourceSpec::File(PathBuf::from("new")),
        }
    }

    fn target() -> NoteTarget {
        NoteTarget {
            file_id: "file:src/lib.rs".into(),
            old_range: None,
            new_range: Some(LineRange { start: 2, end: 2 }),
            hunk_index: Some(0),
            anchor_side: Some(NoteAnchorSide::New),
            anchor_line: Some(2),
        }
    }

    #[test]
    fn the_prompt_carries_only_the_question_file_and_hunk() {
        let sample = file(
            None,
            vec![
                line(LineType::Context, "fn demo() {", Some(1), Some(1)),
                line(LineType::Addition, "    let x = 2;", None, Some(2)),
                line(LineType::Context, "}", Some(3), Some(3)),
            ],
        );

        let prompt = compose_prompt(&sample, &target(), "Why change x?");

        assert!(prompt.starts_with("QUESTION\nWhy change x?"), "{prompt}");
        assert!(prompt.contains("\nFILE\nsrc/lib.rs"), "{prompt}");
        assert!(prompt.contains("new lines 2-2 (hunk 1 of 1)"), "{prompt}");
        assert!(
            prompt.contains("\nHUNK\n@@ -1,3 +1,3 @@ fn demo()"),
            "{prompt}"
        );
        assert!(prompt.contains("+    let x = 2;"), "{prompt}");
    }

    #[test]
    fn a_rename_is_named_in_the_file_section() {
        let sample = file(
            Some("src/old.rs"),
            vec![line(LineType::Addition, "new", None, Some(2))],
        );

        let prompt = compose_prompt(&sample, &target(), "What moved?");

        assert!(
            prompt.contains("src/lib.rs (renamed from src/old.rs)"),
            "{prompt}"
        );
    }

    #[test]
    fn oversized_hunks_are_windowed_on_the_anchor_and_marked() {
        let lines = (1..=800)
            .map(|number| {
                line(
                    LineType::Context,
                    &format!("line {number}"),
                    Some(number),
                    Some(number),
                )
            })
            .collect::<Vec<_>>();
        let sample = file(None, lines);
        let mut anchored = target();
        anchored.anchor_line = Some(500);
        anchored.new_range = Some(LineRange {
            start: 500,
            end: 500,
        });

        let prompt = compose_prompt(&sample, &anchored, "What is here?");

        assert!(
            prompt.len() <= MAX_PROMPT_BYTES + TRUNCATED_HUNK.len() + 16,
            "{}",
            prompt.len()
        );
        assert!(prompt.contains("line 500"), "anchor must survive windowing");
        assert!(!prompt.contains("line 1\n"), "far context must be dropped");
        assert!(prompt.contains(TRUNCATED_HUNK), "{prompt}");
    }

    #[test]
    fn questions_are_clamped_and_sanitized() {
        let sample = file(None, vec![line(LineType::Addition, "x", None, Some(2))]);
        let long = "q".repeat(MAX_QUESTION_CHARS + 500);

        let prompt = compose_prompt(&sample, &target(), &long);
        let question = prompt.lines().nth(1).expect("question line");
        assert_eq!(question.chars().count(), MAX_QUESTION_CHARS);

        let control = compose_prompt(&sample, &target(), "why\u{7}now");
        assert!(
            !control.contains('\u{7}'),
            "control characters must be stripped"
        );
    }
}
