use std::collections::HashMap;

use unicode_width::UnicodeWidthStr;

use crate::core::input::LayoutMode;
use crate::diff::model::DiffFile;
use crate::input::sanitize_terminal_text;

use super::anchor::{capture_viewport_anchor, restore_viewport_anchor};
use super::context::{
    ContextSourceLoader, FileContextState, GapKey, LoadedContextSource, SourceFailure,
    derive_collapsed_gaps, select_gap_for_toggle, source_for_context,
};
use super::geometry::{
    GeometryOptions, PlannedFile, ReviewGeometry, build_review_geometry, resolve_responsive_layout,
    split_columns, stack_columns,
};
use super::navigation::{signed_offset, wrapping_index};
use super::row::{EffectiveLayout, ReviewRow, ReviewRowKey, build_row_plan_with_context};
use super::selection::{SelectionPoint, SelectionRow, project_selection};

const DEFAULT_SIDEBAR_WIDTH: u16 = 34;
const MIN_SIDEBAR_WIDTH: u16 = 20;
const MIN_CONTENT_WIDTH: u16 = 40;
const SIDEBAR_DIVIDER_WIDTH: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkTarget {
    pub file_id: String,
    pub hunk_index: usize,
}

impl HunkTarget {
    pub fn new(file_id: impl Into<String>, hunk_index: usize) -> Self {
        Self {
            file_id: file_id.into(),
            hunk_index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewOptions {
    pub layout: LayoutMode,
    pub show_sidebar: bool,
    pub line_numbers: bool,
    pub wrap_lines: bool,
    pub hunk_headers: bool,
    pub agent_notes: bool,
    pub pager_mode: bool,
    pub annotated_hunks: Vec<HunkTarget>,
}

impl Default for ReviewOptions {
    fn default() -> Self {
        Self {
            layout: LayoutMode::Auto,
            show_sidebar: true,
            line_numbers: true,
            wrap_lines: false,
            hunk_headers: true,
            agent_notes: false,
            pager_mode: false,
            annotated_hunks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollUnit {
    Step,
    HalfPage,
    Page,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewAction {
    Scroll { delta: i32, unit: ScrollUnit },
    ScrollHorizontal(i32),
    JumpTop,
    JumpBottom,
    MoveHunk(i32),
    MoveFile(i32),
    MoveAnnotatedHunk(i32),
    SelectFile(String),
    SetFilter(String),
    SetLayout(LayoutMode),
    ToggleSidebar,
    ToggleLineNumbers,
    ToggleWrap,
    ToggleHunkHeaders,
    ToggleAgentNotes,
    FocusFilter,
    OpenHelp,
    OpenThemeSelector,
    StartNote,
    EditSelectedFile,
    Reload,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewEffect {
    None,
    Redraw,
    FocusFilter,
    OpenHelp,
    OpenThemeSelector,
    StartNote,
    EditFile { path: String, line: Option<u32> },
    Reload,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Binary,
    TooLarge,
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewFileSnapshot {
    pub id: String,
    pub path: String,
    pub previous_path: Option<String>,
    pub additions: usize,
    pub deletions: usize,
    pub stats_truncated: bool,
    pub status: ReviewFileStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarEntrySnapshot {
    Group {
        id: String,
        label: String,
    },
    File {
        id: String,
        name: String,
        annotations_text: Option<String>,
        additions_text: Option<String>,
        deletions_text: Option<String>,
        status: ReviewFileStatus,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewPosition {
    pub file_id: String,
    pub hunk_index: Option<usize>,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewPoint {
    pub x: u16,
    pub y: u16,
}

impl ReviewPoint {
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewHit {
    SidebarFile(String),
    SidebarDivider,
    Scrollbar,
    Collapsed(GapKey),
    Diff(SelectionPoint),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSnapshot {
    pub visible_files: Vec<ReviewFileSnapshot>,
    pub sidebar_entries: Vec<SidebarEntrySnapshot>,
    pub selected_file_id: Option<String>,
    pub selected_hunk_index: Option<usize>,
    pub selected_position: Option<ReviewPosition>,
    pub layout: LayoutMode,
    pub show_sidebar: bool,
    pub sidebar_width: u16,
    pub line_numbers: bool,
    pub wrap_lines: bool,
    pub hunk_headers: bool,
    pub agent_notes: bool,
    pub pager_mode: bool,
    pub filter: String,
    pub scroll_top: usize,
    pub total_height: usize,
    pub max_scroll_top: usize,
    pub horizontal_offset: usize,
}

pub struct ReviewController {
    files: Vec<DiffFile>,
    options: ReviewOptions,
    filter: String,
    visible_indices: Vec<usize>,
    selected_file_id: Option<String>,
    selected_hunk_index: Option<usize>,
    selected_row_key: Option<ReviewRowKey>,
    scroll_top: usize,
    horizontal_offset: usize,
    sidebar_override: Option<bool>,
    sidebar_width: u16,
    geometry: Option<ReviewGeometry>,
    planned_files: Vec<PlannedFile>,
    effective_layout: EffectiveLayout,
    actual_sidebar: bool,
    last_viewport: Option<Viewport>,
    dirty: bool,
    snapshot: ReviewSnapshot,
    contexts: HashMap<String, FileContextState>,
}

pub(crate) struct ReviewRenderView<'a> {
    pub files: &'a [DiffFile],
    pub visible_indices: &'a [usize],
    pub planned_files: &'a [PlannedFile],
    pub geometry: &'a ReviewGeometry,
    pub snapshot: &'a ReviewSnapshot,
}

impl ReviewController {
    pub fn new(files: Vec<DiffFile>, options: ReviewOptions) -> Self {
        let selected_file_id = files.first().map(|file| file.id.clone());
        Self {
            files,
            options,
            filter: String::new(),
            visible_indices: Vec::new(),
            selected_file_id,
            selected_hunk_index: Some(0),
            selected_row_key: None,
            scroll_top: 0,
            horizontal_offset: 0,
            sidebar_override: None,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            geometry: None,
            planned_files: Vec::new(),
            effective_layout: EffectiveLayout::Stack,
            actual_sidebar: false,
            last_viewport: None,
            dirty: true,
            snapshot: empty_snapshot(),
            contexts: HashMap::new(),
        }
    }

    pub fn snapshot(&mut self, viewport: Viewport) -> &ReviewSnapshot {
        self.ensure_geometry(viewport);
        &self.snapshot
    }

    pub(crate) fn render_view(&mut self, viewport: Viewport) -> ReviewRenderView<'_> {
        self.ensure_geometry(viewport);
        ReviewRenderView {
            files: &self.files,
            visible_indices: &self.visible_indices,
            planned_files: &self.planned_files,
            geometry: self
                .geometry
                .as_ref()
                .expect("geometry exists after ensure_geometry"),
            snapshot: &self.snapshot,
        }
    }

    pub fn hit_test(&mut self, point: ReviewPoint, viewport: Viewport) -> Option<ReviewHit> {
        self.ensure_geometry(viewport);
        if point.x >= viewport.width || point.y >= viewport.height {
            return None;
        }
        if point.x == viewport.width.saturating_sub(1)
            && self.snapshot.total_height > usize::from(viewport.height)
        {
            return Some(ReviewHit::Scrollbar);
        }
        let content_x = if self.actual_sidebar {
            if point.x < self.sidebar_width {
                return self
                    .snapshot
                    .sidebar_entries
                    .get(usize::from(point.y))
                    .and_then(|entry| match entry {
                        SidebarEntrySnapshot::File { id, .. } => {
                            Some(ReviewHit::SidebarFile(id.clone()))
                        }
                        SidebarEntrySnapshot::Group { .. } => None,
                    });
            }
            if point.x == self.sidebar_width {
                return Some(ReviewHit::SidebarDivider);
            }
            self.sidebar_width.saturating_add(SIDEBAR_DIVIDER_WIDTH)
        } else {
            0
        };
        let relative_x = usize::from(point.x.saturating_sub(content_x));
        let absolute_y = self.scroll_top.saturating_add(usize::from(point.y));
        let geometry = self.geometry.as_ref()?;
        let row_index = geometry
            .rows
            .partition_point(|row| row.top.saturating_add(row.height) <= absolute_y);
        let bound = geometry.rows.get(row_index)?;
        if absolute_y < bound.top || absolute_y >= bound.top.saturating_add(bound.height) {
            return None;
        }
        let row = self
            .planned_files
            .get(bound.file_index)?
            .plan
            .rows
            .get(bound.row_index)?;
        match row {
            ReviewRow::Collapsed { gap, .. } => Some(ReviewHit::Collapsed(gap.key.clone())),
            ReviewRow::Stack { .. } => {
                let columns = stack_columns(
                    self.content_width(viewport),
                    self.planned_files[bound.file_index].plan.line_number_digits,
                    self.options.line_numbers,
                );
                let local = relative_x.checked_sub(columns.text_cell)?;
                if local >= columns.code_width {
                    return None;
                }
                let wrap_line = absolute_y.saturating_sub(bound.top);
                let offset = if self.options.wrap_lines {
                    wrap_line.saturating_mul(columns.code_width)
                } else {
                    self.horizontal_offset
                };
                Some(ReviewHit::Diff(SelectionPoint::new(
                    row_index,
                    columns
                        .text_cell
                        .saturating_add(offset)
                        .saturating_add(local),
                )))
            }
            ReviewRow::Split { .. } => {
                let columns = split_columns(
                    self.content_width(viewport),
                    self.planned_files[bound.file_index].plan.line_number_digits,
                    self.options.line_numbers,
                );
                let (text_cell, code_width) = if relative_x < columns.divider_cell {
                    (columns.left_text_cell, columns.left_code_width)
                } else if relative_x > columns.divider_cell {
                    (columns.right_text_cell, columns.right_code_width)
                } else {
                    return None;
                };
                let local = relative_x.checked_sub(text_cell)?;
                if local >= code_width {
                    return None;
                }
                let wrap_line = absolute_y.saturating_sub(bound.top);
                let offset = if self.options.wrap_lines {
                    wrap_line.saturating_mul(code_width)
                } else {
                    self.horizontal_offset
                };
                Some(ReviewHit::Diff(SelectionPoint::new(
                    row_index,
                    text_cell.saturating_add(offset).saturating_add(local),
                )))
            }
            ReviewRow::HunkHeader { .. } | ReviewRow::Placeholder { .. } => None,
        }
    }

    pub fn selection_text(
        &mut self,
        anchor: SelectionPoint,
        focus: SelectionPoint,
        viewport: Viewport,
    ) -> String {
        self.ensure_geometry(viewport);
        project_selection(&self.selection_rows(viewport), anchor, focus)
    }

    pub fn selected_line_range(
        &mut self,
        viewport: Viewport,
    ) -> Option<(SelectionPoint, SelectionPoint)> {
        self.ensure_geometry(viewport);
        let geometry = self.geometry.as_ref()?;
        let selected = self.selected_row_key.as_ref()?;
        let selected_index = geometry
            .rows
            .iter()
            .position(|bound| &bound.key == selected)?;
        geometry.rows[selected_index..]
            .iter()
            .enumerate()
            .find_map(|(offset, bound)| {
                if bound.key.file_id != selected.file_id || bound.hunk_index != selected.hunk_index
                {
                    return None;
                }
                let plan = &self.planned_files[bound.file_index].plan;
                let row_index = selected_index.saturating_add(offset);
                match &plan.rows[bound.row_index] {
                    ReviewRow::Stack { cell, .. } => {
                        let columns = stack_columns(
                            self.content_width(viewport),
                            plan.line_number_digits,
                            self.options.line_numbers,
                        );
                        let end = columns
                            .text_cell
                            .saturating_add(UnicodeWidthStr::width(cell.text().as_str()));
                        Some((
                            SelectionPoint::new(row_index, columns.text_cell),
                            SelectionPoint::new(row_index, end),
                        ))
                    }
                    ReviewRow::Split { left, right, .. } => {
                        let columns = split_columns(
                            self.content_width(viewport),
                            plan.line_number_digits,
                            self.options.line_numbers,
                        );
                        let (text_cell, text) = if right.text().is_empty() {
                            (columns.left_text_cell, left.text())
                        } else {
                            (columns.right_text_cell, right.text())
                        };
                        Some((
                            SelectionPoint::new(row_index, text_cell),
                            SelectionPoint::new(
                                row_index,
                                text_cell.saturating_add(UnicodeWidthStr::width(text.as_str())),
                            ),
                        ))
                    }
                    ReviewRow::HunkHeader { .. }
                    | ReviewRow::Collapsed { .. }
                    | ReviewRow::Placeholder { .. } => None,
                }
            })
    }

    pub fn resize_sidebar(&mut self, width: u16, viewport: Viewport) {
        let maximum = viewport
            .width
            .saturating_sub(MIN_CONTENT_WIDTH + SIDEBAR_DIVIDER_WIDTH)
            .max(MIN_SIDEBAR_WIDTH);
        let width = width.clamp(MIN_SIDEBAR_WIDTH, maximum);
        if self.sidebar_width != width {
            self.sidebar_width = width;
            self.dirty = true;
            self.rebuild(viewport, true);
        }
    }

    pub fn scroll_to_mouse_row(&mut self, row: u16, viewport: Viewport) {
        self.ensure_geometry(viewport);
        let denominator = usize::from(viewport.height.saturating_sub(1)).max(1);
        self.scroll_top = self
            .max_scroll_top()
            .saturating_mul(usize::from(row.min(viewport.height.saturating_sub(1))))
            / denominator;
        self.select_from_viewport(viewport);
        self.refresh_snapshot();
    }

    pub fn toggle_context(
        &mut self,
        loader: &mut dyn ContextSourceLoader,
        viewport: Viewport,
    ) -> Result<bool, SourceFailure> {
        self.toggle_context_target(None, loader, viewport)
    }

    pub fn toggle_context_gap(
        &mut self,
        gap: &GapKey,
        loader: &mut dyn ContextSourceLoader,
        viewport: Viewport,
    ) -> Result<bool, SourceFailure> {
        self.toggle_context_target(Some(gap.clone()), loader, viewport)
    }

    fn toggle_context_target(
        &mut self,
        requested: Option<GapKey>,
        loader: &mut dyn ContextSourceLoader,
        viewport: Viewport,
    ) -> Result<bool, SourceFailure> {
        self.ensure_geometry(viewport);
        let file_id = requested
            .as_ref()
            .map(|gap| gap.file_id.clone())
            .or_else(|| self.selected_file_id.clone())
            .ok_or(SourceFailure::Unavailable)?;
        let file = self
            .files
            .iter()
            .find(|file| file.id == file_id)
            .cloned()
            .ok_or(SourceFailure::Unavailable)?;
        let selected_hunk = requested.as_ref().map_or_else(
            || self.selected_hunk_index.unwrap_or(0),
            |gap| gap.hunk_index,
        );
        let initial_gaps = self.contexts.get(&file_id).map_or_else(
            || derive_collapsed_gaps(&file, None),
            |context| context.gaps(&file),
        );
        let mut target =
            requested.filter(|requested| initial_gaps.iter().any(|gap| gap.key == *requested));
        if target.is_none() {
            target = select_gap_for_toggle(&initial_gaps, selected_hunk).cloned();
        }

        if let Some(target) = &target
            && self
                .contexts
                .get(&file_id)
                .is_some_and(|context| context.expanded.contains(target))
        {
            self.contexts
                .entry(file_id)
                .or_default()
                .expanded
                .remove(target);
            self.dirty = true;
            self.rebuild(viewport, true);
            return Ok(false);
        }

        let needs_source = self
            .contexts
            .get(&file_id)
            .is_none_or(|context| context.source.is_none());
        if needs_source {
            let (side, spec) = source_for_context(&file);
            let result = if *spec == crate::diff::model::SourceSpec::None {
                Err(SourceFailure::Unavailable)
            } else {
                loader
                    .load(spec)
                    .and_then(|text| text.ok_or(SourceFailure::Missing))
            };
            self.contexts.entry(file_id.clone()).or_default().source =
                Some(result.map(|text| LoadedContextSource { side, text }));
        }

        let context = self.contexts.entry(file_id.clone()).or_default();
        if target.is_none() {
            let gaps = context.gaps(&file);
            target = select_gap_for_toggle(&gaps, selected_hunk).cloned();
        }
        let Some(target) = target else {
            return context
                .source
                .as_ref()
                .and_then(|source| source.as_ref().err())
                .cloned()
                .map_or(Ok(false), Err);
        };
        context.expanded.insert(target);
        let failure = context
            .source
            .as_ref()
            .and_then(|source| source.as_ref().err())
            .cloned();
        self.dirty = true;
        self.rebuild(viewport, true);
        failure.map_or(Ok(true), Err)
    }

    pub fn apply(&mut self, action: ReviewAction, viewport: Viewport) -> ReviewEffect {
        self.ensure_geometry(viewport);
        if self.options.pager_mode && application_only(&action) {
            return ReviewEffect::None;
        }

        match action {
            ReviewAction::Scroll { delta, unit } => {
                let amount = match unit {
                    ScrollUnit::Step => 1,
                    ScrollUnit::HalfPage => (usize::from(viewport.height) / 2).max(1),
                    ScrollUnit::Page => usize::from(viewport.height).max(1),
                };
                self.scroll_top =
                    signed_offset(self.scroll_top, delta, amount).min(self.max_scroll_top());
                self.select_from_viewport(viewport);
                self.refresh_snapshot();
                ReviewEffect::Redraw
            }
            ReviewAction::ScrollHorizontal(delta) => {
                self.horizontal_offset = signed_offset(self.horizontal_offset, delta, 1)
                    .min(self.max_horizontal_offset(viewport));
                self.refresh_snapshot();
                ReviewEffect::Redraw
            }
            ReviewAction::JumpTop => {
                self.scroll_top = 0;
                self.select_from_viewport(viewport);
                self.refresh_snapshot();
                ReviewEffect::Redraw
            }
            ReviewAction::JumpBottom => {
                self.scroll_top = self.max_scroll_top();
                self.select_from_viewport(viewport);
                self.refresh_snapshot();
                ReviewEffect::Redraw
            }
            ReviewAction::MoveHunk(delta) => {
                self.move_hunk(delta, viewport);
                ReviewEffect::Redraw
            }
            ReviewAction::MoveFile(delta) => {
                self.move_file(delta, viewport);
                ReviewEffect::Redraw
            }
            ReviewAction::MoveAnnotatedHunk(delta) => {
                self.move_annotated_hunk(delta, viewport);
                ReviewEffect::Redraw
            }
            ReviewAction::SelectFile(file_id) => {
                if self.visible_file_ids().any(|visible| visible == file_id) {
                    self.select_target(file_id, 0, viewport);
                }
                ReviewEffect::Redraw
            }
            ReviewAction::SetFilter(filter) => {
                self.filter = filter;
                self.dirty = true;
                self.rebuild(viewport, true);
                ReviewEffect::Redraw
            }
            ReviewAction::SetLayout(layout) => {
                self.options.layout = layout;
                self.dirty = true;
                self.rebuild(viewport, true);
                ReviewEffect::Redraw
            }
            ReviewAction::ToggleSidebar => {
                self.sidebar_override = Some(!self.actual_sidebar);
                self.dirty = true;
                self.rebuild(viewport, true);
                ReviewEffect::Redraw
            }
            ReviewAction::ToggleLineNumbers => {
                self.options.line_numbers = !self.options.line_numbers;
                self.dirty = true;
                self.rebuild(viewport, true);
                ReviewEffect::Redraw
            }
            ReviewAction::ToggleWrap => {
                self.options.wrap_lines = !self.options.wrap_lines;
                if self.options.wrap_lines {
                    self.horizontal_offset = 0;
                }
                self.dirty = true;
                self.rebuild(viewport, true);
                ReviewEffect::Redraw
            }
            ReviewAction::ToggleHunkHeaders => {
                self.options.hunk_headers = !self.options.hunk_headers;
                self.dirty = true;
                self.rebuild(viewport, true);
                ReviewEffect::Redraw
            }
            ReviewAction::ToggleAgentNotes => {
                self.options.agent_notes = !self.options.agent_notes;
                self.refresh_snapshot();
                ReviewEffect::Redraw
            }
            ReviewAction::FocusFilter => ReviewEffect::FocusFilter,
            ReviewAction::OpenHelp => ReviewEffect::OpenHelp,
            ReviewAction::OpenThemeSelector => ReviewEffect::OpenThemeSelector,
            ReviewAction::StartNote => ReviewEffect::StartNote,
            ReviewAction::EditSelectedFile => self.edit_effect(),
            ReviewAction::Reload => ReviewEffect::Reload,
            ReviewAction::Quit => ReviewEffect::Quit,
        }
    }

    fn ensure_geometry(&mut self, viewport: Viewport) {
        if self.dirty || self.last_viewport != Some(viewport) {
            self.rebuild(viewport, true);
        }
    }

    fn rebuild(&mut self, viewport: Viewport, preserve_anchor: bool) {
        let old_anchor = preserve_anchor.then(|| {
            self.geometry.as_ref().map(|geometry| {
                capture_viewport_anchor(
                    geometry,
                    self.scroll_top,
                    self.selected_file_id.as_deref(),
                    self.selected_hunk_index,
                )
            })
        });
        let previous_selection_visible = self
            .selected_file_id
            .as_deref()
            .is_some_and(|id| self.matches_filter_id(id));

        self.visible_indices = self
            .files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| matches_filter(file, &self.filter).then_some(index))
            .collect();
        if !previous_selection_visible {
            self.selected_file_id = self
                .visible_indices
                .first()
                .map(|index| self.files[*index].id.clone());
            self.selected_hunk_index = self.selected_file_id.as_ref().map(|_| 0);
            self.selected_row_key = None;
        }

        let responsive = resolve_responsive_layout(
            self.options.layout,
            viewport.width,
            self.options.show_sidebar,
        );
        self.effective_layout = responsive.layout;
        self.actual_sidebar = self.sidebar_override.unwrap_or(responsive.show_sidebar);
        let content_width = if self.actual_sidebar {
            viewport
                .width
                .saturating_sub(self.sidebar_width + SIDEBAR_DIVIDER_WIDTH)
        } else {
            viewport.width
        };
        self.planned_files = self
            .visible_indices
            .iter()
            .map(|index| {
                let file = &self.files[*index];
                PlannedFile::new(
                    file.id.clone(),
                    build_row_plan_with_context(
                        file,
                        self.effective_layout,
                        self.options.hunk_headers,
                        self.contexts.get(&file.id),
                    ),
                )
            })
            .collect();
        let geometry = build_review_geometry(
            &self.planned_files,
            GeometryOptions {
                content_width,
                viewport_height: viewport.height,
                show_line_numbers: self.options.line_numbers,
                wrap_lines: self.options.wrap_lines,
            },
        );

        if previous_selection_visible {
            if let Some(anchor) = old_anchor.flatten() {
                self.scroll_top = restore_viewport_anchor(&geometry, &anchor);
            }
        } else if let Some(file_id) = self.selected_file_id.as_deref() {
            self.scroll_top = geometry
                .file_section(file_id)
                .map_or(0, |section| section.body_top);
        } else {
            self.scroll_top = 0;
        }
        self.scroll_top = self.scroll_top.min(geometry.max_scroll_top());

        self.selected_row_key = self
            .selected_row_key
            .as_ref()
            .and_then(|key| geometry.row_by_key(key))
            .map(|row| row.key.clone())
            .or_else(|| {
                Some((self.selected_file_id.as_deref()?, self.selected_hunk_index?))
                    .and_then(|(file_id, hunk)| geometry.hunk_anchor(file_id, hunk))
                    .map(|row| row.key.clone())
            })
            .or_else(|| {
                geometry
                    .row_at_offset(self.scroll_top)
                    .map(|row| row.key.clone())
            });
        self.geometry = Some(geometry);
        self.horizontal_offset = self
            .horizontal_offset
            .min(self.max_horizontal_offset(viewport));
        self.last_viewport = Some(viewport);
        self.dirty = false;
        self.refresh_snapshot();
    }

    fn move_file(&mut self, delta: i32, viewport: Viewport) {
        let ids = self.visible_file_ids().collect::<Vec<_>>();
        let current = ids
            .iter()
            .position(|id| Some(id.as_str()) == self.selected_file_id.as_deref())
            .unwrap_or(0);
        if let Some(next) = wrapping_index(current, ids.len(), delta) {
            self.select_target(ids[next].clone(), 0, viewport);
        }
    }

    fn move_hunk(&mut self, delta: i32, viewport: Viewport) {
        let targets = self.visible_hunk_targets();
        let current = targets
            .iter()
            .position(|target| {
                Some(target.file_id.as_str()) == self.selected_file_id.as_deref()
                    && Some(target.hunk_index) == self.selected_hunk_index
            })
            .unwrap_or(0);
        if let Some(next) = wrapping_index(current, targets.len(), delta) {
            let target = targets[next].clone();
            self.select_target(target.file_id, target.hunk_index, viewport);
        }
    }

    fn move_annotated_hunk(&mut self, delta: i32, viewport: Viewport) {
        let targets = self
            .options
            .annotated_hunks
            .iter()
            .filter(|target| {
                self.visible_indices.iter().any(|index| {
                    self.files[*index].id == target.file_id
                        && target.hunk_index < self.files[*index].hunks.len()
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        let current = targets
            .iter()
            .position(|target| {
                Some(target.file_id.as_str()) == self.selected_file_id.as_deref()
                    && Some(target.hunk_index) == self.selected_hunk_index
            })
            .unwrap_or_else(|| {
                if delta.is_negative() {
                    0
                } else {
                    targets.len().saturating_sub(1)
                }
            });
        if let Some(next) = wrapping_index(current, targets.len(), delta) {
            let target = targets[next].clone();
            self.select_target(target.file_id, target.hunk_index, viewport);
        }
    }

    fn select_target(&mut self, file_id: String, hunk_index: usize, viewport: Viewport) {
        self.selected_file_id = Some(file_id);
        self.selected_hunk_index = Some(hunk_index);
        self.selected_row_key = None;
        self.dirty = true;
        self.rebuild(viewport, false);
        if let Some(geometry) = &self.geometry
            && let Some(row) = geometry.hunk_anchor(
                self.selected_file_id.as_deref().unwrap_or_default(),
                hunk_index,
            )
        {
            self.selected_row_key = Some(row.key.clone());
            self.scroll_top = row.top.min(geometry.max_scroll_top());
        }
        self.refresh_snapshot();
    }

    fn select_from_viewport(&mut self, viewport: Viewport) {
        let Some(geometry) = &self.geometry else {
            return;
        };
        let probe = self
            .scroll_top
            .saturating_add(usize::from(viewport.height) / 2)
            .min(geometry.total_height.saturating_sub(1));
        let Some(row) = geometry.row_at_offset(probe) else {
            self.selected_file_id = None;
            self.selected_hunk_index = None;
            self.selected_row_key = None;
            return;
        };
        self.selected_row_key = Some(row.key.clone());
        self.selected_file_id = geometry
            .sections
            .get(row.file_index)
            .map(|section| section.file_id.clone());
        self.selected_hunk_index = row.hunk_index;
    }

    fn visible_hunk_targets(&self) -> Vec<HunkTarget> {
        self.visible_indices
            .iter()
            .flat_map(|index| {
                let file = &self.files[*index];
                (0..file.hunks.len()).map(|hunk_index| HunkTarget::new(&file.id, hunk_index))
            })
            .collect()
    }

    fn visible_file_ids(&self) -> impl Iterator<Item = String> + '_ {
        self.visible_indices
            .iter()
            .map(|index| self.files[*index].id.clone())
    }

    fn matches_filter_id(&self, id: &str) -> bool {
        self.files
            .iter()
            .find(|file| file.id == id)
            .is_some_and(|file| matches_filter(file, &self.filter))
    }

    fn max_scroll_top(&self) -> usize {
        self.geometry
            .as_ref()
            .map_or(0, ReviewGeometry::max_scroll_top)
    }

    fn content_width(&self, viewport: Viewport) -> u16 {
        if self.actual_sidebar {
            viewport
                .width
                .saturating_sub(self.sidebar_width + SIDEBAR_DIVIDER_WIDTH)
        } else {
            viewport.width
        }
    }

    fn selection_rows(&self, viewport: Viewport) -> Vec<SelectionRow> {
        let content_width = self.content_width(viewport);
        self.geometry
            .as_ref()
            .into_iter()
            .flat_map(|geometry| &geometry.rows)
            .map(|bound| {
                let plan = &self.planned_files[bound.file_index].plan;
                match &plan.rows[bound.row_index] {
                    ReviewRow::Stack { cell, .. } => {
                        let columns = stack_columns(
                            content_width,
                            plan.line_number_digits,
                            self.options.line_numbers,
                        );
                        SelectionRow::stack_at(cell.text(), columns.text_cell)
                    }
                    ReviewRow::Split { left, right, .. } => {
                        let columns = split_columns(
                            content_width,
                            plan.line_number_digits,
                            self.options.line_numbers,
                        );
                        SelectionRow::split_at(
                            left.text(),
                            right.text(),
                            columns.divider_cell,
                            columns.left_text_cell,
                            columns.right_text_cell,
                        )
                    }
                    ReviewRow::HunkHeader { text, .. }
                    | ReviewRow::Placeholder { text, .. }
                    | ReviewRow::Collapsed { text, .. } => SelectionRow::stack(text),
                }
            })
            .collect()
    }

    fn max_horizontal_offset(&self, viewport: Viewport) -> usize {
        if self.options.wrap_lines {
            return 0;
        }
        let content_width = self.content_width(viewport);
        self.planned_files
            .iter()
            .flat_map(|planned| {
                let digits = planned.plan.line_number_digits;
                planned.plan.rows.iter().map(move |row| match row {
                    ReviewRow::Stack { cell, .. } => {
                        let columns =
                            stack_columns(content_width, digits, self.options.line_numbers);
                        UnicodeWidthStr::width(cell.text().as_str())
                            .saturating_sub(columns.code_width)
                    }
                    ReviewRow::Split { left, right, .. } => {
                        let columns =
                            split_columns(content_width, digits, self.options.line_numbers);
                        UnicodeWidthStr::width(left.text().as_str())
                            .saturating_sub(columns.left_code_width)
                            .max(
                                UnicodeWidthStr::width(right.text().as_str())
                                    .saturating_sub(columns.right_code_width),
                            )
                    }
                    ReviewRow::HunkHeader { .. }
                    | ReviewRow::Collapsed { .. }
                    | ReviewRow::Placeholder { .. } => 0,
                })
            })
            .max()
            .unwrap_or(0)
    }

    fn edit_effect(&self) -> ReviewEffect {
        let Some(file_id) = self.selected_file_id.as_deref() else {
            return ReviewEffect::None;
        };
        let Some(file) = self.files.iter().find(|file| file.id == file_id) else {
            return ReviewEffect::None;
        };
        ReviewEffect::EditFile {
            path: file.path.clone(),
            line: self
                .selected_row_key
                .as_ref()
                .and_then(|key| key.new_line.or(key.old_line)),
        }
    }

    fn refresh_snapshot(&mut self) {
        let geometry = self.geometry.as_ref();
        let visible_files = self
            .visible_indices
            .iter()
            .map(|index| {
                let file = &self.files[*index];
                ReviewFileSnapshot {
                    id: file.id.clone(),
                    path: file.path.clone(),
                    previous_path: file.previous_path.clone(),
                    additions: file.stats.additions,
                    deletions: file.stats.deletions,
                    stats_truncated: file.stats_truncated,
                    status: file_status(file),
                }
            })
            .collect();
        let sidebar_entries = build_sidebar_entries(
            self.visible_indices.iter().map(|index| &self.files[*index]),
            &self.options.annotated_hunks,
        );
        self.snapshot = ReviewSnapshot {
            visible_files,
            sidebar_entries,
            selected_file_id: self.selected_file_id.clone(),
            selected_hunk_index: self.selected_hunk_index,
            selected_position: self.selected_row_key.as_ref().map(|key| ReviewPosition {
                file_id: key.file_id.clone(),
                hunk_index: key.hunk_index,
                old_line: key.old_line,
                new_line: key.new_line,
            }),
            layout: match self.effective_layout {
                EffectiveLayout::Split => LayoutMode::Split,
                EffectiveLayout::Stack => LayoutMode::Stack,
            },
            show_sidebar: self.actual_sidebar,
            sidebar_width: self.sidebar_width,
            line_numbers: self.options.line_numbers,
            wrap_lines: self.options.wrap_lines,
            hunk_headers: self.options.hunk_headers,
            agent_notes: self.options.agent_notes,
            pager_mode: self.options.pager_mode,
            filter: self.filter.clone(),
            scroll_top: self.scroll_top,
            total_height: geometry.map_or(0, |geometry| geometry.total_height),
            max_scroll_top: geometry.map_or(0, ReviewGeometry::max_scroll_top),
            horizontal_offset: self.horizontal_offset,
        };
    }
}

fn matches_filter(file: &DiffFile, filter: &str) -> bool {
    let filter = filter.trim().to_lowercase();
    filter.is_empty()
        || [
            Some(file.path.as_str()),
            file.previous_path.as_deref(),
            file.summary.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|value| value.to_lowercase().contains(&filter))
}

fn build_sidebar_entries<'a>(
    files: impl Iterator<Item = &'a DiffFile>,
    annotated_hunks: &[HunkTarget],
) -> Vec<SidebarEntrySnapshot> {
    let mut entries = Vec::new();
    let mut active_group = None::<String>;
    for (index, file) in files.enumerate() {
        let path = sanitized_path(&file.path);
        let group = posix_parent(&path);
        if group != active_group.as_deref() {
            active_group = group.map(str::to_owned);
            if let Some(group) = &active_group {
                entries.push(SidebarEntrySnapshot::Group {
                    id: format!("group:{group}:{index}"),
                    label: format!("{group}/"),
                });
            }
        }

        let annotation_count = annotated_hunks
            .iter()
            .filter(|target| target.file_id == file.id)
            .count();
        entries.push(SidebarEntrySnapshot::File {
            id: file.id.clone(),
            name: sidebar_file_name(file, &path),
            annotations_text: (annotation_count > 0).then(|| format!("*{annotation_count}")),
            additions_text: format_sidebar_stat('+', file.stats.additions, file.stats_truncated),
            deletions_text: format_sidebar_stat('-', file.stats.deletions, false),
            status: file_status(file),
        });
    }
    entries
}

fn sanitized_path(path: &str) -> String {
    sanitize_terminal_text(path, false).replace('\\', "/")
}

fn posix_parent(path: &str) -> Option<&str> {
    path.rsplit_once('/')
        .map(|(parent, _)| parent)
        .filter(|parent| !parent.is_empty() && *parent != ".")
}

fn posix_basename(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

fn sidebar_file_name(file: &DiffFile, path: &str) -> String {
    let name = posix_basename(path);
    let Some(previous_path) = file.previous_path.as_deref() else {
        return name.to_owned();
    };
    let previous_path = sanitized_path(previous_path);
    let previous_name = posix_basename(&previous_path);
    if previous_name == name {
        name.to_owned()
    } else {
        format!("{previous_name} -> {name}")
    }
}

fn format_sidebar_stat(prefix: char, value: usize, truncated: bool) -> Option<String> {
    (value > 0).then(|| format!("{prefix}{value}{}", if truncated { "+" } else { "" }))
}

fn file_status(file: &DiffFile) -> ReviewFileStatus {
    use crate::diff::model::FileChangeKind;

    if file.is_too_large {
        ReviewFileStatus::TooLarge
    } else if file.is_binary {
        ReviewFileStatus::Binary
    } else if file.is_untracked {
        ReviewFileStatus::Untracked
    } else {
        match file.change_kind {
            FileChangeKind::Modified => ReviewFileStatus::Modified,
            FileChangeKind::Added => ReviewFileStatus::Added,
            FileChangeKind::Deleted => ReviewFileStatus::Deleted,
            FileChangeKind::Renamed => ReviewFileStatus::Renamed,
            FileChangeKind::Copied => ReviewFileStatus::Copied,
        }
    }
}

fn application_only(action: &ReviewAction) -> bool {
    matches!(
        action,
        ReviewAction::FocusFilter
            | ReviewAction::OpenHelp
            | ReviewAction::OpenThemeSelector
            | ReviewAction::StartNote
            | ReviewAction::EditSelectedFile
            | ReviewAction::Reload
            | ReviewAction::SetFilter(_)
            | ReviewAction::SetLayout(_)
            | ReviewAction::ToggleLineNumbers
            | ReviewAction::ToggleHunkHeaders
            | ReviewAction::ToggleAgentNotes
            | ReviewAction::MoveHunk(_)
            | ReviewAction::MoveFile(_)
            | ReviewAction::MoveAnnotatedHunk(_)
            | ReviewAction::SelectFile(_)
    )
}

fn empty_snapshot() -> ReviewSnapshot {
    ReviewSnapshot {
        visible_files: Vec::new(),
        sidebar_entries: Vec::new(),
        selected_file_id: None,
        selected_hunk_index: None,
        selected_position: None,
        layout: LayoutMode::Stack,
        show_sidebar: false,
        sidebar_width: DEFAULT_SIDEBAR_WIDTH,
        line_numbers: true,
        wrap_lines: false,
        hunk_headers: true,
        agent_notes: false,
        pager_mode: false,
        filter: String::new(),
        scroll_top: 0,
        total_height: 0,
        max_scroll_top: 0,
        horizontal_offset: 0,
    }
}
