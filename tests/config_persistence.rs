use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ramo::app::App;
use ramo::config::{
    ResolvedConfig, ViewPreferenceChanges, ViewPreferences, save_view_preferences,
};
use ramo::core::input::LayoutMode;
use ramo::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, SourceSpec,
};
use ramo::review::Viewport;
use ramo::ui::input::InputMode;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn file() -> DiffFile {
    DiffFile {
        id: "file:src/lib.rs".into(),
        path: "src/lib.rs".into(),
        previous_path: None,
        summary: None,
        agent: None,
        patch: String::new(),
        hunks: vec![Hunk {
            old_start: 1,
            new_start: 1,
            header: "@@ -1 +1 @@".into(),
            lines: vec![DiffLine {
                kind: LineType::Context,
                content: "same".into(),
                old_lineno: Some(1),
                new_lineno: Some(1),
                moved: None,
            }],
        }],
        change_kind: FileChangeKind::Modified,
        is_binary: false,
        is_untracked: false,
        is_too_large: false,
        stats_truncated: false,
        language: Some("rs".into()),
        stats: FileStats {
            additions: 0,
            deletions: 0,
        },
        old_source: SourceSpec::File(PathBuf::from("old")),
        new_source: SourceSpec::File(PathBuf::from("new")),
    }
}

fn preferences() -> ViewPreferences {
    ViewPreferences {
        mode: LayoutMode::Auto,
        theme: "github-dark-default".into(),
        show_sidebar: true,
        line_numbers: true,
        wrap_lines: false,
        hunk_headers: true,
        agent_notes: false,
        transparent_background: false,
        prompt_save_view_preferences: true,
    }
}

#[test]
fn targeted_save_changes_only_owned_global_keys_and_preserves_toml_text() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let original = "# keep this heading\nmode = \"auto\" # mode note\nwatch = true\n\n[diff]\nwrap_lines = false # command stays\n\n[custom_theme]\nbase = \"github-dark-default\"\naccent = \"#112233\"\n";
    std::fs::write(&path, original).unwrap();
    let initial = preferences();
    let current = ViewPreferences {
        mode: LayoutMode::Split,
        theme: "dracula".into(),
        show_sidebar: false,
        line_numbers: false,
        wrap_lines: true,
        hunk_headers: false,
        agent_notes: true,
        transparent_background: true,
        prompt_save_view_preferences: false,
    };

    save_view_preferences(&path, &ViewPreferenceChanges::between(&initial, &current)).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    for untouched in [
        "# keep this heading",
        "watch = true",
        "[diff]\nwrap_lines = false # command stays",
        "[custom_theme]\nbase = \"github-dark-default\"\naccent = \"#112233\"",
    ] {
        assert!(
            saved.contains(untouched),
            "missing {untouched:?} in:\n{saved}"
        );
    }
    for changed in [
        "mode = \"split\" # mode note",
        "theme = \"dracula\"",
        "show_sidebar = false",
        "line_numbers = false",
        "wrap_lines = true",
        "hunk_headers = false",
        "agent_notes = true",
        "transparent_background = true",
        "prompt_save_view_preferences = false",
    ] {
        assert!(saved.contains(changed), "missing {changed:?} in:\n{saved}");
    }
}

#[test]
fn save_creates_parents_and_io_errors_name_the_target() {
    let temp = tempfile::tempdir().unwrap();
    let nested = temp.path().join("new/ramo/config.toml");
    let mut current = preferences();
    current.wrap_lines = true;
    save_view_preferences(
        &nested,
        &ViewPreferenceChanges::between(&preferences(), &current),
    )
    .unwrap();
    assert!(nested.is_file());

    let blocked_parent = temp.path().join("not-a-directory");
    std::fs::write(&blocked_parent, "file").unwrap();
    let target = blocked_parent.join("config.toml");
    let error = save_view_preferences(
        &target,
        &ViewPreferenceChanges::between(&preferences(), &current),
    )
    .unwrap_err();
    assert!(error.to_string().contains(&target.display().to_string()));
}

fn app(config: &ResolvedConfig, path: &Path, pager: bool) -> App {
    App::new_with_preference_path(vec![file()], config, pager, Some(path.to_path_buf()))
}

#[test]
fn quit_prompt_save_discard_cancel_and_never_ask_are_distinct() {
    let temp = tempfile::tempdir().unwrap();
    let viewport = Viewport {
        width: 80,
        height: 12,
    };

    let save_path = temp.path().join("save.toml");
    let mut save = app(&ResolvedConfig::default(), &save_path, false);
    save.handle_ui_key(key(KeyCode::Char('1')), viewport);
    save.handle_ui_key(key(KeyCode::Char('q')), viewport);
    assert_eq!(save.input_mode(), InputMode::SavePrompt);
    assert!(!save.should_quit);
    save.handle_ui_key(key(KeyCode::Esc), viewport);
    assert_eq!(save.input_mode(), InputMode::Normal);
    save.handle_ui_key(key(KeyCode::Char('q')), viewport);
    save.handle_ui_key(key(KeyCode::Enter), viewport);
    assert!(save.should_quit);
    assert!(
        std::fs::read_to_string(&save_path)
            .unwrap()
            .contains("mode = \"split\"")
    );

    let discard_path = temp.path().join("discard.toml");
    let mut discard = app(&ResolvedConfig::default(), &discard_path, false);
    discard.handle_ui_key(key(KeyCode::Char('2')), viewport);
    discard.handle_ui_key(key(KeyCode::Char('q')), viewport);
    discard.handle_ui_key(key(KeyCode::Char('q')), viewport);
    assert!(discard.should_quit);
    assert!(!discard_path.exists());

    let never_path = temp.path().join("never.toml");
    let mut never = app(&ResolvedConfig::default(), &never_path, false);
    never.handle_ui_key(key(KeyCode::Char('1')), viewport);
    never.handle_ui_key(key(KeyCode::Char('q')), viewport);
    never.handle_ui_key(key(KeyCode::Char('n')), viewport);
    assert!(never.should_quit);
    assert!(
        std::fs::read_to_string(&never_path)
            .unwrap()
            .contains("prompt_save_view_preferences = false")
    );
}

#[test]
fn disabled_prompt_and_pager_mode_never_write_preferences() {
    let temp = tempfile::tempdir().unwrap();
    let viewport = Viewport {
        width: 80,
        height: 12,
    };
    let no_prompt_config = ResolvedConfig {
        prompt_save_view_preferences: false,
        ..ResolvedConfig::default()
    };
    let no_prompt_path = temp.path().join("disabled.toml");
    let mut no_prompt = app(&no_prompt_config, &no_prompt_path, false);
    no_prompt.handle_ui_key(key(KeyCode::Char('1')), viewport);
    no_prompt.handle_ui_key(key(KeyCode::Char('q')), viewport);
    assert!(no_prompt.should_quit);
    assert!(!no_prompt_path.exists());

    let pager_path = temp.path().join("pager.toml");
    let mut pager = app(&ResolvedConfig::default(), &pager_path, true);
    pager.handle_ui_key(key(KeyCode::Char('w')), viewport);
    pager.handle_ui_key(key(KeyCode::Char('q')), viewport);
    assert!(pager.should_quit);
    assert!(!pager_path.exists());
}

#[test]
fn save_failure_keeps_the_app_open_and_surfaces_the_path() {
    let temp = tempfile::tempdir().unwrap();
    let blocked = temp.path().join("blocked");
    std::fs::write(&blocked, "file").unwrap();
    let path = blocked.join("config.toml");
    let viewport = Viewport {
        width: 80,
        height: 12,
    };
    let mut app = app(&ResolvedConfig::default(), &path, false);
    app.handle_ui_key(key(KeyCode::Char('1')), viewport);
    app.handle_ui_key(key(KeyCode::Char('q')), viewport);
    app.handle_ui_key(key(KeyCode::Enter), viewport);
    assert!(!app.should_quit);
    assert_eq!(app.input_mode(), InputMode::SavePrompt);
    assert!(
        app.toast
            .as_deref()
            .unwrap()
            .contains(&path.display().to_string())
    );
}
