use std::collections::HashMap;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::{self, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::diff::model::DiffFile;

pub struct Highlighter {
    /// Pre-computed spans per (file_idx, hunk_idx, line_idx)
    cache: HashMap<(usize, usize, usize), Vec<StyledFragment>>,
}

#[derive(Clone)]
struct StyledFragment {
    text: String,
    style: Style,
}

impl Highlighter {
    pub fn new(files: &[DiffFile]) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set.themes["base16-eighties.dark"].clone();

        let mut cache = HashMap::new();

        for (fi, file) in files.iter().enumerate() {
            let ext = file.path.rsplit('.').next().unwrap_or("");
            let syntax = syntax_set
                .find_syntax_by_extension(ext)
                .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

            let mut h = HighlightLines::new(syntax, &theme);

            for (hi, hunk) in file.hunks.iter().enumerate() {
                for (li, line) in hunk.lines.iter().enumerate() {
                    let line_with_nl = format!("{}\n", line.content);
                    let fragments = match h.highlight_line(&line_with_nl, &syntax_set) {
                        Ok(regions) => regions
                            .into_iter()
                            .filter_map(|(style, text)| {
                                let text = text.trim_end_matches('\n');
                                if text.is_empty() {
                                    return None;
                                }
                                Some(StyledFragment {
                                    text: text.to_string(),
                                    style: syntect_to_ratatui_style(style),
                                })
                            })
                            .collect(),
                        Err(_) => vec![StyledFragment {
                            text: line.content.clone(),
                            style: Style::default(),
                        }],
                    };
                    cache.insert((fi, hi, li), fragments);
                }
            }
        }

        Self { cache }
    }

    pub fn get_spans(
        &self,
        file_idx: usize,
        hunk_idx: usize,
        line_idx: usize,
    ) -> Vec<Span<'static>> {
        match self.cache.get(&(file_idx, hunk_idx, line_idx)) {
            Some(fragments) => fragments
                .iter()
                .map(|f| Span::styled(f.text.clone(), f.style))
                .collect(),
            None => Vec::new(),
        }
    }
}

fn syntect_to_ratatui_style(style: highlighting::Style) -> Style {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);

    let mut s = Style::default().fg(fg);
    if style.font_style.contains(highlighting::FontStyle::BOLD) {
        s = s.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(highlighting::FontStyle::ITALIC) {
        s = s.add_modifier(Modifier::ITALIC);
    }
    if style
        .font_style
        .contains(highlighting::FontStyle::UNDERLINE)
    {
        s = s.add_modifier(Modifier::UNDERLINED);
    }
    s
}
