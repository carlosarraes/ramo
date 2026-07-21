use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::{self, ScopeSelectors, StyleModifier, ThemeItem, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::diff::model::DiffFile;

use super::themes::{AppTheme, TerminalAppearance};

const DEFAULT_FILE_THEME_CAPACITY: usize = 16;
const DEFAULT_LINE_CAPACITY: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightCacheStats {
    pub file_theme_entries: usize,
    pub line_entries: usize,
    pub misses: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FileThemeKey {
    file_id: String,
    theme: String,
}

#[derive(Clone)]
struct CachedLine {
    content_hash: u64,
    fragments: Vec<StyledFragment>,
}

#[derive(Default)]
struct FileThemeEntry {
    lines: HashMap<(usize, usize), CachedLine>,
    lru: VecDeque<(usize, usize)>,
}

impl FileThemeEntry {
    fn touch(&mut self, key: (usize, usize)) {
        if let Some(index) = self.lru.iter().position(|candidate| *candidate == key) {
            self.lru.remove(index);
        }
        self.lru.push_back(key);
    }

    fn insert(&mut self, key: (usize, usize), line: CachedLine, capacity: usize) {
        if !self.lines.contains_key(&key) {
            while self.lines.len() >= capacity {
                let Some(oldest) = self.lru.pop_front() else {
                    break;
                };
                self.lines.remove(&oldest);
            }
        }
        self.lines.insert(key, line);
        self.touch(key);
    }
}

#[derive(Clone)]
struct StyledFragment {
    text: String,
    style: Style,
}

pub struct HighlightCache {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    capacity: usize,
    line_capacity: usize,
    entries: HashMap<FileThemeKey, FileThemeEntry>,
    lru: VecDeque<FileThemeKey>,
    misses: usize,
}

impl Default for HighlightCache {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_FILE_THEME_CAPACITY)
    }
}

impl HighlightCache {
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_capacities(capacity, DEFAULT_LINE_CAPACITY)
    }

    pub fn with_capacities(capacity: usize, line_capacity: usize) -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            capacity: capacity.max(1),
            line_capacity: line_capacity.max(1),
            entries: HashMap::new(),
            lru: VecDeque::new(),
            misses: 0,
        }
    }

    pub fn spans(
        &mut self,
        file: &DiffFile,
        hunk_index: usize,
        line_index: usize,
        theme: &AppTheme,
    ) -> Vec<Span<'static>> {
        let Some(line) = file
            .hunks
            .get(hunk_index)
            .and_then(|hunk| hunk.lines.get(line_index))
        else {
            return Vec::new();
        };
        let key = FileThemeKey {
            file_id: file.id.clone(),
            theme: theme.cache_key(),
        };
        let content_hash = hash(&line.content);
        if let Some(fragments) = self.entries.get(&key).and_then(|entry| {
            entry
                .lines
                .get(&(hunk_index, line_index))
                .filter(|cached| cached.content_hash == content_hash)
                .map(|cached| cached.fragments.clone())
        }) {
            self.entries
                .get_mut(&key)
                .expect("cached highlight bucket exists")
                .touch((hunk_index, line_index));
            self.touch(&key);
            return into_spans(fragments);
        }

        self.misses = self.misses.saturating_add(1);
        let fragments = self.highlight(file, &line.content, theme);
        if !self.entries.contains_key(&key) {
            self.evict_for_insert();
            self.entries.insert(key.clone(), FileThemeEntry::default());
        }
        self.entries
            .get_mut(&key)
            .expect("highlight bucket was inserted")
            .insert(
                (hunk_index, line_index),
                CachedLine {
                    content_hash,
                    fragments: fragments.clone(),
                },
                self.line_capacity,
            );
        self.touch(&key);
        into_spans(fragments)
    }

    pub fn stats(&self) -> HighlightCacheStats {
        HighlightCacheStats {
            file_theme_entries: self.entries.len(),
            line_entries: self.entries.values().map(|entry| entry.lines.len()).sum(),
            misses: self.misses,
        }
    }

    pub fn contains_file_theme(&self, file: &DiffFile, theme: &AppTheme) -> bool {
        self.entries.contains_key(&FileThemeKey {
            file_id: file.id.clone(),
            theme: theme.cache_key(),
        })
    }

    fn highlight(
        &self,
        file: &DiffFile,
        content: &str,
        app_theme: &AppTheme,
    ) -> Vec<StyledFragment> {
        let extension = file
            .path
            .rsplit_once('.')
            .map_or("", |(_, extension)| extension);
        let syntax = file
            .language
            .as_deref()
            .and_then(|language| self.syntax_set.find_syntax_by_token(language))
            .or_else(|| self.syntax_set.find_syntax_by_extension(extension))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
        let base_name = match app_theme.appearance {
            TerminalAppearance::Light => "base16-ocean.light",
            TerminalAppearance::Dark => "base16-ocean.dark",
        };
        let mut theme = self.theme_set.themes[base_name].clone();
        add_scope_overrides(&mut theme, &app_theme.syntax_scope_overrides);
        let mut highlighter = HighlightLines::new(syntax, &theme);
        let line_with_newline = format!("{content}\n");
        match highlighter.highlight_line(&line_with_newline, &self.syntax_set) {
            Ok(regions) => regions
                .into_iter()
                .filter_map(|(style, text)| {
                    let text = text.trim_end_matches('\n');
                    (!text.is_empty()).then(|| StyledFragment {
                        text: text.to_owned(),
                        style: syntect_to_ratatui_style(style),
                    })
                })
                .collect(),
            Err(_) => vec![StyledFragment {
                text: content.to_owned(),
                style: Style::default().fg(app_theme.text),
            }],
        }
    }

    fn touch(&mut self, key: &FileThemeKey) {
        if let Some(index) = self.lru.iter().position(|candidate| candidate == key) {
            self.lru.remove(index);
        }
        self.lru.push_back(key.clone());
    }

    fn evict_for_insert(&mut self) {
        while self.entries.len() >= self.capacity {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }
}

fn add_scope_overrides(
    theme: &mut highlighting::Theme,
    overrides: &std::collections::BTreeMap<String, String>,
) {
    for (selector, color) in overrides {
        let (Ok(scope), Some(foreground)) =
            (ScopeSelectors::from_str(selector), syntect_color(color))
        else {
            continue;
        };
        theme.scopes.push(ThemeItem {
            scope,
            style: StyleModifier {
                foreground: Some(foreground),
                ..StyleModifier::default()
            },
        });
    }
}

fn syntect_color(value: &str) -> Option<highlighting::Color> {
    let value = value.strip_prefix('#')?;
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let number = u32::from_str_radix(value, 16).ok()?;
    Some(highlighting::Color {
        r: ((number >> 16) & 0xff) as u8,
        g: ((number >> 8) & 0xff) as u8,
        b: (number & 0xff) as u8,
        a: u8::MAX,
    })
}

fn hash(value: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn into_spans(fragments: Vec<StyledFragment>) -> Vec<Span<'static>> {
    fragments
        .into_iter()
        .map(|fragment| Span::styled(fragment.text, fragment.style))
        .collect()
}

fn syntect_to_ratatui_style(style: highlighting::Style) -> Style {
    let foreground = style.foreground;
    let mut result = Style::default().fg(Color::Rgb(foreground.r, foreground.g, foreground.b));
    if style.font_style.contains(highlighting::FontStyle::BOLD) {
        result = result.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(highlighting::FontStyle::ITALIC) {
        result = result.add_modifier(Modifier::ITALIC);
    }
    if style
        .font_style
        .contains(highlighting::FontStyle::UNDERLINE)
    {
        result = result.add_modifier(Modifier::UNDERLINED);
    }
    result
}
