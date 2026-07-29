use ramo_core::review_map::{
    ClassifierConfig, EnrichmentCoverage, EnrichmentError, EnrichmentProposal, ProposedFileInsight,
    ProposedGroup, ReviewMapAnalysis, ReviewMapIdentity, ReviewMapInput, ReviewMapInputFile,
    build_review_map, merge_enrichment, validate_enrichment,
};

#[test]
fn enrichment_rejects_unknown_paths_and_duplicate_membership() {
    let map = exact_map();
    let unknown = proposal(vec![group("Core", &["src/missing.rs"])]);
    assert!(matches!(
        validate_enrichment(&map, &unknown),
        Err(EnrichmentError::UnknownFile(_))
    ));

    let duplicate = proposal(vec![
        group("One", &["src/lib.rs", "migrations/1.sql"]),
        group("Two", &["src/lib.rs"]),
    ]);
    assert!(matches!(
        validate_enrichment(&map, &duplicate),
        Err(EnrichmentError::DuplicateFile(_))
    ));
}

#[test]
fn enrichment_cannot_reclassify_tests_or_generated_files() {
    let map = exact_map();
    let proposed = proposal(vec![
        group("Core", &["src/lib.rs", "migrations/1.sql"]),
        group("Not tests", &["tests/lib_test.rs"]),
    ]);
    assert!(matches!(
        validate_enrichment(&map, &proposed),
        Err(EnrichmentError::FixedClassification(_))
    ));
}

#[test]
fn valid_merge_adds_interpretation_without_changing_exact_facts() {
    let map = exact_map();
    let exact = exact_facts(&map);
    let mut proposed = proposal(vec![group(
        "Core change",
        &["src/lib.rs", "migrations/1.sql"],
    )]);
    proposed.files.push(ProposedFileInsight {
        path: "src/lib.rs".into(),
        summary: "Introduces the billing behavior.".into(),
        risk: Some("Touches invoice calculation.".into()),
    });
    let merged = merge_enrichment(
        &map,
        &proposed,
        ReviewMapAnalysis {
            model: "qwen3:8b".into(),
            prompt_version: 1,
            completed_at: "2026-07-29T12:00:00Z".into(),
        },
    )
    .unwrap();

    assert_eq!(exact_facts(&merged), exact);
    assert_eq!(merged.totals, map.totals);
    assert_eq!(
        merged.groups[0].insight.as_ref().unwrap().summary,
        "Explains the core change."
    );
    assert_eq!(
        merged
            .files
            .iter()
            .find(|file| file.path == "src/lib.rs")
            .unwrap()
            .recommended_order,
        Some(1)
    );
}

#[test]
fn enrichment_rejects_missing_order_oversized_text_and_duplicate_coverage() {
    let map = exact_map();
    let mut missing_order = proposal(vec![group("Core", &["src/lib.rs", "migrations/1.sql"])]);
    missing_order.review_order.pop();
    assert!(matches!(
        validate_enrichment(&map, &missing_order),
        Err(EnrichmentError::MissingOrder(_))
    ));

    let mut oversized = proposal(vec![group("Core", &["src/lib.rs", "migrations/1.sql"])]);
    oversized.groups[0].label = "x".repeat(81);
    assert!(matches!(
        validate_enrichment(&map, &oversized),
        Err(EnrichmentError::InvalidText { .. })
    ));

    let mut duplicate_coverage = proposal(vec![group("Core", &["src/lib.rs", "migrations/1.sql"])]);
    duplicate_coverage
        .coverage
        .analyzed_paths
        .push("src/lib.rs".into());
    duplicate_coverage
        .coverage
        .truncated_paths
        .push("src/lib.rs".into());
    assert!(matches!(
        validate_enrichment(&map, &duplicate_coverage),
        Err(EnrichmentError::DuplicateCoverage(_))
    ));
}

fn exact_map() -> ramo_core::review_map::ReviewMap {
    build_review_map(
        &ReviewMapInput {
            identity: ReviewMapIdentity {
                repository: "owner/repo".into(),
                pull_request: 7,
                base_sha: "base".into(),
                head_sha: "head".into(),
            },
            files: vec![
                file("src/lib.rs"),
                file("migrations/1.sql"),
                file("tests/lib_test.rs"),
                file("src/client.generated.ts"),
            ],
            codeowners: None,
        },
        &ClassifierConfig::default(),
    )
    .unwrap()
}

fn file(path: &str) -> ReviewMapInputFile {
    ReviewMapInputFile {
        path: path.into(),
        previous_path: None,
        status: "modified".into(),
        additions: 3,
        deletions: 1,
        patch: Some("@@ -1 +1 @@\n-old\n+new\n".into()),
        binary: false,
    }
}

fn proposal(groups: Vec<ProposedGroup>) -> EnrichmentProposal {
    EnrichmentProposal {
        groups,
        files: Vec::new(),
        review_order: vec!["src/lib.rs".into(), "migrations/1.sql".into()],
        coverage: EnrichmentCoverage::default(),
    }
}

fn group(label: &str, paths: &[&str]) -> ProposedGroup {
    ProposedGroup {
        label: label.into(),
        summary: "Explains the core change.".into(),
        risk: None,
        review_priority: 1,
        paths: paths.iter().map(|path| (*path).into()).collect(),
    }
}

fn exact_facts(
    map: &ramo_core::review_map::ReviewMap,
) -> Vec<(
    String,
    String,
    ramo_core::review_map::ReviewFileKind,
    usize,
    usize,
)> {
    map.files
        .iter()
        .map(|file| {
            (
                file.path.clone(),
                file.status.clone(),
                file.kind,
                file.additions,
                file.deletions,
            )
        })
        .collect()
}
