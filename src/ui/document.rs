use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::review::row::wrap_note_text;

use super::themes::AppTheme;

pub const HORIZONTAL_PADDING: u16 = 1;
/// Title row, subtitle row, footer row.
const CHROME_ROWS: u16 = 3;

/// A wrapped, scrollable block of plain text filling a whole screen.
///
/// It remembers the body height it was last drawn at, so scrolling never has to be handed a
/// height by the caller — which is what previously let `G` stop short, because the app passed
/// the diff viewport height while the widget drew against the full frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollableDocument {
    lines: Vec<String>,
    width: u16,
    body_height: u16,
    offset: usize,
}

impl ScrollableDocument {
    pub fn new(body: &str, empty_placeholder: &str, width: u16) -> Self {
        Self {
            lines: wrap(body, empty_placeholder, width),
            width,
            body_height: 1,
            offset: 0,
        }
    }

    /// Called from the widget on every draw, so the document always knows its real geometry.
    pub fn fit(&mut self, body: &str, empty_placeholder: &str, width: u16, height: u16) {
        if width != self.width {
            self.lines = wrap(body, empty_placeholder, width);
            self.width = width;
        }
        self.body_height = height.saturating_sub(CHROME_ROWS).max(1);
        self.clamp();
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn body_height(&self) -> u16 {
        self.body_height
    }

    pub fn scroll_lines(&mut self, delta: i32) {
        self.offset = self.offset.saturating_add_signed(delta as isize);
        self.clamp();
    }

    pub fn scroll_pages(&mut self, pages: i32) {
        let page = i32::from(self.body_height).max(1);
        self.scroll_lines(pages.saturating_mul(page));
    }

    pub fn scroll_half_pages(&mut self, halves: i32) {
        let half = (i32::from(self.body_height) / 2).max(1);
        self.scroll_lines(halves.saturating_mul(half));
    }

    pub fn scroll_to_top(&mut self) {
        self.offset = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.offset = self.max_offset();
    }

    /// Fully-visible text reads as 100%, so the indicator never implies more to scroll to.
    pub fn scrolled_percent(&self) -> u16 {
        let max = self.max_offset();
        if max == 0 {
            return 100;
        }
        ((self.offset.min(max) * 100) / max) as u16
    }

    fn clamp(&mut self) {
        self.offset = self.offset.min(self.max_offset());
    }

    fn max_offset(&self) -> usize {
        self.lines
            .len()
            .saturating_sub(usize::from(self.body_height))
    }
}

fn wrap(body: &str, empty_placeholder: &str, width: u16) -> Vec<String> {
    let body = crate::input::sanitize_terminal_text(body, false);
    let body = if body.trim().is_empty() {
        empty_placeholder.to_owned()
    } else {
        body
    };
    let content = usize::from(width.saturating_sub(HORIZONTAL_PADDING * 2)).max(1);
    wrap_note_text(&body, content)
}

/// Shared chrome for every full-screen document: bold title, muted subtitle, body, footer with
/// a right-aligned scroll percentage.
pub fn render_document(
    area: Rect,
    buffer: &mut Buffer,
    theme: &AppTheme,
    title: &str,
    subtitle: &str,
    footer: &str,
    document: &ScrollableDocument,
) {
    if area.is_empty() {
        return;
    }
    buffer.set_style(area, Style::default().fg(theme.text).bg(theme.background));
    if area.height >= 1 {
        render_strip(
            Rect::new(area.x, area.y, area.width, 1),
            buffer,
            title,
            Style::default()
                .fg(theme.text)
                .bg(theme.panel)
                .add_modifier(Modifier::BOLD),
        );
    }
    if area.height >= 2 {
        render_strip(
            Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
            buffer,
            subtitle,
            Style::default().fg(theme.muted).bg(theme.panel),
        );
    }
    let body_height = area.height.saturating_sub(CHROME_ROWS);
    if body_height > 0 {
        let style = Style::default().fg(theme.text).bg(theme.background);
        let width = usize::from(area.width.saturating_sub(HORIZONTAL_PADDING * 2));
        for (row, line) in document
            .lines
            .iter()
            .skip(document.offset)
            .take(usize::from(body_height))
            .enumerate()
        {
            buffer.set_stringn(
                area.x.saturating_add(HORIZONTAL_PADDING),
                area.y.saturating_add(2).saturating_add(row as u16),
                truncate(line, width),
                width,
                style,
            );
        }
    }
    if area.height >= CHROME_ROWS {
        render_footer(
            Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
            buffer,
            footer,
            document.scrolled_percent(),
            theme,
        );
    }
}

fn render_strip(area: Rect, buffer: &mut Buffer, text: &str, style: Style) {
    buffer.set_style(area, style);
    let width = usize::from(area.width.saturating_sub(HORIZONTAL_PADDING * 2));
    buffer.set_stringn(
        area.x.saturating_add(HORIZONTAL_PADDING),
        area.y,
        truncate(text, width),
        width,
        style,
    );
}

fn render_footer(area: Rect, buffer: &mut Buffer, help: &str, percent: u16, theme: &AppTheme) {
    let style = Style::default().fg(theme.muted).bg(theme.panel_alt);
    buffer.set_style(area, style);
    let progress = format!("{percent}%");
    let help_width = usize::from(area.width).saturating_sub(width(&progress) + 1);
    buffer.set_stringn(
        area.x,
        area.y,
        truncate(help, help_width),
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
    let target = maximum.saturating_sub(1);
    let mut output = String::new();
    for character in value.chars() {
        if width(&output) + UnicodeWidthStr::width(character.to_string().as_str()) > target {
            break;
        }
        output.push(character);
    }
    output.push('…');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(body: &str) -> ScrollableDocument {
        let mut document = ScrollableDocument::new(body, "empty", 40);
        document.fit(body, "empty", 40, 13);
        document
    }

    #[test]
    fn an_empty_body_renders_the_placeholder() {
        for body in ["", "   \n\n  "] {
            assert_eq!(document(body).lines(), ["empty"]);
        }
    }

    #[test]
    fn g_reaches_the_true_last_line() {
        let body = (1..=50)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut document = document(&body);

        document.scroll_to_bottom();
        // Body height is 13 - 3 chrome rows = 10, so the last screen starts at line 40.
        assert_eq!(document.offset(), 40);
        assert_eq!(document.scrolled_percent(), 100);
        // And the last line is actually visible from there.
        assert_eq!(document.lines()[document.offset() + 9], "line 50");
    }

    #[test]
    fn scrolling_clamps_at_both_ends() {
        let body = (1..=50)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut document = document(&body);

        document.scroll_lines(-5);
        assert_eq!(document.offset(), 0);
        document.scroll_pages(99);
        assert_eq!(document.offset(), 40);
        document.scroll_to_top();
        assert_eq!(document.offset(), 0);
        document.scroll_half_pages(1);
        assert_eq!(document.offset(), 5);
    }

    #[test]
    fn text_that_fits_reads_as_fully_scrolled() {
        assert_eq!(document("short").scrolled_percent(), 100);
    }

    #[test]
    fn a_narrower_terminal_rewraps_and_clamps() {
        let body = (1..=50)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut document = document(&body);
        document.scroll_to_bottom();
        document.fit(&body, "empty", 20, 13);
        assert!(document.offset() <= document.max_offset());
    }
}
