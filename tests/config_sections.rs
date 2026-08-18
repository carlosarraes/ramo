use ramo::config::{ConfigPaths, ConfigResolver, migrate_user_config};
use ramo::core::input::{CommonOptions, LayoutMode, PatchSource, ReviewInput, VcsId};

fn patch_input() -> ReviewInput {
    ReviewInput::Patch {
        source: PatchSource::Stdin,
        options: CommonOptions::default(),
    }
}

fn resolve(source: &str) -> ramo::config::ResolvedConfig {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("config.toml");
    std::fs::write(&user, source).unwrap();
    ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: None,
    })
    .resolve(&patch_input())
    .unwrap()
}

#[test]
fn every_section_reaches_the_resolved_config() {
    let resolved = resolve(
        r#"
[general]
vcs = "jj"
watch = true

[view]
mode = "split"
line_numbers = false
color_moved = false

[theme]
name = "aurora-x"

[review]
message = "LGTM"
tests_last = false
test_file_patterns = ["qa/**"]

[ask]
enabled = true
provider = "openai-codex"
model = "gpt-5.6-luna"
effort = "high"
timeout_secs = 90

[map]
enabled = true
model = "gpt-5.6-sol"
start_on = false

[chat]
enabled = true
effort = "low"

[linear]
enabled = false
command = "lnr"
"#,
    );

    assert_eq!(resolved.vcs, Some(VcsId::Jj));
    assert!(resolved.watch);
    assert_eq!(resolved.mode, LayoutMode::Split);
    assert!(!resolved.line_numbers);
    assert!(!resolved.color_moved);
    assert_eq!(resolved.theme, "aurora-x");
    assert_eq!(resolved.review_message.as_deref(), Some("LGTM"));
    assert!(!resolved.tests_last);
    assert_eq!(resolved.test_file_patterns, ["qa/**"]);
    assert!(resolved.ask_enabled);
    assert_eq!(resolved.ask_model, "gpt-5.6-luna");
    assert_eq!(resolved.ask_thinking, "high");
    assert_eq!(resolved.ask_timeout_secs, 90);
    assert!(resolved.ai_summaries);
    assert_eq!(resolved.map_model, "gpt-5.6-sol");
    assert!(!resolved.start_on_map);
    assert!(resolved.chat_enabled);
    assert_eq!(resolved.chat_effort, "low");
    assert!(!resolved.linear_enabled);
    assert_eq!(resolved.linear_command, "lnr");
}

#[test]
fn a_section_beats_the_legacy_flat_key_it_replaced() {
    let resolved = resolve("ask_model = \"old\"\n\n[ask]\nmodel = \"new\"\n");
    assert_eq!(resolved.ask_model, "new");
}

#[test]
fn legacy_flat_keys_are_still_read_so_nothing_hard_fails() {
    let resolved = resolve("ask_enabled = true\nask_thinking = \"low\"\nai_summaries = true\n");
    assert!(resolved.ask_enabled);
    assert_eq!(resolved.ask_thinking, "low");
    assert!(resolved.ai_summaries);
}

#[test]
fn unknown_section_keys_are_named_with_their_section() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("config.toml");
    std::fs::write(&user, "[ask]\nthinking = \"max\"\n").unwrap();
    let error = ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: None,
    })
    .resolve(&patch_input())
    .unwrap_err()
    .to_string();
    assert!(error.contains("ask.thinking"), "{error}");
}

#[test]
fn section_values_are_validated_like_the_flat_keys_were() {
    for (source, expected) in [
        (
            "[ask]\neffort = \"maximum\"\n",
            "[ask] effort must be one of",
        ),
        ("[map]\ntimeout_secs = 0\n", "[map] timeout_secs must be"),
        (
            "[chat]\nprovider = \"two words\"\n",
            "[chat] provider must not contain whitespace",
        ),
        ("[map]\nserver = \"https://example.com\"\n", "loopback"),
        (
            "[linear]\ncommand = \"linear issue\"\n",
            "[linear] command must be a single executable",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let user = temp.path().join("config.toml");
        std::fs::write(&user, source).unwrap();
        let error = ConfigResolver::new(ConfigPaths {
            user: Some(user),
            repo: None,
        })
        .resolve(&patch_input())
        .unwrap_err()
        .to_string();
        assert!(error.contains(expected), "{source:?} produced {error}");
    }
}

#[test]
fn migration_rewrites_flat_keys_and_backs_the_file_up() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("config.toml");
    std::fs::write(
        &user,
        "# keep this heading\nmode = \"stack\"\nask_enabled = true # inline note\nreview_message = \"LGTM\"\n\n[diff]\nwrap_lines = true\n",
    )
    .unwrap();

    let migration = migrate_user_config(&user).unwrap().expect("migrated");
    let migrated = std::fs::read_to_string(&user).unwrap();

    assert!(migrated.contains("[view]"), "{migrated}");
    assert!(migrated.contains("mode = \"stack\""), "{migrated}");
    assert!(migrated.contains("[ask]"), "{migrated}");
    assert!(migrated.contains("enabled = true"), "{migrated}");
    assert!(migrated.contains("[review]"), "{migrated}");
    assert!(migrated.contains("message = \"LGTM\""), "{migrated}");
    assert!(
        migrated.contains("# keep this heading"),
        "comments survive: {migrated}"
    );
    assert!(
        migrated.contains("# inline note"),
        "value comments travel with the key: {migrated}"
    );
    assert!(
        migrated.contains("[diff]") && migrated.contains("wrap_lines = true"),
        "command sections are untouched: {migrated}"
    );
    assert!(!migrated.contains("ask_enabled"), "{migrated}");

    let backup = std::fs::read_to_string(migration.backup).unwrap();
    assert!(
        backup.contains("ask_enabled = true"),
        "backup is the original"
    );
    assert!(
        migration
            .moved
            .iter()
            .any(|m| m.contains("ask_enabled -> [ask] enabled")),
        "{:?}",
        migration.moved
    );

    // The migrated file resolves to the same settings, and re-running is a no-op.
    assert!(migrate_user_config(&user).unwrap().is_none());
    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: None,
    })
    .resolve(&patch_input())
    .unwrap();
    assert!(resolved.ask_enabled);
    assert_eq!(resolved.review_message.as_deref(), Some("LGTM"));
}

#[test]
fn migration_moves_the_custom_theme_table() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("config.toml");
    std::fs::write(
        &user,
        "theme = \"aurora-x\"\n\n[custom_theme]\nbase = \"aurora-x\"\naccent = \"#ff0000\"\n",
    )
    .unwrap();

    migrate_user_config(&user).unwrap().expect("migrated");
    let migrated = std::fs::read_to_string(&user).unwrap();

    assert!(migrated.contains("[theme.custom]"), "{migrated}");
    assert!(!migrated.contains("[custom_theme]"), "{migrated}");

    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: None,
    })
    .resolve(&patch_input())
    .unwrap();
    assert_eq!(resolved.theme, "aurora-x");
    assert_eq!(
        resolved
            .custom_theme
            .and_then(|theme| theme.color("accent").map(str::to_owned)),
        Some("#ff0000".to_owned())
    );
}

#[test]
fn a_file_already_in_the_new_shape_is_left_alone() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("config.toml");
    let source = "[ask]\nenabled = true\n\n[view]\nmode = \"split\"\n";
    std::fs::write(&user, source).unwrap();

    assert!(migrate_user_config(&user).unwrap().is_none());
    assert_eq!(std::fs::read_to_string(&user).unwrap(), source);
}

#[test]
fn both_spellings_of_theme_parse_because_the_key_collides_with_its_section() {
    // `theme` is the one legacy key whose name is also a section name.
    assert_eq!(resolve("theme = \"aurora-x\"\n").theme, "aurora-x");
    assert_eq!(resolve("[theme]\nname = \"aurora-x\"\n").theme, "aurora-x");

    // And the table form must survive a save that runs the migration over it.
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("config.toml");
    std::fs::write(&user, "[theme]\nname = \"vesper\"\n").unwrap();
    assert!(migrate_user_config(&user).unwrap().is_none());
    assert!(
        std::fs::read_to_string(&user).unwrap().contains("vesper"),
        "migrating must not delete the [theme] table"
    );
}
