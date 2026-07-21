use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

use crate::core::input::LayoutMode;
use crate::review::{ReviewAction, ScrollUnit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
    Note,
    Theme,
    Help,
    SavePrompt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppAction {
    Review(ReviewAction),
    Insert(char),
    Backspace,
    Cancel,
    Confirm,
    MoveChoice(i32),
    ToggleFocus,
    ToggleContext,
    BeginSelection,
    YankSelection,
    SendSelection { reset_target: bool },
    DisableSavePrompt,
    Discard,
}

pub fn map_key_event(event: KeyEvent, mode: InputMode, pager_mode: bool) -> Option<AppAction> {
    let action = match mode {
        InputMode::Normal => map_normal(event),
        InputMode::Filter | InputMode::Note => map_text(event, mode),
        InputMode::Theme => map_theme(event),
        InputMode::Help => match event.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => Some(AppAction::Cancel),
            _ => None,
        },
        InputMode::SavePrompt => match event.code {
            KeyCode::Enter | KeyCode::Char('s') => Some(AppAction::Confirm),
            KeyCode::Char('q') => Some(AppAction::Discard),
            KeyCode::Char('n') => Some(AppAction::DisableSavePrompt),
            KeyCode::Esc => Some(AppAction::Cancel),
            _ => None,
        },
    };
    if pager_mode && !action.as_ref().is_some_and(pager_action) {
        None
    } else {
        action
    }
}

pub fn map_mouse_event(event: MouseEvent) -> Option<AppAction> {
    let horizontal = event.modifiers.contains(KeyModifiers::SHIFT);
    let action = match event.kind {
        MouseEventKind::ScrollUp if horizontal => ReviewAction::ScrollHorizontal(-3),
        MouseEventKind::ScrollDown if horizontal => ReviewAction::ScrollHorizontal(3),
        MouseEventKind::ScrollUp => ReviewAction::Scroll {
            delta: -3,
            unit: ScrollUnit::Step,
        },
        MouseEventKind::ScrollDown => ReviewAction::Scroll {
            delta: 3,
            unit: ScrollUnit::Step,
        },
        MouseEventKind::ScrollLeft => ReviewAction::ScrollHorizontal(-3),
        MouseEventKind::ScrollRight => ReviewAction::ScrollHorizontal(3),
        _ => return None,
    };
    Some(AppAction::Review(action))
}

fn map_normal(event: KeyEvent) -> Option<AppAction> {
    if event.code == KeyCode::Char('t') && event.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(AppAction::SendSelection {
            reset_target: event.modifiers.contains(KeyModifiers::SHIFT),
        });
    }
    if event
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    let review = |action| Some(AppAction::Review(action));
    match event.code {
        KeyCode::Down | KeyCode::Char('j') => review(ReviewAction::Scroll {
            delta: 1,
            unit: ScrollUnit::Step,
        }),
        KeyCode::Up | KeyCode::Char('k') => review(ReviewAction::Scroll {
            delta: -1,
            unit: ScrollUnit::Step,
        }),
        KeyCode::Left => review(ReviewAction::ScrollHorizontal(
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                -8
            } else {
                -1
            },
        )),
        KeyCode::Right => review(ReviewAction::ScrollHorizontal(
            if event.modifiers.contains(KeyModifiers::SHIFT) {
                8
            } else {
                1
            },
        )),
        KeyCode::Char(' ') if event.modifiers.contains(KeyModifiers::SHIFT) => {
            review(ReviewAction::Scroll {
                delta: -1,
                unit: ScrollUnit::Page,
            })
        }
        KeyCode::Char(' ') | KeyCode::Char('f') | KeyCode::PageDown => {
            review(ReviewAction::Scroll {
                delta: 1,
                unit: ScrollUnit::Page,
            })
        }
        KeyCode::Char('b') | KeyCode::PageUp => review(ReviewAction::Scroll {
            delta: -1,
            unit: ScrollUnit::Page,
        }),
        KeyCode::Char('d') => review(ReviewAction::Scroll {
            delta: 1,
            unit: ScrollUnit::HalfPage,
        }),
        KeyCode::Char('u') => review(ReviewAction::Scroll {
            delta: -1,
            unit: ScrollUnit::HalfPage,
        }),
        KeyCode::Char('g') | KeyCode::Home => review(ReviewAction::JumpTop),
        KeyCode::Char('G') | KeyCode::End => review(ReviewAction::JumpBottom),
        KeyCode::Char('[') => review(ReviewAction::MoveHunk(-1)),
        KeyCode::Char(']') => review(ReviewAction::MoveHunk(1)),
        KeyCode::Char(',') => review(ReviewAction::MoveFile(-1)),
        KeyCode::Char('.') => review(ReviewAction::MoveFile(1)),
        KeyCode::Char('{') => review(ReviewAction::MoveAnnotatedHunk(-1)),
        KeyCode::Char('}') => review(ReviewAction::MoveAnnotatedHunk(1)),
        KeyCode::Char('1') => review(ReviewAction::SetLayout(LayoutMode::Split)),
        KeyCode::Char('2') => review(ReviewAction::SetLayout(LayoutMode::Stack)),
        KeyCode::Char('0') => review(ReviewAction::SetLayout(LayoutMode::Auto)),
        KeyCode::Char('s') => review(ReviewAction::ToggleSidebar),
        KeyCode::Char('t') => review(ReviewAction::OpenThemeSelector),
        KeyCode::Char('a') => review(ReviewAction::ToggleAgentNotes),
        KeyCode::Char('z') => Some(AppAction::ToggleContext),
        KeyCode::Char('V') => Some(AppAction::BeginSelection),
        KeyCode::Char('y') => Some(AppAction::YankSelection),
        KeyCode::Char('l') => review(ReviewAction::ToggleLineNumbers),
        KeyCode::Char('w') => review(ReviewAction::ToggleWrap),
        KeyCode::Char('m') => review(ReviewAction::ToggleHunkHeaders),
        KeyCode::Char('e') => review(ReviewAction::EditSelectedFile),
        KeyCode::Char('r') => review(ReviewAction::Reload),
        KeyCode::Char('/') => review(ReviewAction::FocusFilter),
        KeyCode::Char('c') => review(ReviewAction::StartNote),
        KeyCode::Tab => Some(AppAction::ToggleFocus),
        KeyCode::Char('?') => review(ReviewAction::OpenHelp),
        KeyCode::Char('q') => review(ReviewAction::Quit),
        KeyCode::Esc => Some(AppAction::Cancel),
        _ => None,
    }
}

fn map_text(event: KeyEvent, mode: InputMode) -> Option<AppAction> {
    match event.code {
        KeyCode::Tab if mode == InputMode::Filter => Some(AppAction::ToggleFocus),
        KeyCode::Esc => Some(AppAction::Cancel),
        KeyCode::Backspace => Some(AppAction::Backspace),
        KeyCode::Char('s')
            if mode == InputMode::Note && event.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(AppAction::Confirm)
        }
        KeyCode::Char(character) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppAction::Insert(character))
        }
        _ => None,
    }
}

fn map_theme(event: KeyEvent) -> Option<AppAction> {
    match event.code {
        KeyCode::Up | KeyCode::BackTab => Some(AppAction::MoveChoice(-1)),
        KeyCode::Down | KeyCode::Tab => Some(AppAction::MoveChoice(1)),
        KeyCode::Enter => Some(AppAction::Confirm),
        KeyCode::Esc => Some(AppAction::Cancel),
        _ => None,
    }
}

fn pager_action(action: &AppAction) -> bool {
    matches!(
        action,
        AppAction::Review(
            ReviewAction::Scroll { .. }
                | ReviewAction::ScrollHorizontal(_)
                | ReviewAction::JumpTop
                | ReviewAction::JumpBottom
                | ReviewAction::ToggleWrap
                | ReviewAction::ToggleSidebar
                | ReviewAction::Quit
        ) | AppAction::BeginSelection
            | AppAction::YankSelection
    )
}
