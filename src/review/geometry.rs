use std::collections::HashMap;
use std::ops::Range;

use unicode_width::UnicodeWidthStr;

use crate::core::input::LayoutMode;

use super::row::{EffectiveLayout, ReviewCell, ReviewRow, ReviewRowKey, RowPlan};

const SPLIT_MIN_WIDTH: u16 = 160;
const FULL_MIN_WIDTH: u16 = 220;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponsiveViewport {
    Full,
    Medium,
    Tight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResponsiveLayout {
    pub viewport: ResponsiveViewport,
    pub layout: EffectiveLayout,
    pub show_sidebar: bool,
}

pub(crate) fn resolve_responsive_layout(
    requested: LayoutMode,
    terminal_width: u16,
    sidebar_requested: bool,
) -> ResponsiveLayout {
    let viewport = if terminal_width >= FULL_MIN_WIDTH {
        ResponsiveViewport::Full
    } else if terminal_width >= SPLIT_MIN_WIDTH {
        ResponsiveViewport::Medium
    } else {
        ResponsiveViewport::Tight
    };
    let layout = match requested {
        LayoutMode::Split => EffectiveLayout::Split,
        LayoutMode::Stack => EffectiveLayout::Stack,
        LayoutMode::Auto if viewport == ResponsiveViewport::Tight => EffectiveLayout::Stack,
        LayoutMode::Auto => EffectiveLayout::Split,
    };
    ResponsiveLayout {
        viewport,
        layout,
        show_sidebar: sidebar_requested && viewport == ResponsiveViewport::Full,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedFile {
    pub file_id: String,
    pub plan: RowPlan,
}

impl PlannedFile {
    pub(crate) fn new(file_id: String, plan: RowPlan) -> Self {
        Self { file_id, plan }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GeometryOptions {
    pub content_width: u16,
    pub viewport_height: u16,
    pub show_line_numbers: bool,
    pub wrap_lines: bool,
}

impl GeometryOptions {
    #[cfg(test)]
    pub(crate) fn fixed(content_width: u16, viewport_height: u16) -> Self {
        Self {
            content_width,
            viewport_height,
            show_line_numbers: false,
            wrap_lines: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileSection {
    pub file_id: String,
    pub file_index: usize,
    pub section_top: usize,
    pub separator_height: usize,
    pub header_top: usize,
    pub header_height: usize,
    pub body_top: usize,
    pub body_height: usize,
    pub section_bottom: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowBounds {
    pub key: ReviewRowKey,
    pub top: usize,
    pub height: usize,
    pub file_index: usize,
    pub hunk_index: Option<usize>,
    pub row_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VisibleWindow {
    pub range: Range<usize>,
    pub top_spacer: usize,
    pub bottom_spacer: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewGeometry {
    pub sections: Vec<FileSection>,
    pub rows: Vec<RowBounds>,
    pub total_height: usize,
    viewport_height: usize,
    row_by_key: HashMap<ReviewRowKey, usize>,
    file_by_id: HashMap<String, usize>,
    hunk_anchor_by_id: HashMap<(String, usize), usize>,
}

impl ReviewGeometry {
    pub(crate) fn row_at_offset(&self, offset: usize) -> Option<&RowBounds> {
        if self.rows.is_empty() {
            return None;
        }
        let index = self
            .rows
            .partition_point(|row| row.top.saturating_add(row.height) <= offset);
        self.rows.get(index).or_else(|| self.rows.last())
    }

    pub(crate) fn row_by_key(&self, key: &ReviewRowKey) -> Option<&RowBounds> {
        self.row_by_key
            .get(key)
            .and_then(|index| self.rows.get(*index))
    }

    pub(crate) fn hunk_anchor(&self, file_id: &str, hunk_index: usize) -> Option<&RowBounds> {
        self.hunk_anchor_by_id
            .get(&(file_id.to_string(), hunk_index))
            .and_then(|index| self.rows.get(*index))
    }

    pub(crate) fn file_section(&self, file_id: &str) -> Option<&FileSection> {
        self.file_by_id
            .get(file_id)
            .and_then(|index| self.sections.get(*index))
    }

    // Consumed by the viewport renderer in Task 5.
    #[allow(dead_code)]
    pub(crate) fn file_at_offset(&self, offset: usize) -> Option<usize> {
        if self.sections.is_empty() {
            return None;
        }
        let next = self
            .sections
            .partition_point(|section| section.section_top <= offset);
        Some(next.saturating_sub(1).min(self.sections.len() - 1))
    }

    pub(crate) fn visible_window(
        &self,
        scroll_top: usize,
        viewport_height: usize,
        overscan_rows: usize,
    ) -> VisibleWindow {
        if self.rows.is_empty() {
            return VisibleWindow {
                range: 0..0,
                top_spacer: 0,
                bottom_spacer: self.total_height,
            };
        }
        let min = scroll_top.saturating_sub(overscan_rows);
        let max = scroll_top
            .saturating_add(viewport_height)
            .saturating_add(overscan_rows);
        let start = self
            .rows
            .partition_point(|row| row.top.saturating_add(row.height) <= min);
        let end = self.rows.partition_point(|row| row.top < max).max(start);
        let top_spacer = self
            .rows
            .get(start)
            .map_or(self.total_height, |row| row.top);
        let rendered_bottom = end
            .checked_sub(1)
            .and_then(|index| self.rows.get(index))
            .map_or(top_spacer, |row| row.top.saturating_add(row.height));
        VisibleWindow {
            range: start..end,
            top_spacer,
            bottom_spacer: self.total_height.saturating_sub(rendered_bottom),
        }
    }

    pub(crate) fn max_scroll_top(&self) -> usize {
        self.total_height.saturating_sub(self.viewport_height)
    }
}

pub(crate) fn build_review_geometry(
    files: &[PlannedFile],
    options: GeometryOptions,
) -> ReviewGeometry {
    let mut sections = Vec::with_capacity(files.len());
    let mut rows = Vec::new();
    let mut row_by_key = HashMap::new();
    let mut file_by_id = HashMap::new();
    let mut hunk_anchor_by_id = HashMap::new();
    let mut cursor = 0usize;

    for (file_index, file) in files.iter().enumerate() {
        let separator_height = usize::from(file_index > 0);
        let header_height = usize::from(file_index > 0);
        let section_top = cursor;
        let header_top = section_top.saturating_add(separator_height);
        let body_top = header_top.saturating_add(header_height);
        let mut body_height = 0usize;

        for (row_index, row) in file.plan.rows.iter().enumerate() {
            let height = measure_row_height(row, file.plan.line_number_digits, options);
            let bound = RowBounds {
                key: row.key().clone(),
                top: body_top.saturating_add(body_height),
                height,
                file_index,
                hunk_index: row.key().hunk_index,
                row_index,
            };
            let bound_index = rows.len();
            row_by_key.insert(bound.key.clone(), bound_index);
            body_height = body_height.saturating_add(height);
            rows.push(bound);
        }

        for (hunk_index, key) in file.plan.hunk_anchor_keys.iter().enumerate() {
            if let Some(index) = row_by_key.get(key) {
                hunk_anchor_by_id.insert((file.file_id.clone(), hunk_index), *index);
            }
        }

        let section_bottom = body_top.saturating_add(body_height);
        file_by_id.insert(file.file_id.clone(), file_index);
        sections.push(FileSection {
            file_id: file.file_id.clone(),
            file_index,
            section_top,
            separator_height,
            header_top,
            header_height,
            body_top,
            body_height,
            section_bottom,
        });
        cursor = section_bottom;
    }

    ReviewGeometry {
        sections,
        rows,
        total_height: cursor,
        viewport_height: usize::from(options.viewport_height),
        row_by_key,
        file_by_id,
        hunk_anchor_by_id,
    }
}

fn measure_row_height(row: &ReviewRow, line_digits: usize, options: GeometryOptions) -> usize {
    if !options.wrap_lines {
        return 1;
    }
    match row {
        ReviewRow::HunkHeader { .. } | ReviewRow::Placeholder { .. } => 1,
        ReviewRow::Stack { cell, .. } => {
            wrapped_height(cell, stack_code_width(options, line_digits))
        }
        ReviewRow::Split { left, right, .. } => {
            let (left_width, right_width) = split_code_widths(options, line_digits);
            wrapped_height(left, left_width).max(wrapped_height(right, right_width))
        }
    }
}

fn stack_code_width(options: GeometryOptions, line_digits: usize) -> usize {
    let gutter = if options.show_line_numbers {
        line_digits.saturating_mul(2).saturating_add(5)
    } else {
        2
    };
    usize::from(options.content_width)
        .saturating_sub(1)
        .saturating_sub(gutter)
}

fn split_code_widths(options: GeometryOptions, line_digits: usize) -> (usize, usize) {
    let total = usize::from(options.content_width);
    let usable = total.saturating_sub(2);
    let left = usable / 2;
    let right = usable.saturating_sub(left);
    let gutter = if options.show_line_numbers {
        line_digits.saturating_add(3)
    } else {
        2
    };
    (left.saturating_sub(gutter), right.saturating_sub(gutter))
}

fn wrapped_height(cell: &ReviewCell, content_width: usize) -> usize {
    if content_width == 0 {
        return 1;
    }
    let width = UnicodeWidthStr::width(cell.text().as_str());
    width.max(1).div_ceil(content_width)
}

#[cfg(test)]
mod tests {
    use crate::core::input::LayoutMode;
    use crate::diff::model::{DiffFile, FileChangeKind};
    use crate::review::row::{EffectiveLayout, build_row_plan};

    use super::{
        GeometryOptions, PlannedFile, ResponsiveViewport, build_review_geometry,
        resolve_responsive_layout,
    };

    fn planned(path: &str, additions: usize) -> PlannedFile {
        let file = DiffFile::for_test(path, FileChangeKind::Modified, additions, 0);
        let plan = build_row_plan(&file, EffectiveLayout::Stack, true);
        PlannedFile::new(file.id, plan)
    }

    #[test]
    fn responsive_thresholds_match_hunk_and_explicit_modes() {
        let wide = resolve_responsive_layout(LayoutMode::Auto, 220, true);
        assert_eq!(wide.viewport, ResponsiveViewport::Full);
        assert_eq!(wide.layout, EffectiveLayout::Split);
        assert!(wide.show_sidebar);

        let medium = resolve_responsive_layout(LayoutMode::Auto, 160, true);
        assert_eq!(medium.viewport, ResponsiveViewport::Medium);
        assert_eq!(medium.layout, EffectiveLayout::Split);
        assert!(!medium.show_sidebar);

        let tight = resolve_responsive_layout(LayoutMode::Auto, 159, true);
        assert_eq!(tight.viewport, ResponsiveViewport::Tight);
        assert_eq!(tight.layout, EffectiveLayout::Stack);
        assert!(!tight.show_sidebar);

        assert_eq!(
            resolve_responsive_layout(LayoutMode::Split, 80, true).layout,
            EffectiveLayout::Split
        );
        assert_eq!(
            resolve_responsive_layout(LayoutMode::Stack, 220, true).layout,
            EffectiveLayout::Stack
        );
    }

    #[test]
    fn file_sections_include_separator_and_header_after_the_first_file() {
        let files = vec![planned("src/a.rs", 2), planned("src/b.rs", 3)];
        let geometry = build_review_geometry(&files, GeometryOptions::fixed(80, 20));
        assert_eq!(geometry.sections.len(), 2);
        assert_eq!(geometry.sections[0].section_top, 0);
        assert_eq!(geometry.sections[0].header_height, 0);
        assert_eq!(geometry.sections[0].body_top, 0);

        let second = &geometry.sections[1];
        assert_eq!(second.section_top, geometry.sections[0].section_bottom);
        assert_eq!(second.separator_height, 1);
        assert_eq!(second.header_height, 1);
        assert_eq!(second.body_top, second.section_top + 2);
    }

    #[test]
    fn wrap_height_uses_terminal_cells_and_tiny_widths_saturate() {
        let mut file = DiffFile::for_test("wide.rs", FileChangeKind::Modified, 1, 0);
        file.hunks[0].lines[0].content = "界界界界".into();
        let plan = build_row_plan(&file, EffectiveLayout::Stack, false);
        let files = vec![PlannedFile::new(file.id, plan)];

        let nowrap = build_review_geometry(
            &files,
            GeometryOptions {
                content_width: 8,
                viewport_height: 5,
                show_line_numbers: false,
                wrap_lines: false,
            },
        );
        assert_eq!(nowrap.rows[0].height, 1);

        let wrapped = build_review_geometry(
            &files,
            GeometryOptions {
                wrap_lines: true,
                ..GeometryOptions::fixed(8, 5)
            },
        );
        assert_eq!(wrapped.rows[0].height, 2);

        let tiny = build_review_geometry(
            &files,
            GeometryOptions {
                content_width: 0,
                viewport_height: 5,
                show_line_numbers: true,
                wrap_lines: true,
            },
        );
        assert_eq!(tiny.rows[0].height, 1);
    }

    #[test]
    fn offset_lookup_and_visible_window_are_bounded() {
        let files = (0..1_000)
            .map(|index| planned(&format!("src/{index}.rs"), 100))
            .collect::<Vec<_>>();
        let geometry = build_review_geometry(&files, GeometryOptions::fixed(120, 20));
        let first = geometry.row_at_offset(0).unwrap();
        let last = geometry
            .row_at_offset(geometry.total_height.saturating_sub(1))
            .unwrap();
        assert_eq!(first.file_index, 0);
        assert_eq!(last.file_index, 999);

        let middle = geometry.sections[500].body_top + 50;
        let window = geometry.visible_window(middle, 20, 4);
        assert!(window.range.len() <= 30);
        assert!(window.top_spacer > 0);
        assert!(window.bottom_spacer > 0);
        assert_eq!(
            window.top_spacer
                + geometry.rows[window.range.clone()]
                    .iter()
                    .map(|row| row.height)
                    .sum::<usize>()
                + window.bottom_spacer,
            geometry.total_height
        );
    }

    #[test]
    fn file_lookup_uses_exact_section_boundaries() {
        let files = vec![planned("a.rs", 2), planned("b.rs", 2)];
        let geometry = build_review_geometry(&files, GeometryOptions::fixed(80, 20));
        let boundary = geometry.sections[1].section_top;
        assert_eq!(geometry.file_at_offset(boundary.saturating_sub(1)), Some(0));
        assert_eq!(geometry.file_at_offset(boundary), Some(1));
        assert_eq!(geometry.file_at_offset(usize::MAX), Some(1));
    }
}
