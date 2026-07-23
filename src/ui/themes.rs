use std::collections::BTreeMap;

use ratatui::style::{Color, Style};

use crate::config::CustomThemeConfig;

pub const DEFAULT_DARK_THEME_ID: &str = "tokyo-night";
pub const DEFAULT_LIGHT_THEME_ID: &str = "github-light-default";

pub const BUNDLED_THEME_IDS: &[&str] = &[
    "andromeeda",
    "aurora-x",
    "ayu-dark",
    "ayu-light",
    "ayu-mirage",
    "catppuccin-frappe",
    "catppuccin-latte",
    "catppuccin-macchiato",
    "catppuccin-mocha",
    "dark-plus",
    "dracula",
    "dracula-soft",
    "everforest-dark",
    "everforest-light",
    "github-dark",
    "github-dark-default",
    "github-dark-dimmed",
    "github-dark-high-contrast",
    "github-light",
    "github-light-default",
    "github-light-high-contrast",
    "gruvbox-dark-hard",
    "gruvbox-dark-medium",
    "gruvbox-dark-soft",
    "gruvbox-light-hard",
    "gruvbox-light-medium",
    "gruvbox-light-soft",
    "horizon",
    "horizon-bright",
    "houston",
    "kanagawa-dragon",
    "kanagawa-lotus",
    "kanagawa-wave",
    "laserwave",
    "light-plus",
    "material-theme",
    "material-theme-darker",
    "material-theme-lighter",
    "material-theme-ocean",
    "material-theme-palenight",
    "min-dark",
    "min-light",
    "monokai",
    "night-owl",
    "night-owl-light",
    "nord",
    "one-dark-pro",
    "one-light",
    "plastic",
    "poimandres",
    "red",
    "rose-pine",
    "rose-pine-dawn",
    "rose-pine-moon",
    "slack-dark",
    "slack-ochin",
    "snazzy-light",
    "solarized-dark",
    "solarized-light",
    "synthwave-84",
    "tokyo-night",
    "vesper",
    "vitesse-black",
    "vitesse-dark",
    "vitesse-light",
];

const BACKGROUNDS: &[&str] = &[
    "#23262e", "#07090f", "#10141c", "#fcfcfc", "#242936", "#303446", "#eff1f5", "#24273a",
    "#1e1e2e", "#1e1e1e", "#282a36", "#282a36", "#2d353b", "#fdf6e3", "#24292e", "#0d1117",
    "#22272e", "#0a0c10", "#ffffff", "#ffffff", "#ffffff", "#1d2021", "#282828", "#32302f",
    "#f9f5d7", "#fbf1c7", "#f2e5bc", "#1c1e26", "#fdf0ed", "#17191e", "#181616", "#f2ecbc",
    "#1f1f28", "#27212e", "#ffffff", "#263238", "#212121", "#fafafa", "#0f111a", "#292d3e",
    "#1f1f1f", "#ffffff", "#272822", "#011627", "#fbfbfb", "#2e3440", "#282c34", "#fafafa",
    "#21252b", "#1b1e28", "#390000", "#191724", "#faf4ed", "#232136", "#222222", "#ffffff",
    "#fafbfc", "#002b36", "#fdf6e3", "#262335", "#1a1b26", "#101010", "#000000", "#121212",
    "#ffffff",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalAppearance {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewLineStyle {
    Context,
    Added,
    Removed,
    MovedAdded,
    MovedRemoved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppTheme {
    pub id: String,
    pub label: String,
    pub appearance: TerminalAppearance,
    pub background: Color,
    pub panel: Color,
    pub panel_alt: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_muted: Color,
    pub text: Color,
    pub muted: Color,
    pub added_bg: Color,
    pub removed_bg: Color,
    pub moved_added_bg: Color,
    pub moved_removed_bg: Color,
    pub context_bg: Color,
    pub added_content_bg: Color,
    pub removed_content_bg: Color,
    pub context_content_bg: Color,
    pub added_sign: Color,
    pub removed_sign: Color,
    pub line_number_bg: Color,
    pub line_number_fg: Color,
    pub selected_hunk: Color,
    pub badge_added: Color,
    pub badge_removed: Color,
    pub badge_neutral: Color,
    pub file_new: Color,
    pub file_deleted: Color,
    pub file_renamed: Color,
    pub file_modified: Color,
    pub file_untracked: Color,
    pub note_border: Color,
    pub note_background: Color,
    pub note_title_background: Color,
    pub note_title_text: Color,
    pub syntax_theme: String,
    pub syntax_scope_overrides: BTreeMap<String, String>,
}

impl AppTheme {
    pub fn row_style(&self, kind: ReviewLineStyle) -> Style {
        Style::default().fg(self.text).bg(self.row_background(kind))
    }

    pub fn gutter_style(&self, kind: ReviewLineStyle) -> Style {
        Style::default()
            .fg(self.line_number_fg)
            .bg(self.row_background(kind))
    }

    pub fn changed_style(&self, kind: ReviewLineStyle) -> Style {
        let background = match kind {
            ReviewLineStyle::Added | ReviewLineStyle::MovedAdded => self.added_content_bg,
            ReviewLineStyle::Removed | ReviewLineStyle::MovedRemoved => self.removed_content_bg,
            ReviewLineStyle::Context => self.context_content_bg,
        };
        Style::default().fg(self.text).bg(background)
    }

    fn row_background(&self, kind: ReviewLineStyle) -> Color {
        match kind {
            ReviewLineStyle::Context => self.context_bg,
            ReviewLineStyle::Added => self.added_bg,
            ReviewLineStyle::Removed => self.removed_bg,
            ReviewLineStyle::MovedAdded => self.moved_added_bg,
            ReviewLineStyle::MovedRemoved => self.moved_removed_bg,
        }
    }

    pub(crate) fn cache_key(&self) -> String {
        let mut key = format!("{}:{:?}:{:?}", self.id, self.text, self.appearance);
        for (scope, color) in &self.syntax_scope_overrides {
            key.push('|');
            key.push_str(scope);
            key.push('=');
            key.push_str(color);
        }
        key
    }
}

#[derive(Debug, Clone, Default)]
pub struct ThemeRegistry {
    custom: Option<CustomThemeConfig>,
}

impl ThemeRegistry {
    pub fn new(custom: Option<CustomThemeConfig>) -> Self {
        Self { custom }
    }

    pub fn selector_items(&self) -> Vec<&'static str> {
        let mut ids = BUNDLED_THEME_IDS.to_vec();
        if self.custom.is_some() {
            ids.push("custom");
        }
        ids
    }

    pub fn resolve(
        &self,
        requested: &str,
        appearance: Option<TerminalAppearance>,
        transparent: bool,
    ) -> AppTheme {
        let requested = if requested == "auto" {
            match appearance {
                Some(TerminalAppearance::Light) => DEFAULT_LIGHT_THEME_ID,
                _ => DEFAULT_DARK_THEME_ID,
            }
        } else {
            requested
        };
        let mut theme = if requested == "custom" {
            self.custom
                .as_ref()
                .map_or_else(|| built_in(DEFAULT_DARK_THEME_ID), build_custom)
        } else {
            normalize_builtin_theme_id(requested)
                .map_or_else(|| built_in(DEFAULT_DARK_THEME_ID), built_in)
        };
        if transparent {
            theme.background = Color::Reset;
            theme.panel = Color::Reset;
            theme.panel_alt = Color::Reset;
            theme.context_bg = Color::Reset;
            theme.context_content_bg = Color::Reset;
            theme.line_number_bg = Color::Reset;
        }
        theme
    }
}

pub fn normalize_builtin_theme_id(id: &str) -> Option<&'static str> {
    let canonical = match id {
        "graphite" => "github-dark-default",
        "midnight" => "github-dark-dimmed",
        "paper" => DEFAULT_LIGHT_THEME_ID,
        "ember" => "dark-plus",
        "zenburn" => "everforest-dark",
        other => BUNDLED_THEME_IDS
            .iter()
            .copied()
            .find(|candidate| *candidate == other)?,
    };
    Some(canonical)
}

fn built_in(id: &str) -> AppTheme {
    let index = BUNDLED_THEME_IDS
        .iter()
        .position(|candidate| *candidate == id)
        .unwrap_or_else(|| {
            BUNDLED_THEME_IDS
                .iter()
                .position(|candidate| *candidate == DEFAULT_DARK_THEME_ID)
                .expect("default theme is bundled")
        });
    let background = rgb(BACKGROUNDS[index]);
    let light = luminance(background) > 0.45;
    let foreground = readable_foreground(foreground_for(id).map(rgb), background);
    let (fallback_added, fallback_removed, fallback_modified) = if light {
        (rgb("#0dbe4e"), rgb("#ff2e3f"), rgb("#009fff"))
    } else {
        (rgb("#5ecc71"), rgb("#ff6762"), rgb("#69b1ff"))
    };
    let (raw_added, raw_removed, raw_modified) = diff_colors_for(id)
        .map(|(added, removed, modified)| (rgb(added), rgb(removed), rgb(modified)))
        .unwrap_or((fallback_added, fallback_removed, fallback_modified));
    let added_sign = readable_diff_sign(raw_added, background);
    let removed_sign = readable_diff_sign(raw_removed, background);
    let modified = readable_diff_sign(raw_modified, background);
    let panel = blend(foreground, background, if light { 0.04 } else { 0.08 });
    let panel_alt = blend(foreground, background, if light { 0.08 } else { 0.12 });
    let border = blend(foreground, background, if light { 0.15 } else { 0.18 });
    let text = readable_foreground(Some(foreground), panel_alt);
    let muted = readable_dim(blend(text, background, 0.56), panel_alt);
    let line_number_fg = readable_dim(blend(text, background, 0.56), background);
    let row_tint = if light { 0.12 } else { 0.20 };
    let content_tint = if light { 0.18 } else { 0.28 };
    let selection_tint = if light { 0.18 } else { 0.25 };
    let added_bg = readable_tint(added_sign, background, text, row_tint);
    let removed_bg = readable_tint(removed_sign, background, text, row_tint);
    let moved_bg = readable_tint(modified, background, text, row_tint);
    let badge_added = readable_chrome(added_sign, panel, panel_alt);
    let badge_removed = readable_chrome(removed_sign, panel, panel_alt);
    let badge_modified = readable_chrome(modified, panel, panel_alt);
    AppTheme {
        id: id.into(),
        label: id.into(),
        appearance: if light {
            TerminalAppearance::Light
        } else {
            TerminalAppearance::Dark
        },
        background,
        panel,
        panel_alt,
        border,
        accent: modified,
        accent_muted: readable_tint(modified, background, text, selection_tint),
        text,
        muted,
        added_bg,
        removed_bg,
        moved_added_bg: moved_bg,
        moved_removed_bg: moved_bg,
        context_bg: background,
        added_content_bg: readable_tint(added_sign, background, text, content_tint),
        removed_content_bg: readable_tint(removed_sign, background, text, content_tint),
        context_content_bg: background,
        added_sign,
        removed_sign,
        line_number_bg: background,
        line_number_fg,
        selected_hunk: blend(modified, background, selection_tint),
        badge_added,
        badge_removed,
        badge_neutral: muted,
        file_new: badge_added,
        file_deleted: badge_removed,
        file_renamed: badge_modified,
        file_modified: badge_modified,
        file_untracked: badge_added,
        note_border: modified,
        note_background: panel,
        note_title_background: panel,
        note_title_text: text,
        syntax_theme: id.into(),
        syntax_scope_overrides: BTreeMap::new(),
    }
}

fn build_custom(custom: &CustomThemeConfig) -> AppTheme {
    let base_id = custom
        .base
        .as_deref()
        .and_then(normalize_builtin_theme_id)
        .unwrap_or(DEFAULT_DARK_THEME_ID);
    let mut theme = built_in(base_id);
    theme.id = "custom".into();
    theme.label = custom.label.clone().unwrap_or_else(|| "Custom".into());
    for (key, value) in &custom.colors {
        let Some(color) = parse_rgb(value) else {
            continue;
        };
        match key.as_str() {
            "background" => theme.background = color,
            "panel" => theme.panel = color,
            "panelAlt" => theme.panel_alt = color,
            "border" => theme.border = color,
            "accent" => theme.accent = color,
            "accentMuted" => theme.accent_muted = color,
            "text" => theme.text = color,
            "muted" => theme.muted = color,
            "addedBg" => theme.added_bg = color,
            "removedBg" => theme.removed_bg = color,
            "movedAddedBg" => theme.moved_added_bg = color,
            "movedRemovedBg" => theme.moved_removed_bg = color,
            "contextBg" => theme.context_bg = color,
            "addedContentBg" => theme.added_content_bg = color,
            "removedContentBg" => theme.removed_content_bg = color,
            "contextContentBg" => theme.context_content_bg = color,
            "addedSignColor" => theme.added_sign = color,
            "removedSignColor" => theme.removed_sign = color,
            "lineNumberBg" => theme.line_number_bg = color,
            "lineNumberFg" => theme.line_number_fg = color,
            "selectedHunk" => theme.selected_hunk = color,
            "badgeAdded" => theme.badge_added = color,
            "badgeRemoved" => theme.badge_removed = color,
            "badgeNeutral" => theme.badge_neutral = color,
            "fileNew" => theme.file_new = color,
            "fileDeleted" => theme.file_deleted = color,
            "fileRenamed" => theme.file_renamed = color,
            "fileModified" => theme.file_modified = color,
            "fileUntracked" => theme.file_untracked = color,
            "noteBorder" => theme.note_border = color,
            "noteBackground" => theme.note_background = color,
            "noteTitleBackground" => theme.note_title_background = color,
            "noteTitleText" => theme.note_title_text = color,
            _ => {}
        }
    }
    theme.syntax_scope_overrides = custom.syntax_scopes.clone();
    theme
}

fn foreground_for(id: &str) -> Option<&'static str> {
    Some(match id {
        "andromeeda" => "#d5ced9",
        "ayu-dark" => "#bfbdb6",
        "ayu-light" => "#5c6166",
        "ayu-mirage" => "#cccac2",
        "catppuccin-frappe" => "#c6d0f5",
        "catppuccin-latte" => "#4c4f69",
        "catppuccin-macchiato" => "#cad3f5",
        "catppuccin-mocha" => "#cdd6f4",
        "dark-plus" => "#d4d4d4",
        "dracula" => "#f8f8f2",
        "dracula-soft" => "#f6f6f4",
        "everforest-dark" => "#d3c6aa",
        "everforest-light" => "#5c6a72",
        "github-dark" => "#e1e4e8",
        "github-dark-default" => "#e6edf3",
        "github-dark-dimmed" => "#adbac7",
        "github-dark-high-contrast" => "#f0f3f6",
        "github-light" => "#24292e",
        "github-light-default" => "#1f2328",
        "github-light-high-contrast" => "#0e1116",
        "gruvbox-dark-hard" | "gruvbox-dark-medium" | "gruvbox-dark-soft" => "#ebdbb2",
        "gruvbox-light-hard" | "gruvbox-light-medium" | "gruvbox-light-soft" => "#3c3836",
        "houston" => "#eef0f9",
        "kanagawa-dragon" => "#c5c9c5",
        "kanagawa-lotus" => "#545464",
        "kanagawa-wave" => "#dcd7ba",
        "laserwave" => "#ffffff",
        "light-plus" => "#000000",
        "material-theme" | "material-theme-darker" => "#eeffff",
        "material-theme-lighter" => "#90a4ae",
        "material-theme-ocean" | "material-theme-palenight" => "#babed8",
        "min-light" => "#212121",
        "monokai" => "#f8f8f2",
        "night-owl" => "#d6deeb",
        "night-owl-light" => "#403f53",
        "nord" => "#d8dee9",
        "one-dark-pro" => "#abb2bf",
        "one-light" => "#383a42",
        "plastic" => "#a9b2c3",
        "poimandres" => "#a6accd",
        "red" => "#f8f8f8",
        "rose-pine" | "rose-pine-moon" => "#e0def4",
        "rose-pine-dawn" => "#575279",
        "slack-dark" => "#e6e6e6",
        "slack-ochin" => "#000000",
        "snazzy-light" => "#565869",
        "solarized-dark" => "#839496",
        "solarized-light" => "#657b83",
        "tokyo-night" => "#a9b1d6",
        "vesper" => "#ffffff",
        "vitesse-black" | "vitesse-dark" => "#dbd7ca",
        "vitesse-light" => "#393a34",
        _ => return None,
    })
}

fn diff_colors_for(id: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match id {
        "andromeeda" => ("#96e072", "#ee5d43", "#7cb7ff"),
        "aurora-x" => ("#63d188", "#dd5074", "#c778db"),
        "ayu-dark" => ("#70bf56", "#f26d78", "#73b8ff"),
        "ayu-light" => ("#6cbf43", "#ff7383", "#478acc"),
        "ayu-mirage" => ("#87d96c", "#f27983", "#80bfff"),
        "catppuccin-frappe" => ("#a6d189", "#e78284", "#e5c890"),
        "catppuccin-latte" => ("#40a02b", "#d20f39", "#df8e1d"),
        "catppuccin-macchiato" => ("#a6da95", "#ed8796", "#eed49f"),
        "catppuccin-mocha" => ("#a6e3a1", "#f38ba8", "#f9e2af"),
        "dracula" => ("#50fa7b", "#ff5555", "#8be9fd"),
        "dracula-soft" => ("#62e884", "#ee6666", "#97e1f1"),
        "everforest-dark" => ("#7a8c66", "#a16366", "#608986"),
        "everforest-light" => ("#b7c155", "#fa9188", "#83b9d0"),
        "github-dark" => ("#34d058", "#ea4a5a", "#79b8ff"),
        "github-dark-default" => ("#3fb950", "#f85149", "#d29922"),
        "github-dark-dimmed" => ("#57ab5a", "#e5534b", "#c69026"),
        "github-dark-high-contrast" => ("#26cd4d", "#ff6a69", "#f0b72f"),
        "github-light" => ("#28a745", "#d73a49", "#005cc5"),
        "github-light-default" => ("#1a7f37", "#cf222e", "#9a6700"),
        "github-light-high-contrast" => ("#055d20", "#a0111f", "#744500"),
        "gruvbox-dark-hard" | "gruvbox-dark-medium" | "gruvbox-dark-soft" => {
            ("#ebdbb2", "#cc241d", "#d79921")
        }
        "gruvbox-light-hard" | "gruvbox-light-medium" | "gruvbox-light-soft" => {
            ("#3c3836", "#cc241d", "#d79921")
        }
        "horizon" => ("#24a075", "#f43e5c", "#fab38e"),
        "horizon-bright" => ("#60c9a0", "#f43e5c", "#af5427"),
        "houston" => ("#4bf3c8", "#f4587e", "#ffd493"),
        "kanagawa-dragon" => ("#8a9a7b", "#c4746e", "#8ba4b0"),
        "kanagawa-lotus" => ("#6f894e", "#c84053", "#4d699b"),
        "kanagawa-wave" => ("#76946a", "#c34043", "#7e9cd8"),
        "laserwave" => ("#74dfc4", "#b381c5", "#74dfc4"),
        "material-theme" => ("#c3e88d", "#98565c", "#5a76a8"),
        "material-theme-darker" => ("#c3e88d", "#964e52", "#586e9e"),
        "material-theme-lighter" => ("#91b859", "#ee8d8b", "#a4b6d5"),
        "material-theme-ocean" => ("#c3e88d", "#8e474f", "#50679b"),
        "material-theme-palenight" => ("#c3e88d", "#99535f", "#5b74ab"),
        "min-light" => ("#77cc00", "#d32f2f", "#e0e0e0"),
        "monokai" => ("#86b42b", "#c4265e", "#6a7ec8"),
        "night-owl" => ("#22da6e", "#87383e", "#a2bffc"),
        "night-owl-light" => ("#08916a", "#de3d3b", "#288ed7"),
        "nord" => ("#a3be8c", "#bf616a", "#ebcb8b"),
        "one-dark-pro" => ("#8cc265", "#e05561", "#4aa5f0"),
        "plastic" => ("#98c379", "#e06c75", "#d19a66"),
        "poimandres" => ("#5fb3a1", "#d0679d", "#add7ff"),
        "rose-pine" => ("#9ccfd8", "#908caa", "#ebbcba"),
        "rose-pine-dawn" => ("#56949f", "#797593", "#d7827e"),
        "rose-pine-moon" => ("#9ccfd8", "#908caa", "#ea9a97"),
        "slack-dark" | "slack-ochin" => ("#ecb22e", "#ffffff", "#ecb22e"),
        "snazzy-light" => ("#2dae58", "#ff5c57", "#00a39f"),
        "solarized-dark" | "solarized-light" => ("#859900", "#dc322f", "#268bd2"),
        "synthwave-84" => ("#63c89e", "#fe4450", "#ae8cc4"),
        "tokyo-night" => ("#449dab", "#914c54", "#6183bb"),
        "vitesse-black" | "vitesse-dark" => ("#4d9375", "#cb7676", "#6394bf"),
        "vitesse-light" => ("#1e754f", "#ab5959", "#296aa3"),
        _ => return None,
    })
}

fn parse_rgb(value: &str) -> Option<Color> {
    let value = value.strip_prefix('#')?;
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let number = u32::from_str_radix(value, 16).ok()?;
    Some(Color::Rgb(
        ((number >> 16) & 0xff) as u8,
        ((number >> 8) & 0xff) as u8,
        (number & 0xff) as u8,
    ))
}

fn rgb(value: &str) -> Color {
    parse_rgb(value).expect("embedded theme colors are valid")
}

fn channels(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(red, green, blue) => (red, green, blue),
        _ => (0, 0, 0),
    }
}

fn blend(foreground: Color, background: Color, ratio: f64) -> Color {
    let (fr, fg, fb) = channels(foreground);
    let (br, bg, bb) = channels(background);
    let mix = |front: u8, back: u8| {
        (f64::from(back) + (f64::from(front) - f64::from(back)) * ratio).round() as u8
    };
    Color::Rgb(mix(fr, br), mix(fg, bg), mix(fb, bb))
}

fn luminance(color: Color) -> f64 {
    let (red, green, blue) = channels(color);
    let linear = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue)
}

fn contrast(foreground: Color, background: Color) -> f64 {
    let a = luminance(foreground);
    let b = luminance(background);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn readable_foreground(preferred: Option<Color>, background: Color) -> Color {
    preferred
        .filter(|color| contrast(*color, background) >= 4.5)
        .unwrap_or_else(|| {
            if luminance(background) > 0.45 {
                rgb("#000000")
            } else {
                rgb("#ffffff")
            }
        })
}

fn readable_dim(preferred: Color, background: Color) -> Color {
    if contrast(preferred, background) >= 4.5 {
        preferred
    } else if luminance(background) > 0.45 {
        blend(rgb("#000000"), background, 0.62)
    } else {
        blend(rgb("#ffffff"), background, 0.62)
    }
}

fn readable_diff_sign(preferred: Color, background: Color) -> Color {
    if contrast(preferred, background) >= 3.0 {
        preferred
    } else if luminance(background) > 0.45 {
        blend(rgb("#000000"), preferred, 0.45)
    } else {
        blend(rgb("#ffffff"), preferred, 0.45)
    }
}

fn readable_tint(tint: Color, background: Color, foreground: Color, preferred: f64) -> Color {
    let mut amount = preferred;
    while amount >= 0.019 {
        let candidate = blend(tint, background, amount);
        if contrast(foreground, candidate) >= 4.5 {
            return candidate;
        }
        amount -= 0.02;
    }
    background
}

fn readable_chrome(preferred: Color, panel: Color, panel_alt: Color) -> Color {
    if contrast(preferred, panel) >= 4.5 && contrast(preferred, panel_alt) >= 4.5 {
        return preferred;
    }
    let anchor = if luminance(panel_alt) > 0.45 {
        rgb("#000000")
    } else {
        rgb("#ffffff")
    };
    for amount in [0.35, 0.5, 0.65, 0.8, 1.0] {
        let candidate = blend(anchor, preferred, amount);
        if contrast(candidate, panel) >= 4.5 && contrast(candidate, panel_alt) >= 4.5 {
            return candidate;
        }
    }
    anchor
}
