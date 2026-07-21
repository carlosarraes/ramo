use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthChar;

use crate::diff::model::{DiffFile, LineType, MovedLineKind};
use crate::review::geometry::RowBounds;
use crate::review::row::{CellKind, ReviewCell, ReviewRow};
use crate::review::{ReviewController, ReviewFileStatus, SidebarEntrySnapshot, Viewport};

use super::highlight::HighlightCache;
use super::themes::{AppTheme, ReviewLineStyle};

const SIDEBAR_WIDTH: u16 = 34;

pub struct ReviewWidget<'a> {
    controller: &'a mut ReviewController,
    theme: &'a AppTheme,
    highlights: &'a mut HighlightCache,
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
        }
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
        let content = if view.snapshot.show_sidebar && area.width > SIDEBAR_WIDTH + 1 {
            let sidebar = Rect::new(area.x, area.y, SIDEBAR_WIDTH, area.height);
            render_sidebar(sidebar, buffer, view.snapshot, self.theme);
            let divider_x = area.x.saturating_add(SIDEBAR_WIDTH);
            for y in area.y..area.bottom() {
                buffer.set_stringn(divider_x, y, "│", 1, Style::default().fg(self.theme.border));
            }
            Rect::new(
                divider_x.saturating_add(1),
                area.y,
                area.width.saturating_sub(SIDEBAR_WIDTH + 1),
                area.height,
            )
        } else {
            area
        };
        render_stream(content, buffer, view, self.theme, self.highlights);
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
    for bound in &view.geometry.rows[window.range] {
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
        );
    }
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
            let gutter = if snapshot.line_numbers {
                digits.saturating_mul(2).saturating_add(5)
            } else {
                2
            };
            let code_width = usize::from(area.width).saturating_sub(1 + gutter);
            for line in 0..bound.height {
                let draw_y = y.saturating_add(line as u16);
                if draw_y >= area.bottom() {
                    break;
                }
                render_cell(
                    area.x + 1,
                    draw_y,
                    gutter,
                    code_width,
                    line,
                    cell,
                    file,
                    bound.hunk_index,
                    buffer,
                    snapshot,
                    theme,
                    highlights,
                    true,
                );
            }
        }
        ReviewRow::Split { left, right, .. } => {
            let usable = usize::from(area.width).saturating_sub(2);
            let left_width = usable / 2;
            let right_width = usable.saturating_sub(left_width);
            let gutter = if snapshot.line_numbers {
                digits.saturating_add(3)
            } else {
                2
            };
            for line in 0..bound.height {
                let draw_y = y.saturating_add(line as u16);
                if draw_y >= area.bottom() {
                    break;
                }
                render_cell(
                    area.x + 1,
                    draw_y,
                    gutter,
                    left_width.saturating_sub(gutter),
                    line,
                    left,
                    file,
                    bound.hunk_index,
                    buffer,
                    snapshot,
                    theme,
                    highlights,
                    false,
                );
                let divider = area.x.saturating_add(1 + left_width as u16);
                buffer.set_stringn(divider, draw_y, "│", 1, Style::default().fg(theme.border));
                render_cell(
                    divider + 1,
                    draw_y,
                    gutter,
                    right_width.saturating_sub(gutter),
                    line,
                    right,
                    file,
                    bound.hunk_index,
                    buffer,
                    snapshot,
                    theme,
                    highlights,
                    false,
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
