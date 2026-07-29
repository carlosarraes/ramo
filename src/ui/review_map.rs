use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::review_map::{ReviewMapRow, ReviewMapSnapshot};

use super::review::ReviewHeading;
use super::themes::AppTheme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewMapHitTarget {
    ToggleGroup { group_id: String },
    OpenFile { file_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewMapHit {
    pub area: Rect,
    pub target: ReviewMapHitTarget,
}

pub struct ReviewMapWidget<'a> {
    heading: &'a ReviewHeading,
    snapshot: &'a ReviewMapSnapshot,
    theme: &'a AppTheme,
}

impl<'a> ReviewMapWidget<'a> {
    pub fn new(
        heading: &'a ReviewHeading,
        snapshot: &'a ReviewMapSnapshot,
        theme: &'a AppTheme,
    ) -> Self {
        Self {
            heading,
            snapshot,
            theme,
        }
    }
}

impl Widget for ReviewMapWidget<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        buffer.set_style(
            area,
            Style::default()
                .fg(self.theme.text)
                .bg(self.theme.background),
        );
        let layout = map_layout(area);
        render_header(
            layout.header,
            buffer,
            self.heading,
            self.snapshot,
            self.theme,
        );
        render_state(layout.state, buffer, self.snapshot, self.theme);
        render_rows(layout.content, buffer, self.snapshot, self.theme);
        render_footer(layout.footer, buffer, self.snapshot, self.theme);
    }
}

pub fn review_map_hits(area: Rect, snapshot: &ReviewMapSnapshot) -> Vec<ReviewMapHit> {
    let content = map_layout(area).content;
    visible_rows(content, snapshot)
        .into_iter()
        .enumerate()
        .map(|(offset, row)| ReviewMapHit {
            area: Rect::new(
                content.x,
                content.y.saturating_add(offset as u16),
                content.width,
                1,
            ),
            target: match row {
                ReviewMapRow::Group { id, .. } => ReviewMapHitTarget::ToggleGroup {
                    group_id: id.clone(),
                },
                ReviewMapRow::File { id, .. } => ReviewMapHitTarget::OpenFile {
                    file_id: id.clone(),
                },
            },
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct MapLayout {
    header: Rect,
    state: Rect,
    content: Rect,
    footer: Rect,
}

fn map_layout(area: Rect) -> MapLayout {
    let header_height = u16::from(area.height >= 1);
    let footer_height = u16::from(area.height >= 2);
    let state_height = u16::from(area.height >= 3);
    let content_height = area
        .height
        .saturating_sub(header_height + state_height + footer_height);
    MapLayout {
        header: Rect::new(area.x, area.y, area.width, header_height),
        state: Rect::new(
            area.x,
            area.y.saturating_add(header_height),
            area.width,
            state_height,
        ),
        content: Rect::new(
            area.x,
            area.y.saturating_add(header_height + state_height),
            area.width,
            content_height,
        ),
        footer: Rect::new(
            area.x,
            area.bottom().saturating_sub(footer_height),
            area.width,
            footer_height,
        ),
    }
}

fn render_header(
    area: Rect,
    buffer: &mut Buffer,
    heading: &ReviewHeading,
    snapshot: &ReviewMapSnapshot,
    theme: &AppTheme,
) {
    if area.is_empty() {
        return;
    }
    let style = Style::default()
        .fg(theme.text)
        .bg(theme.panel_alt)
        .add_modifier(Modifier::BOLD);
    buffer.set_style(area, style);
    let files = format!(
        "{} {} · ",
        snapshot.totals.files,
        if snapshot.totals.files == 1 {
            "file"
        } else {
            "files"
        }
    );
    let added = format!("+{}", snapshot.totals.additions);
    let removed = format!("−{}", snapshot.totals.deletions);
    let stats_width = width(&files) + width(&added) + 1 + width(&removed);
    let prefix_width = usize::from(area.width).saturating_sub(stats_width);
    let prefix = truncate(&format!("{} · Review Map", heading.label()), prefix_width);
    buffer.set_stringn(area.x, area.y, &prefix, prefix_width, style);
    let mut x = area.x.saturating_add(prefix_width as u16);
    if x < area.right() {
        buffer.set_stringn(x, area.y, &files, usize::from(area.right() - x), style);
        x = x.saturating_add(width(&files) as u16);
    }
    if x < area.right() {
        buffer.set_stringn(
            x,
            area.y,
            &added,
            usize::from(area.right() - x),
            style.fg(theme.added_sign),
        );
        x = x.saturating_add(width(&added) as u16);
    }
    if x < area.right() {
        buffer.set_stringn(x, area.y, " ", 1, style);
        x = x.saturating_add(1);
    }
    if x < area.right() {
        buffer.set_stringn(
            x,
            area.y,
            &removed,
            usize::from(area.right() - x),
            style.fg(theme.removed_sign),
        );
    }
}

fn render_state(area: Rect, buffer: &mut Buffer, snapshot: &ReviewMapSnapshot, theme: &AppTheme) {
    if area.is_empty() {
        return;
    }
    let (message, color) = if let Some(failure) = &snapshot.failure {
        (
            format!("{} · r retry · Esc dismiss", failure.message),
            theme.badge_removed,
        )
    } else {
        match snapshot.status {
            ramo_core::review_map::ReviewMapStatus::Enriched => (
                snapshot.analysis_model.as_ref().map_or_else(
                    || "AI review map ready".into(),
                    |model| format!("AI review map ready · {model}"),
                ),
                theme.badge_added,
            ),
            ramo_core::review_map::ReviewMapStatus::Analyzing => {
                ("Analyzing locally…".into(), theme.accent)
            }
            ramo_core::review_map::ReviewMapStatus::Stale => {
                ("PR changed · refresh required".into(), theme.badge_removed)
            }
            ramo_core::review_map::ReviewMapStatus::Unavailable => (
                "Laptop analysis unavailable · r retry · Esc dismiss".into(),
                theme.badge_neutral,
            ),
            ramo_core::review_map::ReviewMapStatus::Failed => (
                "Local analysis failed · r retry · Esc dismiss".into(),
                theme.badge_removed,
            ),
            ramo_core::review_map::ReviewMapStatus::Ready => {
                ("Exact diff structure".into(), theme.muted)
            }
        }
    };
    let style = Style::default().fg(color).bg(theme.background);
    buffer.set_stringn(
        area.x.saturating_add(1),
        area.y,
        truncate(&message, usize::from(area.width.saturating_sub(2))),
        usize::from(area.width.saturating_sub(2)),
        style,
    );
}

fn render_rows(area: Rect, buffer: &mut Buffer, snapshot: &ReviewMapSnapshot, theme: &AppTheme) {
    for (offset, row) in visible_rows(area, snapshot).into_iter().enumerate() {
        let y = area.y.saturating_add(offset as u16);
        let selected = snapshot.selected_id.as_deref() == Some(row.id());
        let background = if selected {
            theme.selected_hunk
        } else {
            theme.background
        };
        let base = Style::default().fg(theme.text).bg(background);
        buffer.set_style(Rect::new(area.x, y, area.width, 1), base);
        let marker = if selected { "›" } else { " " };
        match row {
            ReviewMapRow::Group {
                label,
                additions,
                deletions,
                expanded,
                summary,
                ..
            } => {
                let fold = if *expanded { "▾" } else { "▸" };
                let stats = format!("+{additions} −{deletions}");
                let summary = if area.width >= 70 {
                    summary
                        .as_deref()
                        .map(|summary| format!(" — {summary}"))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let prefix = format!("{marker} {fold} {label}{summary}");
                render_row_with_stats(area, y, buffer, &prefix, &stats, base, theme);
            }
            ReviewMapRow::File {
                path,
                additions,
                deletions,
                reviewed,
                recommended_order,
                summary,
                ..
            } => {
                let order = recommended_order
                    .map_or_else(|| "  ".into(), |order| format!("{} ", order_label(order)));
                let viewed = if *reviewed { "✓ " } else { "" };
                let summary = if area.width >= 70 {
                    summary
                        .as_deref()
                        .map(|summary| format!(" — {summary}"))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let prefix = format!("{marker}   {viewed}{order}{path}{summary}");
                let stats = format!("+{additions} −{deletions}");
                render_row_with_stats(area, y, buffer, &prefix, &stats, base, theme);
            }
        }
    }
}

fn render_row_with_stats(
    area: Rect,
    y: u16,
    buffer: &mut Buffer,
    prefix: &str,
    stats: &str,
    base: Style,
    theme: &AppTheme,
) {
    let stats_width = width(stats);
    let prefix_width = usize::from(area.width).saturating_sub(stats_width + 1);
    buffer.set_stringn(
        area.x,
        y,
        truncate(prefix, prefix_width),
        prefix_width,
        base,
    );
    if stats_width < usize::from(area.width) {
        let stats_x = area.right().saturating_sub(stats_width as u16);
        let (added, removed) = stats.split_once(' ').unwrap_or((stats, ""));
        buffer.set_stringn(stats_x, y, added, width(added), base.fg(theme.added_sign));
        buffer.set_stringn(
            stats_x.saturating_add(width(added) as u16 + 1),
            y,
            removed,
            width(removed),
            base.fg(theme.removed_sign),
        );
    }
}

fn render_footer(area: Rect, buffer: &mut Buffer, snapshot: &ReviewMapSnapshot, theme: &AppTheme) {
    if area.is_empty() {
        return;
    }
    let style = Style::default().fg(theme.muted).bg(theme.panel_alt);
    buffer.set_style(area, style);
    let progress = format!("{}% reviewed", snapshot.reviewed_percent);
    let help_width = usize::from(area.width).saturating_sub(width(&progress) + 1);
    buffer.set_stringn(
        area.x,
        area.y,
        truncate("M code · Enter open · / filter · ? help", help_width),
        help_width,
        style,
    );
    if width(&progress) <= usize::from(area.width) {
        buffer.set_stringn(
            area.right().saturating_sub(width(&progress) as u16),
            area.y,
            &progress,
            width(&progress),
            style.fg(theme.text),
        );
    }
}

fn visible_rows(area: Rect, snapshot: &ReviewMapSnapshot) -> Vec<&ReviewMapRow> {
    let height = usize::from(area.height);
    if height == 0 || snapshot.rows.is_empty() {
        return Vec::new();
    }
    let selected = snapshot
        .selected_id
        .as_deref()
        .and_then(|id| snapshot.rows.iter().position(|row| row.id() == id))
        .unwrap_or_default();
    let start = selected.saturating_add(1).saturating_sub(height);
    snapshot.rows.iter().skip(start).take(height).collect()
}

fn order_label(order: usize) -> String {
    const CIRCLED: [&str; 10] = ["①", "②", "③", "④", "⑤", "⑥", "⑦", "⑧", "⑨", "⑩"];
    CIRCLED
        .get(order.saturating_sub(1))
        .map_or_else(|| order.to_string(), |label| (*label).into())
}

fn width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

fn truncate(value: &str, maximum: usize) -> String {
    if width(value) <= maximum {
        return value.to_owned();
    }
    if maximum == 0 {
        return String::new();
    }
    let ellipsis = if maximum >= 1 { "…" } else { "" };
    let target = maximum.saturating_sub(width(ellipsis));
    let mut output = String::new();
    for character in value.chars() {
        let candidate = format!("{output}{character}");
        if width(&candidate) > target {
            break;
        }
        output.push(character);
    }
    output.push_str(ellipsis);
    output
}
