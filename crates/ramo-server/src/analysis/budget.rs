use ramo_core::review_map::{
    EnrichmentCoverage, EnrichmentExactGroup, EnrichmentInputFile, EnrichmentRequest,
    PatchCoverage, ReviewFileKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBudget {
    pub max_patch_bytes: usize,
    pub max_file_patch_bytes: usize,
    pub max_files_per_batch: usize,
}

impl Default for AnalysisBudget {
    fn default() -> Self {
        Self {
            max_patch_bytes: 96 * 1024,
            max_file_patch_bytes: 24 * 1024,
            max_files_per_batch: 24,
        }
    }
}

pub fn budget_batches(
    request: &EnrichmentRequest,
    budget: &AnalysisBudget,
) -> Vec<EnrichmentRequest> {
    let mut files = request.files.clone();
    files.sort_by_key(|file| file_priority(file.kind));
    let max_files = budget.max_files_per_batch.max(1);
    let mut batches = Vec::<Vec<EnrichmentInputFile>>::new();
    let mut current = Vec::new();
    let mut current_bytes = 0usize;

    for mut file in files {
        prepare_patch(&mut file, budget.max_file_patch_bytes);
        let patch_bytes = file.patch.as_ref().map_or(0, String::len);
        if !current.is_empty()
            && (current.len() >= max_files
                || current_bytes.saturating_add(patch_bytes) > budget.max_patch_bytes)
        {
            batches.push(std::mem::take(&mut current));
            current_bytes = 0;
        }
        if patch_bytes > budget.max_patch_bytes {
            truncate_patch(&mut file, budget.max_patch_bytes);
        }
        current_bytes += file.patch.as_ref().map_or(0, String::len);
        current.push(file);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    if batches.is_empty() {
        batches.push(Vec::new());
    }

    batches
        .into_iter()
        .map(|files| build_batch(request, files))
        .collect()
}

fn prepare_patch(file: &mut EnrichmentInputFile, limit: usize) {
    if file.kind == ReviewFileKind::Generated {
        file.patch = None;
        file.coverage = PatchCoverage::MetadataOnly;
    } else if file.coverage == PatchCoverage::Binary || file.patch.is_none() {
        file.patch = None;
    } else {
        truncate_patch(file, limit);
    }
}

fn truncate_patch(file: &mut EnrichmentInputFile, limit: usize) {
    let Some(patch) = &mut file.patch else {
        return;
    };
    if patch.len() <= limit {
        return;
    }
    let boundary = floor_char_boundary(patch, limit);
    patch.truncate(boundary);
    file.coverage = PatchCoverage::Truncated;
}

fn floor_char_boundary(value: &str, requested: usize) -> usize {
    let mut boundary = requested.min(value.len());
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

fn build_batch(request: &EnrichmentRequest, files: Vec<EnrichmentInputFile>) -> EnrichmentRequest {
    let paths = files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<std::collections::HashSet<_>>();
    let groups = request
        .groups
        .iter()
        .filter_map(|group| {
            let group_paths = group
                .paths
                .iter()
                .filter(|path| paths.contains(path.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            (!group_paths.is_empty()).then(|| EnrichmentExactGroup {
                id: group.id.clone(),
                label: group.label.clone(),
                kind: group.kind,
                paths: group_paths,
            })
        })
        .collect();
    let coverage = coverage_for(&files);
    EnrichmentRequest {
        schema_version: request.schema_version,
        identity: request.identity.clone(),
        groups,
        files,
        coverage,
    }
}

fn coverage_for(files: &[EnrichmentInputFile]) -> EnrichmentCoverage {
    let mut coverage = EnrichmentCoverage::default();
    for file in files {
        match file.coverage {
            PatchCoverage::Full => coverage.analyzed_paths.push(file.path.clone()),
            PatchCoverage::Truncated => coverage.truncated_paths.push(file.path.clone()),
            PatchCoverage::MetadataOnly => coverage.metadata_only_paths.push(file.path.clone()),
            PatchCoverage::Binary => coverage.binary_paths.push(file.path.clone()),
        }
    }
    coverage
}

fn file_priority(kind: ReviewFileKind) -> u8 {
    match kind {
        ReviewFileKind::Migration => 0,
        ReviewFileKind::Authored => 1,
        ReviewFileKind::Documentation => 2,
        ReviewFileKind::Other => 3,
        ReviewFileKind::Test => 4,
        ReviewFileKind::Generated => 5,
    }
}
