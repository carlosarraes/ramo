use crate::diff::model::{DiffFile, DiffLine, LineType, MovedLineKind};
use crate::input::sanitize_terminal_text;

use super::emphasis::{ChangedSpan, emphasize_pair};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EffectiveLayout {
    Split,
    Stack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ReviewRowKind {
    HunkHeader,
    SplitLine,
    StackLine,
    Placeholder,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ReviewRowKey {
    pub file_id: String,
    pub hunk_index: Option<usize>,
    pub kind: ReviewRowKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
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
}

impl ReviewRow {
    fn key(&self) -> &ReviewRowKey {
        match self {
            Self::HunkHeader { key, .. }
            | Self::Split { key, .. }
            | Self::Stack { key, .. }
            | Self::Placeholder { key, .. } => key,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowPlan {
    pub rows: Vec<ReviewRow>,
    pub hunk_anchor_keys: Vec<ReviewRowKey>,
    pub line_number_digits: usize,
}

pub(crate) fn build_row_plan(
    file: &DiffFile,
    layout: EffectiveLayout,
    show_hunk_headers: bool,
) -> RowPlan {
    if file.hunks.is_empty() {
        return placeholder_plan(file);
    }

    let mut rows = Vec::new();
    let mut hunk_anchor_keys = Vec::with_capacity(file.hunks.len());
    for (hunk_index, hunk) in file.hunks.iter().enumerate() {
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

        if let Some(anchor) = rows.get(hunk_start).map(ReviewRow::key).cloned() {
            hunk_anchor_keys.push(anchor);
        }
    }

    RowPlan {
        line_number_digits: max_line_number_digits(file),
        rows,
        hunk_anchor_keys,
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
                    ReviewRowKind::SplitLine,
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
                    ReviewRowKind::SplitLine,
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
            ReviewRowKind::StackLine,
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
    }
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

    use super::{
        CellKind, EffectiveLayout, PlaceholderKind, ReviewRow, ReviewRowKind, build_row_plan,
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
        assert_eq!(key.kind, ReviewRowKind::StackLine);
    }
}
