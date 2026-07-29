use std::collections::{BTreeMap, HashMap, HashSet};

use super::{
    ClassifierConfig, CodeOwners, PatchCoverage, REVIEW_MAP_SCHEMA_VERSION, ReviewFileKind,
    ReviewMap, ReviewMapFile, ReviewMapGroup, ReviewMapInput, ReviewMapStatus, ReviewMapTotals,
    classify_path,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReviewMapError {
    #[error("invalid changed path: {0}")]
    InvalidPath(String),
    #[error("changed path appears more than once: {0}")]
    DuplicatePath(String),
    #[error("invalid CODEOWNERS: {0}")]
    InvalidCodeOwners(String),
    #[error("file belongs to more than one group: {0}")]
    DuplicateMembership(String),
    #[error("file is missing from all groups: {0}")]
    MissingMembership(String),
    #[error("review map totals do not match its files")]
    TotalsMismatch,
}

pub fn build_review_map(
    input: &ReviewMapInput,
    config: &ClassifierConfig,
) -> Result<ReviewMap, ReviewMapError> {
    let owners = input
        .codeowners
        .as_deref()
        .map(CodeOwners::parse)
        .transpose()
        .map_err(|error| ReviewMapError::InvalidCodeOwners(error.to_string()))?;
    let mut seen_paths = HashSet::new();
    let mut files = Vec::with_capacity(input.files.len());
    let mut grouped: BTreeMap<String, (String, ReviewFileKind, Vec<String>)> = BTreeMap::new();

    for source in &input.files {
        let path = normalize_path(&source.path)?;
        if !seen_paths.insert(path.clone()) {
            return Err(ReviewMapError::DuplicatePath(path));
        }
        let previous_path = source
            .previous_path
            .as_deref()
            .map(normalize_path)
            .transpose()?;
        let kind = classify_path(&path, source.patch.as_deref(), config);
        let id = match &previous_path {
            Some(previous) => format!("file:{}:{previous}->{path}", input.identity.head_sha),
            None => format!("file:{}:{path}", input.identity.head_sha),
        };
        let (group_key, group_label) = group_for(&path, kind);
        grouped
            .entry(group_key)
            .or_insert_with(|| (group_label, kind, Vec::new()))
            .2
            .push(id.clone());
        files.push(ReviewMapFile {
            id,
            path: path.clone(),
            previous_path,
            status: source.status.clone(),
            additions: source.additions,
            deletions: source.deletions,
            kind,
            owner: owners
                .as_ref()
                .and_then(|owners| owners.owner_for(&path))
                .map(str::to_owned),
            coverage: coverage(source.binary, source.patch.as_deref()),
            insight: None,
            recommended_order: None,
        });
    }

    let by_id = files
        .iter()
        .map(|file| (file.id.as_str(), file))
        .collect::<HashMap<_, _>>();
    let groups = grouped
        .into_iter()
        .map(|(key, (label, kind, file_ids))| {
            let (additions, deletions) = file_ids.iter().fold((0, 0), |totals, id| {
                let file = by_id[id.as_str()];
                (totals.0 + file.additions, totals.1 + file.deletions)
            });
            ReviewMapGroup {
                id: format!("group:{}:{key}", input.identity.head_sha),
                label,
                kind,
                file_ids,
                additions,
                deletions,
                collapsed_by_default: matches!(
                    kind,
                    ReviewFileKind::Test | ReviewFileKind::Generated
                ),
                insight: None,
            }
        })
        .collect::<Vec<_>>();
    let totals = totals(&files);
    let map = ReviewMap {
        schema_version: REVIEW_MAP_SCHEMA_VERSION,
        identity: input.identity.clone(),
        status: ReviewMapStatus::Ready,
        totals,
        groups,
        files,
        analysis: None,
    };
    validate_exact_map(&map)?;
    Ok(map)
}

pub fn validate_exact_map(map: &ReviewMap) -> Result<(), ReviewMapError> {
    let ids = map
        .files
        .iter()
        .map(|file| file.id.as_str())
        .collect::<HashSet<_>>();
    let mut memberships = HashSet::new();
    for group in &map.groups {
        let mut additions = 0;
        let mut deletions = 0;
        for id in &group.file_ids {
            if !memberships.insert(id.as_str()) {
                return Err(ReviewMapError::DuplicateMembership(id.clone()));
            }
            let Some(file) = map.files.iter().find(|file| file.id == *id) else {
                return Err(ReviewMapError::TotalsMismatch);
            };
            additions += file.additions;
            deletions += file.deletions;
        }
        if additions != group.additions || deletions != group.deletions {
            return Err(ReviewMapError::TotalsMismatch);
        }
    }
    if let Some(missing) = ids.difference(&memberships).next() {
        return Err(ReviewMapError::MissingMembership((*missing).to_owned()));
    }
    if memberships.iter().any(|id| !ids.contains(id)) || totals(&map.files) != map.totals {
        return Err(ReviewMapError::TotalsMismatch);
    }
    Ok(())
}

fn normalize_path(path: &str) -> Result<String, ReviewMapError> {
    let normalized = path.replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains('\0')
        || normalized
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(ReviewMapError::InvalidPath(path.to_owned()));
    }
    Ok(normalized)
}

fn group_for(path: &str, kind: ReviewFileKind) -> (String, String) {
    match kind {
        ReviewFileKind::Test => ("test".into(), "Tests".into()),
        ReviewFileKind::Generated => ("generated".into(), "Generated".into()),
        ReviewFileKind::Migration => ("migration".into(), "Migrations".into()),
        ReviewFileKind::Documentation => ("documentation".into(), "Documentation".into()),
        ReviewFileKind::Authored | ReviewFileKind::Other => {
            let directories = path.split('/').collect::<Vec<_>>();
            if directories.len() == 1 {
                ("authored:root".into(), "Root files".into())
            } else {
                let take = directories.len().saturating_sub(1).min(2);
                let prefix = directories[..take].join("/");
                (format!("authored:{prefix}"), format!("{prefix}/"))
            }
        }
    }
}

fn coverage(binary: bool, patch: Option<&str>) -> PatchCoverage {
    if binary {
        PatchCoverage::Binary
    } else if patch.is_none() {
        PatchCoverage::MetadataOnly
    } else if patch.is_some_and(|patch| patch.ends_with("... diff truncated ...")) {
        PatchCoverage::Truncated
    } else {
        PatchCoverage::Full
    }
}

fn totals(files: &[ReviewMapFile]) -> ReviewMapTotals {
    let mut totals = ReviewMapTotals {
        files: files.len(),
        ..ReviewMapTotals::default()
    };
    for file in files {
        totals.additions += file.additions;
        totals.deletions += file.deletions;
        match file.kind {
            ReviewFileKind::Authored | ReviewFileKind::Other => totals.authored += 1,
            ReviewFileKind::Test => totals.tests += 1,
            ReviewFileKind::Generated => totals.generated += 1,
            ReviewFileKind::Migration => totals.migrations += 1,
            ReviewFileKind::Documentation => totals.documentation += 1,
        }
    }
    totals
}
