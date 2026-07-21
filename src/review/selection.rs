use std::ops::Range;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    pub row: usize,
    pub cell: usize,
}

impl SelectionPoint {
    pub const fn new(row: usize, cell: usize) -> Self {
        Self { row, cell }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionRow {
    Stack {
        text: String,
        text_cell: usize,
    },
    Split {
        left: String,
        right: String,
        divider_cell: usize,
        left_text_cell: usize,
        right_text_cell: usize,
    },
}

impl SelectionRow {
    pub fn stack(text: impl Into<String>) -> Self {
        Self::Stack {
            text: text.into(),
            text_cell: 0,
        }
    }

    pub fn split(left: impl Into<String>, right: impl Into<String>, divider_cell: usize) -> Self {
        Self::Split {
            left: left.into(),
            right: right.into(),
            divider_cell,
            left_text_cell: 0,
            right_text_cell: divider_cell.saturating_add(1),
        }
    }

    pub(crate) fn stack_at(text: impl Into<String>, text_cell: usize) -> Self {
        Self::Stack {
            text: text.into(),
            text_cell,
        }
    }

    pub(crate) fn split_at(
        left: impl Into<String>,
        right: impl Into<String>,
        divider_cell: usize,
        left_text_cell: usize,
        right_text_cell: usize,
    ) -> Self {
        Self::Split {
            left: left.into(),
            right: right.into(),
            divider_cell,
            left_text_cell,
            right_text_cell,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionSide {
    Stack,
    Left,
    Right,
}

pub fn cell_slice(text: &str, range: Range<usize>) -> String {
    if range.start >= range.end {
        return String::new();
    }
    let mut cell = 0usize;
    let mut selected = String::new();
    let mut previous_was_selected = false;
    for character in text.chars() {
        let width = character.width().unwrap_or(0);
        let end = cell.saturating_add(width);
        let overlaps = width > 0 && cell < range.end && end > range.start;
        let include = overlaps || (width == 0 && previous_was_selected);
        if include {
            selected.push(character);
        }
        if width > 0 {
            previous_was_selected = overlaps;
            cell = end;
        }
    }
    selected
}

pub fn word_cell_range(text: &str, target_cell: usize) -> Range<usize> {
    let characters = text
        .chars()
        .scan(0usize, |cell, character| {
            let start = *cell;
            *cell = cell.saturating_add(character.width().unwrap_or(0));
            Some((character, start, *cell))
        })
        .collect::<Vec<_>>();
    let Some(index) = characters
        .iter()
        .position(|(_, start, end)| *start <= target_cell && target_cell < *end)
    else {
        return target_cell..target_cell;
    };
    let is_word = |character: char| character.is_alphanumeric() || character == '_';
    if !is_word(characters[index].0) {
        return characters[index].1..characters[index].2;
    }
    let start = characters[..index]
        .iter()
        .rposition(|(character, _, _)| !is_word(*character))
        .map_or(0, |boundary| boundary + 1);
    let end = characters[index + 1..]
        .iter()
        .position(|(character, _, _)| !is_word(*character))
        .map_or(characters.len(), |boundary| index + 1 + boundary);
    characters[start].1..characters[end - 1].2
}

pub fn line_cell_range(text: &str) -> Range<usize> {
    0..UnicodeWidthStr::width(text)
}

pub fn project_selection(
    rows: &[SelectionRow],
    anchor: SelectionPoint,
    focus: SelectionPoint,
) -> String {
    let Some(anchor_row) = rows.get(anchor.row) else {
        return String::new();
    };
    let side = side_at(anchor_row, anchor.cell);
    let (start, end) = if (anchor.row, anchor.cell) <= (focus.row, focus.cell) {
        (anchor, focus)
    } else {
        (focus, anchor)
    };
    if start.row >= rows.len() {
        return String::new();
    }
    let last_row = end.row.min(rows.len() - 1);
    (start.row..=last_row)
        .map(|row_index| {
            let row = &rows[row_index];
            let text = text_for_side(row, side);
            let width = UnicodeWidthStr::width(text);
            let from = if row_index == start.row {
                local_cell(row, side, start.cell).min(width)
            } else {
                0
            };
            let to = if row_index == last_row {
                local_cell(row, side, end.cell).min(width)
            } else {
                width
            };
            cell_slice(text, from.min(to)..to.max(from))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn side_at(row: &SelectionRow, cell: usize) -> SelectionSide {
    match row {
        SelectionRow::Stack { .. } => SelectionSide::Stack,
        SelectionRow::Split { divider_cell, .. } if cell > *divider_cell => SelectionSide::Right,
        SelectionRow::Split { .. } => SelectionSide::Left,
    }
}

fn text_for_side(row: &SelectionRow, side: SelectionSide) -> &str {
    match (row, side) {
        (SelectionRow::Stack { text, .. }, _)
        | (SelectionRow::Split { left: text, .. }, SelectionSide::Left) => text,
        (SelectionRow::Split { right, .. }, SelectionSide::Right) => right,
        (SelectionRow::Split { left, .. }, SelectionSide::Stack) => left,
    }
}

fn local_cell(row: &SelectionRow, side: SelectionSide, cell: usize) -> usize {
    match (row, side) {
        (SelectionRow::Stack { text_cell, .. }, _) => cell.saturating_sub(*text_cell),
        (
            SelectionRow::Split { left_text_cell, .. },
            SelectionSide::Left | SelectionSide::Stack,
        ) => cell.saturating_sub(*left_text_cell),
        (
            SelectionRow::Split {
                right_text_cell, ..
            },
            SelectionSide::Right,
        ) => cell.saturating_sub(*right_text_cell),
    }
}
