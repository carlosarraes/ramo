use std::collections::{HashMap, HashSet};

use ramo_core::review_map::{ReviewFileKind, ReviewMap, ReviewMapFailureCode, ReviewMapStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewMapAction {
    Move(i32),
    Collapse,
    Expand,
    ToggleExpanded,
    SetFilter(String),
    OpenSelected,
    Retry,
    DismissFailure,
    OpenHelp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewMapEffect {
    None,
    Redraw,
    OpenFile { file_id: String },
    Retry,
    OpenHelp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewMapFailureNotice {
    pub code: ReviewMapFailureCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewMapRow {
    Group {
        id: String,
        label: String,
        kind: ReviewFileKind,
        additions: usize,
        deletions: usize,
        expanded: bool,
        summary: Option<String>,
        risk: Option<String>,
    },
    File {
        id: String,
        path: String,
        kind: ReviewFileKind,
        additions: usize,
        deletions: usize,
        reviewed: bool,
        recommended_order: Option<usize>,
        summary: Option<String>,
        risk: Option<String>,
    },
}

impl ReviewMapRow {
    pub fn id(&self) -> &str {
        match self {
            Self::Group { id, .. } | Self::File { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewMapSnapshot {
    pub status: ReviewMapStatus,
    pub rows: Vec<ReviewMapRow>,
    pub selected_id: Option<String>,
    pub filter: String,
    pub reviewed_percent: u8,
    pub failure: Option<ReviewMapFailureNotice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceError {
    DifferentRevision,
}

impl std::fmt::Display for ReplaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("review map belongs to a different revision")
    }
}

impl std::error::Error for ReplaceError {}

#[derive(Debug, Clone)]
pub struct ReviewMapController {
    map: ReviewMap,
    selected_id: Option<String>,
    expansion_overrides: HashMap<String, bool>,
    filter: String,
    reviewed_paths: HashSet<String>,
    failure: Option<ReviewMapFailureNotice>,
}

impl ReviewMapController {
    pub fn new(map: ReviewMap) -> Self {
        let mut controller = Self {
            map,
            selected_id: None,
            expansion_overrides: HashMap::new(),
            filter: String::new(),
            reviewed_paths: HashSet::new(),
            failure: None,
        };
        controller.ensure_visible_selection();
        controller
    }

    pub fn apply(&mut self, action: ReviewMapAction) -> ReviewMapEffect {
        match action {
            ReviewMapAction::Move(delta) => {
                self.move_selection(delta);
                ReviewMapEffect::Redraw
            }
            ReviewMapAction::Collapse => self.set_selected_expanded(false),
            ReviewMapAction::Expand => self.set_selected_expanded(true),
            ReviewMapAction::ToggleExpanded => self.toggle_selected_expanded(),
            ReviewMapAction::SetFilter(filter) => {
                self.filter = filter;
                self.ensure_visible_selection();
                ReviewMapEffect::Redraw
            }
            ReviewMapAction::OpenSelected => self
                .selected_file_id()
                .map(|file_id| ReviewMapEffect::OpenFile { file_id })
                .unwrap_or(ReviewMapEffect::None),
            ReviewMapAction::Retry => ReviewMapEffect::Retry,
            ReviewMapAction::DismissFailure => {
                self.failure = None;
                ReviewMapEffect::Redraw
            }
            ReviewMapAction::OpenHelp => ReviewMapEffect::OpenHelp,
        }
    }

    pub fn replace_map(&mut self, map: ReviewMap) -> Result<(), ReplaceError> {
        if self.map.identity.head_sha != map.identity.head_sha
            || self.map.identity.repository != map.identity.repository
            || self.map.identity.pull_request != map.identity.pull_request
        {
            return Err(ReplaceError::DifferentRevision);
        }
        self.map = map;
        self.failure = None;
        self.ensure_visible_selection();
        Ok(())
    }

    pub fn selected_id(&self) -> Option<&str> {
        self.selected_id.as_deref()
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn is_reviewed(&self, path: &str) -> bool {
        self.reviewed_paths.contains(path)
    }

    pub fn mark_reviewed(&mut self, path: &str) {
        if self.map.files.iter().any(|file| file.path == path) {
            self.reviewed_paths.insert(path.to_owned());
        }
    }

    pub fn mark_unreviewed(&mut self, path: &str) {
        self.reviewed_paths.remove(path);
    }

    pub fn set_failure(&mut self, code: ReviewMapFailureCode, message: impl Into<String>) {
        self.failure = Some(ReviewMapFailureNotice {
            code,
            message: message.into(),
        });
    }

    pub fn reviewed_percent(&self) -> u8 {
        let total = self.map.files.len();
        if total == 0 {
            return 100;
        }
        let reviewed = self
            .map
            .files
            .iter()
            .filter(|file| self.reviewed_paths.contains(&file.path))
            .count();
        ((reviewed.saturating_mul(100) / total).min(100)) as u8
    }

    pub fn snapshot(&self) -> ReviewMapSnapshot {
        ReviewMapSnapshot {
            status: self.map.status,
            rows: self.visible_rows(),
            selected_id: self.selected_id.clone(),
            filter: self.filter.clone(),
            reviewed_percent: self.reviewed_percent(),
            failure: self.failure.clone(),
        }
    }

    pub fn map(&self) -> &ReviewMap {
        &self.map
    }

    fn move_selection(&mut self, delta: i32) {
        let rows = self.visible_rows();
        if rows.is_empty() {
            self.selected_id = None;
            return;
        }
        let current = self
            .selected_id
            .as_deref()
            .and_then(|id| rows.iter().position(|row| row.id() == id))
            .unwrap_or_default();
        let next = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            current
                .saturating_add(delta as usize)
                .min(rows.len().saturating_sub(1))
        };
        self.selected_id = Some(rows[next].id().to_owned());
    }

    fn set_selected_expanded(&mut self, expanded: bool) -> ReviewMapEffect {
        let Some(group_id) = self.selected_group_id() else {
            return ReviewMapEffect::None;
        };
        self.expansion_overrides.insert(group_id, expanded);
        self.ensure_visible_selection();
        ReviewMapEffect::Redraw
    }

    fn toggle_selected_expanded(&mut self) -> ReviewMapEffect {
        let Some(group_id) = self.selected_group_id() else {
            return ReviewMapEffect::None;
        };
        let expanded = self.group_expanded(&group_id);
        self.expansion_overrides.insert(group_id, !expanded);
        self.ensure_visible_selection();
        ReviewMapEffect::Redraw
    }

    fn selected_group_id(&self) -> Option<String> {
        let selected = self.selected_id.as_deref()?;
        self.map
            .groups
            .iter()
            .find(|group| group.id == selected)
            .map(|group| group.id.clone())
    }

    fn selected_file_id(&self) -> Option<String> {
        let selected = self.selected_id.as_deref()?;
        self.map
            .files
            .iter()
            .find(|file| file.id == selected)
            .map(|file| file.id.clone())
    }

    fn ensure_visible_selection(&mut self) {
        let rows = self.visible_rows();
        if self
            .selected_id
            .as_deref()
            .is_some_and(|id| rows.iter().any(|row| row.id() == id))
        {
            return;
        }
        self.selected_id = rows.first().map(|row| row.id().to_owned());
    }

    fn group_expanded(&self, group_id: &str) -> bool {
        self.expansion_overrides
            .get(group_id)
            .copied()
            .or_else(|| {
                self.map
                    .groups
                    .iter()
                    .find(|group| group.id == group_id)
                    .map(|group| !group.collapsed_by_default)
            })
            .unwrap_or(false)
    }

    fn visible_rows(&self) -> Vec<ReviewMapRow> {
        let query = self.filter.trim().to_lowercase();
        let by_id = self
            .map
            .files
            .iter()
            .map(|file| (file.id.as_str(), file))
            .collect::<HashMap<_, _>>();
        let mut rows = Vec::new();
        for group in &self.map.groups {
            let group_text = format!(
                "{} {} {}",
                group.label,
                group
                    .insight
                    .as_ref()
                    .map_or("", |insight| insight.summary.as_str()),
                group
                    .insight
                    .as_ref()
                    .and_then(|insight| insight.risk.as_deref())
                    .unwrap_or("")
            )
            .to_lowercase();
            let group_matches = query.is_empty() || group_text.contains(&query);
            let matching_files = group
                .file_ids
                .iter()
                .filter_map(|id| by_id.get(id.as_str()).copied())
                .filter(|file| {
                    group_matches
                        || file.path.to_lowercase().contains(&query)
                        || file.insight.as_ref().is_some_and(|insight| {
                            insight.summary.to_lowercase().contains(&query)
                                || insight
                                    .risk
                                    .as_deref()
                                    .is_some_and(|risk| risk.to_lowercase().contains(&query))
                        })
                })
                .collect::<Vec<_>>();
            if !group_matches && matching_files.is_empty() {
                continue;
            }
            let expanded = self.group_expanded(&group.id);
            rows.push(ReviewMapRow::Group {
                id: group.id.clone(),
                label: group.label.clone(),
                kind: group.kind,
                additions: group.additions,
                deletions: group.deletions,
                expanded,
                summary: group
                    .insight
                    .as_ref()
                    .map(|insight| insight.summary.clone()),
                risk: group
                    .insight
                    .as_ref()
                    .and_then(|insight| insight.risk.clone()),
            });
            if expanded || !query.is_empty() {
                rows.extend(matching_files.into_iter().map(|file| {
                    ReviewMapRow::File {
                        id: file.id.clone(),
                        path: file.path.clone(),
                        kind: file.kind,
                        additions: file.additions,
                        deletions: file.deletions,
                        reviewed: self.reviewed_paths.contains(&file.path),
                        recommended_order: file.recommended_order,
                        summary: file.insight.as_ref().map(|insight| insight.summary.clone()),
                        risk: file
                            .insight
                            .as_ref()
                            .and_then(|insight| insight.risk.clone()),
                    }
                }));
            }
        }
        rows
    }
}
