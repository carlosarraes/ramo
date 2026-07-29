use ramo::review_map::{
    ReplaceError, ReviewMapAction, ReviewMapController, ReviewMapEffect, ReviewMapRow,
};
use ramo_core::review_map::{
    FileInsight, GroupInsight, PatchCoverage, ReviewFileKind, ReviewMap, ReviewMapAnalysis,
    ReviewMapFile, ReviewMapGroup, ReviewMapIdentity, ReviewMapStatus, ReviewMapTotals,
};

#[test]
fn enriched_update_preserves_selection_expansion_filter_and_reviewed_paths() {
    let mut controller = ReviewMapController::new(exact_map());
    controller.apply(ReviewMapAction::Move(2));
    controller.apply(ReviewMapAction::Expand);
    controller.apply(ReviewMapAction::Move(-1));
    let selected = controller.selected_id().unwrap().to_owned();
    controller.apply(ReviewMapAction::SetFilter("billing".into()));
    controller.mark_reviewed("src/billing/proration.ts");

    controller.replace_map(enriched_map_same_head()).unwrap();

    assert_eq!(controller.selected_id(), Some(selected.as_str()));
    assert!(controller.is_reviewed("src/billing/proration.ts"));
    assert_eq!(controller.filter(), "billing");
    assert_eq!(controller.reviewed_percent(), 50);
    controller.apply(ReviewMapAction::SetFilter(String::new()));
    assert!(controller.snapshot().rows.iter().any(|row| matches!(
        row,
        ReviewMapRow::Group {
            id,
            expanded: true,
            ..
        } if id == "group:head:tests"
    )));
}

#[test]
fn collapse_navigation_and_open_file_use_stable_ids() {
    let mut controller = ReviewMapController::new(exact_map());
    assert!(matches!(
        controller.snapshot().rows[0],
        ReviewMapRow::Group { expanded: true, .. }
    ));
    controller.apply(ReviewMapAction::Collapse);
    assert_eq!(controller.snapshot().rows.len(), 2);
    controller.apply(ReviewMapAction::Move(1));
    controller.apply(ReviewMapAction::Expand);
    controller.apply(ReviewMapAction::Move(1));

    assert_eq!(
        controller.apply(ReviewMapAction::OpenSelected),
        ReviewMapEffect::OpenFile {
            file_id: "file:head:tests/proration.test.ts".into()
        }
    );
}

#[test]
fn filtering_is_case_insensitive_and_reveals_matching_files() {
    let mut controller = ReviewMapController::new(exact_map());
    controller.apply(ReviewMapAction::SetFilter("PRORATION.TEST".into()));
    let snapshot = controller.snapshot();

    assert_eq!(snapshot.rows.len(), 2);
    assert!(matches!(
        &snapshot.rows[1],
        ReviewMapRow::File { path, .. } if path == "tests/proration.test.ts"
    ));
}

#[test]
fn different_revision_is_rejected_without_losing_current_state() {
    let mut controller = ReviewMapController::new(exact_map());
    controller.mark_reviewed("src/billing/proration.ts");
    let mut replacement = enriched_map_same_head();
    replacement.identity.head_sha = "new-head".into();

    assert_eq!(
        controller.replace_map(replacement),
        Err(ReplaceError::DifferentRevision)
    );
    assert_eq!(controller.map().identity.head_sha, "head");
    assert_eq!(controller.reviewed_percent(), 50);
}

#[test]
fn failures_are_dismissible_and_retry_is_an_effect() {
    let mut controller = ReviewMapController::new(exact_map());
    controller.set_failure(
        ramo_core::review_map::ReviewMapFailureCode::ServerUnreachable,
        "Laptop unavailable",
    );
    assert!(controller.snapshot().failure.is_some());
    assert_eq!(
        controller.apply(ReviewMapAction::Retry),
        ReviewMapEffect::Retry
    );
    controller.apply(ReviewMapAction::DismissFailure);
    assert!(controller.snapshot().failure.is_none());
}

fn exact_map() -> ReviewMap {
    ReviewMap {
        schema_version: 1,
        identity: identity(),
        status: ReviewMapStatus::Ready,
        totals: ReviewMapTotals {
            files: 2,
            additions: 12,
            deletions: 3,
            authored: 1,
            tests: 1,
            ..ReviewMapTotals::default()
        },
        groups: vec![
            group(
                "group:head:src",
                "src/billing/",
                ReviewFileKind::Authored,
                &["file:head:src/billing/proration.ts"],
                false,
            ),
            group(
                "group:head:tests",
                "Tests",
                ReviewFileKind::Test,
                &["file:head:tests/proration.test.ts"],
                true,
            ),
        ],
        files: vec![
            file(
                "file:head:src/billing/proration.ts",
                "src/billing/proration.ts",
                ReviewFileKind::Authored,
                10,
                3,
            ),
            file(
                "file:head:tests/proration.test.ts",
                "tests/proration.test.ts",
                ReviewFileKind::Test,
                2,
                0,
            ),
        ],
        analysis: None,
    }
}

fn enriched_map_same_head() -> ReviewMap {
    let mut map = exact_map();
    map.status = ReviewMapStatus::Enriched;
    map.groups[0].insight = Some(GroupInsight {
        summary: "Updates billing proration.".into(),
        risk: Some("Check invoice totals.".into()),
        review_priority: 1,
    });
    map.files[0].insight = Some(FileInsight {
        summary: "Validates the billing period.".into(),
        risk: None,
    });
    map.files[0].recommended_order = Some(1);
    map.analysis = Some(ReviewMapAnalysis {
        model: "qwen3:8b".into(),
        prompt_version: 1,
        completed_at: "2026-07-29T12:00:00Z".into(),
    });
    map
}

fn identity() -> ReviewMapIdentity {
    ReviewMapIdentity {
        repository: "Mondrio-App/mondrio-platform".into(),
        pull_request: 1914,
        base_sha: "base".into(),
        head_sha: "head".into(),
    }
}

fn group(
    id: &str,
    label: &str,
    kind: ReviewFileKind,
    file_ids: &[&str],
    collapsed_by_default: bool,
) -> ReviewMapGroup {
    ReviewMapGroup {
        id: id.into(),
        label: label.into(),
        kind,
        file_ids: file_ids.iter().map(|id| (*id).into()).collect(),
        additions: if kind == ReviewFileKind::Test { 2 } else { 10 },
        deletions: if kind == ReviewFileKind::Test { 0 } else { 3 },
        collapsed_by_default,
        insight: None,
    }
}

fn file(
    id: &str,
    path: &str,
    kind: ReviewFileKind,
    additions: usize,
    deletions: usize,
) -> ReviewMapFile {
    ReviewMapFile {
        id: id.into(),
        path: path.into(),
        previous_path: None,
        status: "modified".into(),
        additions,
        deletions,
        kind,
        owner: None,
        coverage: PatchCoverage::Full,
        insight: None,
        recommended_order: None,
    }
}
