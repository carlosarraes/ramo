use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthChar;

use crate::diff::model::{DiffFile, LineType, MovedLineKind};
use crate::review::geometry::{RowBounds, split_columns, stack_columns};
use crate::review::row::{CellKind, ReviewCell, ReviewRow};
use crate::review::{
    ReviewController, ReviewFileStatus, SelectionPoint, SidebarEntrySnapshot, Viewport,
};

use super::highlight::HighlightCache;
use super::themes::{AppTheme, ReviewLineStyle};

pub struct ReviewWidget<'a> {
    controller: &'a mut ReviewController,
    theme: &'a AppTheme,
    highlights: &'a mut HighlightCache,
    selection: Option<(SelectionPoint, SelectionPoint)>,
}

impl<'a> ReviewWidget<'a> {
    pub fn new(
        controller: &'a mut ReviewController,
        theme: &'a AppTheme,
        highlights: &'a mut HighlightCache,
    ) -> Self {
        Self {
            controller,
            theme,
            highlights,
            selection: None,
        }
    }

    pub fn selection(mut self, selection: Option<(SelectionPoint, SelectionPoint)>) -> Self {
        self.selection = selection;
        self
    }
}

impl Widget for ReviewWidget<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        buffer.set_style(
            area,
            Style::default()
                .fg(self.theme.text)
                .bg(self.theme.background),
        );
        let viewport = Viewport {
            width: area.width,
            height: area.height,
        };
        let view = self.controller.render_view(viewport);
        let sidebar_width = view.snapshot.sidebar_width;
        let content = if view.snapshot.show_sidebar && area.width > sidebar_width + 1 {
            let sidebar = Rect::new(area.x, area.y, sidebar_width, area.height);
            render_sidebar(sidebar, buffer, view.snapshot, self.theme);
            let divider_x = area.x.saturating_add(sidebar_width);
            for y in area.y..area.bottom() {
                buffer.set_stringn(divider_x, y, "│", 1, Style::default().fg(self.theme.border));
            }
            Rect::new(
                divider_x.saturating_add(1),
                area.y,
                area.width.saturating_sub(sidebar_width + 1),
                area.height,
            )
        } else {
            area
        };
        render_stream(
            content,
            buffer,
            view,
            self.theme,
            self.highlights,
            self.selection,
        );
    }
}

fn render_sidebar(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &crate::review::ReviewSnapshot,
    theme: &AppTheme,
) {
    buffer.set_style(area, Style::default().fg(theme.text).bg(theme.panel));
    for (row, entry) in snapshot
        .sidebar_entries
        .iter()
        .take(usize::from(area.height))
        .enumerate()
    {
        let y = area.y.saturating_add(row as u16);
        match entry {
            SidebarEntrySnapshot::Group { label, .. } => {
                buffer.set_stringn(
                    area.x + 1,
                    y,
                    label,
                    area.width.saturating_sub(2) as usize,
                    Style::default().fg(theme.muted).bg(theme.panel),
                );
            }
            SidebarEntrySnapshot::File {
                id,
                name,
                annotations_text,
                additions_text,
                deletions_text,
                status,
            } => {
                let selected = snapshot.selected_file_id.as_deref() == Some(id);
                let background = if selected {
                    theme.panel_alt
                } else {
                    theme.panel
                };
                buffer.set_style(
                    Rect::new(area.x, y, area.width, 1),
                    Style::default().bg(background),
                );
                let marker = if selected { "› " } else { "  " };
                let style = Style::default()
                    .fg(file_status_color(*status, theme))
                    .bg(background);
                let stats = [
                    annotations_text.as_deref(),
                    additions_text.as_deref(),
                    deletions_text.as_deref(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
                let available = usize::from(area.width.saturating_sub(3));
                let label_width = available.saturating_sub(stats.chars().count().saturating_add(1));
                buffer.set_stringn(area.x + 1, y, marker, 2, style);
                buffer.set_stringn(area.x + 3, y, name, label_width, style);
                if !stats.is_empty() {
                    let x = area
                        .right()
                        .saturating_sub(stats.chars().count() as u16 + 1);
                    buffer.set_stringn(
                        x,
                        y,
                        stats,
                        available,
                        Style::default().fg(theme.text).bg(background),
                    );
                }
            }
        }
    }
}

fn render_stream(
    area: Rect,
    buffer: &mut Buffer,
    view: crate::review::state::ReviewRenderView<'_>,
    theme: &AppTheme,
    highlights: &mut HighlightCache,
    selection: Option<(SelectionPoint, SelectionPoint)>,
) {
    if area.is_empty() {
        return;
    }
    let scroll = view.snapshot.scroll_top;
    for section in &view.geometry.sections {
        if section.separator_height > 0 {
            render_absolute_line(
                area,
                buffer,
                section.section_top,
                scroll,
                "─",
                theme.border,
                theme.background,
            );
        }
        if section.header_height > 0 {
            let file = &view.files[view.visible_indices[section.file_index]];
            let label = file_header(file, view.snapshot.visible_files[section.file_index].status);
            render_absolute_text(
                area,
                buffer,
                section.header_top,
                scroll,
                &label,
                Style::default()
                    .fg(theme.text)
                    .bg(theme.panel_alt)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }

    let window = view
        .geometry
        .visible_window(scroll, usize::from(area.height), 2);
    for (window_offset, bound) in view.geometry.rows[window.range.clone()].iter().enumerate() {
        let Some(y) = visible_y(bound.top, scroll, area) else {
            continue;
        };
        let file = &view.files[view.visible_indices[bound.file_index]];
        let planned = &view.planned_files[bound.file_index].plan;
        let row = &planned.rows[bound.row_index];
        render_row(
            area,
            y,
            bound,
            row,
            planned.line_number_digits,
            file,
            buffer,
            view.snapshot,
            theme,
            highlights,
            window.range.start.saturating_add(window_offset),
            selection,
        );
    }
    render_scrollbar(area, buffer, view.snapshot, theme);
}

#[allow(clippy::too_many_arguments)]
fn render_row(
    area: Rect,
    y: u16,
    bound: &RowBounds,
    row: &ReviewRow,
    digits: usize,
    file: &DiffFile,
    buffer: &mut Buffer,
    snapshot: &crate::review::ReviewSnapshot,
    theme: &AppTheme,
    highlights: &mut HighlightCache,
    geometry_row: usize,
    selection: Option<(SelectionPoint, SelectionPoint)>,
) {
    match row {
        ReviewRow::HunkHeader { text, .. } => {
            fill_line(area, y, buffer, theme.panel_alt);
            buffer.set_stringn(
                area.x + 1,
                y,
                text,
                area.width.saturating_sub(2) as usize,
                Style::default()
                    .fg(theme.accent)
                    .bg(theme.panel_alt)
                    .add_modifier(Modifier::BOLD),
            );
        }
        ReviewRow::Placeholder { text, .. } => {
            buffer.set_stringn(
                area.x + 2,
                y,
                text,
                area.width.saturating_sub(3) as usize,
                Style::default().fg(theme.muted).bg(theme.context_bg),
            );
        }
        ReviewRow::Collapsed { text, .. } => {
            fill_line(area, y, buffer, theme.context_bg);
            buffer.set_stringn(
                area.x + 1,
                y,
                format!("⋯ {text}"),
                area.width.saturating_sub(2) as usize,
                Style::default().fg(theme.muted).bg(theme.context_bg),
            );
        }
        ReviewRow::Stack { cell, .. } => {
            let columns = stack_columns(area.width, digits, snapshot.line_numbers);
            for line in 0..bound.height {
                let draw_y = y.saturating_add(line as u16);
                if draw_y >= area.bottom() {
                    break;
                }
                render_cell(
                    area.x + 1,
                    draw_y,
                    columns.gutter,
                    columns.code_width,
                    line,
                    cell,
                    file,
                    bound.hunk_index,
                    buffer,
                    snapshot,
                    theme,
                    highlights,
                    true,
                    selected_cell_range(
                        selection,
                        geometry_row,
                        columns.text_cell,
                        cell.text().as_str(),
                    ),
                    columns.text_cell,
                );
            }
        }
        ReviewRow::Split { left, right, .. } => {
            let columns = split_columns(area.width, digits, snapshot.line_numbers);
            for line in 0..bound.height {
                let draw_y = y.saturating_add(line as u16);
                if draw_y >= area.bottom() {
                    break;
                }
                render_cell(
                    area.x + 1,
                    draw_y,
                    columns.gutter,
                    columns.left_code_width,
                    line,
                    left,
                    file,
                    bound.hunk_index,
                    buffer,
                    snapshot,
                    theme,
                    highlights,
                    false,
                    (selection_side(selection, columns.divider_cell) != Some(true))
                        .then(|| {
                            selected_cell_range(
                                selection,
                                geometry_row,
                                columns.left_text_cell,
                                left.text().as_str(),
                            )
                        })
                        .flatten(),
                    columns.left_text_cell,
                );
                let divider = area.x.saturating_add(columns.divider_cell as u16);
                buffer.set_stringn(divider, draw_y, "│", 1, Style::default().fg(theme.border));
                render_cell(
                    divider + 1,
                    draw_y,
                    columns.gutter,
                    columns.right_code_width,
                    line,
                    right,
                    file,
                    bound.hunk_index,
                    buffer,
                    snapshot,
                    theme,
                    highlights,
                    false,
                    (selection_side(selection, columns.divider_cell) != Some(false))
                        .then(|| {
                            selected_cell_range(
                                selection,
                                geometry_row,
                                columns.right_text_cell,
                                right.text().as_str(),
                            )
                        })
                        .flatten(),
                    columns.right_text_cell,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_cell(
    x: u16,
    y: u16,
    gutter: usize,
    code_width: usize,
    wrap_line: usize,
    cell: &ReviewCell,
    file: &DiffFile,
    hunk_index: Option<usize>,
    buffer: &mut Buffer,
    snapshot: &crate::review::ReviewSnapshot,
    theme: &AppTheme,
    highlights: &mut HighlightCache,
    stack: bool,
    selection: Option<std::ops::Range<usize>>,
    text_cell: usize,
) {
    let kind = semantic_kind(cell);
    let row_style = theme.row_style(kind);
    buffer.set_style(Rect::new(x, y, (gutter + code_width) as u16, 1), row_style);
    if wrap_line == 0 {
        let numbers = if snapshot.line_numbers {
            if stack {
                let digits = gutter.saturating_sub(5) / 2;
                format!(
                    "{:>digits$} {:>digits$} {} ",
                    cell.old_line.map_or(String::new(), |n| n.to_string()),
                    cell.new_line.map_or(String::new(), |n| n.to_string()),
                    cell.sign
                )
            } else {
                let digits = gutter.saturating_sub(3);
                let number = cell
                    .old_line
                    .or(cell.new_line)
                    .map_or(String::new(), |n| n.to_string());
                format!("{:>digits$} {} ", number, cell.sign)
            }
        } else {
            format!("{} ", cell.sign)
        };
        buffer.set_stringn(x, y, numbers, gutter, theme.gutter_style(kind));
    }
    if let Some(hunk) = hunk_index
        && let Some(line_index) = source_line_index(file, hunk, cell)
    {
        let _ = highlights.spans(file, hunk, line_index, theme);
    }
    let offset = if snapshot.wrap_lines {
        wrap_line.saturating_mul(code_width)
    } else {
        snapshot.horizontal_offset
    };
    render_emphasis(
        (x + gutter as u16, y),
        (code_width, offset),
        cell,
        buffer,
        theme,
        kind,
    );
    if let Some(selection) = selection {
        let visible = text_cell.saturating_add(offset)
            ..text_cell.saturating_add(offset).saturating_add(code_width);
        let start = selection.start.max(visible.start);
        let end = selection.end.min(visible.end);
        if start < end {
            buffer.set_style(
                Rect::new(
                    x.saturating_add(gutter as u16)
                        .saturating_add(start.saturating_sub(visible.start) as u16),
                    y,
                    end.saturating_sub(start) as u16,
                    1,
                ),
                Style::default().bg(theme.accent_muted),
            );
        }
    }
}

fn selection_side(
    selection: Option<(SelectionPoint, SelectionPoint)>,
    divider_cell: usize,
) -> Option<bool> {
    selection.map(|(anchor, _)| anchor.cell > divider_cell)
}

fn selected_cell_range(
    selection: Option<(SelectionPoint, SelectionPoint)>,
    row: usize,
    text_cell: usize,
    text: &str,
) -> Option<std::ops::Range<usize>> {
    let (anchor, focus) = selection?;
    let (start, end) = if (anchor.row, anchor.cell) <= (focus.row, focus.cell) {
        (anchor, focus)
    } else {
        (focus, anchor)
    };
    if row < start.row || row > end.row {
        return None;
    }
    let text_end = text_cell.saturating_add(unicode_width::UnicodeWidthStr::width(text));
    let from = if row == start.row {
        start.cell.max(text_cell)
    } else {
        text_cell
    };
    let to = if row == end.row {
        end.cell.min(text_end)
    } else {
        text_end
    };
    (from < to).then_some(from..to)
}

fn render_emphasis(
    origin: (u16, u16),
    viewport: (usize, usize),
    cell: &ReviewCell,
    buffer: &mut Buffer,
    theme: &AppTheme,
    kind: ReviewLineStyle,
) {
    let (x, y) = origin;
    let (width, offset) = viewport;
    let mut skipped = 0usize;
    let mut written = 0usize;
    let mut cursor_x = x;
    for span in &cell.spans {
        let style = if span.emphasized {
            theme.changed_style(kind)
        } else {
            theme.row_style(kind)
        };
        for character in span.text.chars() {
            let cells = character.width().unwrap_or(0);
            if skipped.saturating_add(cells) <= offset {
                skipped = skipped.saturating_add(cells);
                continue;
            }
            if written.saturating_add(cells) > width {
                return;
            }
            buffer.set_stringn(cursor_x, y, character.to_string(), cells.max(1), style);
            cursor_x = cursor_x.saturating_add(cells as u16);
            written = written.saturating_add(cells);
        }
    }
}

fn semantic_kind(cell: &ReviewCell) -> ReviewLineStyle {
    match (cell.kind, cell.moved) {
        (CellKind::Addition, Some(MovedLineKind::NewMoved | MovedLineKind::NewMovedDimmed)) => {
            ReviewLineStyle::MovedAdded
        }
        (CellKind::Deletion, Some(MovedLineKind::OldMoved | MovedLineKind::OldMovedDimmed)) => {
            ReviewLineStyle::MovedRemoved
        }
        (CellKind::Addition, _) => ReviewLineStyle::Added,
        (CellKind::Deletion, _) => ReviewLineStyle::Removed,
        _ => ReviewLineStyle::Context,
    }
}

fn source_line_index(file: &DiffFile, hunk_index: usize, cell: &ReviewCell) -> Option<usize> {
    file.hunks
        .get(hunk_index)?
        .lines
        .iter()
        .position(|line| match cell.kind {
            CellKind::Addition => {
                line.kind == LineType::Addition && line.new_lineno == cell.new_line
            }
            CellKind::Deletion => {
                line.kind == LineType::Deletion && line.old_lineno == cell.old_line
            }
            CellKind::Context => line.kind == LineType::Context && line.new_lineno == cell.new_line,
            CellKind::Empty => false,
        })
}

fn visible_y(top: usize, scroll: usize, area: Rect) -> Option<u16> {
    let relative = top.checked_sub(scroll)?;
    (relative < usize::from(area.height)).then(|| area.y + relative as u16)
}

fn render_absolute_line(
    area: Rect,
    buffer: &mut Buffer,
    top: usize,
    scroll: usize,
    symbol: &str,
    foreground: ratatui::style::Color,
    background: ratatui::style::Color,
) {
    let Some(y) = visible_y(top, scroll, area) else {
        return;
    };
    buffer.set_stringn(
        area.x,
        y,
        symbol.repeat(area.width as usize),
        area.width as usize,
        Style::default().fg(foreground).bg(background),
    );
}

fn render_absolute_text(
    area: Rect,
    buffer: &mut Buffer,
    top: usize,
    scroll: usize,
    text: &str,
    style: Style,
) {
    let Some(y) = visible_y(top, scroll, area) else {
        return;
    };
    buffer.set_style(Rect::new(area.x, y, area.width, 1), style);
    buffer.set_stringn(
        area.x + 1,
        y,
        text,
        area.width.saturating_sub(2) as usize,
        style,
    );
}

fn fill_line(area: Rect, y: u16, buffer: &mut Buffer, background: ratatui::style::Color) {
    buffer.set_style(
        Rect::new(area.x, y, area.width, 1),
        Style::default().bg(background),
    );
}

fn render_scrollbar(
    area: Rect,
    buffer: &mut Buffer,
    snapshot: &crate::review::ReviewSnapshot,
    theme: &AppTheme,
) {
    if area.is_empty() || snapshot.total_height <= usize::from(area.height) {
        return;
    }
    let height = usize::from(area.height);
    let thumb_height = height
        .saturating_mul(height)
        .checked_div(snapshot.total_height)
        .unwrap_or(1)
        .clamp(1, height);
    let travel = height.saturating_sub(thumb_height);
    let thumb_top = snapshot
        .scroll_top
        .saturating_mul(travel)
        .checked_div(snapshot.max_scroll_top.max(1))
        .unwrap_or(0);
    let x = area.right().saturating_sub(1);
    for row in 0..height {
        let in_thumb = (thumb_top..thumb_top.saturating_add(thumb_height)).contains(&row);
        buffer.set_stringn(
            x,
            area.y.saturating_add(row as u16),
            if in_thumb { "█" } else { "│" },
            1,
            Style::default()
                .fg(if in_thumb { theme.accent } else { theme.border })
                .bg(theme.background),
        );
    }
}

fn file_header(file: &DiffFile, status: ReviewFileStatus) -> String {
    let path = file
        .previous_path
        .as_ref()
        .map_or_else(|| file.path.clone(), |old| format!("{old} → {}", file.path));
    format!(
        "{path} ({})  +{} -{}",
        status_label(status),
        file.stats.additions,
        file.stats.deletions
    )
}

fn status_label(status: ReviewFileStatus) -> &'static str {
    match status {
        ReviewFileStatus::Modified => "modified",
        ReviewFileStatus::Added => "new",
        ReviewFileStatus::Deleted => "deleted",
        ReviewFileStatus::Renamed => "renamed",
        ReviewFileStatus::Copied => "copied",
        ReviewFileStatus::Binary => "binary",
        ReviewFileStatus::TooLarge => "skipped large file",
        ReviewFileStatus::Untracked => "untracked",
    }
}

fn file_status_color(status: ReviewFileStatus, theme: &AppTheme) -> ratatui::style::Color {
    match status {
        ReviewFileStatus::Added => theme.file_new,
        ReviewFileStatus::Deleted => theme.file_deleted,
        ReviewFileStatus::Renamed | ReviewFileStatus::Copied => theme.file_renamed,
        ReviewFileStatus::Untracked => theme.file_untracked,
        _ => theme.file_modified,
    }
}
