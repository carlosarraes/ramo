use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Value};

use crate::core::input::LayoutMode;

use super::model::ViewPreferences;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewPreferenceChanges {
    pub mode: Option<LayoutMode>,
    pub theme: Option<String>,
    pub show_sidebar: Option<bool>,
    pub line_numbers: Option<bool>,
    pub wrap_lines: Option<bool>,
    pub hunk_headers: Option<bool>,
    pub agent_notes: Option<bool>,
    pub transparent_background: Option<bool>,
    pub prompt_save_view_preferences: Option<bool>,
}

impl ViewPreferenceChanges {
    pub fn between(initial: &ViewPreferences, current: &ViewPreferences) -> Self {
        Self {
            mode: changed(initial.mode, current.mode),
            theme: changed(&initial.theme, &current.theme).cloned(),
            show_sidebar: changed(initial.show_sidebar, current.show_sidebar),
            line_numbers: changed(initial.line_numbers, current.line_numbers),
            wrap_lines: changed(initial.wrap_lines, current.wrap_lines),
            hunk_headers: changed(initial.hunk_headers, current.hunk_headers),
            agent_notes: changed(initial.agent_notes, current.agent_notes),
            transparent_background: changed(
                initial.transparent_background,
                current.transparent_background,
            ),
            prompt_save_view_preferences: changed(
                initial.prompt_save_view_preferences,
                current.prompt_save_view_preferences,
            ),
        }
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

fn changed<T: PartialEq>(initial: T, current: T) -> Option<T> {
    (initial != current).then_some(current)
}

#[derive(Debug)]
pub enum ConfigSaveError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml_edit::TomlError,
    },
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for ConfigSaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "invalid config {}: {source}", path.display())
            }
            Self::Write { path, source } => {
                write!(formatter, "failed to write {}: {source}", path.display())
            }
        }
    }
}

impl Error for ConfigSaveError {}

pub fn save_view_preferences(
    path: &Path,
    changes: &ViewPreferenceChanges,
) -> Result<(), ConfigSaveError> {
    if changes.is_empty() {
        return Ok(());
    }
    let source = if path.exists() {
        fs::read_to_string(path).map_err(|source| ConfigSaveError::Read {
            path: path.to_path_buf(),
            source,
        })?
    } else {
        String::new()
    };
    let mut document = source
        .parse::<DocumentMut>()
        .map_err(|source| ConfigSaveError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    // A file still in the flat shape gets sectioned first, so persisting one preference can
    // never leave a stale root key silently shadowed by its own sectioned copy.
    let migrated = !super::migrate::move_legacy_keys(&mut document).is_empty();
    if let Some(mode) = changes.mode {
        set_value(&mut document, "view", "mode", layout_name(mode));
    }
    if let Some(theme) = &changes.theme {
        set_value(&mut document, "theme", "name", theme.clone());
    }
    set_optional_bool(&mut document, "view", "show_sidebar", changes.show_sidebar);
    set_optional_bool(&mut document, "view", "line_numbers", changes.line_numbers);
    set_optional_bool(&mut document, "view", "wrap_lines", changes.wrap_lines);
    set_optional_bool(&mut document, "view", "hunk_headers", changes.hunk_headers);
    set_optional_bool(&mut document, "view", "agent_notes", changes.agent_notes);
    set_optional_bool(
        &mut document,
        "view",
        "transparent_background",
        changes.transparent_background,
    );
    set_optional_bool(
        &mut document,
        "general",
        "prompt_save_view_preferences",
        changes.prompt_save_view_preferences,
    );
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|source| ConfigSaveError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }
    // Sectioning the file detaches the heading comment from the first key it belonged to.
    let rendered = if migrated {
        format!(
            "{}{}",
            super::migrate::leading_comments(&source),
            document.to_string().trim_start_matches('\n')
        )
    } else {
        document.to_string()
    };
    fs::write(path, rendered).map_err(|source| ConfigSaveError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn set_optional_bool(document: &mut DocumentMut, section: &str, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        set_value(document, section, key, value);
    }
}

/// Writes `section.key`, creating the table when absent and preserving any comment already
/// attached to the value. Writing into a table rather than at the document root also avoids
/// the trap where a bare key appended to a file that starts with `[diff]` lands inside it.
fn set_value(document: &mut DocumentMut, section: &str, key: &str, value: impl Into<Value>) {
    let mut replacement = value.into();
    let entry = document
        .entry(section)
        .or_insert_with(|| Item::Table(toml_edit::Table::new()));
    let Some(table) = entry.as_table_mut() else {
        return;
    };
    table.set_implicit(false);
    if let Some(existing) = table.get_mut(key).and_then(Item::as_value_mut) {
        *replacement.decor_mut() = existing.decor().clone();
        *existing = replacement;
    } else {
        table[key] = Item::Value(replacement);
    }
}

fn layout_name(mode: LayoutMode) -> &'static str {
    match mode {
        LayoutMode::Auto => "auto",
        LayoutMode::Split => "split",
        LayoutMode::Stack => "unified",
    }
}
