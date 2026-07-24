use ramo::tmux::TmuxPane;
use ramo::ui::dialogs::{
    AGENT_SKILL_PROMPT, DialogOverlay, ThemeSelection, centered_rect, help_text,
};
use ramo::ui::themes::ThemeRegistry;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn centered_dialog_bounds_saturate_on_tiny_terminals() {
    assert_eq!(
        centered_rect(74, 30, Rect::new(0, 0, 20, 5)),
        Rect::new(0, 0, 20, 5)
    );
    assert_eq!(
        centered_rect(10, 4, Rect::new(2, 3, 20, 10)),
        Rect::new(7, 6, 10, 4)
    );
}

#[test]
fn help_lists_real_direct_bindings_and_contains_no_menu_instructions() {
    let help = help_text(true);
    for binding in [
        "Space / f",
        "d / u / ^D / ^U",
        "[ / ]",
        ", / .",
        "{ / }",
        "1 / 2 / 0",
        "s / t",
        "h / l",
        "n / w / m",
        "/",
        "c",
        "Tab",
        "r / q",
        "Enter save",
        "Shift+Enter newline",
        "Ctrl-S save",
        "Ctrl-T send",
    ] {
        assert!(help.contains(binding), "missing {binding}:\n{help}");
    }
    assert!(!help.contains("F10"));
    assert!(!help.contains("menu"));
    assert!(!help.contains(" / M"));
}

#[test]
fn theme_selection_previews_but_cancel_restores_the_original() {
    let ids = ThemeRegistry::default().selector_items();
    let mut selection = ThemeSelection::new(ids, "github-dark-default");
    selection.move_by(1);
    let preview = selection.preview_id().to_owned();
    assert_ne!(preview, "github-dark-default");
    assert_eq!(selection.cancel_id(), "github-dark-default");
    assert_eq!(selection.confirm_id(), preview);
}

#[test]
fn overlays_render_centered_and_remain_usable_at_small_sizes() {
    let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);
    let mut terminal = Terminal::new(TestBackend::new(32, 9)).unwrap();
    terminal
        .draw(|frame| frame.render_widget(DialogOverlay::help(&theme, true), frame.area()))
        .unwrap();
    let frame = buffer_text(&terminal);
    assert!(frame.contains("Controls help"), "{frame}");
    assert!(frame.contains("Navigation"), "{frame}");

    terminal
        .draw(|frame| {
            frame.render_widget(
                DialogOverlay::theme(&theme, &["one", "two"], 1),
                frame.area(),
            );
        })
        .unwrap();
    let frame = buffer_text(&terminal);
    assert!(frame.contains("Theme"), "{frame}");
    assert!(frame.contains("two"), "{frame}");

    terminal
        .draw(|frame| {
            frame.render_widget(DialogOverlay::agent_skill(&theme), frame.area());
        })
        .unwrap();
    let frame = buffer_text(&terminal);
    assert!(frame.contains("Agent skill"), "{frame}");
    assert!(frame.contains("ramo skill path"), "{frame}");
    assert!(AGENT_SKILL_PROMPT.contains("ramo skill path"));

    let panes = vec![
        TmuxPane {
            id: "%2".into(),
            label: "%2  work:1.2  agent  [claude]".into(),
            current_command: "claude".into(),
        },
        TmuxPane {
            id: "%3".into(),
            label: "%3  work:1.3  shell  [zsh]".into(),
            current_command: "zsh".into(),
        },
    ];
    terminal
        .draw(|frame| {
            frame.render_widget(DialogOverlay::tmux(&theme, &panes, 1), frame.area());
        })
        .unwrap();
    let frame = buffer_text(&terminal);
    assert!(frame.contains("Send to tmux"), "{frame}");
    assert!(frame.contains("[claude]"), "{frame}");
    assert!(frame.contains("[zsh]"), "{frame}");
    assert!(frame.contains("Enter send"), "{frame}");

    terminal
        .draw(|frame| {
            frame.render_widget(DialogOverlay::publish(&theme, 123, 4), frame.area());
        })
        .unwrap();
    let frame = buffer_text(&terminal);
    assert!(frame.contains("Publish review?"), "{frame}");
    assert!(frame.contains("discard"), "{frame}");

    terminal
        .draw(|frame| {
            frame.render_widget(
                DialogOverlay::verdict(&theme, true, "Review submitted from Ramo."),
                frame.area(),
            );
        })
        .unwrap();
    let frame = buffer_text(&terminal);
    assert!(frame.contains("Comment only"), "{frame}");
    assert!(!frame.contains("Approve"), "{frame}");
}
