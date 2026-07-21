use std::path::PathBuf;

use pdiff::diff::model::{
    DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, SourceSpec,
};
use pdiff::ui::highlight::HighlightCache;
use pdiff::ui::themes::ThemeRegistry;

fn file(id: &str, path: &str, lines: &[&str]) -> DiffFile {
    DiffFile {
        id: id.into(),
        path: path.into(),
        previous_path: None,
        summary: None,
        agent: None,
        patch: String::new(),
        hunks: vec![Hunk {
            old_start: 1,
            new_start: 1,
            header: "@@ -1 +1 @@".into(),
            lines: lines
                .iter()
                .enumerate()
                .map(|(index, content)| DiffLine {
                    kind: LineType::Addition,
                    content: (*content).into(),
                    old_lineno: None,
                    new_lineno: Some(index as u32 + 1),
                    moved: None,
                })
                .collect(),
        }],
        change_kind: FileChangeKind::Modified,
        is_binary: false,
        is_untracked: false,
        is_too_large: false,
        stats_truncated: false,
        language: None,
        stats: FileStats {
            additions: lines.len(),
            deletions: 0,
        },
        old_source: SourceSpec::File(PathBuf::from("old")),
        new_source: SourceSpec::File(PathBuf::from("new")),
    }
}

#[test]
fn highlighting_is_lazy_content_sensitive_and_reuses_requested_lines() {
    let registry = ThemeRegistry::default();
    let dark = registry.resolve("github-dark-default", None, false);
    let mut source = file(
        "file:lib",
        "src/lib.rs",
        &[
            "fn one() {}",
            "let untouched = true;",
            "struct NeverRequested;",
        ],
    );
    let mut cache = HighlightCache::with_capacity(4);
    assert_eq!(cache.stats().line_entries, 0);

    let first = cache.spans(&source, 0, 0, &dark);
    assert_eq!(
        first
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>(),
        "fn one() {}"
    );
    assert_eq!(cache.stats().file_theme_entries, 1);
    assert_eq!(cache.stats().line_entries, 1);
    assert_eq!(cache.stats().misses, 1);

    let _ = cache.spans(&source, 0, 0, &dark);
    assert_eq!(cache.stats().misses, 1);
    assert_eq!(cache.stats().line_entries, 1);

    source.hunks[0].lines[0].content = "fn two() {}".into();
    let changed = cache.spans(&source, 0, 0, &dark);
    assert_eq!(
        changed
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>(),
        "fn two() {}"
    );
    assert_eq!(cache.stats().misses, 2);
    assert_eq!(cache.stats().line_entries, 1);
}

#[test]
fn file_theme_lru_is_bounded_across_files_and_theme_cycles() {
    let registry = ThemeRegistry::default();
    let dark = registry.resolve("github-dark-default", None, false);
    let light = registry.resolve("github-light-default", None, false);
    let first = file("file:first", "first.rs", &["fn first() {}"]);
    let second = file("file:second", "second.rs", &["fn second() {}"]);
    let mut cache = HighlightCache::with_capacity(2);

    cache.spans(&first, 0, 0, &dark);
    cache.spans(&first, 0, 0, &light);
    assert!(cache.contains_file_theme(&first, &dark));
    cache.spans(&second, 0, 0, &dark);

    assert_eq!(cache.stats().file_theme_entries, 2);
    assert!(!cache.contains_file_theme(&first, &dark));
    assert!(cache.contains_file_theme(&first, &light));
    assert!(cache.contains_file_theme(&second, &dark));
}
