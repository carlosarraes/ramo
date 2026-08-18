use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Widget;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::diff::model::{DiffFile, LineType, MovedLineKind};
use crate::review::geometry::{RowBounds, RowOwner, split_columns, stack_columns};
use crate::review::row::{CellKind, ReviewCell, ReviewRow};
use crate::review::{
    ReviewController, ReviewFileStatus, ReviewSide, SelectionPoint, SidebarEntrySnapshot, Viewport,
};

use super::highlight::HighlightCache;
use super::themes::{AppTheme, ReviewLineStyle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewHeading {
    Local(String),
    PullRequest {
        number: u64,
        title: String,
        base_ref: String,
        head_ref: String,
    },
}

impl ReviewHeading {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Local(label) => label.clone(),
            Self::PullRequest {
                number,
                title,
                base_ref,
                head_ref,
            } => format!("GitHub PR #{number} · {base_ref} ← {head_ref} · {title}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewAreas {
    pub header: Rect,
    pub content: Rect,
    pub footer: Rect,
    /// The chat pane, when it is open and the terminal is wide enough for it.
    pub chat: Option<Rect>,
}

/// Below this the diff would be squeezed past usefulness, so the chat pane yields rather than
/// shrinking the code — the same discipline the sidebar already follows.
pub const CHAT_MIN_TOTAL_WIDTH: u16 = 100;
const CHAT_PERCENT: u16 = 40;

pub fn review_areas(area: Rect) -> ReviewAreas {
    review_areas_with_chat(area, false)
}

pub fn review_areas_with_chat(area: Rect, chat: bool) -> ReviewAreas {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    let (content, chat) = if chat && area.width >= CHAT_MIN_TOTAL_WIDTH {
        let columns = Layout::horizontal([
            Constraint::Percentage(100 - CHAT_PERCENT),
            Constraint::Percentage(CHAT_PERCENT),
        ])
        .split(rows[1]);
        (columns[0], Some(columns[1]))
    } else {
        (rows[1], None)
    };
    ReviewAreas {
        header: rows[0],
        content,
        footer: rows[2],
        chat,
    }
}

/// The width the diff gets once the chat pane has taken its column. Needed wherever the
/// controller viewport is computed, or rows are planned wider than they are drawn.
pub fn review_content_width(total_width: u16, chat: bool) -> u16 {
    review_areas_with_chat(Rect::new(0, 0, total_width, 3), chat)
        .content
        .width
}

pub struct ReviewHeader<'a> {
    heading: &'a ReviewHeading,
    snapshot: &'a crate::review::ReviewSnapshot,
    theme: &'a AppTheme,
}

impl<'a> ReviewHeader<'a> {
    pub fn new(
        heading: &'a ReviewHeading,
        snapshot: &'a crate::review::ReviewSnapshot,
        theme: &'a AppTheme,
    ) -> Self {
        Self {
            heading,
            snapshot,
            theme,
        }
    }
}

impl Widget for ReviewHeader<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let base = Style::default()
            .fg(self.theme.text)
            .bg(self.theme.panel_alt)
            .add_modifier(Modifier::BOLD);
        buffer.set_style(area, base);

        let file_word = if self.snapshot.total_files == 1 {
            "file"
        } else {
            "files"
        };
        let neutral = format!("{} {file_word} · ", self.snapshot.total_files);
        let additions = format!("+{}", self.snapshot.total_additions);
        let deletions = format!("-{}", self.snapshot.total_deletions);
        let stats_width = UnicodeWidthStr::width(neutral.as_str())
            .saturating_add(UnicodeWidthStr::width(additions.as_str()))
            .saturating_add(1)
            .saturating_add(UnicodeWidthStr::width(deletions.as_str()));
        let available = usize::from(area.width);
        let prefix_width = available.saturating_sub(stats_width);
        let prefix = if prefix_width >= 3 {
            format!(
                "{} · ",
                truncate_cells_with_ellipsis(&self.heading.label(), prefix_width.saturating_sub(3),)
            )
        } else {
            String::new()
        };

        let mut x = area.x;
        buffer.set_stringn(x, area.y, &prefix, prefix_width, base);
        x = x.saturating_add(UnicodeWidthStr::width(prefix.as_str()) as u16);
        buffer.set_stringn(x, area.y, &neutral, available, base);
        x = x.saturating_add(UnicodeWidthStr::width(neutral.as_str()) as u16);
        buffer.set_stringn(
            x,
            area.y,
            &additions,
            available,
            base.fg(self.theme.added_sign),
        );
        x = x.saturating_add(UnicodeWidthStr::width(additions.as_str()) as u16);
        buffer.set_stringn(x, area.y, " ", 1, base);
        x = x.saturating_add(1);
        buffer.set_stringn(
            x,
            area.y,
            &deletions,
            available,
            base.fg(self.theme.removed_sign),
        );
    }
}

pub struct ReviewFooter<'a> {
    status: Option<&'a str>,
    snapshot: &'a crate::review::ReviewSnapshot,
    theme: &'a AppTheme,
    ask_badge: Option<usize>,
}

impl<'a> ReviewFooter<'a> {
    pub fn new(
        status: Option<&'a str>,
        snapshot: &'a crate::review::ReviewSnapshot,
        theme: &'a AppTheme,
    ) -> Self {
        Self {
            status,
            snapshot,
            theme,
            ask_badge: None,
        }
    }

    /// Unread AI answers. Unlike the status toast this survives navigation keys, so the
    /// reviewer can finish reading before jumping.
    pub fn ask_badge(mut self, unread: Option<usize>) -> Self {
        self.ask_badge = unread;
        self
    }
}

impl Widget for ReviewFooter<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let style = Style::default()
            .fg(self.theme.text)
            .bg(self.theme.panel_alt);
        buffer.set_style(area, style);
        let progress = format!("Reviewed {}%", self.snapshot.reviewed_percent);
        let progress_width = UnicodeWidthStr::width(progress.as_str()).min(usize::from(area.width));
        let progress_x = area.right().saturating_sub(progress_width as u16);
        // Reserve the badge before the status truncation math so a long toast cannot eat it.
        let badge = self
            .ask_badge
            .filter(|unread| *unread > 0)
            .map(|unread| format!("AI {unread} · o "));
        let badge_width = badge.as_deref().map_or(0, |badge| {
            UnicodeWidthStr::width(badge).min(usize::from(area.width))
        });
        let badge_x = progress_x.saturating_sub(badge_width as u16);
        if let Some(status) = self.status {
            let status_width = usize::from(badge_x.saturating_sub(area.x).saturating_sub(1));
            buffer.set_stringn(
                area.x,
                area.y,
                truncate_cells(status, status_width),
                status_width,
                style,
            );
        }
        if let Some(badge) = badge {
            buffer.set_stringn(
                badge_x,
                area.y,
                badge,
                badge_width,
                style.fg(self.theme.accent).add_modifier(Modifier::BOLD),
            );
        }
        buffer.set_stringn(progress_x, area.y, progress, progress_width, style);
    }
}

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
    if let Some(trailer) = &view.geometry.trailer {
        render_absolute_line(
            area,
            buffer,
            trailer.section_top,
            scroll,
            "─",
            theme.border,
            theme.background,
        );
        render_absolute_text(
            area,
            buffer,
            trailer.header_top,
            scroll,
            "Unplaced GitHub comments",
            Style::default()
                .fg(theme.text)
                .bg(theme.panel_alt)
                .add_modifier(Modifier::BOLD),
        );
    }

    let window = view
        .geometry
        .visible_window(scroll, usize::from(area.height), 2);
    for (window_offset, bound) in view.geometry.rows[window.range.clone()].iter().enumerate() {
        if bound.top.saturating_add(bound.height) <= scroll
            || bound.top >= scroll.saturating_add(usize::from(area.height))
        {
            continue;
        }
        let first_line = scroll.saturating_sub(bound.top);
        let y = area
            .y
            .saturating_add(bound.top.saturating_sub(scroll) as u16);
        let (row, digits, file) = match bound.owner {
            RowOwner::File { file_index } => {
                let planned = &view.planned_files[file_index].plan;
                (
                    &planned.rows[bound.row_index],
                    planned.line_number_digits,
                    Some(&view.files[view.visible_indices[file_index]]),
                )
            }
            RowOwner::Trailer => {
                let planned = view
                    .trailer_plan
                    .expect("trailer geometry has a trailer plan");
                (&planned.rows[bound.row_index], 1, None)
            }
        };
        render_row(
            area,
            y,
            bound,
            row,
            digits,
            file,
            buffer,
            view.snapshot,
            theme,
            highlights,
            window.range.start.saturating_add(window_offset),
            selection,
            first_line,
            view.cursor_key == Some(&bound.key),
            view.focused_side,
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
    file: Option<&DiffFile>,
    buffer: &mut Buffer,
    snapshot: &crate::review::ReviewSnapshot,
    theme: &AppTheme,
    highlights: &mut HighlightCache,
    geometry_row: usize,
    selection: Option<(SelectionPoint, SelectionPoint)>,
    first_line: usize,
    cursor: bool,
    focused_side: ReviewSide,
) {
    match row {
        ReviewRow::CompactedFile { .. } => {
            let file = file.expect("compacted rows belong to files");
            let RowOwner::File { file_index } = bound.owner else {
                unreachable!("compacted rows cannot belong to the trailer");
            };
            let background = if cursor {
                theme.selected_hunk
            } else {
                theme.panel_alt
            };
            fill_line(area, y, buffer, background);
            let comments = snapshot.visible_files[file_index].github_thread_count;
            let suffix = match comments {
                0 => String::new(),
                1 => " · 1 unresolved thread".into(),
                count => format!(" · {count} unresolved threads"),
            };
            let label = format!(
                "▸ {}{suffix}",
                file_header(file, snapshot.visible_files[file_index].status)
            );
            buffer.set_stringn(
                area.x + 1,
                y,
                label,
                area.width.saturating_sub(2) as usize,
                Style::default().fg(theme.text).bg(background),
            );
        }
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
            let file = file.expect("diff rows belong to files");
            let columns = stack_columns(area.width, digits, snapshot.line_numbers);
            for line in first_line..bound.height {
                let draw_y = y.saturating_add(line.saturating_sub(first_line) as u16);
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
                    cursor,
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
            let file = file.expect("diff rows belong to files");
            let columns = split_columns(area.width, digits, snapshot.line_numbers);
            for line in first_line..bound.height {
                let draw_y = y.saturating_add(line.saturating_sub(first_line) as u16);
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
                    cursor && focused_side == ReviewSide::Left,
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
                    cursor && focused_side == ReviewSide::Right,
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
        ReviewRow::Note { card, .. } => {
            render_note_card(area, y, first_line, card, buffer, theme, cursor);
        }
    }
}

fn render_note_card(
    area: Rect,
    y: u16,
    first_line: usize,
    card: &crate::review::row::NoteCard,
    buffer: &mut Buffer,
    theme: &AppTheme,
    cursor: bool,
) {
    let x = area.x.saturating_add(card.placement.box_left);
    let width = card.placement.box_width.min(area.right().saturating_sub(x));
    if width == 0 {
        return;
    }
    let height = card.height();
    for line in first_line..height {
        let draw_y = y.saturating_add(line.saturating_sub(first_line) as u16);
        if draw_y >= area.bottom() {
            break;
        }
        let (text, style) = if line == 0 {
            let title = format!("─ {} ", card.title);
            let fill = usize::from(width).saturating_sub(2 + title.chars().count());
            (
                format!("┌{title}{}┐", "─".repeat(fill)),
                Style::default()
                    .fg(theme.note_title_text)
                    .bg(
                        if cursor
                            && matches!(
                                card.kind,
                                crate::review::row::NoteCardKind::Github
                                    | crate::review::row::NoteCardKind::Ask
                            )
                        {
                            theme.selected_hunk
                        } else {
                            theme.note_title_background
                        },
                    )
                    .add_modifier(Modifier::BOLD),
            )
        } else if line + 1 == height {
            (
                format!("└{}┘", "─".repeat(usize::from(width).saturating_sub(2))),
                Style::default()
                    .fg(theme.note_border)
                    .bg(theme.note_background),
            )
        } else {
            if line >= 2
                && let Some(markup_line) = card
                    .markup
                    .as_ref()
                    .and_then(|markup| markup.lines.get(line.saturating_sub(2)))
            {
                render_markup_note_line(x, draw_y, width, markup_line, buffer, theme);
                continue;
            }
            let content = if line == 1 {
                card.location.as_str()
            } else {
                card.lines
                    .get(line.saturating_sub(2))
                    .map_or("", String::as_str)
            };
            let available = usize::from(width).saturating_sub(4);
            let content = truncate_cells(content, available);
            let padding =
                available.saturating_sub(unicode_width::UnicodeWidthStr::width(content.as_str()));
            (
                format!("│ {content}{} │", " ".repeat(padding)),
                Style::default()
                    .fg(if line == 1 { theme.muted } else { theme.text })
                    .bg(theme.note_background),
            )
        };
        buffer.set_stringn(x, draw_y, text, usize::from(width), style);
    }
    render_card_caret(area, x, y, width, first_line, card, buffer, theme);
}

/// Paints the edit caret as a reversed cell. Card body lines start two rows in (border, then
/// location) and two columns in (`│ `), which is what maps a `(row, column)` onto the buffer.
#[allow(clippy::too_many_arguments)]
fn render_card_caret(
    area: Rect,
    x: u16,
    y: u16,
    width: u16,
    first_line: usize,
    card: &crate::review::row::NoteCard,
    buffer: &mut Buffer,
    theme: &AppTheme,
) {
    let Some((row, column)) = card.caret else {
        return;
    };
    let line = row.saturating_add(2);
    if line < first_line {
        return;
    }
    let draw_y = y.saturating_add((line - first_line) as u16);
    let draw_x = x.saturating_add(2).saturating_add(column as u16);
    // The caret may sit one cell past the last character, so allow the full inner width.
    if draw_y >= area.bottom() || draw_x >= x.saturating_add(width).saturating_sub(1) {
        return;
    }
    if let Some(cell) = buffer.cell_mut((draw_x, draw_y)) {
        cell.set_style(
            Style::default()
                .fg(theme.note_background)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        );
    }
}

fn render_markup_note_line(
    x: u16,
    y: u16,
    width: u16,
    line: &crate::markup::StmlLine,
    buffer: &mut Buffer,
    theme: &AppTheme,
) {
    let base = Style::default().fg(theme.text).bg(theme.note_background);
    buffer.set_style(Rect::new(x, y, width, 1), base);
    buffer.set_stringn(
        x,
        y,
        "│ ",
        2,
        Style::default()
            .fg(theme.note_border)
            .bg(theme.note_background),
    );
    let available = usize::from(width.saturating_sub(4));
    let mut offset = 0usize;
    for span in &line.spans {
        if offset >= available {
            break;
        }
        let value = truncate_cells(&span.text, available - offset);
        let used = unicode_width::UnicodeWidthStr::width(value.as_str());
        buffer.set_stringn(
            x.saturating_add(2 + offset as u16),
            y,
            value,
            available - offset,
            markup_span_style(span, theme),
        );
        offset = offset.saturating_add(used);
    }
    buffer.set_stringn(
        x.saturating_add(width.saturating_sub(2)),
        y,
        " │",
        2,
        Style::default()
            .fg(theme.note_border)
            .bg(theme.note_background),
    );
}

fn markup_span_style(span: &crate::markup::StmlSpan, theme: &AppTheme) -> Style {
    let mut style = Style::default()
        .fg(span
            .fg
            .as_deref()
            .map_or(theme.text, |color| markup_color(color, theme, false)))
        .bg(span.bg.as_deref().map_or(theme.note_background, |color| {
            markup_color(color, theme, true)
        }));
    let mut modifiers = Modifier::empty();
    if span.bold {
        modifiers |= Modifier::BOLD;
    }
    if span.italic {
        modifiers |= Modifier::ITALIC;
    }
    if span.underline {
        modifiers |= Modifier::UNDERLINED;
    }
    if span.strike {
        modifiers |= Modifier::CROSSED_OUT;
    }
    if span.dim {
        modifiers |= Modifier::DIM;
    }
    style = style.add_modifier(modifiers);
    style
}

fn markup_color(value: &str, theme: &AppTheme, background: bool) -> Color {
    crate::markup::resolve_stml_color(value, theme).unwrap_or(if background {
        theme.note_background
    } else {
        theme.text
    })
}

fn truncate_cells(text: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthChar;

    let mut cells = 0usize;
    text.chars()
        .take_while(|character| {
            let next = cells.saturating_add(character.width().unwrap_or(0));
            if next > width {
                false
            } else {
                cells = next;
                true
            }
        })
        .collect()
}

fn truncate_cells_with_ellipsis(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }

    let mut truncated = truncate_cells(text, width.saturating_sub(1));
    truncated.push('…');
    truncated
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
    cursor: bool,
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
    let syntax = hunk_index
        .and_then(|hunk| {
            source_line_index(file, hunk, cell)
                .map(|line_index| highlights.spans(file, hunk, line_index, theme))
        })
        .unwrap_or_default();
    let offset = if snapshot.wrap_lines {
        wrap_line.saturating_mul(code_width)
    } else {
        snapshot.horizontal_offset
    };
    render_emphasis(
        (x + gutter as u16, y),
        (code_width, offset),
        cell,
        &syntax,
        buffer,
        theme,
        kind,
    );
    if cursor {
        buffer.set_style(
            Rect::new(x, y, (gutter + code_width) as u16, 1),
            Style::default().bg(theme.selected_hunk),
        );
    }
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
    syntax: &[Span<'static>],
    buffer: &mut Buffer,
    theme: &AppTheme,
    kind: ReviewLineStyle,
) {
    let (x, y) = origin;
    let (width, offset) = viewport;
    let mut skipped = 0usize;
    let mut written = 0usize;
    let mut cursor_x = x;
    let syntax_styles = syntax_styles(cell.text().as_str(), syntax);
    let mut character_index = 0usize;
    for span in &cell.spans {
        let base = if span.emphasized {
            theme.changed_style(kind)
        } else {
            theme.row_style(kind)
        };
        for character in span.text.chars() {
            let cells = character.width().unwrap_or(0);
            let style = syntax_styles
                .as_ref()
                .and_then(|styles| styles.get(character_index))
                .copied()
                .map_or(base, |syntax_style| base.patch(syntax_style));
            character_index = character_index.saturating_add(1);
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

fn syntax_styles(text: &str, spans: &[Span<'static>]) -> Option<Vec<Style>> {
    let highlighted = spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    if highlighted != text {
        return None;
    }
    Some(
        spans
            .iter()
            .flat_map(|span| span.content.chars().map(|_| span.style))
            .collect(),
    )
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
