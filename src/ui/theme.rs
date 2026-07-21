use ratatui::style::{Color, Modifier, Style};

use crate::diff::model::LineType;
use crate::ui::themes::{ReviewLineStyle, ThemeRegistry};

pub struct Theme {
    pub addition: Style,
    pub deletion: Style,
    pub context: Style,
    pub line_number: Style,
    pub hunk_header: Style,
    pub selection: Style,
    pub comment_indicator: Style,
    pub status_bar: Style,
    pub mode_normal: Style,
    pub mode_visual: Style,
    pub mode_comment: Style,
    pub file_header: Style,
    pub file_list_active: Style,
    pub file_list_item: Style,
    pub border: Style,
}

impl Default for Theme {
    fn default() -> Self {
        let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);
        Self {
            addition: theme.row_style(ReviewLineStyle::Added),
            deletion: theme.row_style(ReviewLineStyle::Removed),
            context: theme.row_style(ReviewLineStyle::Context),
            line_number: Style::default().fg(theme.line_number_fg),
            hunk_header: Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
            selection: Style::default()
                .bg(theme.selected_hunk)
                .add_modifier(Modifier::BOLD),
            comment_indicator: Style::default().fg(theme.note_border),
            status_bar: Style::default().bg(theme.panel_alt).fg(theme.text),
            mode_normal: Style::default()
                .bg(theme.accent)
                .fg(theme.background)
                .add_modifier(Modifier::BOLD),
            mode_visual: Style::default()
                .bg(theme.file_renamed)
                .fg(theme.background)
                .add_modifier(Modifier::BOLD),
            mode_comment: Style::default()
                .bg(theme.note_border)
                .fg(theme.background)
                .add_modifier(Modifier::BOLD),
            file_header: Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            file_list_active: Style::default()
                .fg(theme.text)
                .bg(theme.panel_alt)
                .add_modifier(Modifier::BOLD),
            file_list_item: Style::default().fg(theme.muted),
            border: Style::default().fg(theme.border),
        }
    }
}

impl Theme {
    pub fn line_style(&self, kind: &LineType) -> Style {
        match kind {
            LineType::Addition => self.addition,
            LineType::Deletion => self.deletion,
            LineType::Context => self.context,
        }
    }

    pub fn lineno_style(&self, kind: &LineType) -> Style {
        let base = self.line_number;
        match kind {
            LineType::Addition => base.bg(self.addition.bg.unwrap_or(Color::Reset)),
            LineType::Deletion => base.bg(self.deletion.bg.unwrap_or(Color::Reset)),
            LineType::Context => base,
        }
    }

    pub fn mode_style(&self, mode: &crate::vim::mode::Mode) -> Style {
        match mode {
            crate::vim::mode::Mode::Normal | crate::vim::mode::Mode::Command => self.mode_normal,
            crate::vim::mode::Mode::VisualLine { .. } => self.mode_visual,
            crate::vim::mode::Mode::CommentInsert | crate::vim::mode::Mode::CommentNormal => {
                self.mode_comment
            }
            crate::vim::mode::Mode::TmuxPanePick => self.mode_visual,
        }
    }
}
