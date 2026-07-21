use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use pdiff::app::App;
use pdiff::config::ResolvedConfig;
use pdiff::core::input::LayoutMode;
use pdiff::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, SourceSpec,
};
use pdiff::review::NativeContextSourceLoader;
use pdiff::review::{
    CollapsedGap, ContextSourceLoader, GapKey, GapPosition, ReviewController, ReviewOptions,
    SourceFailure, SourceSide, Viewport, derive_collapsed_gaps, expand_gap_lines,
    select_gap_for_toggle, source_for_context,
};
use pdiff::vcs::{CommandOutput, CommandRunner, CommandSpec, VcsError};

fn line(kind: LineType, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
    DiffLine {
        kind,
        content: text.into(),
        old_lineno: old,
        new_lineno: new,
        moved: None,
    }
}

fn file(kind: FileChangeKind) -> DiffFile {
    DiffFile {
        id: "file:src/lib.rs".into(),
        path: "src/lib.rs".into(),
        previous_path: None,
        summary: None,
        patch: String::new(),
        hunks: vec![
            Hunk {
                old_start: 4,
                new_start: 4,
                header: "@@ -4,2 +4,2 @@".into(),
                lines: vec![
                    line(LineType::Deletion, Some(4), None, "old"),
                    line(LineType::Addition, None, Some(4), "new"),
                    line(LineType::Context, Some(5), Some(5), "same"),
                ],
            },
            Hunk {
                old_start: 9,
                new_start: 9,
                header: "@@ -9 +9 @@".into(),
                lines: vec![line(LineType::Context, Some(9), Some(9), "later")],
            },
        ],
        change_kind: kind,
        is_binary: false,
        is_untracked: false,
        is_too_large: false,
        stats_truncated: false,
        language: Some("rs".into()),
        stats: FileStats {
            additions: 1,
            deletions: 1,
        },
        old_source: SourceSpec::GitBlob {
            repo_root: PathBuf::from("/repo"),
            reference: "HEAD^".into(),
            path: "src/lib.rs".into(),
        },
        new_source: SourceSpec::GitIndex {
            repo_root: PathBuf::from("/repo"),
            path: "src/lib.rs".into(),
        },
    }
}

#[test]
fn gaps_are_stable_non_overlapping_and_include_trailing_source_range() {
    let gaps = derive_collapsed_gaps(&file(FileChangeKind::Modified), Some((SourceSide::New, 12)));
    assert_eq!(
        gaps.iter()
            .map(|gap| {
                (
                    gap.key.to_string(),
                    gap.old_range.clone(),
                    gap.new_range.clone(),
                )
            })
            .collect::<Vec<_>>(),
        [
            ("before:0".to_owned(), 1..=3, 1..=3),
            ("before:1".to_owned(), 6..=8, 6..=8),
            ("trailing:1".to_owned(), 10..=12, 10..=12),
        ]
    );
}

#[test]
fn keyboard_gap_selection_prefers_current_then_later_then_trailing() {
    let gaps = derive_collapsed_gaps(&file(FileChangeKind::Modified), Some((SourceSide::New, 12)));
    assert_eq!(
        select_gap_for_toggle(&gaps, 0).unwrap().to_string(),
        "before:0"
    );
    assert_eq!(
        select_gap_for_toggle(&gaps[1..], 0).unwrap().to_string(),
        "before:1"
    );
    assert_eq!(
        select_gap_for_toggle(&gaps[2..], 0).unwrap().to_string(),
        "trailing:1"
    );
    assert!(select_gap_for_toggle(&[], 0).is_none());
}

#[test]
fn deleted_files_load_old_source_and_expansion_normalizes_crlf() {
    let deleted = file(FileChangeKind::Deleted);
    let (side, spec) = source_for_context(&deleted);
    assert_eq!(side, SourceSide::Old);
    assert_eq!(spec, &deleted.old_source);

    let gap = derive_collapsed_gaps(&deleted, None).remove(0);
    let lines = expand_gap_lines(&gap, "one\r\ntwo\r\nthree\r\nfour\r\n", SourceSide::Old).unwrap();
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.old_line, line.new_line, line.text.as_str()))
            .collect::<Vec<_>>(),
        [(1, 1, "one"), (2, 2, "two"), (3, 3, "three")]
    );
}

#[test]
fn expansion_preserves_blank_lines_before_a_terminal_newline() {
    let gap = CollapsedGap {
        key: GapKey::new("file:blank", GapPosition::Before, 0),
        old_range: 1..=2,
        new_range: 1..=2,
    };
    let lines = expand_gap_lines(&gap, "one\n\n", SourceSide::New).unwrap();
    assert_eq!(
        lines
            .iter()
            .map(|line| (line.new_line, line.text.as_str()))
            .collect::<Vec<_>>(),
        [(1, "one"), (2, "")]
    );
}

#[derive(Clone)]
struct CountingRunner {
    calls: Arc<AtomicUsize>,
}

impl CommandRunner for CountingRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, VcsError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let stdout = if spec.args.get(1).map(String::as_str) == Some("-s") {
            b"8\n".to_vec()
        } else {
            b"a\nb\nc\nd\n".to_vec()
        };
        Ok(CommandOutput {
            code: 0,
            stdout,
            stderr: Vec::new(),
        })
    }
}

#[test]
fn native_loader_caches_each_spec_and_none_never_spawns() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut loader = NativeContextSourceLoader::new(
        CountingRunner {
            calls: Arc::clone(&calls),
        },
        "git",
        1024,
    );
    assert_eq!(loader.load(&SourceSpec::None).unwrap(), None);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let spec = file(FileChangeKind::Modified).new_source;
    assert_eq!(loader.load(&spec).unwrap().as_deref(), Some("a\nb\nc\nd\n"));
    assert_eq!(loader.load(&spec).unwrap().as_deref(), Some("a\nb\nc\nd\n"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "one size and one content command"
    );
}

#[test]
fn native_loader_invalidation_reads_reloaded_file_contents() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.rs");
    std::fs::write(&path, "first\n").unwrap();
    let spec = SourceSpec::File(path.clone());
    let mut loader = NativeContextSourceLoader::default();
    assert_eq!(loader.load(&spec).unwrap().as_deref(), Some("first\n"));

    std::fs::write(path, "second\n").unwrap();
    assert_eq!(loader.load(&spec).unwrap().as_deref(), Some("first\n"));
    loader.invalidate();
    assert_eq!(loader.load(&spec).unwrap().as_deref(), Some("second\n"));
}

struct FakeLoader {
    calls: usize,
    text: Result<Option<String>, SourceFailure>,
}

impl ContextSourceLoader for FakeLoader {
    fn load(&mut self, _spec: &SourceSpec) -> Result<Option<String>, SourceFailure> {
        self.calls += 1;
        self.text.clone()
    }
}

#[test]
fn controller_expansion_grows_shared_geometry_and_reuses_loaded_source() {
    let mut controller = ReviewController::new(
        vec![file(FileChangeKind::Modified)],
        ReviewOptions::default(),
    );
    let viewport = Viewport {
        width: 80,
        height: 8,
    };
    let collapsed_height = controller.snapshot(viewport).total_height;
    let source = (1..=12)
        .map(|line| format!("source {line}\n"))
        .collect::<String>();
    let mut loader = FakeLoader {
        calls: 0,
        text: Ok(Some(source)),
    };

    assert_eq!(controller.toggle_context(&mut loader, viewport), Ok(true));
    let loaded_collapsed_height = collapsed_height + 1;
    assert_eq!(
        controller.snapshot(viewport).total_height,
        loaded_collapsed_height + 3
    );
    assert_eq!(loader.calls, 1);
    assert_eq!(controller.toggle_context(&mut loader, viewport), Ok(false));
    assert_eq!(
        controller.snapshot(viewport).total_height,
        loaded_collapsed_height
    );
    assert_eq!(controller.toggle_context(&mut loader, viewport), Ok(true));
    assert_eq!(loader.calls, 1);

    controller.apply(
        pdiff::review::ReviewAction::SetLayout(LayoutMode::Split),
        viewport,
    );
    assert_eq!(loader.calls, 1);
    assert_eq!(controller.snapshot(viewport).selected_hunk_index, Some(0));
}

#[test]
fn unavailable_context_has_a_stable_one_row_error_without_loading() {
    let mut unavailable = file(FileChangeKind::Modified);
    unavailable.new_source = SourceSpec::None;
    let mut controller = ReviewController::new(vec![unavailable], ReviewOptions::default());
    let viewport = Viewport {
        width: 80,
        height: 8,
    };
    let before = controller.snapshot(viewport).total_height;
    let mut loader = FakeLoader {
        calls: 0,
        text: Ok(None),
    };
    assert_eq!(
        controller.toggle_context(&mut loader, viewport),
        Err(SourceFailure::Unavailable)
    );
    assert_eq!(loader.calls, 0);
    assert_eq!(controller.snapshot(viewport).total_height, before);
}

struct SharedLoader {
    calls: Arc<AtomicUsize>,
    source: String,
}

impl ContextSourceLoader for SharedLoader {
    fn load(&mut self, _spec: &SourceSpec) -> Result<Option<String>, SourceFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Some(self.source.clone()))
    }
}

#[test]
fn z_routes_through_app_and_reuses_its_owned_native_loader_boundary() {
    let calls = Arc::new(AtomicUsize::new(0));
    let source = (1..=12)
        .map(|line| format!("source {line}\n"))
        .collect::<String>();
    let mut app = App::new_with_context_loader(
        vec![file(FileChangeKind::Modified)],
        &ResolvedConfig::default(),
        false,
        Box::new(SharedLoader {
            calls: Arc::clone(&calls),
            source,
        }),
    );
    let viewport = Viewport {
        width: 80,
        height: 8,
    };
    let collapsed_height = app.review_controller.snapshot(viewport).total_height;

    app.handle_ui_key(
        KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
        viewport,
    );
    assert!(app.review_controller.snapshot(viewport).total_height > collapsed_height);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(app.toast, None);

    app.handle_ui_key(
        KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE),
        viewport,
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
