use std::error::Error;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::input::sanitize_terminal_text;

use super::model::{
    AgentContext, AgentFileContext, LineRange, NoteConfidence, NoteSource, ReviewNote,
};

pub const MAX_AGENT_CONTEXT_BYTES: usize = 1024 * 1024;
pub const MAX_AGENT_FILES: usize = 2_000;
pub const MAX_AGENT_ANNOTATIONS: usize = 10_000;
const MAX_NOTE_TEXT_BYTES: usize = 64 * 1024;
const MAX_NOTE_MARKUP_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentContextSource {
    None,
    File(PathBuf),
    Snapshot(AgentContext),
}

impl Default for AgentContextSource {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug)]
pub enum AgentContextError {
    ConflictingStdin,
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    TooLarge {
        source_label: String,
        limit: usize,
    },
    Invalid {
        source_label: String,
        message: String,
    },
}

impl fmt::Display for AgentContextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConflictingStdin => formatter.write_str(
                "--agent-context - cannot share stdin with patch or pager input; use an agent-context file",
            ),
            Self::Io { path, source } => {
                write!(formatter, "failed to read agent context {}: {source}", path.display())
            }
            Self::TooLarge {
                source_label,
                limit,
            } => write!(
                formatter,
                "agent context {source_label} exceeds the {limit}-byte limit"
            ),
            Self::Invalid {
                source_label,
                message,
            } => write!(formatter, "invalid agent context {source_label}: {message}"),
        }
    }
}

impl Error for AgentContextError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn parse_agent_context(
    source_label: impl Into<String>,
    bytes: &[u8],
) -> Result<AgentContext, AgentContextError> {
    let source_label = source_label.into();
    if bytes.len() > MAX_AGENT_CONTEXT_BYTES {
        return Err(AgentContextError::TooLarge {
            source_label,
            limit: MAX_AGENT_CONTEXT_BYTES,
        });
    }
    let root: Value = serde_json::from_slice(bytes).map_err(|error| {
        invalid(
            &source_label,
            format!(
                "JSON parse failed at line {} column {}: {error}",
                error.line(),
                error.column()
            ),
        )
    })?;
    let object = root
        .as_object()
        .ok_or_else(|| invalid(&source_label, "root must be a JSON object"))?;
    let version = object
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(1);
    let summary = optional_text(object, "summary");
    let raw_files = object
        .get("files")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if raw_files.len() > MAX_AGENT_FILES {
        return Err(invalid(
            &source_label,
            format!("file limit of {MAX_AGENT_FILES} exceeded"),
        ));
    }

    let mut annotation_count = 0usize;
    let mut files = Vec::with_capacity(raw_files.len());
    for (file_index, raw_file) in raw_files.iter().enumerate() {
        let file = raw_file.as_object().ok_or_else(|| {
            invalid(
                &source_label,
                format!("file entries require objects (index {file_index})"),
            )
        })?;
        let path = required_text(file, "path").ok_or_else(|| {
            invalid(
                &source_label,
                format!("file entries require a non-empty path (index {file_index})"),
            )
        })?;
        let raw_annotations = file
            .get("annotations")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        annotation_count = annotation_count.saturating_add(raw_annotations.len());
        if annotation_count > MAX_AGENT_ANNOTATIONS {
            return Err(invalid(
                &source_label,
                format!("annotation limit of {MAX_AGENT_ANNOTATIONS} exceeded"),
            ));
        }
        let mut annotations = Vec::with_capacity(raw_annotations.len());
        for (note_index, raw_note) in raw_annotations.iter().enumerate() {
            annotations.push(parse_note(&source_label, file_index, note_index, raw_note)?);
        }
        files.push(AgentFileContext {
            path,
            summary: optional_text(file, "summary"),
            annotations,
        });
    }

    Ok(AgentContext {
        version,
        summary,
        files,
    })
}

pub fn load_agent_context(path: &Path) -> Result<AgentContext, AgentContextError> {
    let bytes = read_bounded_file(path)?;
    parse_agent_context(path.display().to_string(), &bytes)
}

pub(crate) fn resolve_agent_context(
    configured: Option<&Path>,
    cwd: &Path,
    stdin: &mut dyn Read,
    review_uses_stdin: bool,
) -> Result<(Option<AgentContext>, AgentContextSource), AgentContextError> {
    let Some(configured) = configured else {
        return Ok((None, AgentContextSource::None));
    };
    if configured == Path::new("-") {
        if review_uses_stdin {
            return Err(AgentContextError::ConflictingStdin);
        }
        let bytes = read_bounded(stdin, "stdin")?;
        let context = parse_agent_context("stdin", &bytes)?;
        return Ok((Some(context.clone()), AgentContextSource::Snapshot(context)));
    }
    let path = if configured.is_absolute() {
        configured.to_path_buf()
    } else {
        cwd.join(configured)
    };
    let context = load_agent_context(&path)?;
    Ok((Some(context), AgentContextSource::File(path)))
}

pub(crate) fn reload_agent_context(
    source: &AgentContextSource,
) -> Result<Option<AgentContext>, AgentContextError> {
    match source {
        AgentContextSource::None => Ok(None),
        AgentContextSource::File(path) => load_agent_context(path).map(Some),
        AgentContextSource::Snapshot(context) => Ok(Some(context.clone())),
    }
}

fn parse_note(
    source_label: &str,
    file_index: usize,
    note_index: usize,
    raw_note: &Value,
) -> Result<ReviewNote, AgentContextError> {
    let note = raw_note.as_object().ok_or_else(|| {
        invalid(
            source_label,
            format!("annotations require objects (file {file_index}, annotation {note_index})"),
        )
    })?;
    let summary = required_text(note, "summary").ok_or_else(|| {
        invalid(
            source_label,
            format!(
                "annotations require a non-empty summary (file {file_index}, annotation {note_index})"
            ),
        )
    })?;
    let rationale = optional_text(note, "rationale");
    let text_bytes = summary
        .len()
        .saturating_add(rationale.as_ref().map_or(0, String::len));
    if text_bytes > MAX_NOTE_TEXT_BYTES {
        return Err(invalid(
            source_label,
            format!("annotation text exceeds the {MAX_NOTE_TEXT_BYTES}-byte limit"),
        ));
    }
    let markup = optional_text(note, "markup");
    if markup
        .as_ref()
        .is_some_and(|value| value.len() > MAX_NOTE_MARKUP_BYTES)
    {
        return Err(invalid(
            source_label,
            format!("annotation markup exceeds the {MAX_NOTE_MARKUP_BYTES}-byte limit"),
        ));
    }
    let tags = note
        .get("tags")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(sanitize)
        .collect();
    let confidence = match note.get("confidence").and_then(Value::as_str) {
        Some("low") => Some(NoteConfidence::Low),
        Some("medium") => Some(NoteConfidence::Medium),
        Some("high") => Some(NoteConfidence::High),
        _ => None,
    };
    Ok(ReviewNote {
        id: optional_text(note, "id"),
        old_range: parse_range(source_label, note.get("oldRange"))?,
        new_range: parse_range(source_label, note.get("newRange"))?,
        summary,
        rationale,
        markup,
        tags,
        confidence,
        source: NoteSource::from_raw(optional_text(note, "source")),
        title: optional_text(note, "title"),
        author: optional_text(note, "author"),
        created_at: optional_text(note, "createdAt"),
        updated_at: optional_text(note, "updatedAt"),
        editable: note
            .get("editable")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_range(
    source_label: &str,
    value: Option<&Value>,
) -> Result<Option<LineRange>, AgentContextError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(values) = value.as_array() else {
        return Ok(None);
    };
    if values.len() != 2 {
        return Ok(None);
    }
    let (Some(start), Some(end)) = (values[0].as_u64(), values[1].as_u64()) else {
        return Err(invalid(
            source_label,
            "annotation ranges must be integer pairs",
        ));
    };
    let (Ok(start), Ok(end)) = (u32::try_from(start), u32::try_from(end)) else {
        return Err(invalid(
            source_label,
            "annotation ranges must fit 32-bit line numbers",
        ));
    };
    if start == 0 || end == 0 {
        return Err(invalid(
            source_label,
            "annotation ranges use positive 1-based lines",
        ));
    }
    if end < start {
        return Err(invalid(source_label, "annotation ranges must be ordered"));
    }
    Ok(Some(LineRange { start, end }))
}

fn required_text(object: &Map<String, Value>, key: &str) -> Option<String> {
    optional_text(object, key).filter(|value| !value.is_empty())
}

fn optional_text(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(sanitize)
}

fn sanitize(value: &str) -> String {
    sanitize_terminal_text(value, false)
}

fn invalid(source_label: &str, message: impl Into<String>) -> AgentContextError {
    AgentContextError::Invalid {
        source_label: source_label.to_owned(),
        message: message.into(),
    }
}

fn read_bounded_file(path: &Path) -> Result<Vec<u8>, AgentContextError> {
    let mut file = fs::File::open(path).map_err(|source| AgentContextError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    read_bounded(&mut file, &path.display().to_string()).map_err(|error| match error {
        AgentContextError::Io { source, .. } => AgentContextError::Io {
            path: path.to_path_buf(),
            source,
        },
        other => other,
    })
}

fn read_bounded(reader: &mut dyn Read, source_label: &str) -> Result<Vec<u8>, AgentContextError> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_AGENT_CONTEXT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| AgentContextError::Io {
            path: PathBuf::from(source_label),
            source,
        })?;
    if bytes.len() > MAX_AGENT_CONTEXT_BYTES {
        return Err(AgentContextError::TooLarge {
            source_label: source_label.to_owned(),
            limit: MAX_AGENT_CONTEXT_BYTES,
        });
    }
    Ok(bytes)
}
