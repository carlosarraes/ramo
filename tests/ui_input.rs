use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ramo::app::App;
use ramo::config::ResolvedConfig;
use ramo::core::input::LayoutMode;
use ramo::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, SourceSpec,
};
use ramo::review::{ReviewAction, ReviewSide, ScrollUnit, Viewport};
use ramo::ui::input::{AppAction, InputMode, map_key_event};
use std::path::PathBuf;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn shifted(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}

fn controlled(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

#[test]
fn direct_hunk_keymap_has_no_menu_binding() {
    let cases = [
        (
            key(KeyCode::Down),
            AppAction::Review(ReviewAction::MoveCursor(1)),
        ),
        (
            key(KeyCode::Char('j')),
            AppAction::Review(ReviewAction::MoveCursor(1)),
        ),
        (
            key(KeyCode::Up),
            AppAction::Review(ReviewAction::MoveCursor(-1)),
        ),
        (
            key(KeyCode::Left),
            AppAction::Review(ReviewAction::ScrollHorizontal(-1)),
        ),
        (
            shifted(KeyCode::Right),
            AppAction::Review(ReviewAction::ScrollHorizontal(8)),
        ),
        (
            key(KeyCode::Char(' ')),
            AppAction::Review(ReviewAction::Scroll {
                delta: 1,
                unit: ScrollUnit::Page,
            }),
        ),
        (
            key(KeyCode::Char('b')),
            AppAction::Review(ReviewAction::Scroll {
                delta: -1,
                unit: ScrollUnit::Page,
            }),
        ),
        (
            key(KeyCode::Char('d')),
            AppAction::Review(ReviewAction::Scroll {
                delta: 1,
                unit: ScrollUnit::HalfPage,
            }),
        ),
        (
            key(KeyCode::Char('u')),
            AppAction::Review(ReviewAction::Scroll {
                delta: -1,
                unit: ScrollUnit::HalfPage,
            }),
        ),
        (
            key(KeyCode::Char('g')),
            AppAction::Review(ReviewAction::JumpTop),
        ),
        (
            key(KeyCode::Char('G')),
            AppAction::Review(ReviewAction::JumpBottom),
        ),
        (
            key(KeyCode::Char('[')),
            AppAction::Review(ReviewAction::MoveHunk(-1)),
        ),
        (
            key(KeyCode::Char(']')),
            AppAction::Review(ReviewAction::MoveHunk(1)),
        ),
        (
            key(KeyCode::Char(',')),
            AppAction::Review(ReviewAction::MoveFile(-1)),
        ),
        (
            key(KeyCode::Char('.')),
            AppAction::Review(ReviewAction::MoveFile(1)),
        ),
        (
            key(KeyCode::Char('{')),
            AppAction::Review(ReviewAction::MoveAnnotatedHunk(-1)),
        ),
        (
            key(KeyCode::Char('}')),
            AppAction::Review(ReviewAction::MoveAnnotatedHunk(1)),
        ),
        (
            key(KeyCode::Char('1')),
            AppAction::Review(ReviewAction::SetLayout(LayoutMode::Split)),
        ),
        (
            key(KeyCode::Char('2')),
            AppAction::Review(ReviewAction::SetLayout(LayoutMode::Stack)),
        ),
        (
            key(KeyCode::Char('0')),
            AppAction::Review(ReviewAction::SetLayout(LayoutMode::Auto)),
        ),
        (
            key(KeyCode::Char('s')),
            AppAction::Review(ReviewAction::ToggleSidebar),
        ),
        (
            key(KeyCode::Char('t')),
            AppAction::Review(ReviewAction::OpenThemeSelector),
        ),
        (key(KeyCode::Char('A')), AppAction::OpenAgentSkill),
        (
            key(KeyCode::Char('/')),
            AppAction::Review(ReviewAction::FocusFilter),
        ),
        (
            key(KeyCode::Char('?')),
            AppAction::Review(ReviewAction::OpenHelp),
        ),
        (
            key(KeyCode::Char('q')),
            AppAction::Review(ReviewAction::Quit),
        ),
    ];
    for (event, expected) in cases {
        assert_eq!(
            map_key_event(event, InputMode::Normal, false),
            Some(expected)
        );
    }
    assert_eq!(
        map_key_event(key(KeyCode::F(10)), InputMode::Normal, false),
        None
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('M')), InputMode::Normal, false),
        None
    );
}

#[test]
fn agent_skill_dialog_owns_copy_and_close_keys() {
    assert_eq!(
        map_key_event(key(KeyCode::Char('y')), InputMode::AgentSkill, false),
        Some(AppAction::CopyAgentSkill)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Enter), InputMode::AgentSkill, false),
        Some(AppAction::CopyAgentSkill)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Esc), InputMode::AgentSkill, false),
        Some(AppAction::Cancel)
    );
}

#[test]
fn remaining_direct_bindings_and_modifier_precedence_are_exact() {
    let review = |action| Some(AppAction::Review(action));
    for (event, expected) in [
        (
            key(KeyCode::Right),
            review(ReviewAction::ScrollHorizontal(1)),
        ),
        (
            shifted(KeyCode::Left),
            review(ReviewAction::ScrollHorizontal(-8)),
        ),
        (
            key(KeyCode::PageDown),
            review(ReviewAction::Scroll {
                delta: 1,
                unit: ScrollUnit::Page,
            }),
        ),
        (
            key(KeyCode::PageUp),
            review(ReviewAction::Scroll {
                delta: -1,
                unit: ScrollUnit::Page,
            }),
        ),
        (
            shifted(KeyCode::Char(' ')),
            review(ReviewAction::Scroll {
                delta: -1,
                unit: ScrollUnit::Page,
            }),
        ),
        (key(KeyCode::Home), review(ReviewAction::JumpTop)),
        (key(KeyCode::End), review(ReviewAction::JumpBottom)),
        (
            key(KeyCode::Char('a')),
            review(ReviewAction::ToggleAgentNotes),
        ),
        (
            key(KeyCode::Char('h')),
            review(ReviewAction::FocusSide(ReviewSide::Left)),
        ),
        (
            key(KeyCode::Char('l')),
            review(ReviewAction::FocusSide(ReviewSide::Right)),
        ),
        (
            key(KeyCode::Char('n')),
            review(ReviewAction::ToggleLineNumbers),
        ),
        (key(KeyCode::Char('w')), review(ReviewAction::ToggleWrap)),
        (
            key(KeyCode::Char('m')),
            review(ReviewAction::ToggleHunkHeaders),
        ),
        (
            key(KeyCode::Char('e')),
            review(ReviewAction::EditSelectedFile),
        ),
        (key(KeyCode::Char('r')), review(ReviewAction::Reload)),
        (key(KeyCode::Char('c')), review(ReviewAction::StartNote)),
        (key(KeyCode::Tab), Some(AppAction::ToggleFocus)),
        (key(KeyCode::Char('z')), Some(AppAction::ToggleContext)),
        (key(KeyCode::Char('V')), Some(AppAction::BeginSelection)),
        (key(KeyCode::Char('y')), Some(AppAction::YankSelection)),
    ] {
        assert_eq!(map_key_event(event, InputMode::Normal, false), expected);
    }
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('d')), InputMode::Normal, false),
        review(ReviewAction::Scroll {
            delta: 1,
            unit: ScrollUnit::HalfPage,
        })
    );
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('u')), InputMode::Normal, false),
        review(ReviewAction::Scroll {
            delta: -1,
            unit: ScrollUnit::HalfPage,
        })
    );
    assert_eq!(
        map_key_event(
            KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL),
            InputMode::Normal,
            false,
        ),
        Some(AppAction::Suspend)
    );
    assert_eq!(
        map_key_event(
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
            InputMode::Normal,
            false,
        ),
        Some(AppAction::SendSelection {
            reset_target: false,
        })
    );
    assert_eq!(
        map_key_event(
            KeyEvent::new(
                KeyCode::Char('T'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            InputMode::Normal,
            false,
        ),
        Some(AppAction::SendSelection { reset_target: true })
    );
}

#[test]
fn note_mode_saves_sends_and_only_shift_enter_inserts_a_newline() {
    assert_eq!(
        map_key_event(key(KeyCode::Enter), InputMode::Note, false),
        Some(AppAction::Confirm)
    );
    assert_eq!(
        map_key_event(shifted(KeyCode::Enter), InputMode::Note, false),
        Some(AppAction::Insert('\n'))
    );
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('s')), InputMode::Note, false),
        Some(AppAction::Confirm)
    );
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('t')), InputMode::Note, false),
        Some(AppAction::SendNote {
            reset_target: false,
        })
    );
    assert_eq!(
        map_key_event(
            KeyEvent::new(
                KeyCode::Char('T'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
            InputMode::Note,
            false,
        ),
        Some(AppAction::SendNote { reset_target: true })
    );
}

#[test]
fn focused_text_and_pager_precedence_suppress_global_actions() {
    assert_eq!(
        map_key_event(key(KeyCode::Char('q')), InputMode::Filter, false),
        Some(AppAction::Insert('q'))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('t')), InputMode::Note, false),
        Some(AppAction::Insert('t'))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('t')), InputMode::Normal, true),
        None
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('w')), InputMode::Normal, true),
        Some(AppAction::Review(ReviewAction::ToggleWrap))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('j')), InputMode::Normal, true),
        Some(AppAction::Review(ReviewAction::MoveCursor(1)))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('h')), InputMode::Normal, true),
        Some(AppAction::Review(ReviewAction::FocusSide(ReviewSide::Left)))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char(']')), InputMode::Normal, true),
        Some(AppAction::Review(ReviewAction::MoveHunk(1)))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('q')), InputMode::Normal, true),
        Some(AppAction::Review(ReviewAction::Quit))
    );
}

#[test]
fn dialog_modes_exclusively_own_their_documented_keys() {
    assert_eq!(
        map_key_event(key(KeyCode::Down), InputMode::Theme, false),
        Some(AppAction::MoveChoice(1))
    );
    assert_eq!(
        map_key_event(key(KeyCode::BackTab), InputMode::Theme, false),
        Some(AppAction::MoveChoice(-1))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Enter), InputMode::Theme, false),
        Some(AppAction::Confirm)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('q')), InputMode::Theme, false),
        None
    );
    for code in [KeyCode::Esc, KeyCode::Char('?'), KeyCode::Char('q')] {
        assert_eq!(
            map_key_event(key(code), InputMode::Help, false),
            Some(AppAction::Cancel)
        );
    }
    assert_eq!(
        map_key_event(key(KeyCode::Char('s')), InputMode::Help, false),
        None
    );
    assert_eq!(
        map_key_event(key(KeyCode::Enter), InputMode::SavePrompt, false),
        Some(AppAction::Confirm)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('s')), InputMode::SavePrompt, false),
        Some(AppAction::Confirm)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('q')), InputMode::SavePrompt, false),
        Some(AppAction::Discard)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('n')), InputMode::SavePrompt, false),
        Some(AppAction::DisableSavePrompt)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Esc), InputMode::SavePrompt, false),
        Some(AppAction::Cancel)
    );
    assert_eq!(
        map_key_event(
            KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
            InputMode::Note,
            false,
        ),
        Some(AppAction::Confirm)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Tab), InputMode::Filter, false),
        Some(AppAction::ToggleFocus)
    );
}

fn review_file() -> DiffFile {
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
            header: "@@ -1,30 +1,30 @@".into(),
            lines: (1..=30)
                .map(|line| DiffLine {
                    kind: LineType::Context,
                    content: format!("line {line}"),
                    old_lineno: Some(line),
                    new_lineno: Some(line),
                    moved: None,
                })
                .collect(),
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

#[test]
fn app_keys_mutate_the_rendering_controller_and_dialog_modes_own_closing_keys() {
    let mut app = App::new_with_config(vec![review_file()], &ResolvedConfig::default(), false);
    let view = Viewport {
        width: 180,
        height: 8,
    };
    app.handle_ui_key(key(KeyCode::Char('j')), view);
    assert_eq!(
        app.review_controller
            .snapshot(view)
            .selected_position
            .as_ref()
            .and_then(|position| position.new_line),
        Some(2)
    );
    app.handle_ui_key(key(KeyCode::Char('h')), view);
    assert_eq!(
        app.review_controller.snapshot(view).focused_side,
        ReviewSide::Left
    );
    app.handle_ui_key(key(KeyCode::Char('l')), view);
    assert_eq!(
        app.review_controller.snapshot(view).focused_side,
        ReviewSide::Right
    );
    assert!(app.review_controller.snapshot(view).line_numbers);
    app.handle_ui_key(key(KeyCode::Char('n')), view);
    assert!(!app.review_controller.snapshot(view).line_numbers);

    app.handle_ui_key(key(KeyCode::Char('?')), view);
    assert_eq!(app.input_mode(), InputMode::Help);
    app.handle_ui_key(key(KeyCode::Char('q')), view);
    assert_eq!(app.input_mode(), InputMode::Normal);
    assert!(!app.should_quit);

    app.handle_ui_key(key(KeyCode::Char('/')), view);
    app.handle_ui_key(key(KeyCode::Char('q')), view);
    assert_eq!(app.input_mode(), InputMode::Filter);
    assert_eq!(app.review_controller.snapshot(view).filter, "q");
    app.handle_ui_key(key(KeyCode::Esc), view);
    assert_eq!(app.review_controller.snapshot(view).filter, "");
    assert_eq!(app.input_mode(), InputMode::Normal);
}

#[test]
fn pull_request_dialog_modes_own_their_documented_keys() {
    assert_eq!(
        map_key_event(key(KeyCode::Char('y')), InputMode::PublishPrompt, false),
        Some(AppAction::ConfirmPublish)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('d')), InputMode::PublishPrompt, false),
        Some(AppAction::DiscardRemoteReview)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('o')), InputMode::VerdictPrompt, false),
        Some(AppAction::EditOverallComment)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Enter), InputMode::Message, false),
        Some(AppAction::DismissMessage)
    );
}
