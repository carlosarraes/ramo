use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use super::themes::AppTheme;

pub const AGENT_SKILL_PROMPT: &str = "Load the pdiff skill and use it for this review. Run `pdiff skill path` to get the native skill path.";

pub fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x.saturating_add(area.width.saturating_sub(width) / 2),
        area.y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    )
}

pub fn help_text(can_refresh: bool) -> String {
    let quit = if can_refresh {
        "r / q       reload / quit"
    } else {
        "q           quit"
    };
    format!(
        "Navigation\n\
         ↑ / ↓       move line-by-line\n\
         Space / f   page down\n\
         b           page up\n\
         Shift+Space page up\n\
         d / u       half page down / up\n\
         [ / ]       previous / next hunk\n\
         , / .       previous / next file\n\
         {{ / }}       previous / next comment\n\
         ← / →       scroll code (Shift = faster)\n\
         g / G       top / bottom\n\
         \nView\n\
         1 / 2 / 0   split / stack / auto\n\
         s / t       sidebar / theme selector\n\
         a / z       AI notes / unchanged context\n\
         A           agent skill setup\n\
         l / w / m   lines / wrap / hunk headers\n\
         e           open file in editor\n\
         \nReview\n\
         /           focus file filter\n\
         c           create review note\n\
         Tab         toggle files/filter focus\n\
         ?           close help\n\
         {quit}"
    )
}

#[derive(Debug, Clone)]
pub struct ThemeSelection {
    ids: Vec<&'static str>,
    original: String,
    selected: usize,
}

impl ThemeSelection {
    pub fn new(ids: Vec<&'static str>, current: &str) -> Self {
        let selected = ids.iter().position(|id| *id == current).unwrap_or(0);
        Self {
            ids,
            original: current.to_owned(),
            selected,
        }
    }

    pub fn move_by(&mut self, delta: i32) {
        if self.ids.is_empty() {
            return;
        }
        self.selected =
            (self.selected as i64 + i64::from(delta)).rem_euclid(self.ids.len() as i64) as usize;
    }

    pub fn preview_id(&self) -> &str {
        self.ids
            .get(self.selected)
            .copied()
            .unwrap_or(&self.original)
    }

    pub fn cancel_id(&self) -> &str {
        &self.original
    }

    pub fn confirm_id(&self) -> &str {
        self.preview_id()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn ids(&self) -> &[&'static str] {
        &self.ids
    }
}

pub enum DialogOverlay<'a> {
    Help {
        theme: &'a AppTheme,
        can_refresh: bool,
    },
    AgentSkill {
        theme: &'a AppTheme,
    },
    Theme {
        theme: &'a AppTheme,
        ids: &'a [&'a str],
        selected: usize,
    },
    Save {
        theme: &'a AppTheme,
    },
    Note {
        theme: &'a AppTheme,
        text: &'a str,
    },
}

impl<'a> DialogOverlay<'a> {
    pub fn help(theme: &'a AppTheme, can_refresh: bool) -> Self {
        Self::Help { theme, can_refresh }
    }
    pub fn agent_skill(theme: &'a AppTheme) -> Self {
        Self::AgentSkill { theme }
    }
    pub fn theme(theme: &'a AppTheme, ids: &'a [&'a str], selected: usize) -> Self {
        Self::Theme {
            theme,
            ids,
            selected,
        }
    }
    pub fn save(theme: &'a AppTheme) -> Self {
        Self::Save { theme }
    }
    pub fn note(theme: &'a AppTheme, text: &'a str) -> Self {
        Self::Note { theme, text }
    }
}

impl Widget for DialogOverlay<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        match self {
            Self::Help { theme, can_refresh } => {
                let dialog = centered_rect(74, 30, area);
                render_dialog(
                    dialog,
                    buffer,
                    theme,
                    "Controls help",
                    help_text(can_refresh),
                );
            }
            Self::AgentSkill { theme } => render_dialog(
                centered_rect(78, 11, area),
                buffer,
                theme,
                "Agent skill",
                format!(
                    "{}\n\ny / Enter copy   Esc close",
                    AGENT_SKILL_PROMPT.replace("review. Run", "review.\n\nRun")
                ),
            ),
            Self::Theme {
                theme,
                ids,
                selected,
            } => {
                let dialog = centered_rect(48, (ids.len() as u16).saturating_add(4).min(22), area);
                let lines = ids
                    .iter()
                    .enumerate()
                    .map(|(index, id)| {
                        let marker = if index == selected { "› " } else { "  " };
                        Line::from(vec![
                            Span::styled(marker, Style::default().fg(theme.accent)),
                            Span::styled((*id).to_owned(), Style::default().fg(theme.text)),
                        ])
                    })
                    .collect::<Vec<_>>();
                render_lines(dialog, buffer, theme, "Theme", lines);
            }
            Self::Save { theme } => render_dialog(
                centered_rect(58, 8, area),
                buffer,
                theme,
                "Save view preferences?",
                "Enter/s save   q discard   n never ask   Esc cancel".into(),
            ),
            Self::Note { theme, text } => render_dialog(
                centered_rect(68, 12, area),
                buffer,
                theme,
                "Review note",
                format!("{text}\n\nCtrl-S save   Esc cancel"),
            ),
        }
    }
}

fn render_dialog(area: Rect, buffer: &mut Buffer, theme: &AppTheme, title: &str, text: String) {
    let lines = text
        .lines()
        .map(|line| Line::from(line.to_owned()))
        .collect::<Vec<_>>();
    render_lines(area, buffer, theme, title, lines);
}

fn render_lines(
    area: Rect,
    buffer: &mut Buffer,
    theme: &AppTheme,
    title: &str,
    lines: Vec<Line<'static>>,
) {
    Clear.render(area, buffer);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme.note_title_text)
                .bg(theme.note_title_background)
                .add_modifier(Modifier::BOLD),
        ))
        .border_style(Style::default().fg(theme.note_border))
        .style(Style::default().fg(theme.text).bg(theme.note_background));
    Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .render(area, buffer);
}
