use std::path::PathBuf;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use pdiff::app::App;
use pdiff::config::ResolvedConfig;
use pdiff::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, SourceSpec,
};
use pdiff::review::{ContextSourceLoader, ReviewAction, ScrollUnit, SourceFailure, Viewport};
use pdiff::ui::input::{AppAction, map_mouse_event};

fn mouse(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers,
    }
}

fn file(path: &str, start: u32, lines: usize) -> DiffFile {
    DiffFile {
        id: format!("file:{path}"),
        path: path.into(),
        previous_path: None,
        summary: None,
        agent: None,
        patch: String::new(),
        hunks: vec![Hunk {
            old_start: start,
            new_start: start,
            header: format!("@@ -{start},{lines} +{start},{lines} @@"),
            lines: (0..lines)
                .map(|index| DiffLine {
                    kind: LineType::Context,
                    content: format!("line {index}"),
                    old_lineno: Some(start + index as u32),
                    new_lineno: Some(start + index as u32),
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
fn wheel_mapping_separates_vertical_and_horizontal_scroll() {
    assert_eq!(
        map_mouse_event(mouse(MouseEventKind::ScrollDown, 10, 4, KeyModifiers::NONE,)),
        Some(AppAction::Review(ReviewAction::Scroll {
            delta: 3,
            unit: ScrollUnit::Step,
        }))
    );
    assert_eq!(
        map_mouse_event(mouse(
            MouseEventKind::ScrollDown,
            10,
            4,
            KeyModifiers::SHIFT,
        )),
        Some(AppAction::Review(ReviewAction::ScrollHorizontal(3)))
    );
    assert_eq!(
        map_mouse_event(mouse(MouseEventKind::ScrollLeft, 10, 4, KeyModifiers::NONE,)),
        Some(AppAction::Review(ReviewAction::ScrollHorizontal(-3)))
    );
}

#[test]
fn sidebar_click_selects_the_file_and_divider_drag_is_clamped() {
    let mut app = App::new(vec![
        file("src/alpha.rs", 1, 30),
        file("src/beta.rs", 1, 30),
    ]);
    let viewport = Viewport {
        width: 220,
        height: 12,
    };
    app.review_controller.snapshot(viewport);

    app.handle_mouse(
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            5,
            2,
            KeyModifiers::NONE,
        ),
        viewport,
    );
    let selected = app.review_controller.snapshot(viewport).clone();
    assert_eq!(
        selected.selected_file_id.as_deref(),
        Some("file:src/beta.rs")
    );
    assert!(selected.scroll_top > 0);

    app.handle_mouse(
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            selected.sidebar_width,
            4,
            KeyModifiers::NONE,
        ),
        viewport,
    );
    app.handle_mouse(
        mouse(
            MouseEventKind::Drag(MouseButton::Left),
            80,
            4,
            KeyModifiers::NONE,
        ),
        viewport,
    );
    app.handle_mouse(
        mouse(
            MouseEventKind::Up(MouseButton::Left),
            80,
            4,
            KeyModifiers::NONE,
        ),
        viewport,
    );
    assert_eq!(app.review_controller.snapshot(viewport).sidebar_width, 80);

    for (from, to, expected) in [(80, 219, 179), (179, 2, 20)] {
        app.handle_mouse(
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                from,
                4,
                KeyModifiers::NONE,
            ),
            viewport,
        );
        app.handle_mouse(
            mouse(
                MouseEventKind::Drag(MouseButton::Left),
                to,
                4,
                KeyModifiers::NONE,
            ),
            viewport,
        );
        app.handle_mouse(
            mouse(
                MouseEventKind::Up(MouseButton::Left),
                to,
                4,
                KeyModifiers::NONE,
            ),
            viewport,
        );
        assert_eq!(
            app.review_controller.snapshot(viewport).sidebar_width,
            expected
        );
    }
}

struct SourceLoader;

impl ContextSourceLoader for SourceLoader {
    fn load(&mut self, _spec: &SourceSpec) -> Result<Option<String>, SourceFailure> {
        Ok(Some(
            (1..=20).map(|line| format!("source {line}\n")).collect(),
        ))
    }
}

#[test]
fn collapsed_rows_and_scrollbar_use_shared_geometry_hit_regions() {
    let viewport = Viewport {
        width: 80,
        height: 8,
    };
    let mut app = App::new_with_context_loader(
        vec![file("src/context.rs", 4, 12)],
        &ResolvedConfig::default(),
        false,
        Box::new(SourceLoader),
    );
    let before = app.review_controller.snapshot(viewport).total_height;
    app.handle_mouse(
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            4,
            0,
            KeyModifiers::NONE,
        ),
        viewport,
    );
    assert!(app.review_controller.snapshot(viewport).total_height > before);

    app.handle_mouse(
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            viewport.width - 1,
            1,
            KeyModifiers::NONE,
        ),
        viewport,
    );
    app.handle_mouse(
        mouse(
            MouseEventKind::Drag(MouseButton::Left),
            viewport.width - 1,
            viewport.height - 1,
            KeyModifiers::NONE,
        ),
        viewport,
    );
    app.handle_mouse(
        mouse(
            MouseEventKind::Up(MouseButton::Left),
            viewport.width - 1,
            viewport.height - 1,
            KeyModifiers::NONE,
        ),
        viewport,
    );
    assert!(app.review_controller.snapshot(viewport).scroll_top > 0);
}

#[test]
fn non_left_buttons_release_without_drag_and_outside_coordinates_do_nothing() {
    let viewport = Viewport {
        width: 220,
        height: 12,
    };
    let mut app = App::new(vec![file("src/alpha.rs", 1, 40)]);
    let before = app.review_controller.snapshot(viewport).clone();
    for event in [
        mouse(
            MouseEventKind::Down(MouseButton::Right),
            before.sidebar_width,
            2,
            KeyModifiers::NONE,
        ),
        mouse(
            MouseEventKind::Up(MouseButton::Left),
            50,
            2,
            KeyModifiers::NONE,
        ),
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            500,
            500,
            KeyModifiers::NONE,
        ),
    ] {
        app.handle_mouse(event, viewport);
    }
    app.handle_mouse(
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            140,
            1,
            KeyModifiers::NONE,
        ),
        viewport,
    );
    app.handle_mouse(
        mouse(
            MouseEventKind::Up(MouseButton::Left),
            140,
            1,
            KeyModifiers::NONE,
        ),
        viewport,
    );
    let after = app.review_controller.snapshot(viewport);
    assert_eq!(after.scroll_top, before.scroll_top);
    assert_eq!(after.sidebar_width, before.sidebar_width);
    assert_eq!(after.selected_file_id, before.selected_file_id);
    assert_eq!(app.toast, None);
}
