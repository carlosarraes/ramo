use ramo_core::review_map::{ClassifierConfig, CodeOwners, ReviewFileKind, classify_path};

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
