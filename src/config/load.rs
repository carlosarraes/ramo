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
    "watch",
    "exclude_untracked",
    "line_numbers",
    "wrap_lines",
    "hunk_headers",
    "agent_notes",
    "copy_decorations",
    "prompt_save_view_preferences",
    "transparent_background",
    "color_moved",
];

const COMMAND_SECTIONS: &[&str] = &["diff", "show", "stash_show", "patch", "pager", "difftool"];

#[derive(Debug, Clone, Default)]
pub struct ConfigPaths {
    pub user: Option<PathBuf>,
    pub repo: Option<PathBuf>,
}

impl ConfigPaths {
    pub fn discover(cwd: &Path) -> Self {
        let user = dirs::config_dir().map(|path| path.join("pdiff/config.toml"));
        let repo = cwd.ancestors().find_map(|ancestor| {
            let candidate = ancestor.join(".pdiff/config.toml");
            candidate.is_file().then_some(candidate)
        });
        Self { user, repo }
    }
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
        if let Some(config) = &user {
            resolved.apply_layer(command_layer(config, input.kind()));
        }
        if let Some(config) = &repo {
            resolved.apply_layer(command_layer(config, input.kind()));
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
    toml::from_str(&source)
        .map(Some)
        .map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source: source.to_string(),
        })
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
        return Err(ConfigError::UnknownKey {
            path: path.to_path_buf(),
            key: key.clone(),
        });
    }
    Ok(())
}

fn command_layer(config: &ConfigFile, kind: InputKind) -> &ConfigLayer {
    match kind {
        InputKind::Diff => &config.diff,
        InputKind::Show => &config.show,
        InputKind::StashShow => &config.stash_show,
        InputKind::Patch => &config.patch,
        InputKind::Pager => &config.pager,
        InputKind::Difftool => &config.difftool,
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
