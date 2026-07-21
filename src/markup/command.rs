use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::ui::themes::ThemeRegistry;

use super::{layout_stml, render_stml_to_ansi};

const MAX_MARKUP_READ_BYTES: u64 = 64 * 1024 + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkupColor {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkupRenderOptions {
    pub file: PathBuf,
    pub width: u16,
    pub theme: Option<String>,
    pub color: MarkupColor,
    pub json: bool,
}

pub fn guide() -> &'static str {
    include_str!("guide.md")
}

pub fn render(options: &MarkupRenderOptions) -> io::Result<()> {
    let source = read_source(&options.file)?;
    let result = layout_stml(&source, options.width);
    if result.lines.is_empty() && !result.errors.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            result.errors.join("; "),
        ));
    }
    let plain = result
        .lines
        .iter()
        .map(|line| line.spans.iter().map(|span| span.text.as_str()).collect())
        .collect::<Vec<String>>();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if options.json {
        serde_json::to_writer(
            &mut output,
            &json!({
                "width": options.width,
                "lines": plain,
                "notes": result.errors,
            }),
        )?;
        writeln!(output)?;
    } else {
        let color = match options.color {
            MarkupColor::Auto => stdout.is_terminal(),
            MarkupColor::Always => true,
            MarkupColor::Never => false,
        };
        let lines = if color {
            let theme = ThemeRegistry::default().resolve(
                options.theme.as_deref().unwrap_or("github-dark-default"),
                None,
                false,
            );
            render_stml_to_ansi(&source, options.width, &theme).lines
        } else {
            plain
        };
        for line in lines {
            writeln!(output, "{line}")?;
        }
        for note in &result.errors {
            eprintln!("note: {note}");
        }
    }
    Ok(())
}

fn read_source(path: &Path) -> io::Result<String> {
    let mut source = String::new();
    if path == Path::new("-") {
        io::stdin()
            .lock()
            .take(MAX_MARKUP_READ_BYTES)
            .read_to_string(&mut source)?;
    } else {
        File::open(path)
            .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?
            .take(MAX_MARKUP_READ_BYTES)
            .read_to_string(&mut source)?;
    }
    Ok(source)
}
