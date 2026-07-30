use std::collections::{BTreeMap, BTreeSet};

use super::{EnrichmentProposal, EnrichmentRequest, ReviewFileKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EnrichmentQualityIssue {
    MissingRequiredInsight,
    DuplicateInsight,
    GenericSummary,
    PathOnlySummary,
    BareRisk,
    UnsupportedClaim,
    InvalidText,
}

impl EnrichmentQualityIssue {
    pub const fn category(self) -> &'static str {
        match self {
            Self::MissingRequiredInsight => "missing_required_insight",
            Self::DuplicateInsight => "duplicate_insight",
            Self::GenericSummary => "generic_summary",
            Self::PathOnlySummary => "path_only_summary",
            Self::BareRisk => "bare_risk",
            Self::UnsupportedClaim => "unsupported_claim",
            Self::InvalidText => "invalid_text",
        }
    }
}

pub fn validate_enrichment_quality(
    request: &EnrichmentRequest,
    proposal: &EnrichmentProposal,
) -> Result<(), Vec<EnrichmentQualityIssue>> {
    let mut issues = BTreeSet::new();
    let required_paths: BTreeSet<_> = request
        .files
        .iter()
        .filter(|file| !matches!(file.kind, ReviewFileKind::Test | ReviewFileKind::Generated))
        .map(|file| file.path.as_str())
        .collect();
    let mut insight_counts = BTreeMap::<&str, usize>::new();

    for insight in &proposal.files {
        *insight_counts.entry(&insight.path).or_default() += 1;
        inspect_summary(&insight.summary, &insight.path, &mut issues);
        inspect_risk(insight.risk.as_deref(), &mut issues);
    }

    if required_paths
        .iter()
        .any(|path| insight_counts.get(path).copied().unwrap_or_default() == 0)
    {
        issues.insert(EnrichmentQualityIssue::MissingRequiredInsight);
    }
    if insight_counts.values().any(|count| *count > 1) {
        issues.insert(EnrichmentQualityIssue::DuplicateInsight);
    }

    for group in &proposal.groups {
        inspect_summary(&group.summary, &group.label, &mut issues);
        inspect_risk(group.risk.as_deref(), &mut issues);
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues.into_iter().collect())
    }
}

fn inspect_summary(summary: &str, reference: &str, issues: &mut BTreeSet<EnrichmentQualityIssue>) {
    inspect_text(summary, issues);

    let normalized = summary.trim().to_ascii_lowercase();
    if [
        "this file contains",
        "this group contains",
        "additional files from the deterministic diff structure",
    ]
    .iter()
    .any(|prefix| normalized.starts_with(prefix))
    {
        issues.insert(EnrichmentQualityIssue::GenericSummary);
    }

    if is_path_only_summary(summary, reference) {
        issues.insert(EnrichmentQualityIssue::PathOnlySummary);
    }
}

fn inspect_risk(risk: Option<&str>, issues: &mut BTreeSet<EnrichmentQualityIssue>) {
    let Some(risk) = risk else {
        return;
    };
    inspect_text(risk, issues);

    let normalized = risk
        .split_whitespace()
        .map(|word| {
            word.trim_matches(|character: char| !character.is_ascii_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if matches!(
        normalized.as_slice(),
        [severity] if matches!(severity.as_str(), "low" | "medium" | "high")
    ) || matches!(
        normalized.as_slice(),
        [severity, noun]
            if matches!(severity.as_str(), "low" | "medium" | "high") && noun == "risk"
    ) {
        issues.insert(EnrichmentQualityIssue::BareRisk);
    }
}

fn inspect_text(text: &str, issues: &mut BTreeSet<EnrichmentQualityIssue>) {
    if text
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        issues.insert(EnrichmentQualityIssue::InvalidText);
    }

    let normalized = text.to_ascii_lowercase();
    if [
        "tests pass",
        "tests are passing",
        "full coverage",
        "safe for production",
        "production safe",
        "deployment is safe",
    ]
    .iter()
    .any(|claim| normalized.contains(claim))
    {
        issues.insert(EnrichmentQualityIssue::UnsupportedClaim);
    }
}

fn is_path_only_summary(summary: &str, reference: &str) -> bool {
    let reference_tokens = tokens(reference).collect::<BTreeSet<_>>();
    let stop_words = [
        "a",
        "an",
        "and",
        "are",
        "contains",
        "directory",
        "file",
        "files",
        "folder",
        "for",
        "from",
        "group",
        "in",
        "is",
        "of",
        "the",
        "this",
        "to",
        "with",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();

    let meaningful_tokens = tokens(summary)
        .filter(|token| !stop_words.contains(token.as_str()))
        .collect::<BTreeSet<_>>();

    !meaningful_tokens.is_empty()
        && meaningful_tokens
            .iter()
            .all(|token| reference_tokens.contains(token))
}

fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_lowercase)
}
