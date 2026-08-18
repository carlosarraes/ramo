use ramo::review_map::{ReviewMapAction, ReviewMapController};
use ramo::ui::review::ReviewHeading;
use ramo::ui::review_map::{ReviewMapHitTarget, ReviewMapWidget, review_map_hits};
use ramo::ui::themes::ThemeRegistry;
use ramo_core::review_map::{
    FileInsight, GroupInsight, PatchCoverage, ReviewFileKind, ReviewMap, ReviewMapAnalysis,
    ReviewMapFile, ReviewMapGroup, ReviewMapIdentity, ReviewMapStatus, ReviewMapTotals,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

#[test]
fn enriched_map_shows_totals_groups_order_and_progress() {
    let mut controller = ReviewMapController::new(map(ReviewMapStatus::Enriched));
    controller.mark_reviewed("src/billing/proration.ts");
    let snapshot = controller.snapshot();
    let frame = render(96, 12, &snapshot);

    for expected in [
        "+414",
        "−60",
        "develop ← feat/mon-xxx",
        "Core billing path",
        "① src/billing/proration.ts",
        "50% reviewed",
        "Updates invoice proration",
    ] {
        assert!(frame.contains(expected), "missing {expected:?}:\n{frame}");
    }
}

#[test]
fn narrow_and_short_maps_keep_chrome_selection_and_paths() {
    let controller = ReviewMapController::new(map(ReviewMapStatus::Enriched));
    let snapshot = controller.snapshot();
    let frame = render(44, 7, &snapshot);

    assert!(frame.contains("+414"));
    assert!(frame.contains("src/billing/proration.ts"));
    assert!(frame.contains("0% reviewed"));
    assert!(!frame.contains("Updates invoice proration"));
    assert!(frame.contains('›'));
}

#[test]
fn offline_failure_is_visible_without_replacing_the_exact_tree() {
    let mut controller = ReviewMapController::new(map(ReviewMapStatus::Unavailable));
    controller.set_failure(
        ramo_core::review_map::ReviewMapFailureCode::ServerUnreachable,
        "Laptop analysis unavailable",
    );
    let snapshot = controller.snapshot();
    let frame = render(80, 10, &snapshot);

    assert!(frame.contains("Laptop analysis unavailable"));
    assert!(frame.contains("Esc dismiss"));
    assert!(frame.contains("src/billing/"));
}

#[test]
fn hit_geometry_tracks_visible_group_and_file_rows() {
    let controller = ReviewMapController::new(map(ReviewMapStatus::Ready));
    let snapshot = controller.snapshot();
    let hits = review_map_hits(ratatui::layout::Rect::new(0, 0, 80, 10), &snapshot);

    assert!(matches!(
        hits[0].target,
        ReviewMapHitTarget::ToggleGroup { .. }
    ));
    assert!(matches!(
        hits[1].target,
        ReviewMapHitTarget::OpenFile { .. }
    ));
    assert_eq!(hits[0].area.y, 2);
    assert_eq!(hits[1].area.y, 3);
}

const LONG_SUMMARY: &str = "Extends Finalize with revision_requested and makes request_changes an immediate terminal transition independent of chain mode";
const LONG_RISK: &str = "The new Finalize literal can expose transition consumers that still assume the only terminal outcomes are approved and rejected";

#[test]
fn the_detail_band_shows_the_focused_rows_full_summary_and_risk() {
    let mut controller = ReviewMapController::new(long_map());
    controller.apply(ReviewMapAction::Select(
        "file:head:src/billing/proration.ts".into(),
    ));
    let snapshot = controller.snapshot();
    let flat = flatten(&render(96, 20, &snapshot));

    // Reflowed in full rather than cut, and carrying the risk the tree has never had room for.
    assert!(flat.contains(LONG_SUMMARY), "summary was cut:\n{flat}");
    assert!(flat.contains(LONG_RISK), "risk missing:\n{flat}");
}

#[test]
fn the_tree_dims_summaries_so_paths_stay_scannable() {
    let controller = ReviewMapController::new(long_map());
    let snapshot = controller.snapshot();
    let theme = ThemeRegistry::default().resolve("tokyo-night", None, false);
    let buffer = render_buffer(96, 20, &snapshot);

    let (y, line) = row_containing(&buffer, "src/billing/proration.ts").expect("file row");
    let dash = line.find('\u{2014}').expect("summary separator");
    let path_x = line.find("src/billing").expect("path start");

    assert_eq!(
        buffer[(path_x as u16, y)].fg,
        theme.text,
        "path should be bright"
    );
    assert_eq!(
        buffer[((dash + 2) as u16, y)].fg,
        theme.muted,
        "summary should be dimmed"
    );
}

#[test]
fn inline_summaries_are_cut_on_a_word_boundary() {
    let controller = ReviewMapController::new(long_map());
    let snapshot = controller.snapshot();
    let buffer = render_buffer(96, 20, &snapshot);

    let (_, line) = row_containing(&buffer, "src/billing/proration.ts").expect("file row");
    let dash = line.find('\u{2014}').expect("summary separator");
    let tail = &line[dash + '\u{2014}'.len_utf8()..];
    let shown = tail.split('\u{2026}').next().expect("summary text").trim();

    assert!(
        !shown.is_empty() && LONG_SUMMARY.starts_with(shown),
        "not a prefix: {shown:?}"
    );
    let rest = &LONG_SUMMARY[shown.len()..];
    assert!(
        rest.is_empty() || rest.starts_with(' '),
        "cut mid-word before {rest:?}"
    );
}

fn flatten(frame: &str) -> String {
    frame.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn row_containing(buffer: &Buffer, needle: &str) -> Option<(u16, String)> {
    (0..buffer.area.height).find_map(|y| {
        let line: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        line.contains(needle).then_some((y, line))
    })
}

fn long_map() -> ReviewMap {
    let mut map = map(ReviewMapStatus::Enriched);
    map.files[0].insight = Some(FileInsight {
        summary: LONG_SUMMARY.into(),
        risk: Some(LONG_RISK.into()),
    });
    map
}

fn render_buffer(
    width: u16,
    height: u16,
    snapshot: &ramo::review_map::ReviewMapSnapshot,
) -> Buffer {
    let theme = ThemeRegistry::default().resolve("tokyo-night", None, false);
    let heading = ReviewHeading::PullRequest {
        number: 1914,
        title: "Billing proration".into(),
        base_ref: "develop".into(),
        head_ref: "feat/mon-xxx".into(),
    };
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                ReviewMapWidget::new(&heading, snapshot, &theme),
                frame.area(),
            );
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn render(width: u16, height: u16, snapshot: &ramo::review_map::ReviewMapSnapshot) -> String {
    let buffer = render_buffer(width, height, snapshot);
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn map(status: ReviewMapStatus) -> ReviewMap {
    let files = vec![
        ReviewMapFile {
            id: "file:head:src/billing/proration.ts".into(),
            path: "src/billing/proration.ts".into(),
            previous_path: None,
            status: "modified".into(),
            additions: 212,
            deletions: 38,
            kind: ReviewFileKind::Authored,
            owner: Some("@billing".into()),
            coverage: PatchCoverage::Full,
            insight: Some(FileInsight {
                summary: "Updates invoice proration".into(),
                risk: Some("Check date boundaries".into()),
            }),
            recommended_order: Some(1),
        },
        ReviewMapFile {
            id: "file:head:tests/test_proration.ts".into(),
            path: "tests/test_proration.ts".into(),
            previous_path: None,
            status: "modified".into(),
            additions: 202,
            deletions: 22,
            kind: ReviewFileKind::Test,
            owner: None,
            coverage: PatchCoverage::Full,
            insight: None,
            recommended_order: None,
        },
    ];
    ReviewMap {
        schema_version: 1,
        identity: ReviewMapIdentity {
            repository: "Mondrio-App/mondrio-platform".into(),
            pull_request: 1914,
            base_sha: "base".into(),
            head_sha: "head".into(),
        },
        status,
        totals: ReviewMapTotals {
            files: 2,
            additions: 414,
            deletions: 60,
            authored: 1,
            tests: 1,
            ..ReviewMapTotals::default()
        },
        groups: vec![
            ReviewMapGroup {
                id: "group:head:core".into(),
                label: "Core billing path".into(),
                kind: ReviewFileKind::Authored,
                file_ids: vec![files[0].id.clone()],
                additions: 212,
                deletions: 38,
                collapsed_by_default: false,
                insight: Some(GroupInsight {
                    summary: "Updates invoice proration".into(),
                    risk: Some("Check date boundaries".into()),
                    review_priority: 1,
                }),
            },
            ReviewMapGroup {
                id: "group:head:tests".into(),
                label: "Tests".into(),
                kind: ReviewFileKind::Test,
                file_ids: vec![files[1].id.clone()],
                additions: 202,
                deletions: 22,
                collapsed_by_default: true,
                insight: None,
            },
        ],
        files,
        analysis: (status == ReviewMapStatus::Enriched).then(|| ReviewMapAnalysis {
            model: "qwen2.5-coder:7b".into(),
            prompt_version: 1,
            completed_at: "2026-07-29T12:00:00Z".into(),
        }),
    }
}
