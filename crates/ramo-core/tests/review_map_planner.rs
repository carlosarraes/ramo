use ramo_core::review_map::{
    ClassifierConfig, CodeOwners, ReviewFileKind, ReviewMapIdentity, ReviewMapInput,
    ReviewMapInputFile, build_review_map, classify_path, validate_exact_map,
};

#[test]
fn classifies_language_specific_tests_and_generated_files() {
    let config = ClassifierConfig::default();
    assert_eq!(
        classify_path("backend/test_invoice.py", None, &config),
        ReviewFileKind::Test
    );
    assert_eq!(
        classify_path("web/src/cart.spec.ts", None, &config),
        ReviewFileKind::Test
    );
    assert_eq!(
        classify_path("api/client.generated.ts", None, &config),
        ReviewFileKind::Generated
    );
    assert_eq!(
        classify_path("migrations/0231_credit.sql", None, &config),
        ReviewFileKind::Migration
    );
    assert_eq!(
        classify_path("docs/billing.md", None, &config),
        ReviewFileKind::Documentation
    );
    assert_eq!(
        classify_path("src/billing/invoice.ts", None, &config),
        ReviewFileKind::Authored
    );
}

#[test]
fn generated_marker_is_bounded_and_case_insensitive() {
    let marker = format!("// DO NOT EDIT: generated file\n{}", "x".repeat(4096));
    assert_eq!(
        classify_path("src/client.ts", Some(&marker), &ClassifierConfig::default()),
        ReviewFileKind::Generated
    );
}

#[test]
fn codeowners_uses_the_last_matching_rule() {
    let owners =
        CodeOwners::parse("* @platform\n/backend/ @backend\n/backend/billing/ @billing @finance\n")
            .unwrap();
    assert_eq!(
        owners.owner_for("backend/billing/invoice.py"),
        Some("@billing")
    );
    assert_eq!(owners.owner_for("frontend/app.ts"), Some("@platform"));
}

#[test]
fn codeowners_rejects_rules_without_an_owner() {
    let error = CodeOwners::parse("/backend/\n").unwrap_err();
    assert_eq!(error.line, 1);
    assert!(error.message.contains("owner"));
}

#[test]
fn planner_preserves_every_file_once_and_exact_totals() {
    let input = fixture_input(vec![
        file("src/billing/proration.ts", 96, 12),
        file("src/billing/invoice.ts", 74, 20),
        file("src/billing/__tests__/proration.test.ts", 180, 0),
        file("migrations/0231_credit.sql", 22, 0),
        file("web/api/client.generated.ts", 9_411, 8_203),
    ]);
    let map = build_review_map(&input, &ClassifierConfig::default()).unwrap();
    assert_eq!(
        (map.totals.files, map.totals.additions, map.totals.deletions),
        (5, 9_783, 8_235)
    );
    assert_eq!(
        map.files
            .iter()
            .map(|file| &file.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        5
    );
    assert!(
        map.groups
            .iter()
            .find(|group| group.kind == ReviewFileKind::Test)
            .unwrap()
            .collapsed_by_default
    );
    assert!(
        map.groups
            .iter()
            .find(|group| group.kind == ReviewFileKind::Generated)
            .unwrap()
            .collapsed_by_default
    );
    assert!(validate_exact_map(&map).is_ok());
}

fn fixture_input(files: Vec<ReviewMapInputFile>) -> ReviewMapInput {
    ReviewMapInput {
        identity: ReviewMapIdentity {
            repository: "owner/repo".into(),
            pull_request: 7,
            base_sha: "base".into(),
            head_sha: "head".into(),
        },
        files,
        codeowners: Some("/src/billing/ @billing\n".into()),
    }
}

fn file(path: &str, additions: usize, deletions: usize) -> ReviewMapInputFile {
    ReviewMapInputFile {
        path: path.into(),
        previous_path: None,
        status: "modified".into(),
        additions,
        deletions,
        patch: Some("@@ -1 +1 @@\n-old\n+new\n".into()),
        binary: false,
    }
}
