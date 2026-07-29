use std::collections::{HashMap, HashSet};

use super::{
    GroupInsight, PatchCoverage, ReviewFileKind, ReviewMap, ReviewMapAnalysis, ReviewMapGroup,
    ReviewMapIdentity, ReviewMapStatus, validate_exact_map,
};

const MAX_LABEL_CHARS: usize = 80;
const MAX_SUMMARY_CHARS: usize = 400;
const MAX_RISK_CHARS: usize = 240;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnrichmentRequest {
    pub schema_version: u16,
    pub identity: ReviewMapIdentity,
    pub groups: Vec<EnrichmentExactGroup>,
    pub files: Vec<EnrichmentInputFile>,
    pub coverage: EnrichmentCoverage,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnrichmentExactGroup {
    pub id: String,
    pub label: String,
    pub kind: ReviewFileKind,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnrichmentInputFile {
    pub path: String,
    pub kind: ReviewFileKind,
    pub additions: usize,
    pub deletions: usize,
    pub coverage: PatchCoverage,
    pub patch: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnrichmentCoverage {
    #[serde(default)]
    pub analyzed_paths: Vec<String>,
    #[serde(default)]
    pub truncated_paths: Vec<String>,
    #[serde(default)]
    pub metadata_only_paths: Vec<String>,
    #[serde(default)]
    pub binary_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EnrichmentProposal {
    pub groups: Vec<ProposedGroup>,
    #[serde(default)]
    pub files: Vec<ProposedFileInsight>,
    pub review_order: Vec<String>,
    #[serde(default)]
    pub coverage: EnrichmentCoverage,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposedGroup {
    pub label: String,
    pub summary: String,
    pub risk: Option<String>,
    pub review_priority: usize,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProposedFileInsight {
    pub path: String,
    pub summary: String,
    pub risk: Option<String>,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum EnrichmentError {
    #[error("enrichment references an unknown file: {0}")]
    UnknownFile(String),
    #[error("enrichment assigns a file more than once: {0}")]
    DuplicateFile(String),
    #[error("enrichment cannot regroup a fixed classification: {0}")]
    FixedClassification(String),
    #[error("enrichment omits a reviewable file: {0}")]
    MissingFile(String),
    #[error("review order repeats a file: {0}")]
    DuplicateOrder(String),
    #[error("review order omits a reviewable file: {0}")]
    MissingOrder(String),
    #[error("{field} is empty or exceeds {maximum} characters")]
    InvalidText { field: &'static str, maximum: usize },
    #[error("enrichment coverage repeats a file: {0}")]
    DuplicateCoverage(String),
    #[error("merged enrichment violated the exact map: {0}")]
    InvalidMerge(String),
}

pub fn validate_enrichment(
    map: &ReviewMap,
    proposal: &EnrichmentProposal,
) -> Result<(), EnrichmentError> {
    let files = map
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.kind))
        .collect::<HashMap<_, _>>();
    let flexible = files
        .iter()
        .filter_map(|(path, kind)| (!is_fixed(*kind)).then_some(*path))
        .collect::<HashSet<_>>();
    let mut grouped = HashSet::new();
    for group in &proposal.groups {
        validate_text("group label", &group.label, MAX_LABEL_CHARS)?;
        validate_text("group summary", &group.summary, MAX_SUMMARY_CHARS)?;
        validate_optional_text("group risk", group.risk.as_deref(), MAX_RISK_CHARS)?;
        for path in &group.paths {
            let Some(kind) = files.get(path.as_str()) else {
                return Err(EnrichmentError::UnknownFile(path.clone()));
            };
            if is_fixed(*kind) {
                return Err(EnrichmentError::FixedClassification(path.clone()));
            }
            if !grouped.insert(path.as_str()) {
                return Err(EnrichmentError::DuplicateFile(path.clone()));
            }
        }
    }
    if let Some(path) = flexible.difference(&grouped).next() {
        return Err(EnrichmentError::MissingFile((*path).to_owned()));
    }

    let mut insight_paths = HashSet::new();
    for insight in &proposal.files {
        if !files.contains_key(insight.path.as_str()) {
            return Err(EnrichmentError::UnknownFile(insight.path.clone()));
        }
        if !insight_paths.insert(insight.path.as_str()) {
            return Err(EnrichmentError::DuplicateFile(insight.path.clone()));
        }
        validate_text("file summary", &insight.summary, MAX_SUMMARY_CHARS)?;
        validate_optional_text("file risk", insight.risk.as_deref(), MAX_RISK_CHARS)?;
    }

    let mut ordered = HashSet::new();
    for path in &proposal.review_order {
        if !files.contains_key(path.as_str()) {
            return Err(EnrichmentError::UnknownFile(path.clone()));
        }
        if !flexible.contains(path.as_str()) {
            return Err(EnrichmentError::FixedClassification(path.clone()));
        }
        if !ordered.insert(path.as_str()) {
            return Err(EnrichmentError::DuplicateOrder(path.clone()));
        }
    }
    if let Some(path) = flexible.difference(&ordered).next() {
        return Err(EnrichmentError::MissingOrder((*path).to_owned()));
    }
    validate_coverage(&files, &proposal.coverage)
}

pub fn merge_enrichment(
    map: &ReviewMap,
    proposal: &EnrichmentProposal,
    analysis: ReviewMapAnalysis,
) -> Result<ReviewMap, EnrichmentError> {
    validate_enrichment(map, proposal)?;
    let by_path = map
        .files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect::<HashMap<_, _>>();
    let mut groups = proposal
        .groups
        .iter()
        .enumerate()
        .map(|(index, proposed)| {
            let members = proposed
                .paths
                .iter()
                .map(|path| by_path[path.as_str()])
                .collect::<Vec<_>>();
            let kind = members
                .first()
                .map(|file| file.kind)
                .filter(|kind| members.iter().all(|file| file.kind == *kind))
                .unwrap_or(ReviewFileKind::Other);
            ReviewMapGroup {
                id: format!("group:{}:ai:{index}", map.identity.head_sha),
                label: proposed.label.trim().to_owned(),
                kind,
                file_ids: members.iter().map(|file| file.id.clone()).collect(),
                additions: members.iter().map(|file| file.additions).sum(),
                deletions: members.iter().map(|file| file.deletions).sum(),
                collapsed_by_default: false,
                insight: Some(GroupInsight {
                    summary: proposed.summary.trim().to_owned(),
                    risk: proposed.risk.as_deref().map(str::trim).map(str::to_owned),
                    review_priority: proposed.review_priority,
                }),
            }
        })
        .collect::<Vec<_>>();
    groups.extend(
        map.groups
            .iter()
            .filter(|group| is_fixed(group.kind))
            .cloned(),
    );

    let insights = proposal
        .files
        .iter()
        .map(|insight| (insight.path.as_str(), insight))
        .collect::<HashMap<_, _>>();
    let order = proposal
        .review_order
        .iter()
        .enumerate()
        .map(|(index, path)| (path.as_str(), index + 1))
        .collect::<HashMap<_, _>>();
    let mut merged = map.clone();
    merged.groups = groups;
    for file in &mut merged.files {
        let proposed = insights.get(file.path.as_str());
        let recommended_order = order.get(file.path.as_str()).copied();
        file.insight = proposed.map(|insight| super::FileInsight {
            summary: insight.summary.trim().to_owned(),
            risk: insight.risk.as_deref().map(str::trim).map(str::to_owned),
        });
        file.recommended_order = recommended_order;
    }
    merged.status = ReviewMapStatus::Enriched;
    merged.analysis = Some(analysis);
    validate_exact_map(&merged)
        .map_err(|error| EnrichmentError::InvalidMerge(error.to_string()))?;
    Ok(merged)
}

fn is_fixed(kind: ReviewFileKind) -> bool {
    matches!(kind, ReviewFileKind::Test | ReviewFileKind::Generated)
}

fn validate_text(field: &'static str, value: &str, maximum: usize) -> Result<(), EnrichmentError> {
    let length = value.trim().chars().count();
    if length == 0 || length > maximum {
        Err(EnrichmentError::InvalidText { field, maximum })
    } else {
        Ok(())
    }
}

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
    maximum: usize,
) -> Result<(), EnrichmentError> {
    value.map_or(Ok(()), |value| validate_text(field, value, maximum))
}

fn validate_coverage(
    files: &HashMap<&str, ReviewFileKind>,
    coverage: &EnrichmentCoverage,
) -> Result<(), EnrichmentError> {
    let mut seen = HashSet::new();
    for path in coverage
        .analyzed_paths
        .iter()
        .chain(&coverage.truncated_paths)
        .chain(&coverage.metadata_only_paths)
        .chain(&coverage.binary_paths)
    {
        if !files.contains_key(path.as_str()) {
            return Err(EnrichmentError::UnknownFile(path.clone()));
        }
        if !seen.insert(path.as_str()) {
            return Err(EnrichmentError::DuplicateCoverage(path.clone()));
        }
    }
    Ok(())
}
