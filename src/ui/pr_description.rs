use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;
use unicode_width::UnicodeWidthStr;

use crate::remote_review::PullRequestReviewContext;
use crate::review::row::wrap_note_text;

use super::themes::AppTheme;

pub const EMPTY_DESCRIPTION: &str = "This pull request has no description.";
const FOOTER_HELP: &str = "j/k scroll · d/u half page · g/G ends · P back";
const HORIZONTAL_PADDING: u16 = 1;

/// The description wrapped to a known width. Wrapping is the expensive part, so the app
/// keeps one of these and rebuilds it only when the terminal width changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDescription {
    lines: Vec<String>,
    width: u16,
    offset: usize,
}

impl PrDescription {
    pub fn new(body: &str, width: u16) -> Self {
        Self {
            lines: wrap(body, width),
            width,
            offset: 0,
        }
    }

    /// Re-wraps on a width change and clamps the offset so a resize cannot strand the
    /// view past the end of the text.
    pub fn resize(&mut self, body: &str, width: u16, height: u16) {
        if width == self.width {
            return;
        }
        self.lines = wrap(body, width);
        self.width = width;
        self.clamp(height);
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn scroll(&mut self, delta: i32, height: u16) {
        self.offset = self.offset.saturating_add_signed(delta as isize);
        self.clamp(height);
    }

    pub fn scroll_to_top(&mut self) {
        self.offset = 0;
    }

    pub fn scroll_to_bottom(&mut self, height: u16) {
        self.offset = usize::MAX;
        self.clamp(height);
    }

    fn clamp(&mut self, height: u16) {
        self.offset = self.offset.min(self.max_offset(height));
    }

    fn max_offset(&self, height: u16) -> usize {
        self.lines
            .len()
            .saturating_sub(usize::from(content_height(height)))
    }

    /// How far through the description the viewport currently sits. Fully-visible text
    /// reads as 100%, so the indicator never suggests there is more to scroll to.
    pub fn scrolled_percent(&self, height: u16) -> u16 {
        let max = self.max_offset(height);
        if max == 0 {
            return 100;
        }
        ((self.offset.min(max) * 100) / max) as u16
    }
}

fn wrap(body: &str, width: u16) -> Vec<String> {
    let body = crate::input::sanitize_terminal_text(body, false);
    let body = if body.trim().is_empty() {
        EMPTY_DESCRIPTION.to_owned()
    } else {
        body
    };
    let content = usize::from(width.saturating_sub(HORIZONTAL_PADDING * 2)).max(1);
    wrap_note_text(&body, content)
}

/// Header takes two rows (title, then branches), footer one.
fn content_height(height: u16) -> u16 {
    height.saturating_sub(3)
}

pub struct PrDescriptionWidget<'a> {
    context: &'a PullRequestReviewContext,
    description: &'a PrDescription,
    theme: &'a AppTheme,
}

impl<'a> PrDescriptionWidget<'a> {
    pub fn new(
        context: &'a PullRequestReviewContext,
        description: &'a PrDescription,
        theme: &'a AppTheme,
    ) -> Self {
        Self {
            context,
            description,
            theme,
        }
    }
}

impl Widget for PrDescriptionWidget<'_> {
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
        let header_height = u16::from(area.height >= 1);
        let subtitle_height = u16::from(area.height >= 2);
        let footer_height = u16::from(area.height >= 3);
        let body_height = area
            .height
            .saturating_sub(header_height + subtitle_height + footer_height);

        if header_height == 1 {
            let title = format!("PR #{} · {}", self.context.number, self.context.title);
            render_strip(
                Rect::new(area.x, area.y, area.width, 1),
                buffer,
                &title,
                Style::default()
                    .fg(self.theme.text)
                    .bg(self.theme.panel)
                    .add_modifier(Modifier::BOLD),
            );
        }
        if subtitle_height == 1 {
            let subtitle = format!(
                "{} wants to merge {} into {}",
                self.context.author_login, self.context.head_ref, self.context.base_ref
            );
            render_strip(
                Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
                buffer,
                &subtitle,
                Style::default().fg(self.theme.muted).bg(self.theme.panel),
            );
        }
        if body_height > 0 {
            let body = Rect::new(
                area.x,
                area.y.saturating_add(header_height + subtitle_height),
                area.width,
                body_height,
            );
            let style = Style::default()
                .fg(self.theme.text)
                .bg(self.theme.background);
            for (row, line) in self
                .description
                .lines
                .iter()
                .skip(self.description.offset)
                .take(usize::from(body_height))
                .enumerate()
            {
                let width = usize::from(body.width.saturating_sub(HORIZONTAL_PADDING * 2));
                buffer.set_stringn(
                    body.x.saturating_add(HORIZONTAL_PADDING),
                    body.y.saturating_add(row as u16),
                    truncate(line, width),
                    width,
                    style,
                );
            }
        }
        if footer_height == 1 {
            render_footer(
                Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1),
                buffer,
                self.description.scrolled_percent(area.height),
                self.theme,
            );
        }
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

fn render_footer(area: Rect, buffer: &mut Buffer, percent: u16, theme: &AppTheme) {
    let style = Style::default().fg(theme.muted).bg(theme.panel_alt);
    buffer.set_style(area, style);
    let progress = format!("{percent}%");
    let help_width = usize::from(area.width).saturating_sub(width(&progress) + 1);
    buffer.set_stringn(
        area.x,
        area.y,
        truncate(FOOTER_HELP, help_width),
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
        if width(&output) + character.to_string().width() > target {
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

    const HEIGHT: u16 = 10;

    fn description(body: &str) -> PrDescription {
        PrDescription::new(body, 40)
    }

    #[test]
    fn an_empty_body_renders_the_placeholder() {
        for body in ["", "   \n\n  "] {
            let rendered = description(body);
            assert_eq!(rendered.lines(), [EMPTY_DESCRIPTION]);
        }
    }

    #[test]
    fn indentation_survives_wrapping() {
        let body = "- top level\n    - nested item that is long enough to wrap onto another line";
        let rendered = description(body);

        let nested = rendered
            .lines()
            .iter()
            .filter(|line| line.starts_with("    "))
            .count();
        assert!(
            nested >= 2,
            "the continuation must hang under the indent: {:?}",
            rendered.lines()
        );
    }

    #[test]
    fn scrolling_clamps_at_both_ends() {
        let body = (1..=50)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut rendered = description(&body);
        assert_eq!(rendered.offset(), 0);

        rendered.scroll(-5, HEIGHT);
        assert_eq!(rendered.offset(), 0, "cannot scroll above the top");

        rendered.scroll_to_bottom(HEIGHT);
        let bottom = rendered.offset();
        assert_eq!(bottom, 50 - usize::from(content_height(HEIGHT)));
        rendered.scroll(10, HEIGHT);
        assert_eq!(rendered.offset(), bottom, "cannot scroll past the end");
        assert_eq!(rendered.scrolled_percent(HEIGHT), 100);

        rendered.scroll_to_top();
        assert_eq!(rendered.offset(), 0);
        assert_eq!(rendered.scrolled_percent(HEIGHT), 0);
    }

    #[test]
    fn text_that_fits_reads_as_fully_scrolled() {
        let rendered = description("short");
        assert_eq!(rendered.scrolled_percent(HEIGHT), 100);
    }

    #[test]
    fn a_resize_rewraps_and_clamps_the_offset() {
        let body = (1..=50)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut rendered = description(&body);
        rendered.scroll_to_bottom(HEIGHT);
        let tall = rendered.offset();

        // Same width is a no-op; a real change re-wraps and clamps.
        rendered.resize(&body, 40, HEIGHT);
        assert_eq!(rendered.offset(), tall);

        rendered.resize(&body, 20, HEIGHT);
        assert!(rendered.offset() <= rendered.max_offset(HEIGHT));
    }
}
