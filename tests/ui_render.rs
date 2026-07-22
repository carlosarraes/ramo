use std::path::PathBuf;

use ramo::core::input::LayoutMode;
use ramo::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, MovedLineKind, SourceSpec,
};
use ramo::review::{
    ContextSourceLoader, ReviewAction, ReviewController, ReviewOptions, ReviewSide, SelectionPoint,
    SourceFailure, Viewport,
};
use ramo::ui::highlight::{HighlightCache, HighlightCacheStats};
use ramo::ui::review::ReviewWidget;
use ramo::ui::themes::ThemeRegistry;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

fn file(path: &str, kind: FileChangeKind, line_count: usize) -> DiffFile {
    let lines = (0..line_count)
        .map(|index| DiffLine {
            kind: if index % 2 == 0 {
                LineType::Deletion
            } else {
                LineType::Addition
            },
            content: format!("let item{index:02} = {index};"),
            old_lineno: (index % 2 == 0).then_some(index as u32 + 1),
            new_lineno: (index % 2 == 1).then_some(index as u32 + 1),
            moved: None,
        })
        .collect();
    DiffFile {
        id: format!("file:{path}"),
        path: path.into(),
        previous_path: None,
        summary: None,
        agent: None,
        patch: String::new(),
        hunks: vec![Hunk {
            old_start: 1,
            new_start: 1,
            header: "@@ -1,20 +1,20 @@ render_target".into(),
            lines,
        }],
        change_kind: kind,
        is_binary: false,
        is_untracked: false,
        is_too_large: false,
        stats_truncated: false,
        language: Some("rs".into()),
        stats: FileStats {
            additions: line_count / 2,
            deletions: line_count.div_ceil(2),
        },
        old_source: SourceSpec::File(PathBuf::from("old")),
        new_source: SourceSpec::File(PathBuf::from("new")),
    }
}

fn render(
    width: u16,
    height: u16,
    files: Vec<DiffFile>,
    options: ReviewOptions,
) -> (Buffer, HighlightCacheStats) {
    let mut controller = ReviewController::new(files, options);
    let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);
    let mut highlights = HighlightCache::with_capacity(4);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                ReviewWidget::new(&mut controller, &theme, &mut highlights),
                frame.area(),
            );
        })
        .unwrap();
    (terminal.backend().buffer().clone(), highlights.stats())
}

fn text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_controller(width: u16, height: u16, controller: &mut ReviewController) -> Buffer {
    render_controller_with_selection(width, height, controller, None)
}

fn render_controller_with_selection(
    width: u16,
    height: u16,
    controller: &mut ReviewController,
    selection: Option<(SelectionPoint, SelectionPoint)>,
) -> Buffer {
    let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);
    let mut highlights = HighlightCache::with_capacity(4);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                ReviewWidget::new(controller, &theme, &mut highlights).selection(selection),
                frame.area(),
            );
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn responsive_stream_has_no_top_menu_and_later_files_have_headers() {
    for (width, expected_split, expected_sidebar) in
        [(80, false, false), (160, true, false), (220, true, true)]
    {
        let (buffer, _) = render(
            width,
            18,
            vec![
                file("src/alpha.rs", FileChangeKind::Modified, 4),
                file("docs/beta.rs", FileChangeKind::Renamed, 4),
            ],
            ReviewOptions::default(),
        );
        let frame = text(&buffer);
        assert_eq!(
            frame.contains("│"),
            expected_split || expected_sidebar,
            "{width}:\n{frame}"
        );
        assert!(frame.contains("src/alpha.rs"), "{width}:\n{frame}");
        assert!(frame.contains("docs/beta.rs"), "{width}:\n{frame}");
        assert!(!frame.contains("F10 menu"));
        assert!(!frame.contains("File  View"));
    }
}

#[test]
fn cursor_paints_the_focused_split_side_and_selection_overrides_it() {
    let viewport = Viewport {
        width: 180,
        height: 8,
    };
    let mut controller = ReviewController::new(
        vec![file("src/cursor.rs", FileChangeKind::Modified, 2)],
        ReviewOptions::default(),
    );
    let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);

    let left = render_controller(viewport.width, viewport.height, &mut controller);
    let left_frame = text(&left);
    let (y, row) = left_frame
        .lines()
        .enumerate()
        .find(|(_, row)| row.contains("let item00") && row.contains("let item01"))
        .unwrap();
    let left_x = row.find("let item00").unwrap() as u16;
    let right_x = row.find("let item01").unwrap() as u16;
    assert_eq!(left[(left_x, y as u16)].bg, theme.selected_hunk);
    assert_ne!(left[(right_x, y as u16)].bg, theme.selected_hunk);

    controller.apply(ReviewAction::FocusSide(ReviewSide::Right), viewport);
    let right = render_controller(viewport.width, viewport.height, &mut controller);
    assert_ne!(right[(left_x, y as u16)].bg, theme.selected_hunk);
    assert_eq!(right[(right_x, y as u16)].bg, theme.selected_hunk);

    let selection = controller.selected_line_range(viewport).unwrap();
    let selected = render_controller_with_selection(
        viewport.width,
        viewport.height,
        &mut controller,
        Some(selection),
    );
    assert_eq!(selected[(right_x, y as u16)].bg, theme.accent_muted);
}

#[test]
fn first_file_header_is_visible_without_the_sidebar() {
    let (buffer, _) = render(
        80,
        8,
        vec![file("src/only.rs", FileChangeKind::Modified, 2)],
        ReviewOptions::default(),
    );
    let frame = text(&buffer);
    let header = frame.find("src/only.rs").unwrap();
    let hunk = frame.find("render_target").unwrap();
    assert!(header < hunk, "{frame}");
}

#[test]
fn hunk_headers_can_occupy_zero_rows_and_file_states_render() {
    let mut binary = file("assets/image.bin", FileChangeKind::Modified, 0);
    binary.hunks.clear();
    binary.is_binary = true;
    let (buffer, _) = render(
        220,
        16,
        vec![file("src/new.rs", FileChangeKind::Added, 4), binary],
        ReviewOptions {
            hunk_headers: false,
            ..ReviewOptions::default()
        },
    );
    let frame = text(&buffer);
    assert!(!frame.contains("render_target"));
    assert!(frame.contains("Binary file"));
    assert!(frame.contains("+2"));
}

#[test]
fn renderer_highlights_only_the_bounded_visible_window() {
    let (buffer, stats) = render(
        80,
        8,
        vec![file("src/many.rs", FileChangeKind::Modified, 200)],
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    assert!(text(&buffer).contains("item00"));
    assert!(stats.line_entries <= 12, "{stats:?}");
    assert!(stats.line_entries < 200);
}

#[test]
fn inline_agent_notes_render_inside_the_measured_review_stream() {
    let mut annotated = file("src/note.rs", FileChangeKind::Modified, 4);
    annotated.agent = Some(
        ramo::notes::parse_agent_context(
            "agent.json",
            br#"{"files":[{"path":"src/note.rs","annotations":[{
              "newRange":[2,2],
              "summary":"Check the retry boundary.",
              "rationale":"The final attempt currently sleeps.",
              "author":"pi",
              "tags":["correctness"]
            }]}]}"#,
        )
        .unwrap()
        .files
        .remove(0),
    );
    let (visible, _) = render(
        100,
        18,
        vec![annotated.clone()],
        ReviewOptions {
            layout: LayoutMode::Stack,
            agent_notes: true,
            ..ReviewOptions::default()
        },
    );
    let frame = text(&visible);
    assert!(frame.contains("AI note"), "{frame}");
    assert!(frame.contains("src/note.rs R2"), "{frame}");
    assert!(frame.contains("Check the retry boundary."), "{frame}");
    assert!(frame.contains("pi · correctness"), "{frame}");

    let (hidden, _) = render(
        100,
        18,
        vec![annotated],
        ReviewOptions {
            layout: LayoutMode::Stack,
            agent_notes: false,
            ..ReviewOptions::default()
        },
    );
    assert!(!text(&hidden).contains("Check the retry boundary."));
}

#[test]
fn inline_agent_markup_replaces_plain_fallback_and_keeps_semantic_span_style() {
    let mut annotated = file("src/markup.rs", FileChangeKind::Modified, 4);
    annotated.agent = Some(
        ramo::notes::parse_agent_context(
            "agent.json",
            br#"{"files":[{"path":"src/markup.rs","annotations":[{
              "newRange":[2,2],
              "summary":"Plain fallback must be hidden",
              "markup":"<h2>Refactor</h2><badge color=success>PASS</badge> native <color fg=#0f0>HEX</color>"
            }]}]}"#,
        )
        .unwrap()
        .files
        .remove(0),
    );
    let (buffer, _) = render(
        100,
        18,
        vec![annotated],
        ReviewOptions {
            layout: LayoutMode::Stack,
            agent_notes: true,
            ..ReviewOptions::default()
        },
    );
    let frame = text(&buffer);
    assert!(frame.contains("Refactor"), "{frame}");
    assert!(frame.contains(" PASS  native"), "{frame}");
    assert!(!frame.contains("Plain fallback must be hidden"), "{frame}");
    let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);
    let (y, row) = frame
        .lines()
        .enumerate()
        .find(|(_, row)| row.contains("PASS"))
        .unwrap();
    let x = row.find("PASS").unwrap() as u16;
    assert_eq!(buffer[(x, y as u16)].bg, theme.added_sign);
    assert!(
        buffer[(x, y as u16)]
            .modifier
            .contains(ratatui::style::Modifier::BOLD)
    );
    let hex_x = row.find("HEX").unwrap() as u16;
    assert_eq!(
        buffer[(hex_x, y as u16)].fg,
        ratatui::style::Color::Rgb(0, 255, 0)
    );
}

#[test]
fn moved_rows_keep_moved_paint_while_changed_characters_use_stronger_backgrounds() {
    let mut moved = file("src/moved.rs", FileChangeKind::Modified, 2);
    moved.hunks[0].lines[0].moved = Some(MovedLineKind::OldMoved);
    moved.hunks[0].lines[1].moved = Some(MovedLineKind::NewMoved);
    let viewport = Viewport {
        width: 80,
        height: 8,
    };
    let mut controller = ReviewController::new(
        vec![moved],
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    controller.apply(ReviewAction::MoveCursor(1), viewport);
    let buffer = render_controller(viewport.width, viewport.height, &mut controller);
    let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);
    let rows = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains("let item00"))
        .unwrap();
    let start = row.find("let item00").unwrap();
    assert_eq!(buffer[(start as u16, y as u16)].bg, theme.moved_removed_bg);
    assert_eq!(
        buffer[((start + "let item0".len()) as u16, y as u16)].bg,
        theme.removed_content_bg
    );
}

struct FailingLoader(Result<Option<String>, SourceFailure>);

impl ContextSourceLoader for FailingLoader {
    fn load(&mut self, _spec: &SourceSpec) -> Result<Option<String>, SourceFailure> {
        self.0.clone()
    }
}

#[test]
fn context_source_failures_render_distinct_single_row_states_without_geometry_jumps() {
    let cases = [
        (Ok(None), "Source missing"),
        (Err(SourceFailure::NonUtf8), "Non-UTF-8 source"),
        (
            Err(SourceFailure::TooLarge { limit: 1024 }),
            "Source too large",
        ),
        (
            Err(SourceFailure::Command("git failed".into())),
            "Source command failed",
        ),
    ];
    let viewport = Viewport {
        width: 80,
        height: 20,
    };

    for (result, expected) in cases {
        let mut source_file = file("src/context.rs", FileChangeKind::Modified, 2);
        source_file.hunks[0].old_start = 4;
        source_file.hunks[0].new_start = 4;
        let mut controller = ReviewController::new(vec![source_file], ReviewOptions::default());
        let before = controller.snapshot(viewport).total_height;
        let mut loader = FailingLoader(result);

        assert!(controller.toggle_context(&mut loader, viewport).is_err());
        assert_eq!(controller.snapshot(viewport).total_height, before);
        let frame = text(&render_controller(80, 20, &mut controller));
        assert!(
            frame.contains(expected),
            "expected {expected:?} in:\n{frame}"
        );
    }
}

#[test]
fn stable_selection_projection_is_painted_on_the_selected_terminal_cells() {
    let viewport = Viewport {
        width: 80,
        height: 8,
    };
    let mut controller = ReviewController::new(
        vec![file("src/select.rs", FileChangeKind::Modified, 2)],
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    let selection = controller.selected_line_range(viewport).unwrap();
    assert_eq!(
        controller.selection_text(selection.0, selection.1, viewport),
        "let item00 = 0;"
    );
    let buffer = render_controller_with_selection(80, 8, &mut controller, Some(selection));
    let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);
    let rows = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains("let item00"))
        .unwrap();
    let x = row.find("let item00").unwrap() as u16;
    assert_eq!(buffer[(x, y as u16)].bg, theme.accent_muted);
}

#[test]
fn copied_decorations_config_includes_the_rendered_gutter_for_line_selection() {
    let viewport = Viewport {
        width: 80,
        height: 8,
    };
    let mut controller = ReviewController::new(
        vec![file("src/select.rs", FileChangeKind::Modified, 2)],
        ReviewOptions {
            layout: LayoutMode::Stack,
            copy_decorations: true,
            ..ReviewOptions::default()
        },
    );
    let selection = controller.selected_line_range(viewport).unwrap();

    assert_eq!(
        controller.selection_text(selection.0, selection.1, viewport),
        "1   - let item00 = 0;"
    );
}
