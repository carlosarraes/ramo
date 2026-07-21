use crate::core::input::{LayoutMode, VcsId};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct CustomThemeConfig {
    pub base: Option<String>,
    pub label: Option<String>,
    #[serde(default)]
    pub syntax_scopes: BTreeMap<String, String>,
    #[serde(flatten)]
    pub colors: BTreeMap<String, String>,
}

impl CustomThemeConfig {
    pub(crate) fn merge(&mut self, other: &Self) {
        if other.base.is_some() {
            self.base.clone_from(&other.base);
        }
        if other.label.is_some() {
            self.label.clone_from(&other.label);
        }
        self.colors.extend(other.colors.clone());
        self.syntax_scopes.extend(other.syntax_scopes.clone());
    }

    pub fn color(&self, key: &str) -> Option<&str> {
        self.colors.get(key).map(String::as_str)
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ConfigLayer {
    pub mode: Option<LayoutMode>,
    pub vcs: Option<VcsId>,
    pub theme: Option<String>,
    pub show_sidebar: Option<bool>,
    pub watch: Option<bool>,
    pub exclude_untracked: Option<bool>,
    pub line_numbers: Option<bool>,
    pub wrap_lines: Option<bool>,
    pub hunk_headers: Option<bool>,
    pub agent_notes: Option<bool>,
    pub copy_decorations: Option<bool>,
    pub prompt_save_view_preferences: Option<bool>,
    pub transparent_background: Option<bool>,
    pub color_moved: Option<bool>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ConfigFile {
    #[serde(flatten)]
    pub global: ConfigLayer,
    #[serde(default)]
    pub diff: ConfigLayer,
    #[serde(default)]
    pub show: ConfigLayer,
    #[serde(default)]
    pub stash_show: ConfigLayer,
    #[serde(default)]
    pub patch: ConfigLayer,
    #[serde(default)]
    pub pager: ConfigLayer,
    #[serde(default)]
    pub difftool: ConfigLayer,
    pub custom_theme: Option<CustomThemeConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    pub mode: LayoutMode,
    pub vcs: Option<VcsId>,
    pub theme: String,
    pub show_sidebar: bool,
    pub watch: bool,
    pub exclude_untracked: bool,
    pub line_numbers: bool,
    pub wrap_lines: bool,
    pub hunk_headers: bool,
    pub agent_notes: bool,
    pub copy_decorations: bool,
    pub prompt_save_view_preferences: bool,
    pub transparent_background: bool,
    pub color_moved: bool,
    pub custom_theme: Option<CustomThemeConfig>,
}

impl Default for ResolvedConfig {
    fn default() -> Self {
        Self {
            mode: LayoutMode::Auto,
            vcs: None,
            theme: "github-dark-default".into(),
            show_sidebar: true,
            watch: false,
            exclude_untracked: false,
            line_numbers: true,
            wrap_lines: false,
            hunk_headers: true,
            agent_notes: false,
            copy_decorations: false,
            prompt_save_view_preferences: true,
            transparent_background: false,
            color_moved: true,
            custom_theme: None,
        }
    }
}

impl ResolvedConfig {
    pub(crate) fn apply_layer(&mut self, layer: &ConfigLayer) {
        if let Some(value) = layer.mode {
            self.mode = value;
        }
        if let Some(value) = layer.vcs {
            self.vcs = Some(value);
        }
        if let Some(value) = &layer.theme {
            self.theme = value.clone();
        }
        apply(&mut self.show_sidebar, layer.show_sidebar);
        apply(&mut self.watch, layer.watch);
        apply(&mut self.exclude_untracked, layer.exclude_untracked);
        apply(&mut self.line_numbers, layer.line_numbers);
        apply(&mut self.wrap_lines, layer.wrap_lines);
        apply(&mut self.hunk_headers, layer.hunk_headers);
        apply(&mut self.agent_notes, layer.agent_notes);
        apply(&mut self.copy_decorations, layer.copy_decorations);
        apply(
            &mut self.prompt_save_view_preferences,
            layer.prompt_save_view_preferences,
        );
        apply(
            &mut self.transparent_background,
            layer.transparent_background,
        );
        apply(&mut self.color_moved, layer.color_moved);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewPreferences {
    pub mode: LayoutMode,
    pub theme: String,
    pub show_sidebar: bool,
    pub line_numbers: bool,
    pub wrap_lines: bool,
    pub hunk_headers: bool,
    pub agent_notes: bool,
    pub transparent_background: bool,
    pub prompt_save_view_preferences: bool,
}

impl From<&ResolvedConfig> for ViewPreferences {
    fn from(config: &ResolvedConfig) -> Self {
        Self {
            mode: config.mode,
            theme: config.theme.clone(),
            show_sidebar: config.show_sidebar,
            line_numbers: config.line_numbers,
            wrap_lines: config.wrap_lines,
            hunk_headers: config.hunk_headers,
            agent_notes: config.agent_notes,
            transparent_background: config.transparent_background,
            prompt_save_view_preferences: config.prompt_save_view_preferences,
        }
    }
}

fn apply(target: &mut bool, value: Option<bool>) {
    if let Some(value) = value {
        *target = value;
    }
}
