use unicode_width::UnicodeWidthStr;

use crate::core::input::LayoutMode;
use crate::diff::model::DiffFile;
use crate::input::sanitize_terminal_text;

use super::anchor::{capture_viewport_anchor, restore_viewport_anchor};
use super::geometry::{
    GeometryOptions, PlannedFile, ReviewGeometry, build_review_geometry, resolve_responsive_layout,
};
use super::navigation::{signed_offset, wrapping_index};
use super::row::{EffectiveLayout, ReviewRowKey, build_row_plan};

const SIDEBAR_WIDTH: u16 = 34;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSnapshot {
    pub visible_files: Vec<ReviewFileSnapshot>,
    pub sidebar_entries: Vec<SidebarEntrySnapshot>,
    pub selected_file_id: Option<String>,
    pub selected_hunk_index: Option<usize>,
    pub selected_position: Option<ReviewPosition>,
    pub layout: LayoutMode,
    pub show_sidebar: bool,
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
    geometry: Option<ReviewGeometry>,
    planned_files: Vec<PlannedFile>,
    effective_layout: EffectiveLayout,
    actual_sidebar: bool,
    last_viewport: Option<Viewport>,
    dirty: bool,
    snapshot: ReviewSnapshot,
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
            geometry: None,
            planned_files: Vec::new(),
            effective_layout: EffectiveLayout::Stack,
            actual_sidebar: false,
            last_viewport: None,
            dirty: true,
            snapshot: empty_snapshot(),
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
                .saturating_sub(SIDEBAR_WIDTH + SIDEBAR_DIVIDER_WIDTH)
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
                    build_row_plan(file, self.effective_layout, self.options.hunk_headers),
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

    fn max_horizontal_offset(&self, viewport: Viewport) -> usize {
        if self.options.wrap_lines {
            return 0;
        }
        let widest = self
            .visible_indices
            .iter()
            .flat_map(|index| &self.files[*index].hunks)
            .flat_map(|hunk| &hunk.lines)
            .map(|line| {
                let text = sanitize_terminal_text(&line.content, false).replace('\t', "  ");
                UnicodeWidthStr::width(text.as_str())
            })
            .max()
            .unwrap_or(0);
        let mut width = viewport.width;
        if self.actual_sidebar {
            width = width.saturating_sub(SIDEBAR_WIDTH + SIDEBAR_DIVIDER_WIDTH);
        }
        let code_width = match self.effective_layout {
            EffectiveLayout::Split => usize::from(width / 2).saturating_sub(8),
            EffectiveLayout::Stack => usize::from(width).saturating_sub(12),
        };
        widest.saturating_sub(code_width)
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
