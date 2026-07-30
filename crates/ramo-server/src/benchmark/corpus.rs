use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ramo_core::review_map::{REVIEW_MAP_CLASSIFIER_VERSION, REVIEW_MAP_SCHEMA_VERSION};

use crate::ReviewMapFailure;
use crate::analysis::AnalysisBudget;
use crate::ollama::PROMPT_VERSION;

use super::{benchmark_io, invalid};

pub const CANDIDATE_MODELS: [&str; 3] = ["qwen3:8b", "qwen3-coder:30b", "qwen2.5-coder:7b"];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkManifest {
    pub repository_path: PathBuf,
    pub repository: String,
    pub pull_requests: Vec<u64>,
    pub candidates: Vec<String>,
    #[serde(rename = "analysis_contract_version")]
    pub prompt_version: u32,
    pub schema_version: u16,
    pub classifier_version: u32,
    pub budget: BenchmarkBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkBudget {
    pub max_prompt_tokens: usize,
    pub max_files_per_batch: usize,
}

impl From<AnalysisBudget> for BenchmarkBudget {
    fn from(value: AnalysisBudget) -> Self {
        Self {
            max_prompt_tokens: value.max_prompt_tokens,
            max_files_per_batch: value.max_files_per_batch,
        }
    }
}

impl From<BenchmarkBudget> for AnalysisBudget {
    fn from(value: BenchmarkBudget) -> Self {
        Self {
            max_prompt_tokens: value.max_prompt_tokens,
            max_files_per_batch: value.max_files_per_batch,
        }
    }
}

impl BenchmarkManifest {
    pub fn new(
        repository_path: PathBuf,
        repository: String,
        pull_requests: Vec<u64>,
        candidates: Vec<String>,
    ) -> Result<Self, ReviewMapFailure> {
        if repository.trim().is_empty() {
            return Err(invalid("The benchmark repository identity cannot be empty"));
        }
        if !(6..=10).contains(&pull_requests.len()) {
            return Err(invalid(
                "A benchmark corpus must contain 6 to 10 pull requests",
            ));
        }
        if pull_requests.contains(&0)
            || pull_requests.iter().copied().collect::<HashSet<_>>().len() != pull_requests.len()
        {
            return Err(invalid(
                "Benchmark pull request numbers must be positive and distinct",
            ));
        }
        if candidates.is_empty()
            || candidates
                .iter()
                .any(|candidate| candidate.trim().is_empty())
            || candidates.iter().collect::<HashSet<_>>().len() != candidates.len()
        {
            return Err(invalid(
                "Benchmark candidates must be non-empty and distinct",
            ));
        }
        Ok(Self {
            repository_path,
            repository,
            pull_requests,
            candidates,
            prompt_version: PROMPT_VERSION,
            schema_version: REVIEW_MAP_SCHEMA_VERSION,
            classifier_version: REVIEW_MAP_CLASSIFIER_VERSION,
            budget: AnalysisBudget::default().into(),
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), ReviewMapFailure> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|error| benchmark_io("Could not serialize benchmark manifest", error))?;
        write_private(path, &json)
    }

    pub fn load(path: &Path) -> Result<Self, ReviewMapFailure> {
        let bytes = std::fs::read(path)
            .map_err(|error| benchmark_io("Could not read benchmark manifest", error))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| benchmark_io("Could not parse benchmark manifest", error))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkCase {
    pub pull_request: u64,
}

impl BenchmarkCase {
    pub fn new(pull_request: u64) -> Self {
        Self { pull_request }
    }
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<(), ReviewMapFailure> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Benchmark artifact path has no parent directory"))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| benchmark_io("Could not create benchmark directory", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| benchmark_io("Could not protect benchmark directory", error))?;
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true).mode(0o600);
        use std::io::Write;
        let mut file = options
            .open(path)
            .map_err(|error| benchmark_io("Could not create benchmark artifact", error))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| benchmark_io("Could not write benchmark artifact", error))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| benchmark_io("Could not protect benchmark artifact", error))?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, bytes)
        .map_err(|error| benchmark_io("Could not write benchmark artifact", error))?;
    Ok(())
}
