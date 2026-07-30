use ramo_core::review_map::{
    EnrichmentCoverage, EnrichmentExactGroup, EnrichmentInputFile, EnrichmentProposal,
    EnrichmentQualityIssue, EnrichmentRequest, PatchCoverage, ProposedFileInsight, ProposedGroup,
    REVIEW_MAP_SCHEMA_VERSION, ReviewFileKind, ReviewMapIdentity, validate_enrichment_quality,
};

#[test]
fn rejects_missing_authored_insight_generic_group_and_bare_risk() {
    let request = request_with_authored("src/billing/invoice.rs");
    let proposal = proposal(
        Vec::new(),
        "This group contains billing files.",
        Some("low"),
    );

    let issues = validate_enrichment_quality(&request, &proposal).unwrap_err();

    assert!(issues.contains(&EnrichmentQualityIssue::MissingRequiredInsight));
    assert!(issues.contains(&EnrichmentQualityIssue::GenericSummary));
    assert!(issues.contains(&EnrichmentQualityIssue::BareRisk));
}

#[test]
fn rejects_generic_file_summary_and_unsupported_verdicts() {
    let request = request_with_authored("src/billing/invoice.rs");
    let proposal = proposal(
        vec![ProposedFileInsight {
            path: "src/billing/invoice.rs".into(),
            summary:
                "This file contains invoice changes; all tests are passing with full coverage."
                    .into(),
            risk: None,
        }],
        "Coordinates invoice rounding and persistence changes.",
        None,
    );

    let issues = validate_enrichment_quality(&request, &proposal).unwrap_err();

    assert!(issues.contains(&EnrichmentQualityIssue::GenericSummary));
    assert!(issues.contains(&EnrichmentQualityIssue::UnsupportedClaim));
}

#[test]
fn rejects_path_only_summary_and_duplicate_insight() {
    let request = request_with_authored("src/billing/invoice.rs");
    let duplicate = ProposedFileInsight {
        path: "src/billing/invoice.rs".into(),
        summary: "Billing invoice file.".into(),
        risk: None,
    };
    let proposal = proposal(
        vec![duplicate.clone(), duplicate],
        "Coordinates invoice calculation and persistence changes.",
        None,
    );

    let issues = validate_enrichment_quality(&request, &proposal).unwrap_err();

    assert!(issues.contains(&EnrichmentQualityIssue::PathOnlySummary));
    assert!(issues.contains(&EnrichmentQualityIssue::DuplicateInsight));
}

#[test]
fn rejects_control_characters_and_normalizer_fallback_copy() {
    let request = request_with_authored("src/billing/invoice.rs");
    let proposal = proposal(
        vec![ProposedFileInsight {
            path: "src/billing/invoice.rs".into(),
            summary: "Changes invoice\u{0007} rounding behavior.".into(),
            risk: None,
        }],
        "Additional files from the deterministic diff structure.",
        None,
    );

    let issues = validate_enrichment_quality(&request, &proposal).unwrap_err();

    assert!(issues.contains(&EnrichmentQualityIssue::InvalidText));
    assert!(issues.contains(&EnrichmentQualityIssue::GenericSummary));
}

#[test]
fn accepts_behavior_specific_authored_insight_without_speculative_risk() {
    let request = request_with_authored("src/billing/invoice.rs");
    let proposal = proposal(
        vec![ProposedFileInsight {
            path: "src/billing/invoice.rs".into(),
            summary: "Changes rounding from line-level amounts to the final invoice total.".into(),
            risk: None,
        }],
        "Coordinates invoice calculation and persistence changes.",
        Some("Review stored totals when legacy invoices are recalculated."),
    );

    assert_eq!(validate_enrichment_quality(&request, &proposal), Ok(()));
}

fn request_with_authored(path: &str) -> EnrichmentRequest {
    EnrichmentRequest {
        schema_version: REVIEW_MAP_SCHEMA_VERSION,
        identity: ReviewMapIdentity {
            repository: "owner/repo".into(),
            pull_request: 7,
            base_sha: "base".into(),
            head_sha: "head".into(),
        },
        groups: vec![EnrichmentExactGroup {
            id: "authored".into(),
            label: "src/billing/".into(),
            kind: ReviewFileKind::Authored,
            paths: vec![path.into()],
        }],
        files: vec![EnrichmentInputFile {
            path: path.into(),
            kind: ReviewFileKind::Authored,
            additions: 8,
            deletions: 3,
            coverage: PatchCoverage::Full,
            patch: Some("@@ -1 +1 @@\n-old\n+new\n".into()),
        }],
        coverage: EnrichmentCoverage {
            analyzed_paths: vec![path.into()],
            ..EnrichmentCoverage::default()
        },
    }
}

fn proposal(
    files: Vec<ProposedFileInsight>,
    group_summary: &str,
    group_risk: Option<&str>,
) -> EnrichmentProposal {
    EnrichmentProposal {
        groups: vec![ProposedGroup {
            label: "Billing behavior".into(),
            summary: group_summary.into(),
            risk: group_risk.map(str::to_owned),
            review_priority: 1,
            paths: vec!["src/billing/invoice.rs".into()],
        }],
        files,
        review_order: vec!["src/billing/invoice.rs".into()],
        coverage: EnrichmentCoverage::default(),
    }
}
