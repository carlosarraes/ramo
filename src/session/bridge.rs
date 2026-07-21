use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::core::input::{CommonOptions, LayoutMode, PatchSource, ReviewInput};
use crate::input::LoadedReview;
use crate::notes::{LiveNoteInput, NoteAnchorSide};
use crate::review::{ReviewController, Viewport};
use crate::watch::WatchRuntime;

use super::{build_snapshot, session_timestamp};

const MAX_SESSION_NOTE_TEXT_BYTES: usize = 64 * 1024;
const MAX_SESSION_NOTE_MARKUP_BYTES: usize = 64 * 1024;

pub fn apply_session_request(
    controller: &mut ReviewController,
    request_id: &str,
    input: &Value,
    viewport: Viewport,
) -> Result<Value, String> {
    let action = input
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| "session command is missing its action".to_owned())?;
    match action {
        "navigate" => navigate(controller, input, viewport),
        "comment-add" => add_comment(controller, request_id, input, viewport),
        "comment-apply" => apply_comments(controller, request_id, input, viewport),
        "comment-list" => list_comments(controller, input, viewport),
        "comment-rm" => remove_comment(controller, input, viewport),
        "comment-clear" => clear_comments(controller, input, viewport),
        "reload" => {
            Err("live session reload is not available until its input transaction is ready".into())
        }
        _ => Err(format!("unsupported live session action {action:?}")),
    }
}

#[derive(Debug, Clone)]
pub struct AppliedSessionReload {
    pub input: ReviewInput,
    pub loaded: LoadedReview,
    pub cwd: PathBuf,
}

pub fn apply_session_reload(
    controller: &mut ReviewController,
    runtime: &mut WatchRuntime,
    input: &Value,
    viewport: Viewport,
) -> Result<AppliedSessionReload, String> {
    let next_input = input
        .get("nextInput")
        .ok_or_else(|| "session reload requires nextInput".to_owned())?;
    let next_input = parse_review_input(next_input)?;
    let source_path = input
        .get("sourcePath")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let replaced = runtime.replace_input(next_input, source_path.as_deref(), Instant::now())?;
    controller.replace_files(replaced.loaded.changeset.files.clone(), viewport);
    Ok(AppliedSessionReload {
        input: replaced.input,
        loaded: replaced.loaded,
        cwd: replaced.cwd,
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct WireOptions {
    mode: Option<LayoutMode>,
    theme: Option<String>,
    agent_context: Option<PathBuf>,
    pager: Option<bool>,
    watch: Option<bool>,
    exclude_untracked: Option<bool>,
    line_numbers: Option<bool>,
    wrap_lines: Option<bool>,
    hunk_headers: Option<bool>,
    agent_notes: Option<bool>,
    transparent_background: Option<bool>,
}

impl From<WireOptions> for CommonOptions {
    fn from(options: WireOptions) -> Self {
        Self {
            mode: options.mode,
            theme: options.theme,
            agent_context: options.agent_context,
            pager: options.pager,
            watch: options.watch,
            exclude_untracked: options.exclude_untracked,
            line_numbers: options.line_numbers,
            wrap_lines: options.wrap_lines,
            hunk_headers: options.hunk_headers,
            agent_notes: options.agent_notes,
            transparent_background: options.transparent_background,
        }
    }
}

fn parse_review_input(value: &Value) -> Result<ReviewInput, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "session reload nextInput must be an object".to_owned())?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "session reload nextInput requires a kind".to_owned())?;
    let options = object.get("options").cloned().unwrap_or_else(|| json!({}));
    let options: CommonOptions = serde_json::from_value::<WireOptions>(options)
        .map_err(|error| format!("invalid session reload options: {error}"))?
        .into();
    let strings = |field: &str| -> Result<Vec<String>, String> {
        object
            .get(field)
            .map(|value| {
                serde_json::from_value(value.clone())
                    .map_err(|error| format!("{field} must be an array of strings: {error}"))
            })
            .unwrap_or_else(|| Ok(Vec::new()))
    };
    let optional_string = |fields: &[&str]| -> Result<Option<String>, String> {
        fields
            .iter()
            .find_map(|field| object.get(*field))
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{} must be a string", fields[0]))
            })
            .transpose()
    };
    Ok(match kind {
        "diff" if object.contains_key("left") || object.contains_key("right") => {
            ReviewInput::FilePair {
                left: required_path(object, "left")?,
                right: required_path(object, "right")?,
                display_path: optional_string(&["displayPath"])?.map(PathBuf::from),
                options,
            }
        }
        "diff" | "vcs" => ReviewInput::VcsDiff {
            range: optional_string(&["range"])?,
            staged: object
                .get("staged")
                .map(|value| {
                    value
                        .as_bool()
                        .ok_or_else(|| "staged must be a boolean".to_owned())
                })
                .transpose()?
                .unwrap_or(false),
            pathspecs: strings("pathspecs")?,
            options,
        },
        "show" => ReviewInput::Show {
            reference: optional_string(&["reference", "ref"])?,
            pathspecs: strings("pathspecs")?,
            options,
        },
        "stash" | "stash-show" => ReviewInput::StashShow {
            reference: optional_string(&["reference", "ref"])?,
            options,
        },
        "files" | "difftool" => ReviewInput::FilePair {
            left: required_path(object, "left")?,
            right: required_path(object, "right")?,
            display_path: optional_string(&["displayPath"])?.map(PathBuf::from),
            options,
        },
        "patch" => ReviewInput::Patch {
            source: PatchSource::File(
                optional_string(&["path", "file"])?
                    .filter(|path| path != "-")
                    .map(PathBuf::from)
                    .ok_or_else(|| {
                        "session reload does not support stdin-backed patch input".to_owned()
                    })?,
            ),
            options,
        },
        "pager" => {
            return Err("session reload does not support pager input".into());
        }
        _ => return Err(format!("unsupported session reload input kind {kind:?}")),
    })
}

fn required_path(object: &serde_json::Map<String, Value>, field: &str) -> Result<PathBuf, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("session reload {field} must be a non-empty path"))
}

fn navigate(
    controller: &mut ReviewController,
    input: &Value,
    viewport: Viewport,
) -> Result<Value, String> {
    let hunk_index = input
        .get("hunkNumber")
        .and_then(Value::as_u64)
        .map(|number| {
            usize::try_from(number)
                .ok()
                .filter(|number| *number > 0)
                .map(|number| number - 1)
                .ok_or_else(|| "hunk numbers are positive and 1-based".to_owned())
        })
        .transpose()?;
    let side = input
        .get("side")
        .and_then(Value::as_str)
        .map(parse_side)
        .transpose()?;
    let line = input
        .get("line")
        .and_then(Value::as_u64)
        .map(positive_u32)
        .transpose()?;
    let comment_delta = match input.get("commentDirection").and_then(Value::as_str) {
        Some("next") => Some(1),
        Some("prev") => Some(-1),
        Some(_) => return Err("comment direction must be next or prev".into()),
        None => None,
    };
    let (file_id, file_path, hunk_index) = controller.navigate_session_target(
        input.get("filePath").and_then(Value::as_str),
        hunk_index,
        side,
        line,
        comment_delta,
        viewport,
    )?;
    Ok(json!({
        "fileId":file_id,"filePath":file_path,"hunkIndex":hunk_index,
        "selectedHunk":{"index":hunk_index}
    }))
}

fn add_comment(
    controller: &mut ReviewController,
    request_id: &str,
    input: &Value,
    viewport: Viewport,
) -> Result<Value, String> {
    let prepared = parse_comment(input, format!("mcp:{request_id}"))?;
    validate_comment(controller, &prepared)?;
    let reveal = input
        .get("reveal")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let added = controller.add_live_note(prepared, viewport)?;
    if reveal {
        controller.toggle_agent_notes(true, viewport);
        controller.navigate_session_target(
            Some(&added.target.file_id),
            added.target.hunk_index,
            None,
            None,
            None,
            viewport,
        )?;
    }
    Ok(applied_comment(&added, controller))
}

fn apply_comments(
    controller: &mut ReviewController,
    request_id: &str,
    input: &Value,
    viewport: Viewport,
) -> Result<Value, String> {
    let comments = input
        .get("comments")
        .and_then(Value::as_array)
        .ok_or_else(|| "comment batch requires a comments array".to_owned())?;
    if comments.len() > super::MAX_SESSION_COMMENT_BATCH {
        return Err(format!(
            "comment batch exceeds {} comments",
            super::MAX_SESSION_COMMENT_BATCH
        ));
    }
    let prepared = comments
        .iter()
        .enumerate()
        .map(|(index, comment)| parse_comment(comment, format!("mcp:{request_id}:{index}")))
        .collect::<Result<Vec<_>, _>>()?;
    let mut ids = BTreeSet::new();
    for comment in &prepared {
        if !ids.insert(comment.id.clone()) {
            return Err(format!("duplicate live note id {}", comment.id));
        }
        validate_comment(controller, comment)?;
    }
    let mut applied = Vec::with_capacity(prepared.len());
    for comment in prepared {
        match controller.add_live_note(comment, viewport) {
            Ok(note) => applied.push(note),
            Err(error) => {
                for note in &applied {
                    if let Some(id) = note.note.id.as_deref() {
                        let _ = controller.remove_session_note(id, viewport);
                    }
                }
                return Err(error);
            }
        }
    }
    if input.get("revealMode").and_then(Value::as_str) == Some("first")
        && let Some(first) = applied.first()
    {
        let file_id = first.target.file_id.clone();
        let hunk_index = first.target.hunk_index;
        controller.toggle_agent_notes(true, viewport);
        controller.navigate_session_target(
            Some(&file_id),
            hunk_index,
            None,
            None,
            None,
            viewport,
        )?;
    }
    Ok(json!({
        "applied":applied.iter().map(|note| applied_comment(note, controller)).collect::<Vec<_>>()
    }))
}

fn list_comments(
    controller: &mut ReviewController,
    input: &Value,
    viewport: Viewport,
) -> Result<Value, String> {
    let file = input.get("filePath").and_then(Value::as_str);
    if let Some(file) = file {
        controller.validate_session_file(file)?;
    }
    let kind = input.get("type").and_then(Value::as_str).unwrap_or("live");
    let snapshot = build_snapshot(controller, viewport, session_timestamp());
    let matches_file = |value: &Value| {
        file.is_none_or(|file| value.get("filePath").and_then(Value::as_str) == Some(file))
    };
    let values = match kind {
        "live" | "agent" => serde_json::to_value(snapshot.state.live_comments).unwrap_or_default(),
        "all" => serde_json::to_value(snapshot.state.review_notes).unwrap_or_default(),
        "ai" | "user" => Value::Array(
            snapshot
                .state
                .review_notes
                .into_iter()
                .filter(|note| note.source == kind)
                .map(|note| serde_json::to_value(note).unwrap_or_default())
                .collect(),
        ),
        _ => return Err("comment type must be live, all, ai, agent, or user".into()),
    };
    Ok(Value::Array(
        values
            .as_array()
            .into_iter()
            .flatten()
            .filter(|value| matches_file(value))
            .cloned()
            .collect(),
    ))
}

fn remove_comment(
    controller: &mut ReviewController,
    input: &Value,
    viewport: Viewport,
) -> Result<Value, String> {
    let id = required_string(input, "commentId")?;
    let source = controller.remove_session_note(id, viewport)?;
    Ok(json!({
        "commentId":id,"removed":true,"source":source,
        "remainingCommentCount":controller.live_notes().len()+controller.human_notes().len()
    }))
}

fn clear_comments(
    controller: &mut ReviewController,
    input: &Value,
    viewport: Viewport,
) -> Result<Value, String> {
    let file = input.get("filePath").and_then(Value::as_str);
    if let Some(file) = file {
        controller.validate_session_file(file)?;
    }
    let include_user = input
        .get("includeUser")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let cleared = controller.clear_session_notes(file, include_user, viewport);
    Ok(json!({
        "removedCount":cleared.removed_live+cleared.removed_user,
        "remainingCommentCount":controller.live_notes().len()+controller.human_notes().len(),
        "filePath":file,"includeUser":include_user,
        "removedLiveCommentCount":cleared.removed_live,"removedUserNoteCount":cleared.removed_user,
        "remainingLiveCommentCount":controller.live_notes().len(),
        "remainingUserNoteCount":controller.human_notes().len()
    }))
}

fn parse_comment(value: &Value, id: String) -> Result<LiveNoteInput, String> {
    let hunk_index = value
        .get("hunkNumber")
        .and_then(Value::as_u64)
        .map(|number| {
            usize::try_from(number)
                .ok()
                .filter(|number| *number > 0)
                .map(|number| number - 1)
                .ok_or_else(|| "hunk numbers are positive and 1-based".to_owned())
        })
        .transpose()?;
    let summary = required_string(value, "summary")?.to_owned();
    let rationale = optional_string(value, "rationale");
    let markup = optional_string(value, "markup");
    if summary.len() + rationale.as_ref().map_or(0, String::len) > MAX_SESSION_NOTE_TEXT_BYTES {
        return Err(format!(
            "live note summary and rationale exceed {MAX_SESSION_NOTE_TEXT_BYTES} bytes"
        ));
    }
    if markup
        .as_ref()
        .is_some_and(|markup| markup.len() > MAX_SESSION_NOTE_MARKUP_BYTES)
    {
        return Err(format!(
            "live note markup exceeds {MAX_SESSION_NOTE_MARKUP_BYTES} bytes"
        ));
    }
    Ok(LiveNoteInput {
        id,
        file_path: required_string(value, "filePath")?.to_owned(),
        hunk_index,
        side: parse_side(required_string(value, "side")?)?,
        line: positive_u32(
            value
                .get("line")
                .and_then(Value::as_u64)
                .ok_or_else(|| "comment line must be a positive integer".to_owned())?,
        )?,
        summary,
        rationale,
        markup,
        author: optional_string(value, "author"),
        created_at: session_timestamp(),
    })
}

fn validate_comment(controller: &ReviewController, input: &LiveNoteInput) -> Result<(), String> {
    if input.summary.trim().is_empty() {
        return Err("live note summary cannot be empty".into());
    }
    if controller
        .live_notes()
        .iter()
        .any(|note| note.note.id.as_deref() == Some(&input.id))
    {
        return Err(format!("live note id {} already exists", input.id));
    }
    let file = controller
        .files()
        .iter()
        .find(|file| {
            file.id == input.file_path
                || file.path == input.file_path
                || file.previous_path.as_deref() == Some(input.file_path.as_str())
        })
        .ok_or_else(|| format!("No diff file matches {}.", input.file_path))?;
    let found = file.hunks.iter().enumerate().any(|(index, hunk)| {
        input.hunk_index.is_none_or(|requested| requested == index)
            && hunk.lines.iter().any(|line| match input.side {
                NoteAnchorSide::Old => line.old_lineno == Some(input.line),
                NoteAnchorSide::New => line.new_lineno == Some(input.line),
            })
    });
    found.then_some(()).ok_or_else(|| {
        format!(
            "line {} is not part of {} on the requested side",
            input.line, file.path
        )
    })
}

fn applied_comment(note: &crate::notes::LiveNote, controller: &ReviewController) -> Value {
    let file_path = controller
        .files()
        .iter()
        .find(|file| file.id == note.target.file_id)
        .map(|file| file.path.as_str());
    json!({
        "commentId":note.note.id,"fileId":note.target.file_id,
        "filePath":file_path,
        "hunkIndex":note.target.hunk_index,"side":match note.target.anchor_side { Some(NoteAnchorSide::Old)=>"old", _=>"new" },
        "line":note.target.anchor_line,"markupWidth":note.markup_width,"markupNotes":note.markup_notes
    })
}

fn required_string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{field} must be a non-empty string"))
}

fn optional_string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(Value::as_str).map(str::to_owned)
}

fn parse_side(value: &str) -> Result<NoteAnchorSide, String> {
    match value {
        "old" => Ok(NoteAnchorSide::Old),
        "new" => Ok(NoteAnchorSide::New),
        _ => Err("diff side must be old or new".into()),
    }
}

fn positive_u32(value: u64) -> Result<u32, String> {
    u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| "line must be a positive 32-bit integer".into())
}
