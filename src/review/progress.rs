use std::collections::{HashMap, HashSet};

use crate::diff::model::{DiffFile, LineType};

use super::row::{CellKind, ReviewCell, ReviewRow};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ChangedLineKey {
    file_id: String,
    side: ChangedSide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ChangedSide {
    Old(u32),
    New(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReviewProgressSnapshot {
    pub reviewed: usize,
    pub total: usize,
    pub percent: u8,
}

pub(crate) struct ReviewProgress {
    ordered: Vec<ChangedLineKey>,
    ordinal: HashMap<ChangedLineKey, usize>,
    reviewed: HashSet<ChangedLineKey>,
}

impl ReviewProgress {
    pub(crate) fn new(files: &[DiffFile]) -> Self {
        let mut progress = Self {
            ordered: Vec::new(),
            ordinal: HashMap::new(),
            reviewed: HashSet::new(),
        };
        progress.rebuild_order(files);
        progress
    }

    pub(crate) fn row_keys(file_id: &str, row: &ReviewRow) -> Vec<ChangedLineKey> {
        match row {
            ReviewRow::Stack { cell, .. } => Self::cell_key(file_id, cell).into_iter().collect(),
            ReviewRow::Split { left, right, .. } => [
                Self::cell_key(file_id, left),
                Self::cell_key(file_id, right),
            ]
            .into_iter()
            .flatten()
            .collect(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn observe_through(
        &mut self,
        keys: impl IntoIterator<Item = ChangedLineKey>,
    ) -> bool {
        let Some(maximum) = keys
            .into_iter()
            .filter_map(|key| self.ordinal.get(&key).copied())
            .max()
        else {
            return false;
        };
        let before = self.reviewed.len();
        self.reviewed
            .extend(self.ordered.iter().take(maximum.saturating_add(1)).cloned());
        self.reviewed.len() != before
    }

    pub(crate) fn mark_file_reviewed(&mut self, file_id: &str) -> bool {
        let before = self.reviewed.len();
        self.reviewed.extend(
            self.ordered
                .iter()
                .filter(|key| key.file_id == file_id)
                .cloned(),
        );
        self.reviewed.len() != before
    }

    pub(crate) fn is_file_reviewed(&self, file_id: &str) -> bool {
        self.ordered
            .iter()
            .filter(|key| key.file_id == file_id)
            .all(|key| self.reviewed.contains(key))
    }

    pub(crate) fn replace_files(&mut self, files: &[DiffFile]) {
        self.rebuild_order(files);
        self.reviewed.retain(|key| self.ordinal.contains_key(key));
    }

    pub(crate) fn snapshot(&self) -> ReviewProgressSnapshot {
        let total = self.ordered.len();
        let reviewed = self.reviewed.len().min(total);
        let percent = reviewed
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(100) as u8;
        ReviewProgressSnapshot {
            reviewed,
            total,
            percent,
        }
    }

    fn rebuild_order(&mut self, files: &[DiffFile]) {
        self.ordered = files
            .iter()
            .flat_map(|file| {
                file.hunks.iter().flat_map(|hunk| {
                    hunk.lines.iter().filter_map(|line| {
                        let side = match line.kind {
                            LineType::Addition => ChangedSide::New(line.new_lineno?),
                            LineType::Deletion => ChangedSide::Old(line.old_lineno?),
                            LineType::Context => return None,
                        };
                        Some(ChangedLineKey {
                            file_id: file.id.clone(),
                            side,
                        })
                    })
                })
            })
            .collect();
        self.ordinal = self
            .ordered
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, key)| (key, index))
            .collect();
    }

    fn cell_key(file_id: &str, cell: &ReviewCell) -> Option<ChangedLineKey> {
        let side = match cell.kind {
            CellKind::Addition => ChangedSide::New(cell.new_line?),
            CellKind::Deletion => ChangedSide::Old(cell.old_line?),
            CellKind::Context | CellKind::Empty => return None,
        };
        Some(ChangedLineKey {
            file_id: file_id.to_owned(),
            side,
        })
    }
}
