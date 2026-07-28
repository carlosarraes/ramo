use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RgbColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SyntaxSpan {
    pub text: String,
    pub foreground: RgbColor,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

pub struct SyntaxHighlighter {
    syntaxes: SyntaxSet,
    themes: ThemeSet,
}

impl SyntaxHighlighter {
    pub fn tokyo_night() -> Self {
        Self {
            syntaxes: SyntaxSet::load_defaults_newlines(),
            themes: ThemeSet::load_defaults(),
        }
    }

    pub fn highlight_line(
        &mut self,
        path: &str,
        language: Option<&str>,
        content: &str,
    ) -> Vec<SyntaxSpan> {
        if content.is_empty() {
            return vec![plain("")];
        }
        let extension = path.rsplit_once('.').map_or("", |(_, extension)| extension);
        let syntax = language
            .and_then(|language| self.syntaxes.find_syntax_by_token(language))
            .or_else(|| self.syntaxes.find_syntax_by_extension(extension))
            .unwrap_or_else(|| self.syntaxes.find_syntax_plain_text());
        let theme = &self.themes.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);
        let source = format!("{content}\n");
        let Ok(regions) = highlighter.highlight_line(&source, &self.syntaxes) else {
            return vec![plain(content)];
        };
        let mut spans = regions
            .into_iter()
            .filter_map(|(style, text)| {
                let text = text.trim_end_matches('\n');
                (!text.is_empty()).then(|| SyntaxSpan {
                    text: text.to_owned(),
                    foreground: RgbColor {
                        red: style.foreground.r,
                        green: style.foreground.g,
                        blue: style.foreground.b,
                    },
                    bold: style.font_style.contains(FontStyle::BOLD),
                    italic: style.font_style.contains(FontStyle::ITALIC),
                    underline: style.font_style.contains(FontStyle::UNDERLINE),
                })
            })
            .collect::<Vec<_>>();
        if spans.is_empty() {
            spans.push(plain(content));
        }
        spans
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::tokyo_night()
    }
}

fn plain(text: &str) -> SyntaxSpan {
    SyntaxSpan {
        text: text.to_owned(),
        foreground: RgbColor {
            red: 0xc0,
            green: 0xca,
            blue: 0xf5,
        },
        bold: false,
        italic: false,
        underline: false,
    }
}
