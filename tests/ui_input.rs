use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ramo::app::{App, AppScreen};
use ramo::config::ResolvedConfig;
use ramo::core::input::LayoutMode;
use ramo::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, SourceSpec,
};
use ramo::review::{ReviewAction, ReviewSide, ScrollUnit, Viewport};
use ramo::review_map::{ReviewMapAction, ReviewMapRow};
use ramo::ui::input::{AppAction, InputMode, PrScroll, map_key_event};
use ramo_core::review_map::{
    ClassifierConfig, ReviewMapIdentity, ReviewMapInput, ReviewMapInputFile, build_review_map,
};
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

fn alted(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT)
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
        Some(AppAction::ToggleReviewMap)
    );
}

#[test]
fn uppercase_m_toggles_map_without_stealing_lowercase_m() {
    assert_eq!(
        map_key_event(key(KeyCode::Char('M')), InputMode::Normal, false),
        Some(AppAction::ToggleReviewMap)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('m')), InputMode::Normal, false),
        Some(AppAction::Review(ReviewAction::ToggleHunkHeaders))
    );
}

#[test]
fn map_mode_owns_navigation_open_filter_and_retry() {
    for (event, expected) in [
        (
            key(KeyCode::Char('j')),
            AppAction::ReviewMap(ReviewMapAction::Move(1)),
        ),
        (
            key(KeyCode::Char('k')),
            AppAction::ReviewMap(ReviewMapAction::Move(-1)),
        ),
        (
            key(KeyCode::Char('h')),
            AppAction::ReviewMap(ReviewMapAction::Collapse),
        ),
        (
            key(KeyCode::Char('l')),
            AppAction::ReviewMap(ReviewMapAction::Expand),
        ),
        (
            key(KeyCode::Enter),
            AppAction::ReviewMap(ReviewMapAction::OpenSelected),
        ),
        (key(KeyCode::Char('/')), AppAction::FocusReviewMapFilter),
        (
            key(KeyCode::Char('r')),
            AppAction::ReviewMap(ReviewMapAction::Retry),
        ),
        (key(KeyCode::Char('M')), AppAction::ToggleReviewMap),
    ] {
        assert_eq!(
            map_key_event(event, InputMode::ReviewMap, false),
            Some(expected)
        );
    }
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
            key(KeyCode::Char('i')),
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
fn map_and_review_screens_share_the_existing_review_controller() {
    let mut app = App::new_with_config(vec![review_file()], &ResolvedConfig::default(), false);
    let exact = build_review_map(
        &ReviewMapInput {
            identity: ReviewMapIdentity {
                repository: "owner/repository".into(),
                pull_request: 7,
                base_sha: "base".into(),
                head_sha: "head".into(),
            },
            files: vec![ReviewMapInputFile {
                path: "src/lib.rs".into(),
                previous_path: None,
                status: "modified".into(),
                additions: 0,
                deletions: 0,
                patch: Some("@@ -1 +1 @@".into()),
                binary: false,
            }],
            codeowners: None,
        },
        &ClassifierConfig::default(),
    )
    .unwrap();
    app.attach_review_map(exact, None, true);
    let view = Viewport {
        width: 100,
        height: 20,
    };

    assert_eq!(app.screen(), AppScreen::ReviewMap);
    app.handle_ui_key(key(KeyCode::Char('j')), view);
    app.handle_ui_key(key(KeyCode::Enter), view);
    assert_eq!(app.screen(), AppScreen::Review);
    assert_eq!(
        app.review_controller
            .snapshot(view)
            .selected_file_id
            .as_deref(),
        Some("file:src/lib.rs")
    );

    app.handle_ui_key(key(KeyCode::Char('M')), view);
    assert_eq!(app.screen(), AppScreen::ReviewMap);
    assert_eq!(app.input_mode(), InputMode::ReviewMap);
    app.handle_ui_key(key(KeyCode::Char('/')), view);
    app.handle_ui_key(key(KeyCode::Char('l')), view);
    assert_eq!(app.input_mode(), InputMode::Filter);
    app.handle_ui_key(key(KeyCode::Esc), view);
    assert_eq!(app.input_mode(), InputMode::ReviewMap);
}

fn map_marks_file_reviewed(app: &App) -> bool {
    app.review_map_snapshot()
        .expect("review map is attached")
        .rows
        .iter()
        .any(|row| matches!(row, ReviewMapRow::File { reviewed: true, .. }))
}

#[test]
fn viewing_a_file_from_the_code_screen_syncs_the_review_map_check() {
    let mut app = App::new_with_config(vec![review_file()], &ResolvedConfig::default(), false);
    let exact = build_review_map(
        &ReviewMapInput {
            identity: ReviewMapIdentity {
                repository: "owner/repository".into(),
                pull_request: 7,
                base_sha: "base".into(),
                head_sha: "head".into(),
            },
            files: vec![ReviewMapInputFile {
                path: "src/lib.rs".into(),
                previous_path: None,
                status: "modified".into(),
                additions: 0,
                deletions: 0,
                patch: Some("@@ -1 +1 @@".into()),
                binary: false,
            }],
            codeowners: None,
        },
        &ClassifierConfig::default(),
    )
    .unwrap();
    app.attach_review_map(exact, None, false);
    let view = Viewport {
        width: 100,
        height: 20,
    };

    assert_eq!(app.screen(), AppScreen::Review);
    assert!(!map_marks_file_reviewed(&app));

    app.handle_ui_key(key(KeyCode::Char('v')), view);
    assert!(map_marks_file_reviewed(&app));

    app.handle_ui_key(key(KeyCode::Enter), view);
    assert!(!map_marks_file_reviewed(&app));
}

#[test]
fn test_compaction_keys_do_not_shadow_theme_or_tmux_bindings() {
    assert_eq!(
        map_key_event(key(KeyCode::Char('T')), InputMode::Normal, false),
        Some(AppAction::Review(ReviewAction::ToggleTestFiles))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('v')), InputMode::Normal, false),
        Some(AppAction::Review(ReviewAction::ToggleFileViewed))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('V')), InputMode::Normal, false),
        Some(AppAction::BeginSelection)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('v')), InputMode::Normal, true),
        Some(AppAction::Review(ReviewAction::ToggleFileViewed))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Enter), InputMode::Normal, false),
        Some(AppAction::Review(ReviewAction::ExpandSelectedFile))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('t')), InputMode::Normal, false),
        Some(AppAction::Review(ReviewAction::OpenThemeSelector))
    );
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('t')), InputMode::Normal, false),
        Some(AppAction::SendSelection {
            reset_target: false,
        })
    );
}

#[test]
fn ask_mode_owns_its_keys_and_never_sends_a_question_to_tmux() {
    assert_eq!(
        map_key_event(key(KeyCode::Char('a')), InputMode::Normal, false),
        Some(AppAction::Review(ReviewAction::StartAsk))
    );
    assert_eq!(
        map_key_event(key(KeyCode::Enter), InputMode::Ask, false),
        Some(AppAction::Confirm)
    );
    assert_eq!(
        map_key_event(shifted(KeyCode::Enter), InputMode::Ask, false),
        Some(AppAction::Insert('\n'))
    );
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('s')), InputMode::Ask, false),
        Some(AppAction::Confirm)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Esc), InputMode::Ask, false),
        Some(AppAction::Cancel)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Backspace), InputMode::Ask, false),
        Some(AppAction::Backspace)
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('x')), InputMode::Ask, false),
        Some(AppAction::Insert('x'))
    );
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('t')), InputMode::Ask, false),
        None,
        "a question must never be sent to tmux"
    );
}

#[test]
fn asking_is_unavailable_in_pager_mode() {
    assert_eq!(
        map_key_event(key(KeyCode::Char('a')), InputMode::Normal, true),
        None
    );
    assert_eq!(
        map_key_event(key(KeyCode::Char('o')), InputMode::Normal, true),
        None
    );
}

#[test]
fn o_jumps_to_a_ready_ai_answer() {
    assert_eq!(
        map_key_event(key(KeyCode::Char('o')), InputMode::Normal, false),
        Some(AppAction::JumpAskAnswer)
    );
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

#[test]
fn p_opens_the_pr_description_and_that_mode_owns_its_scroll_keys() {
    assert_eq!(
        map_key_event(key(KeyCode::Char('P')), InputMode::Normal, false),
        Some(AppAction::TogglePrDescription)
    );
    // A pull request is never a pager context, so the key is inert there.
    assert_eq!(
        map_key_event(key(KeyCode::Char('P')), InputMode::Normal, true),
        None
    );

    for (code, expected) in [
        (KeyCode::Char('j'), PrScroll::Line(1)),
        (KeyCode::Down, PrScroll::Line(1)),
        (KeyCode::Char('k'), PrScroll::Line(-1)),
        (KeyCode::Char('d'), PrScroll::HalfPage(1)),
        (KeyCode::Char('u'), PrScroll::HalfPage(-1)),
        (KeyCode::Char('b'), PrScroll::Page(-1)),
        (KeyCode::Char('g'), PrScroll::Top),
        (KeyCode::Char('G'), PrScroll::Bottom),
    ] {
        assert_eq!(
            map_key_event(key(code), InputMode::PrDescription, false),
            Some(AppAction::ScrollPrDescription(expected)),
            "{code:?}"
        );
    }
    assert_eq!(
        map_key_event(
            controlled(KeyCode::Char('d')),
            InputMode::PrDescription,
            false
        ),
        Some(AppAction::ScrollPrDescription(PrScroll::HalfPage(1)))
    );

    for code in [KeyCode::Char('P'), KeyCode::Char('q'), KeyCode::Esc] {
        assert_eq!(
            map_key_event(key(code), InputMode::PrDescription, false),
            Some(AppAction::TogglePrDescription),
            "{code:?}"
        );
    }
    // Typing keys must not leak into the review underneath.
    assert_eq!(
        map_key_event(key(KeyCode::Char('c')), InputMode::PrDescription, false),
        None
    );
}

#[test]
fn every_text_mode_accepts_the_readline_shortcuts() {
    use ramo::ui::input::TextEdit;

    for mode in [
        InputMode::Filter,
        InputMode::Note,
        InputMode::Ask,
        InputMode::OverallComment,
    ] {
        for (event, expected) in [
            (controlled(KeyCode::Char('a')), TextEdit::Home),
            (controlled(KeyCode::Char('e')), TextEdit::End),
            (controlled(KeyCode::Char('b')), TextEdit::Left),
            (controlled(KeyCode::Char('f')), TextEdit::Right),
            (controlled(KeyCode::Char('u')), TextEdit::KillToStart),
            (controlled(KeyCode::Char('k')), TextEdit::KillToEnd),
            (controlled(KeyCode::Char('w')), TextEdit::DeleteWordBack),
            (controlled(KeyCode::Char('d')), TextEdit::DeleteForward),
            (alted(KeyCode::Char('b')), TextEdit::WordLeft),
            (alted(KeyCode::Char('f')), TextEdit::WordRight),
            (key(KeyCode::Home), TextEdit::Home),
            (key(KeyCode::End), TextEdit::End),
            (key(KeyCode::Left), TextEdit::Left),
            (key(KeyCode::Delete), TextEdit::DeleteForward),
        ] {
            assert_eq!(
                map_key_event(event, mode, false),
                Some(AppAction::Edit(expected)),
                "{mode:?} {event:?}"
            );
        }
    }
}

#[test]
fn readline_never_shadows_the_existing_save_and_send_bindings() {
    // Ctrl-S saves in Note/Ask and the overall comment; Ctrl-T is the tmux send.
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('s')), InputMode::Note, false),
        Some(AppAction::Confirm)
    );
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('s')), InputMode::Ask, false),
        Some(AppAction::Confirm)
    );
    assert_eq!(
        map_key_event(
            controlled(KeyCode::Char('s')),
            InputMode::OverallComment,
            false
        ),
        Some(AppAction::SaveOverallComment)
    );
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('t')), InputMode::Note, false),
        Some(AppAction::SendNote {
            reset_target: false
        })
    );
}

#[test]
fn alt_chords_no_longer_type_a_literal_character() {
    // Alt-b used to fall through to Insert('b') because only CONTROL was excluded.
    for mode in [InputMode::Note, InputMode::Ask, InputMode::OverallComment] {
        assert_ne!(
            map_key_event(alted(KeyCode::Char('b')), mode, false),
            Some(AppAction::Insert('b')),
            "{mode:?}"
        );
    }
    // An unbound Alt chord is inert rather than inserting.
    assert_eq!(
        map_key_event(alted(KeyCode::Char('z')), InputMode::Note, false),
        None
    );
}

#[test]
fn editing_still_works_in_pager_mode() {
    // The pager allow-list drops anything not whitelisted, so edits must be listed there.
    assert_eq!(
        map_key_event(controlled(KeyCode::Char('a')), InputMode::Note, true),
        Some(AppAction::Edit(ramo::ui::input::TextEdit::Home))
    );
}

#[test]
fn readline_editing_mutates_the_filter_through_the_real_key_path() {
    let mut app = App::new_with_config(vec![review_file()], &ResolvedConfig::default(), false);
    let view = Viewport {
        width: 100,
        height: 24,
    };

    app.handle_ui_key(key(KeyCode::Char('/')), view);
    for character in "src lib".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), view);
    }
    assert_eq!(app.review_controller.snapshot(view).filter, "src lib");

    // Ctrl-W drops one whitespace-delimited word.
    app.handle_ui_key(controlled(KeyCode::Char('w')), view);
    assert_eq!(app.review_controller.snapshot(view).filter, "src ");

    // Ctrl-A then typing inserts at the START, proving the caret is real.
    app.handle_ui_key(controlled(KeyCode::Char('a')), view);
    app.handle_ui_key(key(KeyCode::Char('x')), view);
    assert_eq!(app.review_controller.snapshot(view).filter, "xsrc ");

    // Ctrl-E returns to the end; Ctrl-U then clears everything before it.
    app.handle_ui_key(controlled(KeyCode::Char('e')), view);
    app.handle_ui_key(controlled(KeyCode::Char('u')), view);
    assert_eq!(app.review_controller.snapshot(view).filter, "");
}

#[test]
fn readline_editing_drives_the_note_draft_and_its_caret() {
    let mut app = App::new_with_config(vec![review_file()], &ResolvedConfig::default(), false);
    let view = Viewport {
        width: 100,
        height: 24,
    };
    app.review_controller.snapshot(view);

    app.handle_ui_key(key(KeyCode::Char('c')), view);
    assert_eq!(app.input_mode(), InputMode::Note);
    for character in "needs a test".chars() {
        app.handle_ui_key(key(KeyCode::Char(character)), view);
    }

    let draft = |app: &App| {
        app.review_controller
            .human_note_draft()
            .map(|draft| (draft.body.clone(), draft.caret))
            .expect("draft open")
    };
    assert_eq!(draft(&app), ("needs a test".to_owned(), 12));

    app.handle_ui_key(controlled(KeyCode::Char('a')), view);
    assert_eq!(draft(&app).1, 0, "Ctrl-A moves the caret without editing");
    assert_eq!(draft(&app).0, "needs a test", "and leaves the text alone");

    app.handle_ui_key(controlled(KeyCode::Char('k')), view);
    assert_eq!(draft(&app), (String::new(), 0));
}
