use std::collections::BTreeMap;

use ramo::config::{ConfigPaths, ConfigResolver, CustomThemeConfig};
use ramo::core::input::{CommonOptions, PatchSource, ReviewInput};
use ramo::ui::themes::{
    AppTheme, BUNDLED_THEME_IDS, ReviewLineStyle, TerminalAppearance, ThemeRegistry,
};
use ratatui::style::Color;

const HUNK_THEME_ORDER: &[&str] = &[
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

#[test]
fn registry_preserves_hunks_reference_order_and_legacy_aliases() {
    assert_eq!(BUNDLED_THEME_IDS, HUNK_THEME_ORDER);
    let registry = ThemeRegistry::default();
    assert_eq!(registry.selector_items(), HUNK_THEME_ORDER);
    for (legacy, canonical) in [
        ("graphite", "github-dark-default"),
        ("midnight", "github-dark-dimmed"),
        ("paper", "github-light-default"),
        ("ember", "dark-plus"),
        ("zenburn", "everforest-dark"),
    ] {
        assert_eq!(registry.resolve(legacy, None, false).id, canonical);
    }
}

#[test]
fn fallback_auto_and_transparent_surfaces_are_predictable() {
    let registry = ThemeRegistry::default();
    assert_eq!(
        registry.resolve("missing", None, false).id,
        "github-dark-default"
    );
    assert_eq!(
        registry
            .resolve("auto", Some(TerminalAppearance::Light), false)
            .id,
        "github-light-default"
    );
    assert_eq!(
        registry
            .resolve("auto", Some(TerminalAppearance::Dark), false)
            .id,
        "github-dark-default"
    );

    let opaque = registry.resolve("github-dark-default", None, false);
    let transparent = registry.resolve("github-dark-default", None, true);
    assert_eq!(transparent.background, Color::Reset);
    assert_eq!(transparent.panel, Color::Reset);
    assert_eq!(transparent.context_bg, Color::Reset);
    assert_eq!(transparent.line_number_bg, Color::Reset);
    assert_eq!(transparent.added_bg, opaque.added_bg);
    assert_eq!(transparent.removed_bg, opaque.removed_bg);
    assert_eq!(transparent.moved_added_bg, opaque.moved_added_bg);
    assert_eq!(transparent.moved_removed_bg, opaque.moved_removed_bg);
    assert_eq!(transparent.note_background, opaque.note_background);
}

#[test]
fn semantic_rows_gutters_and_changed_content_use_matching_palettes() {
    let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);
    for (kind, row, content) in [
        (
            ReviewLineStyle::Added,
            theme.added_bg,
            theme.added_content_bg,
        ),
        (
            ReviewLineStyle::Removed,
            theme.removed_bg,
            theme.removed_content_bg,
        ),
        (
            ReviewLineStyle::MovedAdded,
            theme.moved_added_bg,
            theme.added_content_bg,
        ),
        (
            ReviewLineStyle::MovedRemoved,
            theme.moved_removed_bg,
            theme.removed_content_bg,
        ),
    ] {
        assert_eq!(theme.row_style(kind).bg, Some(row));
        assert_eq!(theme.gutter_style(kind).bg, Some(row));
        assert_eq!(theme.changed_style(kind).bg, Some(content));
        assert_ne!(row, content);
    }
}

#[test]
fn custom_theme_inherits_validated_colors_and_preserves_exact_scopes() {
    let custom = CustomThemeConfig {
        base: Some("catppuccin-mocha".into()),
        label: Some("My Theme".into()),
        colors: BTreeMap::from([
            ("text".into(), "#ffffff".into()),
            ("addedBg".into(), "#112233".into()),
        ]),
        syntax_scopes: BTreeMap::from([("keyword.control.rust".into(), "#ff00ff".into())]),
        ..CustomThemeConfig::default()
    };
    let registry = ThemeRegistry::new(Some(custom));
    let base = registry.resolve("catppuccin-mocha", None, false);
    let resolved = registry.resolve("custom", None, false);
    assert_eq!(resolved.id, "custom");
    assert_eq!(resolved.label, "My Theme");
    assert_eq!(resolved.background, base.background);
    assert_eq!(resolved.text, Color::Rgb(255, 255, 255));
    assert_eq!(resolved.added_bg, Color::Rgb(17, 34, 51));
    assert_eq!(
        resolved.syntax_scope_overrides,
        BTreeMap::from([("keyword.control.rust".into(), "#ff00ff".into())])
    );
}

#[test]
fn config_rejects_invalid_custom_colors_and_unknown_fields_with_paths() {
    let temp = tempfile::tempdir().unwrap();
    let input = ReviewInput::Patch {
        source: PatchSource::Stdin,
        options: CommonOptions::default(),
    };
    for (name, source, expected) in [
        (
            "bad-color.toml",
            "[custom_theme]\naccent = \"blue\"\n",
            "custom_theme.accent",
        ),
        (
            "unknown.toml",
            "[custom_theme]\nmadeUp = \"#112233\"\n",
            "custom_theme.madeUp",
        ),
        (
            "bad-scope.toml",
            "[custom_theme.syntax_scopes]\n\"keyword.control\" = \"red\"\n",
            "custom_theme.syntax_scopes.keyword.control",
        ),
    ] {
        let path = temp.path().join(name);
        std::fs::write(&path, source).unwrap();
        let error = ConfigResolver::new(ConfigPaths {
            user: Some(path.clone()),
            repo: None,
        })
        .resolve(&input)
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains(&path.display().to_string()), "{message}");
        assert!(message.contains(expected), "{message}");
    }
}

#[test]
fn user_and_repository_custom_theme_layers_merge_by_field_and_scope() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("user.toml");
    let repo = temp.path().join("repo.toml");
    std::fs::write(
        &user,
        concat!(
            "theme = \"custom\"\n",
            "[custom_theme]\n",
            "base = \"graphite\"\n",
            "accent = \"#123456\"\n",
            "[custom_theme.syntax_scopes]\n",
            "\"keyword.control\" = \"#abcdef\"\n",
        ),
    )
    .unwrap();
    std::fs::write(
        &repo,
        concat!(
            "[custom_theme]\n",
            "label = \"Repository\"\n",
            "panel = \"#654321\"\n",
            "[custom_theme.syntax_scopes]\n",
            "\"string.quoted\" = \"#fedcba\"\n",
        ),
    )
    .unwrap();
    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: Some(repo),
    })
    .resolve(&ReviewInput::Patch {
        source: PatchSource::Stdin,
        options: CommonOptions::default(),
    })
    .unwrap();
    let custom = resolved.custom_theme.unwrap();
    assert_eq!(custom.base.as_deref(), Some("github-dark-default"));
    assert_eq!(custom.label.as_deref(), Some("Repository"));
    assert_eq!(custom.color("accent"), Some("#123456"));
    assert_eq!(custom.color("panel"), Some("#654321"));
    assert_eq!(custom.syntax_scopes.len(), 2);
}

#[test]
fn deprecated_semantic_syntax_is_translated_and_emits_one_startup_notice() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("user.toml");
    std::fs::write(
        &user,
        concat!(
            "theme = \"custom\"\n",
            "[custom_theme.syntax]\n",
            "keyword = \"#112233\"\n",
            "comment = \"#445566\"\n",
            "[custom_theme.syntax_scopes]\n",
            "comment = \"#abcdef\"\n",
        ),
    )
    .unwrap();
    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: None,
    })
    .resolve(&ReviewInput::Patch {
        source: PatchSource::Stdin,
        options: CommonOptions::default(),
    })
    .unwrap();
    let custom = resolved.custom_theme.unwrap();

    assert_eq!(custom.syntax_scopes.get("keyword"), Some(&"#112233".into()));
    assert_eq!(custom.syntax_scopes.get("comment"), Some(&"#abcdef".into()));
    assert_eq!(
        custom.syntax_scopes.get("punctuation.definition.comment"),
        Some(&"#445566".into())
    );
    assert_eq!(resolved.startup_notices.len(), 1);
    assert!(resolved.startup_notices[0].contains("Deprecated [custom_theme.syntax]"));
}

fn _assert_app_theme_is_clone(_: AppTheme) {}
