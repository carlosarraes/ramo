use std::path::PathBuf;

use pdiff::core::input::LayoutMode;
use pdiff::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, MovedLineKind, SourceSpec,
};
use pdiff::review::{ReviewController, ReviewOptions};
use pdiff::ui::highlight::{HighlightCache, HighlightCacheStats};
use pdiff::ui::review::ReviewWidget;
use pdiff::ui::themes::ThemeRegistry;
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
        assert_eq!(
            frame.contains("src/"),
            expected_sidebar,
            "{width}:\n{frame}"
        );
        assert!(frame.contains("docs/beta.rs"), "{width}:\n{frame}");
        assert!(!frame.contains("F10 menu"));
        assert!(!frame.contains("File  View"));
    }
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
fn moved_rows_keep_moved_paint_while_changed_characters_use_stronger_backgrounds() {
    let mut moved = file("src/moved.rs", FileChangeKind::Modified, 2);
    moved.hunks[0].lines[0].moved = Some(MovedLineKind::OldMoved);
    moved.hunks[0].lines[1].moved = Some(MovedLineKind::NewMoved);
    let (buffer, _) = render(
        80,
        8,
        vec![moved],
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    );
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
