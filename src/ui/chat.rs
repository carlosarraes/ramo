use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

use crate::chat::{ChatState, ChatTurn};
use crate::review::row::wrap_note_text;

use super::text_input::TextInput;
use super::themes::AppTheme;

const EMPTY: &str =
    "Ask about this pull request. The model can read the repository, but cannot change anything.";

pub struct ChatPane<'a> {
    turns: &'a [ChatTurn],
    draft: &'a TextInput,
    focused: bool,
    theme: &'a AppTheme,
}

impl<'a> ChatPane<'a> {
    pub fn new(
        turns: &'a [ChatTurn],
        draft: &'a TextInput,
        focused: bool,
        theme: &'a AppTheme,
    ) -> Self {
        Self {
            turns,
            draft,
            focused,
            theme,
        }
    }
}

impl Widget for ChatPane<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        buffer.set_style(
            area,
            Style::default().fg(self.theme.text).bg(self.theme.panel),
        );
        // A one-column rule separates the pane from the diff without stealing a whole column
        // of text width from either side.
        for y in area.y..area.bottom() {
            buffer.set_stringn(
                area.x,
                y,
                "│",
                1,
                Style::default().fg(self.theme.border).bg(self.theme.panel),
            );
        }
        let inner = Rect::new(
            area.x.saturating_add(2),
            area.y,
            area.width.saturating_sub(3),
            area.height,
        );
        if inner.width == 0 || inner.height < 3 {
            return;
        }
        let width = usize::from(inner.width);

        // The input owns the last two rows; the transcript fills what is left, scrolled to the
        // end so the newest exchange is always the one on screen.
        let input_top = inner.bottom().saturating_sub(2);
        let mut lines: Vec<(String, Style)> = Vec::new();
        let question_style = Style::default()
            .fg(self.theme.accent)
            .bg(self.theme.panel)
            .add_modifier(Modifier::BOLD);
        let body_style = Style::default().fg(self.theme.text).bg(self.theme.panel);
        let muted = Style::default().fg(self.theme.muted).bg(self.theme.panel);
        if self.turns.is_empty() {
            for line in wrap_note_text(EMPTY, width) {
                lines.push((line, muted));
            }
        }
        for turn in self.turns {
            for line in wrap_note_text(&format!("you: {}", turn.question), width) {
                lines.push((line, question_style));
            }
            let (text, style) = match &turn.state {
                ChatState::Pending => ("thinking…".to_owned(), muted),
                ChatState::Answered(answer) => (answer.clone(), body_style),
                ChatState::Failed(error) => (error.clone(), muted),
            };
            for line in wrap_note_text(&text, width) {
                lines.push((line, style));
            }
            lines.push((String::new(), body_style));
        }
        let visible = usize::from(input_top.saturating_sub(inner.y));
        let start = lines.len().saturating_sub(visible);
        for (row, (line, style)) in lines.iter().skip(start).take(visible).enumerate() {
            buffer.set_stringn(inner.x, inner.y + row as u16, line, width, *style);
        }

        buffer.set_stringn(
            inner.x,
            input_top,
            "─".repeat(width),
            width,
            Style::default().fg(self.theme.border).bg(self.theme.panel),
        );
        let prompt = format!("> {}", self.draft.value());
        let input_style = if self.focused { body_style } else { muted };
        buffer.set_stringn(
            inner.x,
            input_top.saturating_add(1),
            &prompt,
            width,
            input_style,
        );
        if self.focused {
            // The caret sits after "> " plus however many characters precede it.
            let column = 2 + self.draft.caret().min(self.draft.char_count());
            if column < width
                && let Some(cell) =
                    buffer.cell_mut((inner.x.saturating_add(column as u16), input_top + 1))
            {
                cell.set_style(
                    Style::default()
                        .fg(self.theme.panel)
                        .bg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                );
            }
        }
    }
}
