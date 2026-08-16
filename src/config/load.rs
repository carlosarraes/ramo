use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::input::{CommonOptions, InputKind, ReviewInput};

use super::model::{ConfigFile, ConfigLayer, ResolvedConfig};

const PREFERENCE_KEYS: &[&str] = &[
    "mode",
    "vcs",
    "theme",
    "show_sidebar",
    "watch",
    "exclude_untracked",
    "line_numbers",
    "wrap_lines",
    "hunk_headers",
    "agent_notes",
    "copy_decorations",
    "prompt_save_view_preferences",
    "transparent_background",
    "transparentBackground",
    "color_moved",
    "test_file_patterns",
    "review_map_server",
    "review_map_token_file",
    "ai_summaries",
    "start_on_map",
    "tests_last",
    "ask_enabled",
    "ask_provider",
    "ask_model",
    "ask_thinking",
    "ask_timeout_secs",
];

const COMMAND_SECTIONS: &[&str] = &["diff", "show", "stash_show", "patch", "pager", "difftool"];
const CUSTOM_THEME_COLOR_KEYS: &[&str] = &[
    "background",
    "panel",
    "panelAlt",
    "border",
    "accent",
    "accentMuted",
    "text",
    "muted",
    "addedBg",
    "removedBg",
    "movedAddedBg",
    "movedRemovedBg",
    "contextBg",
    "addedContentBg",
    "removedContentBg",
    "contextContentBg",
    "addedSignColor",
    "removedSignColor",
    "lineNumberBg",
    "lineNumberFg",
    "selectedHunk",
    "badgeAdded",
    "badgeRemoved",
    "badgeNeutral",
    "fileNew",
    "fileDeleted",
    "fileRenamed",
    "fileModified",
    "fileUntracked",
    "noteBorder",
    "noteBackground",
    "noteTitleBackground",
    "noteTitleText",
];
const LEGACY_SYNTAX_KEYS: &[&str] = &[
    "default",
    "keyword",
    "string",
    "comment",
    "number",
    "function",
    "property",
    "type",
    "variable",
    "operator",
    "punctuation",
];
const LEGACY_SYNTAX_NOTICE: &str = "Deprecated [custom_theme.syntax] translated approximately • migrate to [custom_theme.syntax_scopes]";

#[derive(Debug, Clone, Default)]
pub struct ConfigPaths {
    pub user: Option<PathBuf>,
    pub repo: Option<PathBuf>,
}

impl ConfigPaths {
    pub fn discover(cwd: &Path) -> Self {
        let user = user_config_dir().map(|path| path.join("ramo/config.toml"));
        let repo = cwd.ancestors().find_map(|ancestor| {
            let candidate = ancestor.join(".ramo/config.toml");
            candidate.is_file().then_some(candidate)
        });
        Self { user, repo }
    }
}

pub(crate) fn user_config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(dirs::config_dir)
}

#[derive(Debug, Clone)]
pub struct ConfigResolver {
    paths: ConfigPaths,
}

impl ConfigResolver {
    pub fn new(paths: ConfigPaths) -> Self {
        Self { paths }
    }

    pub fn resolve(&self, input: &ReviewInput) -> Result<ResolvedConfig, ConfigError> {
        let user = read_config(self.paths.user.as_deref())?;
        let repo = read_config(self.paths.repo.as_deref())?;
        let mut resolved = ResolvedConfig::default();

        if let Some(config) = &user {
            resolved.apply_layer(&config.global);
        }
        if let Some(config) = &repo {
            resolved.apply_layer(&config.global);
        }
        if let Some(custom_theme) = user
            .as_ref()
            .and_then(|config| config.custom_theme.as_ref())
        {
            resolved.custom_theme = Some(custom_theme.clone());
        }
        if let Some(custom_theme) = repo
            .as_ref()
            .and_then(|config| config.custom_theme.as_ref())
        {
            match &mut resolved.custom_theme {
                Some(base) => base.merge(custom_theme),
                None => resolved.custom_theme = Some(custom_theme.clone()),
            }
        }
        if let Some(config) = &user {
            resolved.apply_layer(command_layer(config, input.kind()));
        }
        if let Some(config) = &repo {
            resolved.apply_layer(command_layer(config, input.kind()));
        }
        if user
            .as_ref()
            .is_some_and(|config| config.uses_legacy_syntax)
            || repo
                .as_ref()
                .is_some_and(|config| config.uses_legacy_syntax)
        {
            resolved.startup_notices.push(LEGACY_SYNTAX_NOTICE.into());
        }

        if input.kind() == InputKind::Pager || input.options().pager == Some(true) {
            if let Some(config) = &user {
                resolved.apply_layer(&config.pager);
            }
            if let Some(config) = &repo {
                resolved.apply_layer(&config.pager);
            }
        }

        apply_cli_options(&mut resolved, input.options());
        Ok(resolved)
    }
}

fn read_config(path: Option<&Path>) -> Result<Option<ConfigFile>, ConfigError> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let value = toml::from_str::<toml::Value>(&source).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source: source.to_string(),
    })?;
    validate_keys(path, &value)?;
    let mut config =
        toml::from_str::<ConfigFile>(&source).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source: source.to_string(),
        })?;
    if let Some(base) = config
        .custom_theme
        .as_mut()
        .and_then(|custom| custom.base.as_mut())
    {
        let Some(canonical) = crate::ui::themes::normalize_builtin_theme_id(base) else {
            return Err(ConfigError::Parse {
                path: path.to_path_buf(),
                source: format!("custom_theme.base is not a known built-in theme id: {base}"),
            });
        };
        *base = canonical.to_owned();
    }
    if let Some(custom) = config.custom_theme.as_mut()
        && !custom.legacy_syntax.is_empty()
    {
        config.uses_legacy_syntax = true;
        let exact = std::mem::take(&mut custom.syntax_scopes);
        custom.syntax_scopes = legacy_syntax_scopes(&custom.legacy_syntax);
        custom.syntax_scopes.extend(exact);
        custom.legacy_syntax.clear();
    }
    validate_test_file_patterns(path, &config)?;
    validate_review_map_config(path, &config)?;
    validate_ask_config(path, &config)?;
    Ok(Some(config))
}

pub(crate) const ASK_THINKING_LEVELS: &[&str] =
    &["off", "minimal", "low", "medium", "high", "xhigh", "max"];
const ASK_TIMEOUT_RANGE: std::ops::RangeInclusive<u64> = 5..=600;

fn validate_ask_config(path: &Path, config: &ConfigFile) -> Result<(), ConfigError> {
    let invalid = |source: String| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    };
    for layer in [
        &config.global,
        &config.diff,
        &config.show,
        &config.stash_show,
        &config.patch,
        &config.pager,
        &config.difftool,
    ] {
        // Provider and model become argv for the pi CLI, so they must be single bare words.
        for (key, value) in [
            ("ask_provider", &layer.ask_provider),
            ("ask_model", &layer.ask_model),
        ] {
            let Some(value) = value else { continue };
            if value.trim().is_empty() {
                return Err(invalid(format!("{key} must not be empty")));
            }
            if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err(invalid(format!(
                    "{key} must not contain whitespace or control characters: {value:?}"
                )));
            }
        }
        if let Some(thinking) = &layer.ask_thinking
            && !ASK_THINKING_LEVELS.contains(&thinking.as_str())
        {
            return Err(invalid(format!(
                "ask_thinking must be one of {}: got {thinking:?}",
                ASK_THINKING_LEVELS.join(", ")
            )));
        }
        if let Some(timeout) = layer.ask_timeout_secs
            && !ASK_TIMEOUT_RANGE.contains(&timeout)
        {
            return Err(invalid(format!(
                "ask_timeout_secs must be between {} and {}: got {timeout}",
                ASK_TIMEOUT_RANGE.start(),
                ASK_TIMEOUT_RANGE.end()
            )));
        }
    }
    Ok(())
}

fn validate_review_map_config(path: &Path, config: &ConfigFile) -> Result<(), ConfigError> {
    for layer in [
        &config.global,
        &config.diff,
        &config.show,
        &config.stash_show,
        &config.patch,
        &config.pager,
        &config.difftool,
    ] {
        if let Some(endpoint) = &layer.review_map_server {
            crate::review_map::validate_loopback_endpoint(endpoint).map_err(|error| {
                ConfigError::Parse {
                    path: path.to_path_buf(),
                    source: error.to_string(),
                }
            })?;
        }
        if let Some(token_file) = &layer.review_map_token_file
            && !token_file.is_absolute()
        {
            return Err(ConfigError::Parse {
                path: path.to_path_buf(),
                source: "review_map_token_file must be an absolute path".into(),
            });
        }
    }
    Ok(())
}

fn validate_test_file_patterns(path: &Path, config: &ConfigFile) -> Result<(), ConfigError> {
    for layer in [
        &config.global,
        &config.diff,
        &config.show,
        &config.stash_show,
        &config.patch,
        &config.pager,
        &config.difftool,
    ] {
        for pattern in layer.test_file_patterns.iter().flatten() {
            crate::review::validate_test_file_pattern(pattern).map_err(|source| {
                ConfigError::Parse {
                    path: path.to_path_buf(),
                    source,
                }
            })?;
        }
    }
    Ok(())
}

fn validate_keys(path: &Path, value: &toml::Value) -> Result<(), ConfigError> {
    let Some(table) = value.as_table() else {
        return Err(ConfigError::Parse {
            path: path.to_path_buf(),
            source: "configuration root must be a TOML table".into(),
        });
    };
    for (key, value) in table {
        if PREFERENCE_KEYS.contains(&key.as_str()) {
            continue;
        }
        if COMMAND_SECTIONS.contains(&key.as_str()) {
            let Some(section) = value.as_table() else {
                return Err(ConfigError::Parse {
                    path: path.to_path_buf(),
                    source: format!("{key} must be a TOML table"),
                });
            };
            for section_key in section.keys() {
                if !PREFERENCE_KEYS.contains(&section_key.as_str()) {
                    return Err(ConfigError::UnknownKey {
                        path: path.to_path_buf(),
                        key: format!("{key}.{section_key}"),
                    });
                }
            }
            continue;
        }
        if key == "custom_theme" {
            validate_custom_theme(path, value)?;
            continue;
        }
        return Err(ConfigError::UnknownKey {
            path: path.to_path_buf(),
            key: key.clone(),
        });
    }
    Ok(())
}

fn validate_custom_theme(path: &Path, value: &toml::Value) -> Result<(), ConfigError> {
    let Some(table) = value.as_table() else {
        return Err(ConfigError::Parse {
            path: path.to_path_buf(),
            source: "custom_theme must be a TOML table".into(),
        });
    };
    for (key, value) in table {
        if key == "base" || key == "label" {
            if !value.is_str() {
                return invalid_custom_value(path, &format!("custom_theme.{key}"), "a string");
            }
            continue;
        }
        if key == "syntax_scopes" {
            let Some(scopes) = value.as_table() else {
                return invalid_custom_value(path, "custom_theme.syntax_scopes", "a TOML table");
            };
            for (scope, color) in scopes {
                validate_hex_color(path, &format!("custom_theme.syntax_scopes.{scope}"), color)?;
            }
            continue;
        }
        if key == "syntax" {
            let Some(syntax) = value.as_table() else {
                return invalid_custom_value(path, "custom_theme.syntax", "a TOML table");
            };
            for (role, color) in syntax {
                if !LEGACY_SYNTAX_KEYS.contains(&role.as_str()) {
                    return Err(ConfigError::UnknownKey {
                        path: path.to_path_buf(),
                        key: format!("custom_theme.syntax.{role}"),
                    });
                }
                validate_hex_color(path, &format!("custom_theme.syntax.{role}"), color)?;
            }
            continue;
        }
        if CUSTOM_THEME_COLOR_KEYS.contains(&key.as_str()) {
            validate_hex_color(path, &format!("custom_theme.{key}"), value)?;
            continue;
        }
        return Err(ConfigError::UnknownKey {
            path: path.to_path_buf(),
            key: format!("custom_theme.{key}"),
        });
    }
    Ok(())
}

fn legacy_syntax_scopes(
    legacy: &std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    const MAPPINGS: &[(&str, &[&str])] = &[
        ("default", &["source"]),
        ("keyword", &["keyword"]),
        ("string", &["string"]),
        ("comment", &["comment", "punctuation.definition.comment"]),
        ("number", &["constant.numeric"]),
        (
            "function",
            &[
                "entity.name.function",
                "support.function",
                "variable.function",
            ],
        ),
        (
            "property",
            &["variable.other.property", "support.variable.property"],
        ),
        (
            "type",
            &[
                "entity.name.type",
                "entity.name.class",
                "support.type",
                "support.class",
            ],
        ),
        ("variable", &["variable"]),
        ("operator", &["keyword.operator"]),
        ("punctuation", &["punctuation"]),
    ];
    let mut scopes = std::collections::BTreeMap::new();
    for (role, selectors) in MAPPINGS {
        if let Some(color) = legacy.get(*role) {
            for selector in *selectors {
                scopes.insert((*selector).to_owned(), color.clone());
            }
        }
    }
    scopes
}

fn validate_hex_color(path: &Path, key: &str, value: &toml::Value) -> Result<(), ConfigError> {
    let valid = value.as_str().is_some_and(|color| {
        color.len() == 7
            && color.starts_with('#')
            && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    if valid {
        Ok(())
    } else {
        invalid_custom_value(path, key, "a hex color like #112233")
    }
}

fn invalid_custom_value<T>(path: &Path, key: &str, expected: &str) -> Result<T, ConfigError> {
    Err(ConfigError::Parse {
        path: path.to_path_buf(),
        source: format!("expected {key} to be {expected}"),
    })
}

fn command_layer(config: &ConfigFile, kind: InputKind) -> &ConfigLayer {
    match kind {
        InputKind::Diff => &config.diff,
        InputKind::Show => &config.show,
        InputKind::StashShow => &config.stash_show,
        InputKind::Patch => &config.patch,
        InputKind::Pager => &config.pager,
        InputKind::Difftool => &config.difftool,
        InputKind::PullRequest => &config.diff,
    }
}

fn apply_cli_options(resolved: &mut ResolvedConfig, options: &CommonOptions) {
    if let Some(value) = options.mode {
        resolved.mode = value;
    }
    if let Some(value) = &options.theme {
        resolved.theme = value.clone();
    }
    apply(&mut resolved.watch, options.watch);
    apply(&mut resolved.exclude_untracked, options.exclude_untracked);
    apply(&mut resolved.line_numbers, options.line_numbers);
    apply(&mut resolved.wrap_lines, options.wrap_lines);
    apply(&mut resolved.hunk_headers, options.hunk_headers);
    apply(&mut resolved.agent_notes, options.agent_notes);
    apply(
        &mut resolved.transparent_background,
        options.transparent_background,
    );
    apply(&mut resolved.start_on_map, options.start_on_map);
    apply(&mut resolved.tests_last, options.tests_last);
    apply(&mut resolved.ask_enabled, options.ask_enabled);
}

fn apply(target: &mut bool, value: Option<bool>) {
    if let Some(value) = value {
        *target = value;
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: String,
    },
    UnknownKey {
        path: PathBuf,
        key: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "failed to read {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "invalid config {}: {source}", path.display())
            }
            Self::UnknownKey { path, key } => {
                write!(formatter, "unknown config key {key} in {}", path.display())
            }
        }
    }
}

impl Error for ConfigError {}
