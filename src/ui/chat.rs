use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

use crate::chat::{ChatState, ChatTurn};
use crate::review::row::{wrap_note_text, wrap_note_text_indexed};

use super::text_input::TextInput;
use super::themes::AppTheme;

const EMPTY: &str =
    "Ask about this pull request. The model can read the repository, but cannot change anything.";

/// A draft can grow to this many rows before the input scrolls internally; past it the pane
/// would be all composer and no conversation.
const MAX_INPUT_ROWS: u16 = 6;

/// Only the chrome differs between the two layouts — the transcript, the growing composer and the
/// scroll maths below are all pure functions of `area`, so they serve both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatPresentation {
    /// Beside the diff: a vertical rule, and text inset past it.
    Side,
    /// The whole screen: no rule, symmetric inset, and a title row like the other overlays.
    Full,
}

pub struct ChatPane<'a> {
    turns: &'a [ChatTurn],
    draft: &'a TextInput,
    focused: bool,
    scroll: usize,
    presentation: ChatPresentation,
    title: Option<&'a str>,
    theme: &'a AppTheme,
}

impl<'a> ChatPane<'a> {
    pub fn new(
        turns: &'a [ChatTurn],
        draft: &'a TextInput,
        focused: bool,
        scroll: usize,
        theme: &'a AppTheme,
    ) -> Self {
        Self {
            turns,
            draft,
            focused,
            scroll,
            presentation: ChatPresentation::Side,
            title: None,
            theme,
        }
    }

    pub fn presentation(mut self, presentation: ChatPresentation, title: Option<&'a str>) -> Self {
        self.presentation = presentation;
        self.title = title;
        self
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
        let full = self.presentation == ChatPresentation::Full;
        if !full {
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
        }
        let mut inner = if full {
            Rect::new(
                area.x.saturating_add(1),
                area.y,
                area.width.saturating_sub(2),
                area.height,
            )
        } else {
            Rect::new(
                area.x.saturating_add(2),
                area.y,
                area.width.saturating_sub(3),
                area.height,
            )
        };
        if full && inner.height > 2 {
            // A title row, so a full-screen chat announces itself the way the other overlays do.
            buffer.set_stringn(
                inner.x,
                inner.y,
                self.title.unwrap_or("Chat"),
                usize::from(inner.width),
                Style::default()
                    .fg(self.theme.text)
                    .bg(self.theme.panel)
                    .add_modifier(Modifier::BOLD),
            );
            inner = Rect::new(
                inner.x,
                inner.y.saturating_add(1),
                inner.width,
                inner.height.saturating_sub(1),
            );
        }
        if inner.width == 0 || inner.height < 3 {
            return;
        }
        let width = usize::from(inner.width);

        // The composer grows with the draft and the transcript takes what is left, so a second
        // line of typing pushes the conversation up rather than falling off the pane.
        let content = width.saturating_sub(2).max(1);
        let draft_lines = wrapped_draft(self.draft.value(), content);
        let input_rows = (draft_lines.len() as u16).clamp(1, MAX_INPUT_ROWS);
        let input_top = inner.bottom().saturating_sub(input_rows.saturating_add(1));
        if input_top <= inner.y {
            return;
        }

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
        let held_back = self.scroll.min(lines.len().saturating_sub(visible));
        let start = lines.len().saturating_sub(visible + held_back);
        for (row, (line, style)) in lines.iter().skip(start).take(visible).enumerate() {
            buffer.set_stringn(inner.x, inner.y + row as u16, line, width, *style);
        }

        let rule = if held_back == 0 {
            "─".repeat(width)
        } else {
            // Only shown while scrolled back, so the pane never looks stuck away from the newest
            // reply without saying why.
            let note = format!(" ↑ {held_back} more ");
            let bar = width.saturating_sub(note.chars().count());
            format!("{}{note}", "─".repeat(bar))
        };
        buffer.set_stringn(
            inner.x,
            input_top,
            &rule,
            width,
            Style::default().fg(self.theme.border).bg(self.theme.panel),
        );

        let input_style = if self.focused { body_style } else { muted };
        let caret = caret_position(self.draft, &draft_lines);
        // With more draft than rows, follow the caret rather than pinning to the top.
        let first = caret
            .0
            .saturating_sub(usize::from(input_rows).saturating_sub(1));
        for row in 0..usize::from(input_rows) {
            let Some(line) = draft_lines.get(first + row) else {
                break;
            };
            let prefix = if first + row == 0 { "> " } else { "  " };
            buffer.set_stringn(
                inner.x,
                input_top.saturating_add(1 + row as u16),
                format!("{prefix}{line}"),
                width,
                input_style,
            );
        }
        if self.focused
            && let Some(row) = caret.0.checked_sub(first)
            && row < usize::from(input_rows)
        {
            let column = 2 + caret.1;
            if column < width
                && let Some(cell) = buffer.cell_mut((
                    inner.x.saturating_add(column as u16),
                    input_top.saturating_add(1 + row as u16),
                ))
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

/// Wraps the draft for display. A trailing newline has to survive as its own empty row, or the
/// caret would sit on a line the reviewer cannot see after pressing Shift-Enter.
fn wrapped_draft(value: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = wrap_note_text_indexed(value, width)
        .into_iter()
        .map(|line| line.text)
        .collect();
    if value.is_empty() || value.ends_with('\n') {
        lines.push(String::new());
    }
    lines
}

/// Maps the caret's character offset onto a (row, column) in the wrapped draft.
fn caret_position(draft: &TextInput, lines: &[String]) -> (usize, usize) {
    let caret = draft.caret().min(draft.char_count());
    let mut seen = 0;
    for (row, line) in lines.iter().enumerate() {
        let count = line.chars().count();
        // `<=` so a caret at the end of a line lands after its last character rather than
        // jumping to the start of the next one.
        if caret <= seen + count {
            return (row, caret - seen);
        }
        // The wrap consumed either a newline or the space it broke on.
        seen += count + 1;
    }
    lines
        .last()
        .map_or((0, 0), |line| (lines.len() - 1, line.chars().count()))
}
