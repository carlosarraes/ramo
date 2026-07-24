use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use super::themes::AppTheme;

pub const AGENT_SKILL_PROMPT: &str = "Load the ramo skill and use it for this review. Run `ramo skill path` to get the native skill path.";

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
         j / k, ↑ / ↓ previous / next line\n\
         h / l       focus left / right\n\
         Space / f   page down\n\
         b           page up\n\
         Shift+Space page up\n\
         d / u / ^D / ^U half page down / up\n\
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
         n / w / m   numbers / wrap / hunk headers\n\
         e           open file in editor\n\
         \nReview\n\
         /           focus file filter\n\
         c           create review note\n\
         Enter save  Shift+Enter newline\n\
         Ctrl-S save Ctrl-T send\n\
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
    Tmux {
        theme: &'a AppTheme,
        panes: &'a [crate::tmux::TmuxPane],
        selected: usize,
    },
    Publish {
        theme: &'a AppTheme,
        number: u64,
        count: usize,
    },
    Verdict {
        theme: &'a AppTheme,
        self_authored: bool,
        body: &'a str,
    },
    OverallComment {
        theme: &'a AppTheme,
        text: &'a str,
    },
    Message {
        theme: &'a AppTheme,
        title: &'a str,
        body: &'a str,
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
    pub fn tmux(theme: &'a AppTheme, panes: &'a [crate::tmux::TmuxPane], selected: usize) -> Self {
        Self::Tmux {
            theme,
            panes,
            selected,
        }
    }
    pub fn publish(theme: &'a AppTheme, number: u64, count: usize) -> Self {
        Self::Publish {
            theme,
            number,
            count,
        }
    }
    pub fn verdict(theme: &'a AppTheme, self_authored: bool, body: &'a str) -> Self {
        Self::Verdict {
            theme,
            self_authored,
            body,
        }
    }
    pub fn overall_comment(theme: &'a AppTheme, text: &'a str) -> Self {
        Self::OverallComment { theme, text }
    }
    pub fn message(theme: &'a AppTheme, title: &'a str, body: &'a str) -> Self {
        Self::Message { theme, title, body }
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
                format!(
                    "{text}\n\nEnter save   Shift+Enter newline\nCtrl-S save   Ctrl-T send   Esc cancel"
                ),
            ),
            Self::Tmux {
                theme,
                panes,
                selected,
            } => {
                let dialog =
                    centered_rect(82, (panes.len() as u16).saturating_add(5).min(22), area);
                let mut lines = vec![
                    Line::from("Enter send   Esc cancel".to_owned()),
                    Line::from("j/k move   g/G first/last".to_owned()),
                ];
                lines.extend(panes.iter().enumerate().map(|(index, pane)| {
                    let marker = if index == selected { "› " } else { "  " };
                    Line::from(vec![
                        Span::styled(marker, Style::default().fg(theme.accent)),
                        Span::styled(pane.label.clone(), Style::default().fg(theme.text)),
                    ])
                }));
                render_lines(dialog, buffer, theme, "Send to tmux", lines);
            }
            Self::Publish {
                theme,
                number,
                count,
            } => {
                let question = if count == 0 {
                    format!("Submit a review to GitHub PR #{number} with no inline comments?")
                } else {
                    format!("Publish {count} comments to GitHub PR #{number}?")
                };
                render_dialog(
                    centered_rect(72, 8, area),
                    buffer,
                    theme,
                    "Publish review?",
                    format!("{question}\n\ny publish   n/Esc keep reviewing   d discard and quit"),
                );
            }
            Self::Verdict {
                theme,
                self_authored,
                body,
            } => {
                let choices = if self_authored {
                    "c Comment only"
                } else {
                    "c Comment only   a Approve   r Request changes"
                };
                render_dialog(
                    centered_rect(78, 12, area),
                    buffer,
                    theme,
                    "Submit GitHub review",
                    format!(
                        "{choices}\no Edit overall comment   Esc keep reviewing\n\nOverall comment:\n{body}"
                    ),
                );
            }
            Self::OverallComment { theme, text } => render_dialog(
                centered_rect(72, 14, area),
                buffer,
                theme,
                "Overall comment",
                format!("{text}\n\nEnter/Ctrl-S save   Shift+Enter newline   Esc cancel"),
            ),
            Self::Message { theme, title, body } => render_dialog(
                centered_rect(72, 10, area),
                buffer,
                theme,
                title,
                format!("{body}\n\nEnter/Esc close"),
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
