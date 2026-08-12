use ramo::review_map::ReviewMapController;
use ramo::ui::review::ReviewHeading;
use ramo::ui::review_map::{ReviewMapHitTarget, ReviewMapWidget, review_map_hits};
use ramo::ui::themes::ThemeRegistry;
use ramo_core::review_map::{
    FileInsight, GroupInsight, PatchCoverage, ReviewFileKind, ReviewMap, ReviewMapAnalysis,
    ReviewMapFile, ReviewMapGroup, ReviewMapIdentity, ReviewMapStatus, ReviewMapTotals,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

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

fn render(width: u16, height: u16, snapshot: &ramo::review_map::ReviewMapSnapshot) -> String {
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
