use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

use crate::core::input::LayoutMode;
use crate::remote_review::ReviewVerdict;
use crate::review::{ReviewAction, ReviewSide, ScrollUnit};
use crate::review_map::ReviewMapAction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
    Note,
    Ask,
    Theme,
    Help,
    AgentSkill,
    SavePrompt,
    PublishPrompt,
    VerdictPrompt,
    OverallComment,
    Message,
    ReviewMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppAction {
    Review(ReviewAction),
    ReviewMap(ReviewMapAction),
    ToggleReviewMap,
    JumpAskAnswer,
    FocusReviewMapFilter,
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
    SendNote { reset_target: bool },
    Suspend,
    OpenAgentSkill,
    CopyAgentSkill,
    DisableSavePrompt,
    Discard,
    ConfirmPublish,
    KeepReviewing,
    DiscardRemoteReview,
    ChooseVerdict(ReviewVerdict),
    EditOverallComment,
    SaveOverallComment,
    DismissMessage,
}

pub fn map_key_event(event: KeyEvent, mode: InputMode, pager_mode: bool) -> Option<AppAction> {
    let action = match mode {
        InputMode::Normal => map_normal(event),
        InputMode::ReviewMap => map_review_map(event),
        InputMode::Filter | InputMode::Note | InputMode::Ask => map_text(event, mode),
        InputMode::Theme => map_theme(event),
        InputMode::Help => match event.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => Some(AppAction::Cancel),
            _ => None,
        },
        InputMode::AgentSkill => match event.code {
            KeyCode::Char('y') | KeyCode::Enter => Some(AppAction::CopyAgentSkill),
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('A') => Some(AppAction::Cancel),
            _ => None,
        },
        InputMode::SavePrompt => match event.code {
            KeyCode::Enter | KeyCode::Char('s') => Some(AppAction::Confirm),
            KeyCode::Char('q') => Some(AppAction::Discard),
            KeyCode::Char('n') => Some(AppAction::DisableSavePrompt),
            KeyCode::Esc => Some(AppAction::Cancel),
            _ => None,
        },
        InputMode::PublishPrompt => match event.code {
            KeyCode::Char('y') => Some(AppAction::ConfirmPublish),
            KeyCode::Char('n') | KeyCode::Esc => Some(AppAction::KeepReviewing),
            KeyCode::Char('d') => Some(AppAction::DiscardRemoteReview),
            _ => None,
        },
        InputMode::VerdictPrompt => match event.code {
            KeyCode::Char('c') => Some(AppAction::ChooseVerdict(ReviewVerdict::Comment)),
            KeyCode::Char('a') => Some(AppAction::ChooseVerdict(ReviewVerdict::Approve)),
            KeyCode::Char('r') => Some(AppAction::ChooseVerdict(ReviewVerdict::RequestChanges)),
            KeyCode::Char('o') => Some(AppAction::EditOverallComment),
            KeyCode::Esc => Some(AppAction::KeepReviewing),
            _ => None,
        },
        InputMode::OverallComment => map_overall_comment(event),
        InputMode::Message => match event.code {
            KeyCode::Enter | KeyCode::Esc => Some(AppAction::DismissMessage),
            _ => None,
        },
    };
    if pager_mode && !action.as_ref().is_some_and(pager_action) {
        None
    } else {
        action
    }
}

fn map_overall_comment(event: KeyEvent) -> Option<AppAction> {
    if event.code == KeyCode::Char('s') && event.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(AppAction::SaveOverallComment);
    }
    if event.code == KeyCode::Enter {
        return Some(if event.modifiers.contains(KeyModifiers::SHIFT) {
            AppAction::Insert('\n')
        } else {
            AppAction::SaveOverallComment
        });
    }
    match event.code {
        KeyCode::Esc => Some(AppAction::Cancel),
        KeyCode::Backspace => Some(AppAction::Backspace),
        KeyCode::Char(character) if !event.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(AppAction::Insert(character))
        }
        _ => None,
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
    if event.code == KeyCode::Char('z') && event.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(AppAction::Suspend);
    }
    if matches!(event.code, KeyCode::Char('t' | 'T'))
        && event.modifiers.contains(KeyModifiers::CONTROL)
    {
        return Some(AppAction::SendSelection {
            reset_target: event.modifiers.contains(KeyModifiers::SHIFT),
        });
    }
    if event.modifiers.contains(KeyModifiers::CONTROL) {
        let action = match event.code {
            KeyCode::Char('d') => Some(ReviewAction::Scroll {
                delta: 1,
                unit: ScrollUnit::HalfPage,
            }),
            KeyCode::Char('u') => Some(ReviewAction::Scroll {
                delta: -1,
                unit: ScrollUnit::HalfPage,
            }),
            _ => None,
        };
        if let Some(action) = action {
            return Some(AppAction::Review(action));
        }
    }
    if event
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    let review = |action| Some(AppAction::Review(action));
    match event.code {
        KeyCode::Down | KeyCode::Char('j') => review(ReviewAction::MoveCursor(1)),
        KeyCode::Up | KeyCode::Char('k') => review(ReviewAction::MoveCursor(-1)),
        KeyCode::Char('h') => review(ReviewAction::FocusSide(ReviewSide::Left)),
        KeyCode::Char('l') => review(ReviewAction::FocusSide(ReviewSide::Right)),
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
        KeyCode::Char('T') => review(ReviewAction::ToggleTestFiles),
        KeyCode::Char('i') => review(ReviewAction::ToggleAgentNotes),
        KeyCode::Char('a') => review(ReviewAction::StartAsk),
        KeyCode::Char('o') => Some(AppAction::JumpAskAnswer),
        KeyCode::Char('A') => Some(AppAction::OpenAgentSkill),
        KeyCode::Char('z') => Some(AppAction::ToggleContext),
        KeyCode::Char('v') => review(ReviewAction::ToggleFileViewed),
        KeyCode::Char('V') => Some(AppAction::BeginSelection),
        KeyCode::Char('y') => Some(AppAction::YankSelection),
        KeyCode::Char('n') => review(ReviewAction::ToggleLineNumbers),
        KeyCode::Char('w') => review(ReviewAction::ToggleWrap),
        KeyCode::Char('m') => review(ReviewAction::ToggleHunkHeaders),
        KeyCode::Char('M') => Some(AppAction::ToggleReviewMap),
        KeyCode::Char('e') => review(ReviewAction::EditSelectedFile),
        KeyCode::Char('r') => review(ReviewAction::Reload),
        KeyCode::Char('/') => review(ReviewAction::FocusFilter),
        KeyCode::Char('c') => review(ReviewAction::StartNote),
        KeyCode::Enter => review(ReviewAction::ExpandSelectedFile),
        KeyCode::Tab => Some(AppAction::ToggleFocus),
        KeyCode::Char('?') => review(ReviewAction::OpenHelp),
        KeyCode::Char('q') => review(ReviewAction::Quit),
        KeyCode::Esc => Some(AppAction::Cancel),
        _ => None,
    }
}

fn map_review_map(event: KeyEvent) -> Option<AppAction> {
    if event
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return None;
    }
    let map = |action| Some(AppAction::ReviewMap(action));
    match event.code {
        KeyCode::Down | KeyCode::Char('j') => map(ReviewMapAction::Move(1)),
        KeyCode::Up | KeyCode::Char('k') => map(ReviewMapAction::Move(-1)),
        KeyCode::Left | KeyCode::Char('h') => map(ReviewMapAction::Collapse),
        KeyCode::Right | KeyCode::Char('l') => map(ReviewMapAction::Expand),
        KeyCode::Enter => map(ReviewMapAction::OpenSelected),
        KeyCode::Char('/') => Some(AppAction::FocusReviewMapFilter),
        KeyCode::Char('r') => map(ReviewMapAction::Retry),
        KeyCode::Char('M') => Some(AppAction::ToggleReviewMap),
        KeyCode::Char('?') => map(ReviewMapAction::OpenHelp),
        KeyCode::Esc => map(ReviewMapAction::DismissFailure),
        _ => None,
    }
}

fn map_text(event: KeyEvent, mode: InputMode) -> Option<AppAction> {
    if mode == InputMode::Note
        && matches!(event.code, KeyCode::Char('t' | 'T'))
        && event.modifiers.contains(KeyModifiers::CONTROL)
    {
        // A question is not a note, so Ask deliberately has no tmux send.
        return Some(AppAction::SendNote {
            reset_target: event.modifiers.contains(KeyModifiers::SHIFT),
        });
    }
    if matches!(mode, InputMode::Note | InputMode::Ask) {
        if event.code == KeyCode::Char('s') && event.modifiers.contains(KeyModifiers::CONTROL) {
            return Some(AppAction::Confirm);
        }
        if event.code == KeyCode::Enter {
            return Some(if event.modifiers.contains(KeyModifiers::SHIFT) {
                AppAction::Insert('\n')
            } else {
                AppAction::Confirm
            });
        }
    }
    match event.code {
        KeyCode::Tab if mode == InputMode::Filter => Some(AppAction::ToggleFocus),
        KeyCode::Esc => Some(AppAction::Cancel),
        KeyCode::Backspace => Some(AppAction::Backspace),
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
                | ReviewAction::MoveCursor(_)
                | ReviewAction::FocusSide(_)
                | ReviewAction::JumpTop
                | ReviewAction::JumpBottom
                | ReviewAction::MoveHunk(_)
                | ReviewAction::MoveFile(_)
                | ReviewAction::ToggleWrap
                | ReviewAction::ToggleSidebar
                | ReviewAction::ToggleTestFiles
                | ReviewAction::ToggleFileViewed
                | ReviewAction::ExpandSelectedFile
                | ReviewAction::ExpandCompactedFile(_)
                | ReviewAction::StartNote
                | ReviewAction::Quit
        ) | AppAction::Insert(_)
            | AppAction::Backspace
            | AppAction::Cancel
            | AppAction::Confirm
            | AppAction::BeginSelection
            | AppAction::YankSelection
            | AppAction::Suspend
    )
}
