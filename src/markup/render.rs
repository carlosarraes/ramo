use ratatui::style::Color;

use crate::ui::themes::AppTheme;

use super::{StmlLine, StmlSpan, layout_stml};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StmlTextRenderResult {
    pub lines: Vec<String>,
    pub errors: Vec<String>,
}

pub fn render_stml_to_text(markup: &str, width: u16) -> StmlTextRenderResult {
    let result = layout_stml(markup, width);
    StmlTextRenderResult {
        lines: result.lines.iter().map(line_text).collect(),
        errors: result.errors,
    }
}

pub fn render_stml_to_ansi(markup: &str, width: u16, theme: &AppTheme) -> StmlTextRenderResult {
    let result = layout_stml(markup, width);
    StmlTextRenderResult {
        lines: result
            .lines
            .iter()
            .map(|line| ansi_line(line, theme))
            .collect(),
        errors: result.errors,
    }
}

pub fn resolve_stml_color(token: &str, theme: &AppTheme) -> Option<Color> {
    let value = token.trim().to_ascii_lowercase();
    let semantic = match value.as_str() {
        "accent" => Some(theme.accent),
        "info" => Some(theme.accent_muted),
        "success" => Some(theme.added_sign),
        "danger" | "error" => Some(theme.removed_sign),
        "warning" => Some(theme.file_modified),
        "muted" => Some(theme.muted),
        "subtle" => Some(theme.panel_alt),
        "heading" | "text" => Some(theme.text),
        "panel" | "bg" => Some(theme.panel),
        "note-border" => Some(theme.note_border),
        "badge-text" => Some(theme.background),
        _ => None,
    };
    semantic
        .or_else(|| parse_hex(&value))
        .or_else(|| named_color(&value))
}

fn line_text(line: &StmlLine) -> String {
    line.spans.iter().map(|span| span.text.as_str()).collect()
}

fn ansi_line(line: &StmlLine, theme: &AppTheme) -> String {
    line.spans
        .iter()
        .map(|span| ansi_span(span, theme))
        .collect()
}

fn ansi_span(span: &StmlSpan, theme: &AppTheme) -> String {
    let mut codes = Vec::new();
    if span.bold {
        codes.push("1".into());
    }
    if span.dim {
        codes.push("2".into());
    }
    if span.italic {
        codes.push("3".into());
    }
    if span.underline {
        codes.push("4".into());
    }
    if span.strike {
        codes.push("9".into());
    }
    if let Some(color) = span
        .fg
        .as_deref()
        .and_then(|token| resolve_stml_color(token, theme))
        && let Some(code) = ansi_color(color, false)
    {
        codes.push(code);
    }
    if let Some(color) = span
        .bg
        .as_deref()
        .and_then(|token| resolve_stml_color(token, theme))
        && let Some(code) = ansi_color(color, true)
    {
        codes.push(code);
    }
    if codes.is_empty() {
        span.text.clone()
    } else {
        format!("\x1b[{}m{}\x1b[0m", codes.join(";"), span.text)
    }
}

fn ansi_color(color: Color, background: bool) -> Option<String> {
    let prefix = if background { 48 } else { 38 };
    match color {
        Color::Reset => None,
        Color::Black => Some(if background { "40" } else { "30" }.into()),
        Color::Red => Some(if background { "41" } else { "31" }.into()),
        Color::Green => Some(if background { "42" } else { "32" }.into()),
        Color::Yellow => Some(if background { "43" } else { "33" }.into()),
        Color::Blue => Some(if background { "44" } else { "34" }.into()),
        Color::Magenta => Some(if background { "45" } else { "35" }.into()),
        Color::Cyan => Some(if background { "46" } else { "36" }.into()),
        Color::Gray => Some(if background { "47" } else { "37" }.into()),
        Color::DarkGray => Some(if background { "100" } else { "90" }.into()),
        Color::LightRed => Some(if background { "101" } else { "91" }.into()),
        Color::LightGreen => Some(if background { "102" } else { "92" }.into()),
        Color::LightYellow => Some(if background { "103" } else { "93" }.into()),
        Color::LightBlue => Some(if background { "104" } else { "94" }.into()),
        Color::LightMagenta => Some(if background { "105" } else { "95" }.into()),
        Color::LightCyan => Some(if background { "106" } else { "96" }.into()),
        Color::White => Some(if background { "107" } else { "97" }.into()),
        Color::Indexed(index) => Some(format!("{prefix};5;{index}")),
        Color::Rgb(red, green, blue) => Some(format!("{prefix};2;{red};{green};{blue}")),
    }
}

fn parse_hex(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#')?;
    match hex.len() {
        3 => {
            let mut values = hex
                .chars()
                .map(|digit| digit.to_digit(16).map(|value| value as u8));
            let red = values.next()??;
            let green = values.next()??;
            let blue = values.next()??;
            Some(Color::Rgb(red * 17, green * 17, blue * 17))
        }
        6 => Some(Color::Rgb(
            u8::from_str_radix(&hex[0..2], 16).ok()?,
            u8::from_str_radix(&hex[2..4], 16).ok()?,
            u8::from_str_radix(&hex[4..6], 16).ok()?,
        )),
        _ => None,
    }
}

fn named_color(value: &str) -> Option<Color> {
    let hex = match value {
        "black" => "#1c1c1c",
        "red" => "#e05252",
        "green" => "#4fb469",
        "yellow" => "#d9a331",
        "blue" => "#4f8fd9",
        "magenta" => "#b969d9",
        "cyan" => "#3fb5b5",
        "white" => "#e8e8e8",
        "gray" | "grey" => "#8a8a8a",
        "orange" => "#e0873d",
        "purple" => "#9a6fd0",
        "pink" => "#d9699a",
        _ => return None,
    };
    parse_hex(hex)
}
