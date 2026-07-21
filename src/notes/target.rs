use std::collections::BTreeSet;

use crate::core::input::LayoutMode;
use crate::diff::model::{DiffFile, Hunk};

use super::{LineRange, NoteSource, ReviewNote};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoteAnchorSide {
    Old,
    New,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoteTarget {
    pub file_id: String,
    pub old_range: Option<LineRange>,
    pub new_range: Option<LineRange>,
    pub hunk_index: Option<usize>,
    pub anchor_side: Option<NoteAnchorSide>,
    pub anchor_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanNote {
    pub id: String,
    pub target: NoteTarget,
    pub body: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanNoteDraft {
    pub id: String,
    pub target: NoteTarget,
    pub body: String,
    pub editing: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveNoteInput {
    pub id: String,
    pub file_path: String,
    pub hunk_index: Option<usize>,
    pub side: NoteAnchorSide,
    pub line: u32,
    pub summary: String,
    pub rationale: Option<String>,
    pub markup: Option<String>,
    pub author: Option<String>,
    pub created_at: String,
}

impl LiveNoteInput {
    pub fn minimal(
        id: impl Into<String>,
        file_path: impl Into<String>,
        side: NoteAnchorSide,
        line: u32,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            file_path: file_path.into(),
            hunk_index: None,
            side,
            line,
            summary: summary.into(),
            rationale: None,
            markup: None,
            author: None,
            created_at: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveNote {
    pub target: NoteTarget,
    pub note: ReviewNote,
    pub markup_width: u16,
    pub markup_notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClearedSessionNotes {
    pub removed_live: usize,
    pub removed_user: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteBoxLayout {
    pub box_width: u16,
    pub box_left: u16,
    pub content_width: u16,
}

pub fn note_source(note: &ReviewNote) -> NoteSource {
    match note.source {
        NoteSource::User => NoteSource::User,
        NoteSource::Agent => NoteSource::Agent,
        NoteSource::Ai => NoteSource::Ai,
        NoteSource::Named(ref value) if value == "mcp" => NoteSource::Agent,
        NoteSource::Named(ref value) => NoteSource::Named(value.clone()),
    }
}

pub fn resolve_note_target(file: &DiffFile, note: &ReviewNote) -> NoteTarget {
    let hunk_index = file
        .hunks
        .iter()
        .position(|hunk| note_overlaps_hunk(note, hunk));
    let (anchor_side, anchor_line) = if let Some(range) = note.new_range {
        (Some(NoteAnchorSide::New), Some(range.start))
    } else if let Some(range) = note.old_range {
        (Some(NoteAnchorSide::Old), Some(range.start))
    } else {
        (None, None)
    };
    NoteTarget {
        file_id: file.id.clone(),
        old_range: note.old_range,
        new_range: note.new_range,
        hunk_index,
        anchor_side,
        anchor_line,
    }
}

pub fn resolve_ranges_target(
    file: &DiffFile,
    old_range: Option<LineRange>,
    new_range: Option<LineRange>,
) -> NoteTarget {
    let hunk_index = file.hunks.iter().position(|hunk| {
        new_range
            .zip(hunk_range(hunk, NoteAnchorSide::New))
            .is_some_and(|(note, hunk)| overlaps(note, hunk))
            || old_range
                .zip(hunk_range(hunk, NoteAnchorSide::Old))
                .is_some_and(|(note, hunk)| overlaps(note, hunk))
    });
    let (anchor_side, anchor_line) = if let Some(range) = new_range {
        (Some(NoteAnchorSide::New), Some(range.start))
    } else if let Some(range) = old_range {
        (Some(NoteAnchorSide::Old), Some(range.start))
    } else {
        (None, None)
    };
    NoteTarget {
        file_id: file.id.clone(),
        old_range,
        new_range,
        hunk_index,
        anchor_side,
        anchor_line,
    }
}

pub fn annotated_hunks(file: &DiffFile) -> Vec<usize> {
    file.agent
        .as_ref()
        .into_iter()
        .flat_map(|agent| &agent.annotations)
        .filter_map(|note| resolve_note_target(file, note).hunk_index)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn stable_note_id(file: &DiffFile, note: &ReviewNote) -> String {
    if let Some(id) = note.id.as_deref().filter(|id| !id.is_empty()) {
        return id.to_owned();
    }
    let mut value = 0xcbf2_9ce4_8422_2325_u64;
    for part in [
        file.id.as_bytes(),
        note.source.as_str().as_bytes(),
        note.summary.as_bytes(),
        note.rationale.as_deref().unwrap_or("").as_bytes(),
        note.markup.as_deref().unwrap_or("").as_bytes(),
    ] {
        for byte in part {
            value ^= u64::from(*byte);
            value = value.wrapping_mul(0x0000_0100_0000_01b3);
        }
        value ^= 0xff;
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    for range in [note.old_range, note.new_range].into_iter().flatten() {
        for byte in range
            .start
            .to_le_bytes()
            .into_iter()
            .chain(range.end.to_le_bytes())
        {
            value ^= u64::from(byte);
            value = value.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("note:{value:016x}")
}

pub fn annotation_range_label(note: &ReviewNote, file: Option<&DiffFile>) -> String {
    let mut locations = Vec::new();
    if let Some(range) = note.old_range {
        locations.push(format_range('L', range));
    }
    if let Some(range) = note.new_range {
        locations.push(format_range('R', range));
    }
    let location = if locations.is_empty() {
        "hunk".to_owned()
    } else {
        locations.join(" → ")
    };
    file.map_or(location.clone(), |file| format!("{} {location}", file.path))
}

pub fn note_box_layout(
    layout: LayoutMode,
    anchor_side: Option<NoteAnchorSide>,
    width: u16,
) -> NoteBoxLayout {
    let layout = match layout {
        LayoutMode::Auto if width >= 160 => LayoutMode::Split,
        LayoutMode::Auto => LayoutMode::Stack,
        explicit => explicit,
    };
    let can_dock = layout == LayoutMode::Split && width >= 84;
    let left_width = width / 2;
    let right_width = width.saturating_sub(left_width);
    let preferred = match (can_dock, anchor_side) {
        (true, Some(NoteAnchorSide::Old)) => left_width,
        (true, Some(NoteAnchorSide::New)) => right_width,
        _ => width.saturating_sub(4).max(34),
    };
    let maximum = width.saturating_sub(4).max(1);
    let minimum = 28.min(maximum);
    let box_width = preferred.clamp(minimum, maximum);
    let box_left = match (can_dock, anchor_side) {
        (true, Some(NoteAnchorSide::Old)) => 0,
        (true, Some(NoteAnchorSide::New)) => width.saturating_sub(box_width),
        _ => 4.min(width.saturating_sub(box_width)),
    };
    NoteBoxLayout {
        box_width,
        box_left,
        content_width: box_width.saturating_sub(4).max(1),
    }
}

fn note_overlaps_hunk(note: &ReviewNote, hunk: &Hunk) -> bool {
    note.new_range
        .zip(hunk_range(hunk, NoteAnchorSide::New))
        .is_some_and(|(note, hunk)| overlaps(note, hunk))
        || note
            .old_range
            .zip(hunk_range(hunk, NoteAnchorSide::Old))
            .is_some_and(|(note, hunk)| overlaps(note, hunk))
}

fn hunk_range(hunk: &Hunk, side: NoteAnchorSide) -> Option<LineRange> {
    let mut lines = hunk.lines.iter().filter_map(|line| match side {
        NoteAnchorSide::Old => line.old_lineno,
        NoteAnchorSide::New => line.new_lineno,
    });
    let first = lines.next()?;
    let (start, end) = lines.fold((first, first), |(start, end), line| {
        (start.min(line), end.max(line))
    });
    Some(LineRange { start, end })
}

fn overlaps(left: LineRange, right: LineRange) -> bool {
    left.start <= right.end && right.start <= left.end
}

fn format_range(prefix: char, range: LineRange) -> String {
    if range.start == range.end {
        format!("{prefix}{}", range.start)
    } else {
        format!("{prefix}{}–{prefix}{}", range.start, range.end)
    }
}
