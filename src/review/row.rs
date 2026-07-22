use crate::diff::model::{DiffFile, DiffLine, LineType, MovedLineKind};
use crate::input::sanitize_terminal_text;
use crate::notes::{
    HumanNote, HumanNoteDraft, LiveNote, NoteBoxLayout, NoteSource, NoteTarget, ReviewNote,
    annotation_range_label, note_box_layout, note_source, resolve_note_target, stable_note_id,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::context::{
    CollapsedGap, FileContextState, GapPosition, SourceFailure, derive_collapsed_gaps,
    expand_gap_lines,
};
use super::emphasis::{ChangedSpan, emphasize_pair};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EffectiveLayout {
    Split,
    Stack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ReviewRowKind {
    HunkHeader,
    DiffLine,
    Collapsed,
    ExpandedContext,
    Placeholder,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ReviewRowKey {
    pub file_id: String,
    pub hunk_index: Option<usize>,
    pub kind: ReviewRowKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub note_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CellKind {
    Context,
    Addition,
    Deletion,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReviewCell {
    pub kind: CellKind,
    pub sign: char,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub moved: Option<MovedLineKind>,
    pub spans: Vec<ChangedSpan>,
}

impl ReviewCell {
    pub(crate) fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlaceholderKind {
    Binary,
    TooLarge,
    NoTextChanges,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReviewRow {
    HunkHeader {
        key: ReviewRowKey,
        text: String,
    },
    Split {
        key: ReviewRowKey,
        left: ReviewCell,
        right: ReviewCell,
    },
    Stack {
        key: ReviewRowKey,
        cell: ReviewCell,
    },
    Placeholder {
        key: ReviewRowKey,
        kind: PlaceholderKind,
        text: String,
    },
    Collapsed {
        key: ReviewRowKey,
        gap: CollapsedGap,
        text: String,
    },
    Note {
        key: ReviewRowKey,
        card: NoteCard,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteCard {
    pub id: String,
    pub target: NoteTarget,
    pub source: NoteSource,
    pub title: String,
    pub location: String,
    pub lines: Vec<String>,
    pub markup: Option<Box<NoteMarkup>>,
    pub tags: Vec<String>,
    pub author: Option<String>,
    pub placement: NoteBoxLayout,
    pub human: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteMarkup {
    pub lines: Vec<crate::markup::StmlLine>,
    pub notes: Vec<String>,
}

impl NoteCard {
    pub(crate) fn height(&self) -> usize {
        self.lines.len().saturating_add(3).max(4)
    }
}

impl ReviewRow {
    pub(super) fn key(&self) -> &ReviewRowKey {
        match self {
            Self::HunkHeader { key, .. }
            | Self::Split { key, .. }
            | Self::Stack { key, .. }
            | Self::Collapsed { key, .. }
            | Self::Placeholder { key, .. }
            | Self::Note { key, .. } => key,
        }
    }

    pub(super) fn is_selectable(&self) -> bool {
        matches!(self, Self::Split { .. } | Self::Stack { .. })
    }

    pub(super) fn available_sides(&self) -> (bool, bool) {
        match self {
            Self::Split { left, right, .. } => {
                (left.kind != CellKind::Empty, right.kind != CellKind::Empty)
            }
            Self::Stack { cell, .. } => match cell.kind {
                CellKind::Addition => (false, true),
                CellKind::Deletion => (true, false),
                CellKind::Context => (true, true),
                CellKind::Empty => (false, false),
            },
            _ => (false, false),
        }
    }

    pub(super) fn cursor_lines(&self, focus_right: bool) -> (Option<u32>, Option<u32>) {
        match self {
            Self::Split { left, right, .. }
                if left.kind == CellKind::Deletion && right.kind == CellKind::Addition =>
            {
                if focus_right {
                    (None, right.new_line)
                } else {
                    (left.old_line, None)
                }
            }
            _ => {
                let key = self.key();
                (key.old_line, key.new_line)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowPlan {
    pub rows: Vec<ReviewRow>,
    pub hunk_anchor_keys: Vec<ReviewRowKey>,
    pub line_number_digits: usize,
}

pub(crate) struct NotePlanOptions<'a> {
    pub human_notes: &'a [HumanNote],
    pub live_notes: &'a [LiveNote],
    pub draft: Option<&'a HumanNoteDraft>,
    pub show_agent_notes: bool,
    pub content_width: u16,
}

#[cfg(test)]
pub(crate) fn build_row_plan(
    file: &DiffFile,
    layout: EffectiveLayout,
    show_hunk_headers: bool,
) -> RowPlan {
    build_row_plan_with_context(file, layout, show_hunk_headers, None)
}

pub(crate) fn build_row_plan_with_context(
    file: &DiffFile,
    layout: EffectiveLayout,
    show_hunk_headers: bool,
    context: Option<&FileContextState>,
) -> RowPlan {
    if file.hunks.is_empty() {
        return placeholder_plan(file);
    }

    let mut rows = Vec::new();
    let mut hunk_anchor_keys = Vec::with_capacity(file.hunks.len());
    let gaps = context.map_or_else(
        || derive_collapsed_gaps(file, None),
        |context| context.gaps(file),
    );
    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
        if let Some(gap) = gaps
            .iter()
            .find(|gap| gap.key.position == GapPosition::Before && gap.key.hunk_index == hunk_index)
        {
            append_gap_rows(file, layout, context, gap, &mut rows);
        }
        let hunk_start = rows.len();
        if show_hunk_headers {
            rows.push(ReviewRow::HunkHeader {
                key: row_key(
                    file,
                    Some(hunk_index),
                    ReviewRowKind::HunkHeader,
                    None,
                    None,
                ),
                text: sanitize_line(&hunk.header),
            });
        }

        match layout {
            EffectiveLayout::Split => build_split_rows(file, hunk_index, &hunk.lines, &mut rows),
            EffectiveLayout::Stack => build_stack_rows(file, hunk_index, &hunk.lines, &mut rows),
        }

        if let Some(anchor) = rows[hunk_start..]
            .iter()
            .find(|row| row.is_selectable())
            .map(ReviewRow::key)
            .cloned()
        {
            hunk_anchor_keys.push(anchor);
        }
    }
    if let Some(gap) = gaps
        .iter()
        .find(|gap| gap.key.position == GapPosition::Trailing)
    {
        append_gap_rows(file, layout, context, gap, &mut rows);
    }

    RowPlan {
        line_number_digits: max_line_number_digits(file),
        rows,
        hunk_anchor_keys,
    }
}

pub(crate) fn build_row_plan_with_notes(
    file: &DiffFile,
    layout: EffectiveLayout,
    show_hunk_headers: bool,
    context: Option<&FileContextState>,
    options: NotePlanOptions<'_>,
) -> RowPlan {
    let mut plan = build_row_plan_with_context(file, layout, show_hunk_headers, context);
    if plan.rows.is_empty() {
        return plan;
    }
    let layout_mode = match layout {
        EffectiveLayout::Split => crate::core::input::LayoutMode::Split,
        EffectiveLayout::Stack => crate::core::input::LayoutMode::Stack,
    };
    let mut placements = Vec::<(usize, NoteCard)>::new();
    if let Some(agent) = &file.agent {
        for note in &agent.annotations {
            let source = note_source(note);
            if !options.show_agent_notes && source != NoteSource::User {
                continue;
            }
            let target = resolve_note_target(file, note);
            let anchor = note_anchor_index(&plan.rows, &target);
            placements.push((
                anchor,
                external_card(
                    file,
                    note,
                    target,
                    source,
                    layout_mode,
                    options.content_width,
                ),
            ));
        }
    }
    if options.show_agent_notes {
        for live in options
            .live_notes
            .iter()
            .filter(|note| note.target.file_id == file.id)
        {
            let anchor = note_anchor_index(&plan.rows, &live.target);
            placements.push((
                anchor,
                external_card(
                    file,
                    &live.note,
                    live.target.clone(),
                    NoteSource::Agent,
                    layout_mode,
                    options.content_width,
                ),
            ));
        }
    }
    for note in options
        .human_notes
        .iter()
        .filter(|note| note.target.file_id == file.id)
    {
        let anchor = note_anchor_index(&plan.rows, &note.target);
        placements.push((
            anchor,
            human_card(file, note, layout_mode, options.content_width),
        ));
    }
    if let Some(draft) = options
        .draft
        .filter(|draft| draft.target.file_id == file.id)
    {
        let anchor = note_anchor_index(&plan.rows, &draft.target);
        placements.push((
            anchor,
            draft_card(file, draft, layout_mode, options.content_width),
        ));
    }
    if placements.is_empty() {
        return plan;
    }
    let mut rows = Vec::with_capacity(plan.rows.len().saturating_add(placements.len()));
    for (index, row) in plan.rows.into_iter().enumerate() {
        rows.push(row);
        for (_, card) in placements.iter().filter(|(anchor, _)| *anchor == index) {
            rows.push(ReviewRow::Note {
                key: ReviewRowKey {
                    file_id: file.id.clone(),
                    hunk_index: card.target.hunk_index,
                    kind: ReviewRowKind::Note,
                    old_line: card.target.old_range.map(|range| range.start),
                    new_line: card.target.new_range.map(|range| range.start),
                    note_id: Some(card.id.clone()),
                },
                card: card.clone(),
            });
        }
    }
    plan.rows = rows;
    plan
}

fn append_gap_rows(
    file: &DiffFile,
    layout: EffectiveLayout,
    context: Option<&FileContextState>,
    gap: &CollapsedGap,
    rows: &mut Vec<ReviewRow>,
) {
    let expanded = context.is_some_and(|context| context.expanded.contains(&gap.key));
    let side = context
        .and_then(|context| context.source.as_ref())
        .and_then(|source| source.as_ref().ok())
        .map_or(super::context::SourceSide::New, |source| source.side);
    let count = gap.line_count(side);
    let mut label = format!(
        "{count} unchanged {}",
        if count == 1 { "line" } else { "lines" }
    );
    let expanded_lines = if expanded {
        match context.and_then(|context| context.source.as_ref()) {
            Some(Ok(source)) => match expand_gap_lines(gap, &source.text, source.side) {
                Ok(lines) => {
                    label = format!(
                        "Hide {count} unchanged {}",
                        if count == 1 { "line" } else { "lines" }
                    );
                    Some(lines)
                }
                Err(failure) => {
                    label = context_failure_label(count, &failure);
                    None
                }
            },
            Some(Err(failure)) => {
                label = context_failure_label(count, failure);
                None
            }
            None => {
                label = format!(
                    "Loading {count} unchanged {}…",
                    if count == 1 { "line" } else { "lines" }
                );
                None
            }
        }
    } else {
        None
    };
    rows.push(ReviewRow::Collapsed {
        key: row_key(
            file,
            Some(gap.key.hunk_index),
            ReviewRowKind::Collapsed,
            Some(*gap.old_range.start()),
            Some(*gap.new_range.start()),
        ),
        gap: gap.clone(),
        text: label,
    });
    if let Some(lines) = expanded_lines {
        for line in lines {
            let cell = ReviewCell {
                kind: CellKind::Context,
                sign: ' ',
                old_line: Some(line.old_line),
                new_line: Some(line.new_line),
                moved: None,
                spans: (!line.text.is_empty())
                    .then(|| ChangedSpan::plain(line.text))
                    .into_iter()
                    .collect(),
            };
            let key = row_key(
                file,
                Some(gap.key.hunk_index),
                ReviewRowKind::ExpandedContext,
                cell.old_line,
                cell.new_line,
            );
            match layout {
                EffectiveLayout::Split => rows.push(ReviewRow::Split {
                    key,
                    left: cell.clone(),
                    right: cell,
                }),
                EffectiveLayout::Stack => rows.push(ReviewRow::Stack { key, cell }),
            }
        }
    }
}

fn context_failure_label(count: usize, failure: &SourceFailure) -> String {
    let lines = if count == 1 { "line" } else { "lines" };
    match failure {
        SourceFailure::Unavailable => format!("Context unavailable for {count} unchanged {lines}"),
        SourceFailure::Missing => format!("Source missing for {count} unchanged {lines}"),
        SourceFailure::TooLarge { .. } => {
            format!("Source too large to expand {count} unchanged {lines}")
        }
        SourceFailure::NonUtf8 => {
            format!("Non-UTF-8 source cannot expand {count} unchanged {lines}")
        }
        SourceFailure::Io(_) => format!("Could not read {count} unchanged {lines}"),
        SourceFailure::Command(_) => format!("Source command failed for {count} unchanged {lines}"),
        SourceFailure::ShortSource => format!("Source too short for {count} unchanged {lines}"),
    }
}

fn placeholder_plan(file: &DiffFile) -> RowPlan {
    let (kind, text) = if file.is_binary {
        (PlaceholderKind::Binary, "Binary file")
    } else if file.is_too_large {
        (PlaceholderKind::TooLarge, "File is too large to display")
    } else {
        (PlaceholderKind::NoTextChanges, "No textual changes")
    };
    RowPlan {
        rows: vec![ReviewRow::Placeholder {
            key: row_key(file, None, ReviewRowKind::Placeholder, None, None),
            kind,
            text: text.into(),
        }],
        hunk_anchor_keys: Vec::new(),
        line_number_digits: 1,
    }
}

fn build_split_rows(
    file: &DiffFile,
    hunk_index: usize,
    lines: &[DiffLine],
    rows: &mut Vec<ReviewRow>,
) {
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind == LineType::Context {
            let cell = cell_from_line(&lines[index], None);
            rows.push(ReviewRow::Split {
                key: row_key(
                    file,
                    Some(hunk_index),
                    ReviewRowKind::DiffLine,
                    cell.old_line,
                    cell.new_line,
                ),
                left: cell.clone(),
                right: cell,
            });
            index += 1;
            continue;
        }

        let block_start = index;
        while index < lines.len() && lines[index].kind != LineType::Context {
            index += 1;
        }
        let block = &lines[block_start..index];
        let deletions = block
            .iter()
            .filter(|line| line.kind == LineType::Deletion)
            .collect::<Vec<_>>();
        let additions = block
            .iter()
            .filter(|line| line.kind == LineType::Addition)
            .collect::<Vec<_>>();
        for pair_index in 0..deletions.len().max(additions.len()) {
            let deletion = deletions.get(pair_index).copied();
            let addition = additions.get(pair_index).copied();
            let (old_spans, new_spans) = match (deletion, addition) {
                (Some(old), Some(new)) => {
                    emphasize_pair(&sanitize_line(&old.content), &sanitize_line(&new.content))
                }
                (Some(old), None) => (emphasized_text(&sanitize_line(&old.content)), Vec::new()),
                (None, Some(new)) => (Vec::new(), emphasized_text(&sanitize_line(&new.content))),
                (None, None) => unreachable!(),
            };
            let left =
                deletion.map_or_else(empty_cell, |line| cell_from_line(line, Some(old_spans)));
            let right =
                addition.map_or_else(empty_cell, |line| cell_from_line(line, Some(new_spans)));
            rows.push(ReviewRow::Split {
                key: row_key(
                    file,
                    Some(hunk_index),
                    ReviewRowKind::DiffLine,
                    left.old_line,
                    right.new_line,
                ),
                left,
                right,
            });
        }
    }
}

fn build_stack_rows(
    file: &DiffFile,
    hunk_index: usize,
    lines: &[DiffLine],
    rows: &mut Vec<ReviewRow>,
) {
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind == LineType::Context {
            push_stack_row(file, hunk_index, &lines[index], None, rows);
            index += 1;
            continue;
        }

        let block_start = index;
        while index < lines.len() && lines[index].kind != LineType::Context {
            index += 1;
        }
        let block = &lines[block_start..index];
        let deletion_count = block
            .iter()
            .filter(|line| line.kind == LineType::Deletion)
            .count();
        let mut deletion_index = 0;
        let mut addition_index = 0;
        let deletions = block
            .iter()
            .filter(|line| line.kind == LineType::Deletion)
            .collect::<Vec<_>>();
        let additions = block
            .iter()
            .filter(|line| line.kind == LineType::Addition)
            .collect::<Vec<_>>();
        let emphasized = (0..deletions.len().max(additions.len()))
            .map(
                |pair_index| match (deletions.get(pair_index), additions.get(pair_index)) {
                    (Some(old), Some(new)) => {
                        emphasize_pair(&sanitize_line(&old.content), &sanitize_line(&new.content))
                    }
                    (Some(old), None) => {
                        (emphasized_text(&sanitize_line(&old.content)), Vec::new())
                    }
                    (None, Some(new)) => {
                        (Vec::new(), emphasized_text(&sanitize_line(&new.content)))
                    }
                    (None, None) => unreachable!(),
                },
            )
            .collect::<Vec<_>>();

        for line in block {
            let spans = match line.kind {
                LineType::Deletion => {
                    let spans = emphasized[deletion_index].0.clone();
                    deletion_index += 1;
                    spans
                }
                LineType::Addition => {
                    let spans = emphasized[addition_index].1.clone();
                    addition_index += 1;
                    spans
                }
                LineType::Context => unreachable!(),
            };
            push_stack_row(file, hunk_index, line, Some(spans), rows);
        }
        debug_assert_eq!(deletion_index, deletion_count);
    }
}

fn push_stack_row(
    file: &DiffFile,
    hunk_index: usize,
    line: &DiffLine,
    spans: Option<Vec<ChangedSpan>>,
    rows: &mut Vec<ReviewRow>,
) {
    rows.push(ReviewRow::Stack {
        key: row_key(
            file,
            Some(hunk_index),
            ReviewRowKind::DiffLine,
            line.old_lineno,
            line.new_lineno,
        ),
        cell: cell_from_line(line, spans),
    });
}

fn cell_from_line(line: &DiffLine, spans: Option<Vec<ChangedSpan>>) -> ReviewCell {
    let kind = match line.kind {
        LineType::Context => CellKind::Context,
        LineType::Addition => CellKind::Addition,
        LineType::Deletion => CellKind::Deletion,
    };
    ReviewCell {
        kind,
        sign: match kind {
            CellKind::Context => ' ',
            CellKind::Addition => '+',
            CellKind::Deletion => '-',
            CellKind::Empty => ' ',
        },
        old_line: line.old_lineno,
        new_line: line.new_lineno,
        moved: line.moved,
        spans: spans.unwrap_or_else(|| plain_text(&sanitize_line(&line.content))),
    }
}

fn empty_cell() -> ReviewCell {
    ReviewCell {
        kind: CellKind::Empty,
        sign: ' ',
        old_line: None,
        new_line: None,
        moved: None,
        spans: Vec::new(),
    }
}

fn plain_text(text: &str) -> Vec<ChangedSpan> {
    (!text.is_empty())
        .then(|| ChangedSpan::plain(text))
        .into_iter()
        .collect()
}

fn emphasized_text(text: &str) -> Vec<ChangedSpan> {
    (!text.is_empty())
        .then(|| ChangedSpan::emphasized(text))
        .into_iter()
        .collect()
}

fn sanitize_line(text: &str) -> String {
    sanitize_terminal_text(text, false).replace('\t', "  ")
}

fn row_key(
    file: &DiffFile,
    hunk_index: Option<usize>,
    kind: ReviewRowKind,
    old_line: Option<u32>,
    new_line: Option<u32>,
) -> ReviewRowKey {
    ReviewRowKey {
        file_id: file.id.clone(),
        hunk_index,
        kind,
        old_line,
        new_line,
        note_id: None,
    }
}

fn note_anchor_index(rows: &[ReviewRow], target: &NoteTarget) -> usize {
    let exact = rows.iter().position(|row| {
        let key = row.key();
        key.hunk_index == target.hunk_index
            && match target.anchor_side {
                Some(crate::notes::NoteAnchorSide::New) => key.new_line == target.anchor_line,
                Some(crate::notes::NoteAnchorSide::Old) => key.old_line == target.anchor_line,
                None => false,
            }
    });
    exact
        .or_else(|| {
            rows.iter()
                .position(|row| row.key().hunk_index == target.hunk_index)
        })
        .unwrap_or(0)
}

fn external_card(
    file: &DiffFile,
    note: &ReviewNote,
    target: NoteTarget,
    source: NoteSource,
    layout: crate::core::input::LayoutMode,
    width: u16,
) -> NoteCard {
    let placement = note_box_layout(layout, target.anchor_side, width);
    let mut body = note.summary.clone();
    if let Some(rationale) = &note.rationale {
        body.push('\n');
        body.push_str(rationale);
    }
    if !note.tags.is_empty() || note.author.is_some() || note.confidence.is_some() {
        body.push('\n');
        if let Some(author) = &note.author {
            body.push_str(author);
        }
        if !note.tags.is_empty() {
            if note.author.is_some() {
                body.push_str(" · ");
            }
            body.push_str(&note.tags.join(" · "));
        }
        if let Some(confidence) = &note.confidence {
            if note.author.is_some() || !note.tags.is_empty() {
                body.push_str(" · ");
            }
            body.push_str(confidence.as_str());
            body.push_str(" confidence");
        }
    }
    let markup = note
        .markup
        .as_deref()
        .map(|markup| crate::markup::layout_stml_cached(markup, placement.content_width));
    let markup_lines = markup
        .as_ref()
        .filter(|result| !result.lines.is_empty())
        .map(|result| result.lines.clone());
    let lines = markup_lines.as_ref().map_or_else(
        || wrap_note_text(&body, usize::from(placement.content_width)),
        |lines| {
            lines
                .iter()
                .map(|line| line.spans.iter().map(|span| span.text.as_str()).collect())
                .collect()
        },
    );
    NoteCard {
        id: stable_note_id(file, note),
        target,
        title: note.title.clone().unwrap_or_else(|| match source {
            NoteSource::User => "Your note".into(),
            NoteSource::Agent => "Agent note".into(),
            NoteSource::Ai => "AI note".into(),
            NoteSource::Named(ref value) => format!("{value} note"),
        }),
        location: annotation_range_label(note, Some(file)),
        lines,
        markup: markup_lines.map(|lines| {
            Box::new(NoteMarkup {
                lines,
                notes: markup.map_or_else(Vec::new, |result| result.errors.clone()),
            })
        }),
        tags: note.tags.clone(),
        author: note.author.clone(),
        placement,
        source,
        human: false,
    }
}

fn human_card(
    file: &DiffFile,
    note: &HumanNote,
    layout: crate::core::input::LayoutMode,
    width: u16,
) -> NoteCard {
    let placement = note_box_layout(layout, note.target.anchor_side, width);
    let location = target_location(file, &note.target);
    NoteCard {
        id: note.id.clone(),
        target: note.target.clone(),
        source: NoteSource::User,
        title: "Your note".into(),
        location,
        lines: wrap_note_text(&note.body, usize::from(placement.content_width)),
        markup: None,
        tags: Vec::new(),
        author: None,
        placement,
        human: true,
    }
}

fn draft_card(
    file: &DiffFile,
    draft: &HumanNoteDraft,
    layout: crate::core::input::LayoutMode,
    width: u16,
) -> NoteCard {
    let placement = note_box_layout(layout, draft.target.anchor_side, width);
    let body = if draft.body.is_empty() {
        "Write a note".to_owned()
    } else {
        draft.body.clone()
    };
    NoteCard {
        id: draft.id.clone(),
        target: draft.target.clone(),
        source: NoteSource::User,
        title: "Draft note".into(),
        location: target_location(file, &draft.target),
        lines: wrap_note_text(&body, usize::from(placement.content_width)),
        markup: None,
        tags: Vec::new(),
        author: None,
        placement,
        human: true,
    }
}

fn target_location(file: &DiffFile, target: &NoteTarget) -> String {
    let mut parts = Vec::new();
    if let Some(range) = target.old_range {
        parts.push(if range.start == range.end {
            format!("L{}", range.start)
        } else {
            format!("L{}–L{}", range.start, range.end)
        });
    }
    if let Some(range) = target.new_range {
        parts.push(if range.start == range.end {
            format!("R{}", range.start)
        } else {
            format!("R{}–R{}", range.start, range.end)
        });
    }
    format!(
        "{} {}",
        file.path,
        if parts.is_empty() {
            "hunk".into()
        } else {
            parts.join(" → ")
        }
    )
}

fn wrap_note_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut output = Vec::new();
    for source_line in text.lines() {
        let mut current = String::new();
        let mut current_width = 0usize;
        for word in source_line.split_whitespace() {
            let word_width = UnicodeWidthStr::width(word);
            if current_width > 0 && current_width.saturating_add(1 + word_width) <= width {
                current.push(' ');
                current.push_str(word);
                current_width += 1 + word_width;
            } else {
                if !current.is_empty() {
                    output.push(std::mem::take(&mut current));
                    current_width = 0;
                }
                for character in word.chars() {
                    let character_width = character.width().unwrap_or(0);
                    if current_width > 0 && current_width.saturating_add(character_width) > width {
                        output.push(std::mem::take(&mut current));
                        current_width = 0;
                    }
                    current.push(character);
                    current_width = current_width.saturating_add(character_width);
                }
            }
        }
        if !current.is_empty() {
            output.push(current);
        } else if source_line.is_empty() {
            output.push(String::new());
        }
    }
    if output.is_empty() {
        output.push(String::new());
    }
    output
}

fn max_line_number_digits(file: &DiffFile) -> usize {
    file.hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .flat_map(|line| [line.old_lineno, line.new_lineno])
        .flatten()
        .max()
        .unwrap_or(1)
        .to_string()
        .len()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::diff::model::{
        DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, MovedLineKind, SourceSpec,
    };

    use crate::review::context::{FileContextState, derive_collapsed_gaps};

    use super::{
        CellKind, EffectiveLayout, PlaceholderKind, ReviewRow, ReviewRowKind, build_row_plan,
        build_row_plan_with_context,
    };

    fn line(kind: LineType, content: &str, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine {
            kind,
            content: content.into(),
            old_lineno: old,
            new_lineno: new,
            moved: None,
        }
    }

    fn file(lines: Vec<DiffLine>) -> DiffFile {
        DiffFile {
            id: "file:src/lib.rs".into(),
            path: "src/lib.rs".into(),
            previous_path: None,
            summary: None,
            agent: None,
            patch: String::new(),
            hunks: vec![Hunk {
                old_start: 1,
                new_start: 1,
                header: "@@ -1,4 +1,3 @@ function".into(),
                lines,
            }],
            change_kind: FileChangeKind::Modified,
            is_binary: false,
            is_untracked: false,
            is_too_large: false,
            stats_truncated: false,
            language: Some("Rust".into()),
            stats: FileStats {
                additions: 2,
                deletions: 3,
            },
            old_source: SourceSpec::File(PathBuf::from("old")),
            new_source: SourceSpec::File(PathBuf::from("new")),
        }
    }

    #[test]
    fn split_rows_pair_changed_blocks_and_fill_uneven_cells() {
        let file = file(vec![
            line(LineType::Context, "fn demo() {", Some(1), Some(1)),
            line(LineType::Deletion, "  old_one();", Some(2), None),
            line(LineType::Deletion, "  old_two();", Some(3), None),
            line(LineType::Addition, "  new_one();", None, Some(2)),
            line(LineType::Context, "}", Some(4), Some(3)),
        ]);

        let plan = build_row_plan(&file, EffectiveLayout::Split, true);
        assert!(matches!(plan.rows[0], ReviewRow::HunkHeader { .. }));
        let ReviewRow::Split { left, right, .. } = &plan.rows[1] else {
            panic!("expected split context row");
        };
        assert_eq!(
            (left.kind, right.kind),
            (CellKind::Context, CellKind::Context)
        );
        assert_eq!((left.old_line, right.new_line), (Some(1), Some(1)));

        let ReviewRow::Split { left, right, .. } = &plan.rows[2] else {
            panic!("expected paired changed row");
        };
        assert_eq!(
            (left.kind, right.kind),
            (CellKind::Deletion, CellKind::Addition)
        );
        assert_eq!((left.old_line, right.new_line), (Some(2), Some(2)));
        assert!(left.spans.iter().any(|span| span.emphasized));
        assert!(right.spans.iter().any(|span| span.emphasized));

        let ReviewRow::Split { left, right, .. } = &plan.rows[3] else {
            panic!("expected uneven changed row");
        };
        assert_eq!(
            (left.kind, right.kind),
            (CellKind::Deletion, CellKind::Empty)
        );
        assert_eq!(left.old_line, Some(3));
        assert_eq!(right.new_line, None);
    }

    #[test]
    fn stack_rows_preserve_diff_order_and_line_numbers() {
        let file = file(vec![
            line(LineType::Deletion, "old", Some(7), None),
            line(LineType::Addition, "new", None, Some(9)),
            line(LineType::Context, "same", Some(8), Some(10)),
        ]);
        let plan = build_row_plan(&file, EffectiveLayout::Stack, false);
        assert_eq!(plan.rows.len(), 3);

        let kinds = plan
            .rows
            .iter()
            .map(|row| match row {
                ReviewRow::Stack { cell, .. } => cell.kind,
                _ => panic!("expected only stack lines"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [CellKind::Deletion, CellKind::Addition, CellKind::Context]
        );

        let ReviewRow::Stack { cell, .. } = &plan.rows[2] else {
            unreachable!()
        };
        assert_eq!((cell.old_line, cell.new_line), (Some(8), Some(10)));
        assert_eq!(plan.line_number_digits, 2);
    }

    #[test]
    fn row_keys_are_stable_and_semantic() {
        let sample = file(vec![line(LineType::Addition, "new", None, Some(1))]);
        let first = build_row_plan(&sample, EffectiveLayout::Split, true);
        let second = build_row_plan(&sample.clone(), EffectiveLayout::Split, true);
        assert_eq!(first.rows, second.rows);
        assert_eq!(first.hunk_anchor_keys.len(), 1);
        assert_eq!(first.hunk_anchor_keys[0].file_id, sample.id);
        assert_eq!(first.hunk_anchor_keys[0].hunk_index, Some(0));
        assert_eq!(first.hunk_anchor_keys[0].kind, ReviewRowKind::HunkHeader);

        let baseline = file(vec![
            line(LineType::Context, "same", Some(10), Some(10)),
            line(LineType::Addition, "target", None, Some(11)),
        ]);
        let with_preceding_row = file(vec![
            line(LineType::Context, "earlier", Some(9), Some(9)),
            line(LineType::Context, "same", Some(10), Some(10)),
            line(LineType::Addition, "target", None, Some(11)),
        ]);
        let target_key = |file: &DiffFile| {
            build_row_plan(file, EffectiveLayout::Stack, false)
                .rows
                .into_iter()
                .find_map(|row| match row {
                    ReviewRow::Stack { key, cell, .. } if cell.new_line == Some(11) => Some(key),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(target_key(&baseline), target_key(&with_preceding_row));
    }

    #[test]
    fn placeholders_distinguish_binary_large_and_metadata_only_files() {
        for (binary, large, expected) in [
            (true, false, PlaceholderKind::Binary),
            (false, true, PlaceholderKind::TooLarge),
            (false, false, PlaceholderKind::NoTextChanges),
        ] {
            let mut file = file(Vec::new());
            file.hunks.clear();
            file.is_binary = binary;
            file.is_too_large = large;
            let plan = build_row_plan(&file, EffectiveLayout::Split, true);
            assert_eq!(plan.rows.len(), 1);
            assert!(matches!(
                &plan.rows[0],
                ReviewRow::Placeholder { kind, .. } if *kind == expected
            ));
        }
    }

    #[test]
    fn moved_classes_and_sanitized_tab_expansion_survive_planning() {
        let mut moved = line(
            LineType::Addition,
            "\tnew\x1b]8;;https://bad\x1b\\ link\x1b]8;;\x1b\\\x07",
            None,
            Some(12),
        );
        moved.moved = Some(MovedLineKind::NewMovedDimmed);
        let file = file(vec![moved]);
        let plan = build_row_plan(&file, EffectiveLayout::Stack, false);
        let ReviewRow::Stack { cell, key } = &plan.rows[0] else {
            unreachable!()
        };
        assert_eq!(cell.moved, Some(MovedLineKind::NewMovedDimmed));
        assert_eq!(cell.text(), "  new link");
        assert!(!cell.text().contains("https://bad"));
        assert_eq!(key.kind, ReviewRowKind::DiffLine);
    }

    #[test]
    fn expanded_gap_without_a_loaded_source_has_one_loading_row() {
        let mut sample = file(vec![line(LineType::Context, "changed", Some(4), Some(4))]);
        sample.hunks[0].old_start = 4;
        sample.hunks[0].new_start = 4;
        let gap = derive_collapsed_gaps(&sample, None).remove(0);
        let mut context = FileContextState::default();
        context.expanded.insert(gap.key);

        let plan =
            build_row_plan_with_context(&sample, EffectiveLayout::Stack, true, Some(&context));
        assert_eq!(
            plan.rows
                .iter()
                .filter(|row| matches!(row, ReviewRow::Collapsed { .. }))
                .count(),
            1
        );
        assert!(matches!(
            &plan.rows[0],
            ReviewRow::Collapsed { text, .. } if text == "Loading 3 unchanged lines…"
        ));
    }
}
