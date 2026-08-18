use crate::core::input::{LayoutMode, VcsId};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct CustomThemeConfig {
    pub base: Option<String>,
    pub label: Option<String>,
    #[serde(default)]
    pub syntax_scopes: BTreeMap<String, String>,
    #[serde(default, rename = "syntax")]
    #[doc(hidden)]
    pub legacy_syntax: BTreeMap<String, String>,
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

/// Sectioned configuration. Each section is a distinct struct rather than another
/// `ConfigLayer`, so a section can only carry the keys that belong to it and the command
/// sections (`[diff]`, `[show]`, …) keep owning the flat per-command overrides.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct GeneralSection {
    pub vcs: Option<VcsId>,
    pub watch: Option<bool>,
    pub exclude_untracked: Option<bool>,
    pub prompt_save_view_preferences: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct ViewSection {
    pub mode: Option<LayoutMode>,
    pub show_sidebar: Option<bool>,
    pub line_numbers: Option<bool>,
    pub wrap_lines: Option<bool>,
    pub hunk_headers: Option<bool>,
    pub agent_notes: Option<bool>,
    pub transparent_background: Option<bool>,
    pub color_moved: Option<bool>,
    pub copy_decorations: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct ThemeSection {
    pub name: Option<String>,
    pub custom: Option<CustomThemeConfig>,
}

/// `theme` is the one key whose legacy flat spelling collides with its own section name:
/// `theme = "aurora-x"` and `[theme] name = "aurora-x"` must both parse. Untagged, so the
/// scalar form is tried first and the table form second.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub enum ThemeSetting {
    Name(String),
    Section(Box<ThemeSection>),
}

impl ThemeSetting {
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Name(name) => Some(name.as_str()),
            Self::Section(section) => section.name.as_deref(),
        }
    }

    pub fn custom(&self) -> Option<&CustomThemeConfig> {
        match self {
            Self::Name(_) => None,
            Self::Section(section) => section.custom.as_ref(),
        }
    }

    pub fn custom_mut(&mut self) -> Option<&mut CustomThemeConfig> {
        match self {
            Self::Name(_) => None,
            Self::Section(section) => section.custom.as_mut(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct ReviewSection {
    pub message: Option<String>,
    pub tests_last: Option<bool>,
    pub test_file_patterns: Option<Vec<String>>,
}

/// Shared shape for every AI section. `effort` is the sectioned name for what the flat keys
/// called `ask_thinking`; it is pi's `--thinking` level.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct AgentSection {
    pub enabled: Option<bool>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct MapSection {
    pub enabled: Option<bool>,
    /// `"pi"` (default) or `"ollama"`. Read by `ramo-server`, which owns the analyzer.
    pub backend: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub timeout_secs: Option<u64>,
    pub start_on: Option<bool>,
    pub server: Option<String>,
    pub token_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct LinearSection {
    pub enabled: Option<bool>,
    pub command: Option<String>,
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
    #[serde(rename = "transparentBackground")]
    #[doc(hidden)]
    pub transparent_background_camel: Option<bool>,
    pub color_moved: Option<bool>,
    pub test_file_patterns: Option<Vec<String>>,
    pub review_map_server: Option<String>,
    pub review_map_token_file: Option<PathBuf>,
    pub ai_summaries: Option<bool>,
    pub start_on_map: Option<bool>,
    pub tests_last: Option<bool>,
    pub ask_enabled: Option<bool>,
    pub ask_provider: Option<String>,
    pub ask_model: Option<String>,
    pub ask_thinking: Option<String>,
    pub ask_timeout_secs: Option<u64>,
    pub review_message: Option<String>,
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
    #[serde(default)]
    pub general: GeneralSection,
    #[serde(default)]
    pub view: ViewSection,
    #[serde(default)]
    pub theme: Option<ThemeSetting>,
    #[serde(default)]
    pub review: ReviewSection,
    #[serde(default)]
    pub ask: AgentSection,
    #[serde(default)]
    pub map: MapSection,
    #[serde(default)]
    pub chat: AgentSection,
    #[serde(default)]
    pub linear: LinearSection,
    #[serde(skip)]
    pub(crate) uses_legacy_syntax: bool,
}

impl ConfigFile {
    /// Every layer that can carry flat per-command keys, in no particular order. Four
    /// validators used to hand-roll this same list.
    pub(crate) fn layers(&self) -> [&ConfigLayer; 7] {
        [
            &self.global,
            &self.diff,
            &self.show,
            &self.stash_show,
            &self.patch,
            &self.pager,
            &self.difftool,
        ]
    }
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
    pub test_file_patterns: Vec<String>,
    pub review_map_server: String,
    pub review_map_token_file: Option<PathBuf>,
    pub ai_summaries: bool,
    pub start_on_map: bool,
    pub tests_last: bool,
    pub ask_enabled: bool,
    pub ask_provider: String,
    pub ask_model: String,
    pub ask_thinking: String,
    pub ask_timeout_secs: u64,
    pub map_backend: String,
    pub map_provider: String,
    pub map_model: String,
    pub map_effort: String,
    pub map_timeout_secs: u64,
    pub chat_enabled: bool,
    pub chat_provider: String,
    pub chat_model: String,
    pub chat_effort: String,
    pub chat_timeout_secs: u64,
    pub linear_enabled: bool,
    pub linear_command: String,
    /// `None` keeps the count-aware default body; `Some` replaces it verbatim, so an
    /// empty string is a deliberate "publish with no overall comment".
    pub review_message: Option<String>,
    pub custom_theme: Option<CustomThemeConfig>,
    pub startup_notices: Vec<String>,
}

impl Default for ResolvedConfig {
    fn default() -> Self {
        Self {
            mode: LayoutMode::Stack,
            vcs: None,
            theme: "auto".into(),
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
            test_file_patterns: Vec::new(),
            review_map_server: "http://127.0.0.1:47831".into(),
            review_map_token_file: None,
            // The map now sends whole patches to a remote provider, so it opts in like Ask.
            ai_summaries: false,
            start_on_map: true,
            tests_last: true,
            // Remote inference is opt-in: enabling it sends diff hunks off this machine.
            ask_enabled: false,
            ask_provider: "openai-codex".into(),
            ask_model: "gpt-5.6-luna".into(),
            ask_thinking: "max".into(),
            ask_timeout_secs: 180,
            map_backend: "pi".into(),
            map_provider: "openai-codex".into(),
            map_model: "gpt-5.6-luna".into(),
            map_effort: "max".into(),
            map_timeout_secs: 180,
            chat_enabled: false,
            chat_provider: "openai-codex".into(),
            chat_model: "gpt-5.6-luna".into(),
            chat_effort: "max".into(),
            chat_timeout_secs: 300,
            linear_enabled: true,
            linear_command: "linear".into(),
            review_message: None,
            custom_theme: None,
            startup_notices: Vec::new(),
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
            layer
                .transparent_background_camel
                .or(layer.transparent_background),
        );
        apply(&mut self.color_moved, layer.color_moved);
        if let Some(patterns) = &layer.test_file_patterns {
            self.test_file_patterns.extend(patterns.iter().cloned());
        }
        if let Some(server) = &layer.review_map_server {
            self.review_map_server.clone_from(server);
        }
        if let Some(token_file) = &layer.review_map_token_file {
            self.review_map_token_file = Some(token_file.clone());
        }
        apply(&mut self.ai_summaries, layer.ai_summaries);
        apply(&mut self.start_on_map, layer.start_on_map);
        apply(&mut self.tests_last, layer.tests_last);
        apply(&mut self.ask_enabled, layer.ask_enabled);
        if let Some(provider) = &layer.ask_provider {
            self.ask_provider.clone_from(provider);
        }
        if let Some(model) = &layer.ask_model {
            self.ask_model.clone_from(model);
        }
        if let Some(thinking) = &layer.ask_thinking {
            self.ask_thinking.clone_from(thinking);
        }
        if let Some(timeout) = layer.ask_timeout_secs {
            self.ask_timeout_secs = timeout;
        }
        if let Some(message) = &layer.review_message {
            self.review_message = Some(message.clone());
        }
    }

    /// Applies the sectioned form. Sections are read after the flat legacy keys of the same
    /// layer, so a file carrying both resolves in favour of the section.
    pub(crate) fn apply_sections(&mut self, file: &ConfigFile) {
        if let Some(value) = file.general.vcs {
            self.vcs = Some(value);
        }
        apply(&mut self.watch, file.general.watch);
        apply(&mut self.exclude_untracked, file.general.exclude_untracked);
        apply(
            &mut self.prompt_save_view_preferences,
            file.general.prompt_save_view_preferences,
        );

        if let Some(value) = file.view.mode {
            self.mode = value;
        }
        apply(&mut self.show_sidebar, file.view.show_sidebar);
        apply(&mut self.line_numbers, file.view.line_numbers);
        apply(&mut self.wrap_lines, file.view.wrap_lines);
        apply(&mut self.hunk_headers, file.view.hunk_headers);
        apply(&mut self.agent_notes, file.view.agent_notes);
        apply(
            &mut self.transparent_background,
            file.view.transparent_background,
        );
        apply(&mut self.color_moved, file.view.color_moved);
        apply(&mut self.copy_decorations, file.view.copy_decorations);

        if let Some(name) = file.theme.as_ref().and_then(ThemeSetting::name) {
            self.theme = name.to_owned();
        }

        if let Some(message) = &file.review.message {
            self.review_message = Some(message.clone());
        }
        apply(&mut self.tests_last, file.review.tests_last);
        if let Some(patterns) = &file.review.test_file_patterns {
            self.test_file_patterns.extend(patterns.iter().cloned());
        }

        apply(&mut self.ask_enabled, file.ask.enabled);
        assign(&mut self.ask_provider, &file.ask.provider);
        assign(&mut self.ask_model, &file.ask.model);
        assign(&mut self.ask_thinking, &file.ask.effort);
        if let Some(timeout) = file.ask.timeout_secs {
            self.ask_timeout_secs = timeout;
        }

        apply(&mut self.ai_summaries, file.map.enabled);
        assign(&mut self.map_backend, &file.map.backend);
        assign(&mut self.map_provider, &file.map.provider);
        assign(&mut self.map_model, &file.map.model);
        assign(&mut self.map_effort, &file.map.effort);
        if let Some(timeout) = file.map.timeout_secs {
            self.map_timeout_secs = timeout;
        }
        apply(&mut self.start_on_map, file.map.start_on);
        assign(&mut self.review_map_server, &file.map.server);
        if let Some(token_file) = &file.map.token_file {
            self.review_map_token_file = Some(token_file.clone());
        }

        apply(&mut self.chat_enabled, file.chat.enabled);
        assign(&mut self.chat_provider, &file.chat.provider);
        assign(&mut self.chat_model, &file.chat.model);
        assign(&mut self.chat_effort, &file.chat.effort);
        if let Some(timeout) = file.chat.timeout_secs {
            self.chat_timeout_secs = timeout;
        }

        apply(&mut self.linear_enabled, file.linear.enabled);
        assign(&mut self.linear_command, &file.linear.command);
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

fn assign(target: &mut String, value: &Option<String>) {
    if let Some(value) = value {
        target.clone_from(value);
    }
}

fn apply(target: &mut bool, value: Option<bool>) {
    if let Some(value) = value {
        *target = value;
    }
}
