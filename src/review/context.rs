use std::collections::{HashMap, HashSet};
use std::fmt;
use std::ops::RangeInclusive;

use crate::diff::model::{DiffFile, FileChangeKind, LineType, SourceSpec};
use crate::input::sanitize_terminal_text;
use crate::vcs::source::{SourceError, SourceReader};
use crate::vcs::{CommandRunner, SystemCommandRunner};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GapPosition {
    Before,
    Trailing,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GapKey {
    pub file_id: String,
    pub position: GapPosition,
    pub hunk_index: usize,
}

impl GapKey {
    pub fn new(file_id: impl Into<String>, position: GapPosition, hunk_index: usize) -> Self {
        Self {
            file_id: file_id.into(),
            position,
            hunk_index,
        }
    }
}

impl fmt::Display for GapKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let position = match self.position {
            GapPosition::Before => "before",
            GapPosition::Trailing => "trailing",
        };
        write!(formatter, "{position}:{}", self.hunk_index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceSide {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollapsedGap {
    pub key: GapKey,
    pub old_range: RangeInclusive<u32>,
    pub new_range: RangeInclusive<u32>,
}

impl CollapsedGap {
    pub fn line_count(&self, side: SourceSide) -> usize {
        let range = match side {
            SourceSide::Old => &self.old_range,
            SourceSide::New => &self.new_range,
        };
        range.end().saturating_sub(*range.start()).saturating_add(1) as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLine {
    pub old_line: u32,
    pub new_line: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceFailure {
    Unavailable,
    Missing,
    TooLarge { limit: usize },
    NonUtf8,
    Io(String),
    Command(String),
    ShortSource,
}

impl fmt::Display for SourceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("context source is unavailable"),
            Self::Missing => formatter.write_str("context source is missing"),
            Self::TooLarge { limit } => write!(formatter, "context source exceeded {limit} bytes"),
            Self::NonUtf8 => formatter.write_str("context source is not valid UTF-8"),
            Self::Io(message) | Self::Command(message) => formatter.write_str(message),
            Self::ShortSource => {
                formatter.write_str("context source is shorter than the collapsed range")
            }
        }
    }
}

pub fn derive_collapsed_gaps(
    file: &DiffFile,
    source_length: Option<(SourceSide, u32)>,
) -> Vec<CollapsedGap> {
    let mut gaps = Vec::new();
    let mut old_cursor = 1u32;
    let mut new_cursor = 1u32;
    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        let old_gap = hunk.old_start.saturating_sub(old_cursor);
        let new_gap = hunk.new_start.saturating_sub(new_cursor);
        let count = old_gap.min(new_gap);
        if count > 0 {
            gaps.push(CollapsedGap {
                key: GapKey::new(&file.id, GapPosition::Before, hunk_index),
                old_range: old_cursor..=old_cursor.saturating_add(count - 1),
                new_range: new_cursor..=new_cursor.saturating_add(count - 1),
            });
        }
        let old_count = hunk
            .lines
            .iter()
            .filter(|line| line.kind != LineType::Addition)
            .count() as u32;
        let new_count = hunk
            .lines
            .iter()
            .filter(|line| line.kind != LineType::Deletion)
            .count() as u32;
        old_cursor = old_cursor.max(hunk.old_start.saturating_add(old_count));
        new_cursor = new_cursor.max(hunk.new_start.saturating_add(new_count));
    }
    if let (Some((side, length)), Some(last_hunk)) =
        (source_length, file.hunks.len().checked_sub(1))
    {
        let side_cursor = match side {
            SourceSide::Old => old_cursor,
            SourceSide::New => new_cursor,
        };
        if side_cursor <= length {
            let count = length.saturating_sub(side_cursor).saturating_add(1);
            gaps.push(CollapsedGap {
                key: GapKey::new(&file.id, GapPosition::Trailing, last_hunk),
                old_range: old_cursor..=old_cursor.saturating_add(count - 1),
                new_range: new_cursor..=new_cursor.saturating_add(count - 1),
            });
        }
    }
    gaps
}

pub fn select_gap_for_toggle(gaps: &[CollapsedGap], selected_hunk: usize) -> Option<&GapKey> {
    gaps.iter()
        .find(|gap| gap.key.position == GapPosition::Before && gap.key.hunk_index >= selected_hunk)
        .or_else(|| {
            gaps.iter()
                .find(|gap| gap.key.position == GapPosition::Trailing)
        })
        .map(|gap| &gap.key)
}

pub fn source_for_context(file: &DiffFile) -> (SourceSide, &SourceSpec) {
    if file.change_kind == FileChangeKind::Deleted {
        (SourceSide::Old, &file.old_source)
    } else {
        (SourceSide::New, &file.new_source)
    }
}

pub fn expand_gap_lines(
    gap: &CollapsedGap,
    source: &str,
    side: SourceSide,
) -> Result<Vec<ContextLine>, SourceFailure> {
    let normalized = source.replace("\r\n", "\n");
    let lines = normalized.lines().collect::<Vec<_>>();
    let selected = match side {
        SourceSide::Old => &gap.old_range,
        SourceSide::New => &gap.new_range,
    };
    let start = selected.start().saturating_sub(1) as usize;
    let count = gap.line_count(side);
    if start.saturating_add(count) > lines.len() {
        return Err(SourceFailure::ShortSource);
    }
    Ok((0..count)
        .map(|offset| ContextLine {
            old_line: gap.old_range.start().saturating_add(offset as u32),
            new_line: gap.new_range.start().saturating_add(offset as u32),
            text: sanitize_terminal_text(lines[start + offset], false).replace('\t', "  "),
        })
        .collect())
}

pub trait ContextSourceLoader: Send {
    fn load(&mut self, spec: &SourceSpec) -> Result<Option<String>, SourceFailure>;

    fn invalidate(&mut self) {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedContextSource {
    pub side: SourceSide,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FileContextState {
    pub expanded: HashSet<GapKey>,
    pub source: Option<Result<LoadedContextSource, SourceFailure>>,
}

impl FileContextState {
    pub(crate) fn gaps(&self, file: &DiffFile) -> Vec<CollapsedGap> {
        let length = self.source.as_ref().and_then(|source| {
            source.as_ref().ok().map(|loaded| {
                (
                    loaded.side,
                    normalized_source_line_count(&loaded.text) as u32,
                )
            })
        });
        derive_collapsed_gaps(file, length)
    }
}

pub(crate) fn normalized_source_line_count(source: &str) -> usize {
    let normalized = source.replace("\r\n", "\n");
    normalized.lines().count()
}

pub struct NativeContextSourceLoader<R = SystemCommandRunner> {
    runner: R,
    git_executable: String,
    max_bytes: usize,
    cache: HashMap<SourceSpec, Result<Option<String>, SourceFailure>>,
}

impl<R> NativeContextSourceLoader<R> {
    pub fn new(runner: R, git_executable: impl Into<String>, max_bytes: usize) -> Self {
        Self {
            runner,
            git_executable: git_executable.into(),
            max_bytes,
            cache: HashMap::new(),
        }
    }
}

impl Default for NativeContextSourceLoader<SystemCommandRunner> {
    fn default() -> Self {
        Self::new(SystemCommandRunner, "git", 8 * 1024 * 1024)
    }
}

impl<R: CommandRunner> ContextSourceLoader for NativeContextSourceLoader<R> {
    fn load(&mut self, spec: &SourceSpec) -> Result<Option<String>, SourceFailure> {
        if *spec == SourceSpec::None {
            return Ok(None);
        }
        if let Some(cached) = self.cache.get(spec) {
            return cached.clone();
        }
        let result = {
            let mut reader = SourceReader::new(&self.runner, &self.git_executable, self.max_bytes);
            reader.read(spec).map_err(map_source_error)
        };
        self.cache.insert(spec.clone(), result.clone());
        result
    }

    fn invalidate(&mut self) {
        self.cache.clear();
    }
}

fn map_source_error(error: SourceError) -> SourceFailure {
    match error {
        SourceError::TooLarge { limit } => SourceFailure::TooLarge { limit },
        SourceError::Io { source, .. } => SourceFailure::Io(source.to_string()),
        SourceError::NonUtf8 => SourceFailure::NonUtf8,
        SourceError::Command(error) => SourceFailure::Command(error.to_string()),
        SourceError::Git { object, stderr } => SourceFailure::Command(format!(
            "failed to read Git source {object}: {}",
            stderr.trim()
        )),
    }
}
