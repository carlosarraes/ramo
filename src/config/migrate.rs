use std::fs;
use std::path::Path;

use toml_edit::{DocumentMut, Item};

/// Where every pre-0.1.0 flat key now lives. The loader still reads the flat form, so this
/// table only drives the one-time rewrite and the notice that explains it.
const MOVES: &[(&str, &str, &str)] = &[
    ("mode", "view", "mode"),
    ("show_sidebar", "view", "show_sidebar"),
    ("line_numbers", "view", "line_numbers"),
    ("wrap_lines", "view", "wrap_lines"),
    ("hunk_headers", "view", "hunk_headers"),
    ("agent_notes", "view", "agent_notes"),
    ("transparent_background", "view", "transparent_background"),
    ("transparentBackground", "view", "transparent_background"),
    ("color_moved", "view", "color_moved"),
    ("copy_decorations", "view", "copy_decorations"),
    ("vcs", "general", "vcs"),
    ("watch", "general", "watch"),
    ("exclude_untracked", "general", "exclude_untracked"),
    (
        "prompt_save_view_preferences",
        "general",
        "prompt_save_view_preferences",
    ),
    ("theme", "theme", "name"),
    ("review_message", "review", "message"),
    ("tests_last", "review", "tests_last"),
    ("test_file_patterns", "review", "test_file_patterns"),
    ("ask_enabled", "ask", "enabled"),
    ("ask_provider", "ask", "provider"),
    ("ask_model", "ask", "model"),
    ("ask_thinking", "ask", "effort"),
    ("ask_timeout_secs", "ask", "timeout_secs"),
    ("ai_summaries", "map", "enabled"),
    ("start_on_map", "map", "start_on"),
    ("review_map_server", "map", "server"),
    ("review_map_token_file", "map", "token_file"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    /// `old -> [section] key` lines, for the startup notice.
    pub moved: Vec<String>,
    pub backup: std::path::PathBuf,
}

/// Rewrites a legacy flat config into sections, in place, after backing it up.
///
/// Only ever called for the **user** config. A repository `.ramo/config.toml` is checked into
/// someone's project and shared with their teammates, so it is read through the compatibility
/// shim and never rewritten.
pub fn migrate_user_config(path: &Path) -> Result<Option<Migration>, std::io::Error> {
    if !path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(path)?;
    let Ok(mut document) = source.parse::<DocumentMut>() else {
        // A malformed file is the loader's problem to report, with its own error.
        return Ok(None);
    };
    let moved = move_legacy_keys(&mut document);
    finish_migration(path, &source, &document, moved)
}

/// Moves every legacy flat key into its section, in place, returning what moved. Shared with
/// the save-on-quit writer so persisting a preference can never leave a stale root duplicate
/// shadowed by its own sectioned copy.
pub(crate) fn move_legacy_keys(document: &mut DocumentMut) -> Vec<String> {
    // `theme` is both a legacy scalar key and the name of its own section, so match only
    // scalars here — otherwise migrating would delete the `[theme]` table it just created.
    let legacy: Vec<_> = MOVES
        .iter()
        .filter(|(old, _, _)| {
            document
                .get(old)
                .is_some_and(|item| item.as_value().is_some())
        })
        .collect();
    let has_custom_theme = document.get("custom_theme").is_some();
    let mut moved = Vec::with_capacity(legacy.len());
    for (old, section, key) in legacy {
        let Some(item) = document.remove(old) else {
            continue;
        };
        let Some(mut value) = item.as_value().cloned() else {
            continue;
        };
        // A comment sitting above the key is usually a heading for the file, not for this
        // value; carrying it into a section would bury it. The trailing (inline) comment does
        // belong to the value, so that one travels.
        let suffix = value
            .decor()
            .suffix()
            .and_then(|raw| raw.as_str())
            .unwrap_or("")
            .to_owned();
        let decor = value.decor_mut();
        decor.set_prefix(" ");
        decor.set_suffix(suffix);
        let entry = document
            .entry(section)
            .or_insert_with(|| Item::Table(toml_edit::Table::new()));
        let Some(table) = entry.as_table_mut() else {
            continue;
        };
        table.set_implicit(false);
        // A section key already present wins: it is the newer spelling.
        if table.get(key).is_none() {
            table[key] = Item::Value(value);
        }
        moved.push(format!("{old} -> [{section}] {key}"));
    }
    if has_custom_theme
        && document
            .get("theme")
            .and_then(|theme| theme.get("custom"))
            .is_none()
        && let Some(custom) = document.remove("custom_theme")
    {
        let entry = document
            .entry("theme")
            .or_insert_with(|| Item::Table(toml_edit::Table::new()));
        if let Some(table) = entry.as_table_mut() {
            table.set_implicit(false);
            table["custom"] = custom;
            moved.push("[custom_theme] -> [theme.custom]".to_owned());
        }
    }
    moved
}

fn finish_migration(
    path: &Path,
    source: &str,
    document: &DocumentMut,
    moved: Vec<String>,
) -> Result<Option<Migration>, std::io::Error> {
    if moved.is_empty() {
        return Ok(None);
    }
    let backup = path.with_extension("toml.bak");
    fs::write(&backup, source)?;
    fs::write(
        path,
        format!(
            "{}{}",
            leading_comments(source),
            document.to_string().trim_start_matches('\n')
        ),
    )?;
    Ok(Some(Migration { moved, backup }))
}

/// The comment block at the top of the file, which otherwise disappears with the first key
/// that moves into a section.
pub(crate) fn leading_comments(source: &str) -> String {
    let mut out = String::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
        } else if trimmed.is_empty() && !out.is_empty() {
            continue;
        } else {
            break;
        }
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}
