use ramo_core::review_map::{
    EnrichmentCoverage, EnrichmentExactGroup, EnrichmentInputFile, EnrichmentRequest,
    PatchCoverage, ReviewFileKind,
};

pub const OLLAMA_CONTEXT_TOKENS: usize = 32_768;
pub const OLLAMA_OUTPUT_TOKENS: usize = 6_144;
pub const OLLAMA_SAFETY_TOKENS: usize = 2_048;
pub const MAX_PROMPT_TOKENS: usize =
    OLLAMA_CONTEXT_TOKENS - OLLAMA_OUTPUT_TOKENS - OLLAMA_SAFETY_TOKENS;

pub const fn estimate_tokens(bytes: usize) -> usize {
    bytes.div_ceil(3)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisBudget {
    pub max_prompt_tokens: usize,
    pub max_files_per_batch: usize,
}

impl Default for AnalysisBudget {
    fn default() -> Self {
        Self {
            max_prompt_tokens: MAX_PROMPT_TOKENS,
            // Per-call latency tracks file count, not prompt size: every file costs its own
            // summary and risk sentence. A request can sit well inside the token budget and still
            // carry enough files to run the model past its timeout.
            max_files_per_batch: 16,
        }
    }
}

pub fn budget_batches<F>(
    request: &EnrichmentRequest,
    budget: &AnalysisBudget,
    estimate_prompt: F,
) -> Vec<EnrichmentRequest>
where
    F: Fn(&EnrichmentRequest) -> usize,
{
    let mut files = request.files.clone();
    files.sort_by_key(|file| file_priority(file.kind));
    let max_files = budget.max_files_per_batch.max(1);
    let mut batches = Vec::<EnrichmentRequest>::new();
    let mut current = Vec::new();

    for mut file in files {
        prepare_patch(&mut file);
        let candidate = build_batch_with_extra(request, &current, file.clone());
        if !current.is_empty()
            && (current.len() >= max_files
                || estimate_prompt(&candidate) > budget.max_prompt_tokens)
        {
            batches.push(build_batch(request, std::mem::take(&mut current)));
        }

        if current.is_empty() {
            file = truncate_to_prompt_budget(
                request,
                file,
                budget.max_prompt_tokens,
                &estimate_prompt,
            );
        }
        current.push(file);
    }

    if !current.is_empty() {
        batches.push(build_batch(request, current));
    }
    if batches.is_empty() {
        batches.push(build_batch(request, Vec::new()));
    }
    batches
}

fn build_batch_with_extra(
    request: &EnrichmentRequest,
    current: &[EnrichmentInputFile],
    extra: EnrichmentInputFile,
) -> EnrichmentRequest {
    let mut files = current.to_vec();
    files.push(extra);
    build_batch(request, files)
}

fn prepare_patch(file: &mut EnrichmentInputFile) {
    if file.kind == ReviewFileKind::Generated {
        file.patch = None;
        file.coverage = PatchCoverage::MetadataOnly;
    } else if file.coverage == PatchCoverage::Binary || file.patch.is_none() {
        file.patch = None;
    }
}

fn truncate_to_prompt_budget<F>(
    request: &EnrichmentRequest,
    mut file: EnrichmentInputFile,
    max_prompt_tokens: usize,
    estimate_prompt: &F,
) -> EnrichmentInputFile
where
    F: Fn(&EnrichmentRequest) -> usize,
{
    let Some(original) = file.patch.clone() else {
        return file;
    };
    if estimate_prompt(&build_batch(request, vec![file.clone()])) <= max_prompt_tokens {
        return file;
    }

    let mut low = 0usize;
    let mut high = original.len();
    while low < high {
        let requested = low + (high - low).div_ceil(2);
        let boundary = floor_char_boundary(&original, requested);
        let mut candidate = file.clone();
        candidate.patch = Some(original[..boundary].to_owned());
        candidate.coverage = PatchCoverage::Truncated;
        if estimate_prompt(&build_batch(request, vec![candidate])) <= max_prompt_tokens {
            low = requested;
        } else {
            high = requested - 1;
        }
    }
    let boundary = floor_char_boundary(&original, low);
    file.patch = Some(original[..boundary].to_owned());
    file.coverage = PatchCoverage::Truncated;
    file
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

#[cfg(test)]
mod tests {
    use super::*;
    use ramo_core::review_map::ReviewMapIdentity;

    fn request_with_files(count: usize) -> EnrichmentRequest {
        EnrichmentRequest {
            schema_version: 1,
            identity: ReviewMapIdentity {
                repository: "owner/name".into(),
                pull_request: 2410,
                base_sha: "base".into(),
                head_sha: "head".into(),
            },
            groups: Vec::new(),
            files: (0..count)
                .map(|index| EnrichmentInputFile {
                    path: format!("frontend/src/module{index}.ts"),
                    kind: ReviewFileKind::Authored,
                    additions: 20,
                    deletions: 3,
                    coverage: PatchCoverage::Full,
                    patch: Some("@@ -1 +1 @@\n-before\n+after\n".into()),
                })
                .collect(),
            coverage: EnrichmentCoverage::default(),
        }
    }

    #[test]
    fn a_request_inside_the_token_budget_still_splits_on_file_count() {
        // PR #2410 was 21 small frontend files at roughly 15k estimated tokens — comfortably under
        // the token budget, so it became a single request and timed the model out. The per-call
        // cost tracks file count, because every file needs its own summary and risk sentence.
        let request = request_with_files(21);
        let batches = budget_batches(&request, &AnalysisBudget::default(), |_| 15_000);

        assert!(
            batches.len() > 1,
            "21 files became {} batch(es)",
            batches.len()
        );
        let total: usize = batches.iter().map(|batch| batch.files.len()).sum();
        assert_eq!(total, 21, "every file must still be analysed");
        assert!(
            batches
                .iter()
                .all(|batch| batch.files.len() <= AnalysisBudget::default().max_files_per_batch),
            "a batch exceeded the file cap"
        );
    }
}
