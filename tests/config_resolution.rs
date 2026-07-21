use pdiff::config::{ConfigPaths, ConfigResolver};
use pdiff::core::input::{CommonOptions, LayoutMode, ReviewInput, VcsId};

#[test]
fn builtin_user_repo_command_and_cli_layers_merge_in_order() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("user.toml");
    let repo = temp.path().join("repo/.pdiff/config.toml");
    std::fs::create_dir_all(repo.parent().unwrap()).unwrap();
    std::fs::write(
        &user,
        "mode = \"stack\"\nshow_sidebar = false\nline_numbers = false\n",
    )
    .unwrap();
    std::fs::write(&repo, "line_numbers = true\n[diff]\nwrap_lines = true\n").unwrap();
    let input = ReviewInput::VcsDiff {
        range: None,
        staged: false,
        pathspecs: vec![],
        options: CommonOptions {
            mode: Some(LayoutMode::Split),
            ..Default::default()
        },
    };
    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: Some(repo),
    })
    .resolve(&input)
    .unwrap();
    assert_eq!(resolved.mode, LayoutMode::Split);
    assert!(!resolved.show_sidebar);
    assert!(resolved.line_numbers);
    assert!(resolved.wrap_lines);
}

fn patch_input(options: CommonOptions) -> ReviewInput {
    ReviewInput::Patch {
        source: pdiff::core::input::PatchSource::Stdin,
        options,
    }
}

#[test]
fn repository_command_values_override_user_command_values() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("user.toml");
    let repo = temp.path().join("repo.toml");
    std::fs::write(&user, "[patch]\nwrap_lines = false\n").unwrap();
    std::fs::write(&repo, "[patch]\nwrap_lines = true\n").unwrap();
    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: Some(repo),
    })
    .resolve(&patch_input(CommonOptions::default()))
    .unwrap();
    assert!(resolved.wrap_lines);
}

#[test]
fn explicit_false_cli_value_overrides_true_config_value() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("user.toml");
    std::fs::write(&user, "line_numbers = true\n").unwrap();
    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: None,
    })
    .resolve(&patch_input(CommonOptions {
        line_numbers: Some(false),
        ..Default::default()
    }))
    .unwrap();
    assert!(!resolved.line_numbers);
}

#[test]
fn malformed_and_unknown_config_errors_name_the_file_and_key() {
    let temp = tempfile::tempdir().unwrap();
    let malformed = temp.path().join("malformed.toml");
    std::fs::write(&malformed, "mode = [\n").unwrap();
    let malformed_error = ConfigResolver::new(ConfigPaths {
        user: Some(malformed.clone()),
        repo: None,
    })
    .resolve(&patch_input(CommonOptions::default()))
    .unwrap_err();
    assert!(
        malformed_error
            .to_string()
            .contains(&malformed.display().to_string())
    );

    let unknown = temp.path().join("unknown.toml");
    std::fs::write(&unknown, "menu_bar = true\n").unwrap();
    let unknown_error = ConfigResolver::new(ConfigPaths {
        user: Some(unknown.clone()),
        repo: None,
    })
    .resolve(&patch_input(CommonOptions::default()))
    .unwrap_err();
    let message = unknown_error.to_string();
    assert!(message.contains(&unknown.display().to_string()));
    assert!(message.contains("menu_bar"));
}

#[test]
fn missing_files_are_ignored() {
    let temp = tempfile::tempdir().unwrap();
    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(temp.path().join("absent-user.toml")),
        repo: Some(temp.path().join("absent-repo.toml")),
    })
    .resolve(&patch_input(CommonOptions::default()))
    .unwrap();
    assert_eq!(resolved.mode, LayoutMode::Auto);
    assert_eq!(resolved.theme, "auto");
    assert!(resolved.line_numbers);
}

#[test]
fn discovery_chooses_the_nearest_repository_config() {
    let temp = tempfile::tempdir().unwrap();
    let outer = temp.path().join(".pdiff/config.toml");
    let inner_root = temp.path().join("nested/project");
    let inner = inner_root.join(".pdiff/config.toml");
    let cwd = inner_root.join("src");
    std::fs::create_dir_all(outer.parent().unwrap()).unwrap();
    std::fs::create_dir_all(inner.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::write(&outer, "mode = \"stack\"\n").unwrap();
    std::fs::write(&inner, "mode = \"split\"\n").unwrap();
    assert_eq!(ConfigPaths::discover(&cwd).repo, Some(inner));
}

#[test]
fn pager_section_overrides_command_section_for_pager_chrome() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("user.toml");
    std::fs::write(
        &user,
        "[diff]\nwrap_lines = false\n[pager]\nwrap_lines = true\n",
    )
    .unwrap();
    let input = ReviewInput::VcsDiff {
        range: None,
        staged: false,
        pathspecs: vec![],
        options: CommonOptions {
            pager: Some(true),
            ..Default::default()
        },
    };
    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: None,
    })
    .resolve(&input)
    .unwrap();
    assert!(resolved.wrap_lines);
}

#[test]
fn vcs_config_is_typed_and_rejects_unknown_providers() {
    let temp = tempfile::tempdir().unwrap();
    let valid = temp.path().join("valid.toml");
    std::fs::write(&valid, "vcs = \"jj\"\n").unwrap();
    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(valid),
        repo: None,
    })
    .resolve(&patch_input(CommonOptions::default()))
    .unwrap();
    assert_eq!(resolved.vcs, Some(VcsId::Jj));

    let invalid = temp.path().join("invalid.toml");
    std::fs::write(&invalid, "vcs = \"mercurial\"\n").unwrap();
    let error = ConfigResolver::new(ConfigPaths {
        user: Some(invalid),
        repo: None,
    })
    .resolve(&patch_input(CommonOptions::default()))
    .unwrap_err();
    assert!(error.to_string().contains("unknown variant `mercurial`"));
}

#[test]
fn transparent_background_accepts_hunks_camel_case_compatibility_key() {
    let temp = tempfile::tempdir().unwrap();
    let user = temp.path().join("user.toml");
    std::fs::write(
        &user,
        "transparent_background = false\ntransparentBackground = true\n",
    )
    .unwrap();

    let resolved = ConfigResolver::new(ConfigPaths {
        user: Some(user),
        repo: None,
    })
    .resolve(&patch_input(CommonOptions::default()))
    .unwrap();

    assert!(resolved.transparent_background);
}
