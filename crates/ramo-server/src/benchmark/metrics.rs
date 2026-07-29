use std::path::Path;

use ramo_core::review_map::ReviewMapFailureCode;

use crate::ReviewMapFailure;

use super::corpus::write_private;
use super::{BenchmarkCase, BenchmarkManifest, benchmark_io};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionState {
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CandidateMeasurement {
    pub case: BenchmarkCase,
    pub candidate_id: String,
    pub model: String,
    pub model_digest: String,
    pub prompt_version: u32,
    pub request_digest: String,
    pub wall_time_ms: u64,
    pub ollama_total_duration_ns: u64,
    pub prompt_eval_count: u64,
    pub eval_count: u64,
    pub schema_valid: bool,
    pub semantic_valid: bool,
    pub repair_count: u8,
    pub unknown_reference_count: usize,
    pub peak_rss_bytes: Option<u64>,
    pub completion: CompletionState,
    #[serde(default)]
    pub failure_code: Option<ReviewMapFailureCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkRun {
    pub run_id: String,
    pub seed: u64,
    pub repository: String,
    pub candidates: Vec<String>,
    #[serde(rename = "analysis_contract_version")]
    pub prompt_version: u32,
    pub measurements: Vec<CandidateMeasurement>,
}

impl BenchmarkRun {
    pub fn new(run_id: String, manifest: &BenchmarkManifest, seed: u64) -> Self {
        Self {
            run_id,
            seed,
            repository: manifest.repository.clone(),
            candidates: manifest.candidates.clone(),
            prompt_version: manifest.prompt_version,
            measurements: Vec::new(),
        }
    }

    pub fn record(&mut self, measurement: CandidateMeasurement) {
        self.measurements.push(measurement);
    }

    pub fn is_completed(
        &self,
        pull_request: u64,
        model: &str,
        model_digest: &str,
        prompt_version: u32,
    ) -> bool {
        self.measurements.iter().any(|measurement| {
            measurement.case.pull_request == pull_request
                && measurement.model == model
                && measurement.model_digest == model_digest
                && measurement.prompt_version == prompt_version
                && measurement.completion == CompletionState::Completed
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), ReviewMapFailure> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| benchmark_io("Could not serialize benchmark run", error))?;
        write_private(path, &bytes)
    }

    pub fn load(path: &Path) -> Result<Self, ReviewMapFailure> {
        let bytes = std::fs::read(path)
            .map_err(|error| benchmark_io("Could not read benchmark run", error))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| benchmark_io("Could not parse benchmark run", error))
    }

    pub fn is_compatible_with(&self, manifest: &BenchmarkManifest) -> bool {
        self.repository == manifest.repository
            && self.candidates == manifest.candidates
            && self.prompt_version == manifest.prompt_version
    }

    pub fn append_measurement(
        path: &Path,
        measurement: &CandidateMeasurement,
    ) -> Result<(), ReviewMapFailure> {
        use std::io::Write;
        let parent = path
            .parent()
            .ok_or_else(|| super::invalid("Benchmark measurement path has no parent directory"))?;
        std::fs::create_dir_all(parent)
            .map_err(|error| benchmark_io("Could not create benchmark run directory", error))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(
                |error| benchmark_io("Could not protect benchmark run directory", error),
            )?;
            let mut options = std::fs::OpenOptions::new();
            options.create(true).append(true).mode(0o600);
            let mut file = options
                .open(path)
                .map_err(|error| benchmark_io("Could not open benchmark measurements", error))?;
            serde_json::to_writer(&mut file, measurement).map_err(|error| {
                benchmark_io("Could not serialize benchmark measurement", error)
            })?;
            file.write_all(b"\n")
                .and_then(|()| file.sync_all())
                .map_err(|error| benchmark_io("Could not append benchmark measurement", error))?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|error| benchmark_io("Could not protect benchmark measurements", error))?;
        }
        #[cfg(not(unix))]
        {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|error| benchmark_io("Could not open benchmark measurements", error))?;
            serde_json::to_writer(&mut file, measurement).map_err(|error| {
                benchmark_io("Could not serialize benchmark measurement", error)
            })?;
            file.write_all(b"\n")
                .map_err(|error| benchmark_io("Could not append benchmark measurement", error))?;
        }
        Ok(())
    }
}
