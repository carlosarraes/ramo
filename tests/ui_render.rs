use std::path::PathBuf;

use ramo::core::input::LayoutMode;
use ramo::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, MovedLineKind, SourceSpec,
};
use ramo::remote_review::{
    GithubReviewThread, GithubThreadComment, GithubThreadSubject, RemoteLineSide,
};
use ramo::review::{
    ContextSourceLoader, ReviewAction, ReviewController, ReviewOptions, ReviewSide, SelectionPoint,
    SourceFailure, Viewport,
};
use ramo::ui::highlight::{HighlightCache, HighlightCacheStats};
use ramo::ui::review::{ReviewFooter, ReviewHeader, ReviewHeading, ReviewWidget, review_areas};
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

fn github_thread(id: &str, path: &str, subject: GithubThreadSubject) -> GithubReviewThread {
    GithubReviewThread {
        id: id.into(),
        path: path.into(),
        is_resolved: false,
        is_outdated: false,
        subject,
        comments: vec![
            GithubThreadComment {
                id: format!("{id}:root"),
                author: "alice".into(),
                body: "root feedback".into(),
                created_at: "2026-07-26T14:32:00Z".into(),
                url: "https://github.com/owner/repo/pull/123#discussion_r1".into(),
            },
            GithubThreadComment {
                id: format!("{id}:reply"),
                author: "bob".into(),
                body: "reply feedback".into(),
                created_at: "2026-07-26T15:10:00Z".into(),
                url: "https://github.com/owner/repo/pull/123#discussion_r2".into(),
            },
        ],
        url: "https://github.com/owner/repo/pull/123#discussion_r1".into(),
    }
}

#[test]
fn github_threads_render_inline_and_in_the_unplaced_trailer() {
    let viewport = Viewport {
        width: 120,
        height: 40,
    };
    let mut controller = ReviewController::new(
        vec![file("src/lib.rs", FileChangeKind::Modified, 4)],
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    controller.attach_github_threads(
        vec![
            github_thread(
                "placed",
                "src/lib.rs",
                GithubThreadSubject::Line {
                    side: Some(RemoteLineSide::Right),
                    start_side: Some(RemoteLineSide::Right),
                    start_line: Some(2),
                    end_line: Some(2),
                },
            ),
            github_thread("unplaced", "src/absent.rs", GithubThreadSubject::File),
        ],
        viewport,
    );

    let frame = text(&render_controller(120, 40, &mut controller));
    for expected in [
        "GitHub · @alice · 2026-07-26T14:32:00Z",
        "src/lib.rs RIGHT:2",
        "root feedback",
        "↳ @bob · 2026-07-26T15:10:00Z",
        "reply feedback",
        "https://github.com/owner/repo/pull/123#discussion_r1",
        "Unplaced GitHub comments",
        "file is not present in the frozen diff",
        "src/absent.rs",
    ] {
        assert!(
            frame.contains(expected),
            "missing {expected:?} in {frame:?}"
        );
    }

    let _ = render_controller(24, 12, &mut controller);
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

fn render_chrome(
    width: u16,
    height: u16,
    heading: &ReviewHeading,
    snapshot: &ramo::review::ReviewSnapshot,
    status: Option<&str>,
    theme: &ramo::ui::themes::AppTheme,
) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let areas = review_areas(frame.area());
            frame.render_widget(ReviewHeader::new(heading, snapshot, theme), areas.header);
            frame.render_widget(ReviewFooter::new(status, snapshot, theme), areas.footer);
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
            ReviewOptions {
                layout: LayoutMode::Auto,
                ..ReviewOptions::default()
            },
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
        ReviewOptions {
            layout: LayoutMode::Split,
            ..ReviewOptions::default()
        },
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
    let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);
    let (y, row) = frame
        .lines()
        .enumerate()
        .find(|(_, row)| row.contains("let item00"))
        .unwrap();
    let x = row.find("let item00").unwrap() as u16;
    assert_eq!(buffer[(x, y as u16)].bg, theme.selected_hunk);
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
    assert_ne!(
        buffer[(start as u16, y as u16)].fg,
        theme.text,
        "the `let` token should retain its syntax foreground"
    );
    assert_eq!(
        buffer[((start + "let item0".len()) as u16, y as u16)].bg,
        theme.removed_content_bg
    );
}

#[test]
fn syntax_foregrounds_render_over_diff_backgrounds_and_emphasis_stays_stronger() {
    let mut source = file("src/lib.rs", FileChangeKind::Modified, 2);
    source.hunks[0].lines[0].content = "fn highlighted(value: usize) -> usize { value + 1 }".into();
    source.hunks[0].lines[1].content = "fn highlighted(value: usize) -> usize { value + 2 }".into();
    let mut controller = ReviewController::new(
        vec![source],
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    let buffer = render_controller(100, 8, &mut controller);
    let theme = ThemeRegistry::default().resolve("github-dark-default", None, false);
    let frame = text(&buffer);
    let (y, row) = frame
        .lines()
        .enumerate()
        .find(|(_, row)| row.contains("+ 2 }"))
        .unwrap();
    let keyword_x = row.find("fn").unwrap() as u16;
    let changed_x = row.rfind('2').unwrap() as u16;

    assert_ne!(buffer[(keyword_x, y as u16)].fg, theme.text);
    // Unchanged tokens keep the ordinary line background; only the edited token is stronger.
    assert_eq!(buffer[(keyword_x, y as u16)].bg, theme.added_bg);
    assert_eq!(buffer[(changed_x, y as u16)].bg, theme.added_content_bg);
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

#[test]
fn compact_test_file_is_one_summary_row_while_source_stays_expanded() {
    let mut test = file("tests/widget.rs", FileChangeKind::Modified, 2);
    test.hunks[0].lines[0].content = "test-only-old".into();
    test.hunks[0].lines[1].content = "test-only-new".into();
    let mut controller = ReviewController::new(
        vec![test, file("src/lib.rs", FileChangeKind::Modified, 2)],
        ReviewOptions::default(),
    );
    let viewport = Viewport {
        width: 80,
        height: 12,
    };
    controller.apply(ReviewAction::ToggleTestFiles, viewport);
    let buffer = render_controller(viewport.width, viewport.height, &mut controller);
    let frame = text(&buffer);

    assert!(frame.contains("▸ tests/widget.rs"));
    assert!(frame.contains("+1 -1"));
    assert!(!frame.contains("test-only-old"));
    assert!(frame.contains("src/lib.rs"));
    assert!(frame.contains("let item00"));
}

#[test]
fn review_chrome_keeps_colored_totals_and_progress_visible() {
    let mut controller = ReviewController::new(
        vec![file("src/lib.rs", FileChangeKind::Modified, 2)],
        ReviewOptions::default(),
    );
    let mut snapshot = controller
        .snapshot(Viewport {
            width: 80,
            height: 8,
        })
        .clone();
    snapshot.total_files = 14;
    snapshot.total_additions = 200;
    snapshot.total_deletions = 50;
    snapshot.reviewed_lines = 125;
    snapshot.total_changed_lines = 250;
    snapshot.reviewed_percent = 50;
    let theme = ThemeRegistry::default().resolve("tokyo-night", None, false);
    let buffer = render_chrome(
        80,
        4,
        &ReviewHeading::PullRequest {
            number: 123,
            title: "Improve review flow".into(),
            base_ref: "develop".into(),
            head_ref: "feat/mon-xxx".into(),
        },
        &snapshot,
        Some(" Filter: src"),
        &theme,
    );
    let frame = text(&buffer);
    let header = frame.lines().next().unwrap();
    let footer = frame.lines().last().unwrap();

    assert!(header.contains("GitHub PR #123"));
    assert!(header.contains("develop ← feat/mon-xxx"));
    assert!(header.contains("14 files · +200 -50"));
    assert!(footer.contains("Filter: src"));
    assert!(footer.contains("Reviewed 50%"));
    let cell_x = |needle: &str| {
        let byte = header.find(needle).unwrap();
        header[..byte].chars().count() as u16
    };
    assert_eq!(buffer[(cell_x("+200"), 0)].fg, theme.added_sign);
    assert_eq!(buffer[(cell_x("-50"), 0)].fg, theme.removed_sign);
}

#[test]
fn constrained_pr_heading_preserves_the_source_branch_prefix() {
    let mut controller = ReviewController::new(
        vec![file("src/lib.rs", FileChangeKind::Modified, 2)],
        ReviewOptions::default(),
    );
    let mut snapshot = controller
        .snapshot(Viewport {
            width: 60,
            height: 8,
        })
        .clone();
    snapshot.total_additions = 200;
    snapshot.total_deletions = 50;
    let theme = ThemeRegistry::default().resolve("tokyo-night", None, false);
    let buffer = render_chrome(
        60,
        4,
        &ReviewHeading::PullRequest {
            number: 123,
            title: "Improve review flow".into(),
            base_ref: "develop".into(),
            head_ref: "feat/mon-very-long-description".into(),
        },
        &snapshot,
        None,
        &theme,
    );
    let frame = text(&buffer);
    let header = frame.lines().next().unwrap();

    assert!(header.contains("develop ← feat/"));
    assert!(header.contains("… · 1 file"));
    assert!(!header.contains("description"));
    assert!(header.contains("+200 -50"));
}

#[test]
fn narrow_review_chrome_keeps_totals_and_progress() {
    let mut controller = ReviewController::new(
        vec![file("src/lib.rs", FileChangeKind::Modified, 2)],
        ReviewOptions::default(),
    );
    let mut snapshot = controller
        .snapshot(Viewport {
            width: 32,
            height: 8,
        })
        .clone();
    snapshot.total_additions = 200;
    snapshot.total_deletions = 50;
    snapshot.reviewed_percent = 50;
    let theme = ThemeRegistry::default().resolve("tokyo-night", None, false);
    let buffer = render_chrome(
        32,
        4,
        &ReviewHeading::Local("A title that must be truncated".into()),
        &snapshot,
        Some("A long transient message"),
        &theme,
    );
    let frame = text(&buffer);

    assert!(frame.lines().next().unwrap().contains("+200 -50"));
    assert!(frame.lines().last().unwrap().contains("Reviewed 50%"));
}

#[test]
fn tiny_review_chrome_clips_without_writing_outside_the_buffer() {
    let mut controller = ReviewController::new(
        vec![file("src/lib.rs", FileChangeKind::Modified, 2)],
        ReviewOptions::default(),
    );
    let snapshot = controller
        .snapshot(Viewport {
            width: 8,
            height: 2,
        })
        .clone();
    let theme = ThemeRegistry::default().resolve("tokyo-night", None, false);

    let buffer = render_chrome(
        8,
        2,
        &ReviewHeading::Local("Working tree".into()),
        &snapshot,
        None,
        &theme,
    );

    assert_eq!(buffer.area.width, 8);
    assert_eq!(buffer.area.height, 2);
}

#[test]
fn ask_cards_render_pending_then_answered_without_the_agent_notes_toggle() {
    let mut controller = ReviewController::new(
        vec![file("src/lib.rs", FileChangeKind::Modified, 2)],
        ReviewOptions {
            layout: LayoutMode::Stack,
            agent_notes: false,
            ..ReviewOptions::default()
        },
    );
    let view = Viewport {
        width: 100,
        height: 24,
    };
    let id = controller.begin_ask(None, view).expect("draft anchored");
    controller.update_ask_draft("what changed here?", view);
    controller.commit_ask_draft(view).expect("pending question");

    let pending = text(&render_controller(100, 24, &mut controller));
    assert!(pending.contains("Ask AI"), "{pending}");
    assert!(pending.contains("asking"), "{pending}");
    assert!(pending.contains("Q: what changed here?"), "{pending}");

    controller.resolve_ask(
        &id,
        ramo::notes::AskNoteState::Answered("It renames the helper.".into()),
        view,
    );
    let answered = text(&render_controller(100, 24, &mut controller));
    assert!(answered.contains("It renames the helper."), "{answered}");
    assert!(!answered.contains("asking"), "{answered}");
}

#[test]
fn the_ask_badge_shows_unread_answers_next_to_progress() {
    let mut controller = ReviewController::new(
        vec![file("src/lib.rs", FileChangeKind::Modified, 2)],
        ReviewOptions::default(),
    );
    let snapshot = controller
        .snapshot(Viewport {
            width: 80,
            height: 6,
        })
        .clone();
    let theme = ThemeRegistry::default().resolve("tokyo-night", None, false);

    let with_badge = {
        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let areas = review_areas(frame.area());
                frame.render_widget(
                    ReviewFooter::new(
                        Some("a very long status message that fills the footer row"),
                        &snapshot,
                        &theme,
                    )
                    .ask_badge(Some(2)),
                    areas.footer,
                );
            })
            .unwrap();
        text(terminal.backend().buffer())
    };
    assert!(with_badge.contains("AI 2 · o"), "{with_badge}");
    assert!(with_badge.contains("Reviewed"), "{with_badge}");

    let without_badge = {
        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let areas = review_areas(frame.area());
                frame.render_widget(
                    ReviewFooter::new(None, &snapshot, &theme).ask_badge(Some(0)),
                    areas.footer,
                );
            })
            .unwrap();
        text(terminal.backend().buffer())
    };
    assert!(!without_badge.contains("AI"), "{without_badge}");
}

#[test]
fn a_rejected_model_stays_readable_on_the_failed_card() {
    let mut controller = ReviewController::new(
        vec![file("src/lib.rs", FileChangeKind::Modified, 2)],
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
    let view = Viewport {
        width: 100,
        height: 24,
    };
    let id = controller.begin_ask(None, view).expect("draft anchored");
    controller.update_ask_draft("what is this?", view);
    controller.commit_ask_draft(view).expect("pending question");
    let error = ramo::ask::AskError::ModelRejected {
        provider: "opencode-go".into(),
        model: "deepseek-v4-flash".into(),
        stderr: "Model is not supported".into(),
    }
    .to_string();
    controller.resolve_ask(&id, ramo::notes::AskNoteState::Failed(error), view);

    let frame = text(&render_controller(100, 24, &mut controller));
    assert!(frame.contains("failed"), "{frame}");
    // Wrapping may break the sentence, but never the model id the user must copy into
    // their config, nor the flag that lists the valid ids.
    assert!(frame.contains("deepseek-v4-flash"), "{frame}");
    assert!(frame.contains("--list-models"), "{frame}");
    assert!(frame.contains("ask_model"), "{frame}");
}
