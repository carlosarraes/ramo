use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use ramo_core::review_map::{EnrichmentProposal, ReviewMap};

use crate::ReviewMapFailure;

use super::{BenchmarkRun, benchmark_io, invalid};

#[derive(Debug, Clone)]
pub struct BlindCandidateOutput {
    pub pull_request: u64,
    pub candidate_id: String,
    pub model: String,
    pub exact_map: ReviewMap,
    pub proposal: EnrichmentProposal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlindChoice {
    CandidateA,
    CandidateB,
    Tie,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DimensionScores {
    pub grouping: u8,
    pub accuracy: u8,
    pub order: u8,
    pub risks: u8,
    pub noise: u8,
}

impl DimensionScores {
    pub fn all(score: u8) -> Self {
        Self {
            grouping: score,
            accuracy: score,
            order: score,
            risks: score,
            noise: score,
        }
    }

    fn valid(self) -> bool {
        [
            self.grouping,
            self.accuracy,
            self.order,
            self.risks,
            self.noise,
        ]
        .into_iter()
        .all(|score| (1..=5).contains(&score))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlindJudgment {
    pub candidate_a: DimensionScores,
    pub candidate_b: DimensionScores,
    pub overall: BlindChoice,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BlindPayload {
    pub comparison_id: String,
    pub exact_map: ReviewMap,
    pub candidate_a: BlindCandidateView,
    pub candidate_b: BlindCandidateView,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BlindCandidateView {
    pub label: &'static str,
    pub proposal: EnrichmentProposal,
}

pub struct BlindSession {
    seed: u64,
    comparisons: Vec<BlindComparison>,
    outputs: HashMap<(u64, String), BlindCandidateOutput>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BlindComparison {
    id: String,
    pull_request: u64,
    candidate_a_id: String,
    candidate_b_id: String,
    judgment: Option<BlindJudgment>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedBlindSession {
    seed: u64,
    comparisons: Vec<BlindComparison>,
}

impl BlindSession {
    pub fn from_outputs(
        outputs: Vec<BlindCandidateOutput>,
        seed: u64,
    ) -> Result<Self, ReviewMapFailure> {
        let mut by_case = BTreeMap::<u64, Vec<String>>::new();
        let mut indexed = HashMap::new();
        for output in outputs {
            by_case
                .entry(output.pull_request)
                .or_default()
                .push(output.candidate_id.clone());
            if indexed
                .insert((output.pull_request, output.candidate_id.clone()), output)
                .is_some()
            {
                return Err(invalid(
                    "Blind benchmark output contains a duplicate candidate",
                ));
            }
        }
        let mut comparisons = Vec::new();
        for (case_index, (pull_request, candidate_ids)) in by_case.iter_mut().enumerate() {
            candidate_ids.sort();
            candidate_ids.dedup();
            if candidate_ids.len() < 2 {
                continue;
            }
            let mut pair_index = 0usize;
            for left in 0..candidate_ids.len() {
                for right in (left + 1)..candidate_ids.len() {
                    let swap = (case_index + pair_index + seed as usize) % 2 == 0;
                    let (candidate_a_id, candidate_b_id) = if swap {
                        (candidate_ids[right].clone(), candidate_ids[left].clone())
                    } else {
                        (candidate_ids[left].clone(), candidate_ids[right].clone())
                    };
                    comparisons.push(BlindComparison {
                        id: format!("comparison-{}", comparisons.len() + 1),
                        pull_request: *pull_request,
                        candidate_a_id,
                        candidate_b_id,
                        judgment: None,
                    });
                    pair_index += 1;
                }
            }
        }
        if comparisons.is_empty() {
            return Err(invalid(
                "Blind judging requires at least two valid candidates for one case",
            ));
        }
        Ok(Self {
            seed,
            comparisons,
            outputs: indexed,
        })
    }

    pub fn from_run_directory(
        run_directory: &Path,
        run: &BenchmarkRun,
    ) -> Result<Self, ReviewMapFailure> {
        let mut outputs = Vec::new();
        for measurement in &run.measurements {
            if measurement.completion != super::CompletionState::Completed {
                continue;
            }
            if outputs.iter().any(|output: &BlindCandidateOutput| {
                output.pull_request == measurement.case.pull_request
                    && output.candidate_id == measurement.candidate_id
            }) {
                continue;
            }
            let path = run_directory
                .join("private")
                .join(measurement.case.pull_request.to_string())
                .join(format!("{}.json", measurement.candidate_id));
            let bytes = std::fs::read(path)
                .map_err(|error| benchmark_io("Could not read private benchmark output", error))?;
            let private = serde_json::from_slice::<StoredCandidateOutput>(&bytes)
                .map_err(|error| benchmark_io("Could not parse private benchmark output", error))?;
            outputs.push(BlindCandidateOutput {
                pull_request: measurement.case.pull_request,
                candidate_id: measurement.candidate_id.clone(),
                model: measurement.model.clone(),
                exact_map: private.exact_map,
                proposal: private.proposal,
            });
        }
        Self::from_outputs(outputs, run.seed)
    }

    pub fn open(run_directory: &Path, run: &BenchmarkRun) -> Result<Self, ReviewMapFailure> {
        let fresh = Self::from_run_directory(run_directory, run)?;
        let path = run_directory.join("judgments.json");
        if path.is_file() {
            Self::load(&path, fresh.outputs.into_values().collect())
        } else {
            Ok(fresh)
        }
    }

    pub fn next(&self) -> Option<BlindPayload> {
        let comparison = self
            .comparisons
            .iter()
            .find(|comparison| comparison.judgment.is_none())?;
        let a = &self.outputs[&(comparison.pull_request, comparison.candidate_a_id.clone())];
        let b = &self.outputs[&(comparison.pull_request, comparison.candidate_b_id.clone())];
        Some(BlindPayload {
            comparison_id: comparison.id.clone(),
            exact_map: a.exact_map.clone(),
            candidate_a: BlindCandidateView {
                label: "Candidate A",
                proposal: a.proposal.clone(),
            },
            candidate_b: BlindCandidateView {
                label: "Candidate B",
                proposal: b.proposal.clone(),
            },
        })
    }

    pub fn submit(&mut self, judgment: BlindJudgment) -> Result<(), ReviewMapFailure> {
        if !judgment.candidate_a.valid() || !judgment.candidate_b.valid() {
            return Err(invalid("Every blind score must be between 1 and 5"));
        }
        let comparison = self
            .comparisons
            .iter_mut()
            .find(|comparison| comparison.judgment.is_none())
            .ok_or_else(|| invalid("Blind judging is already complete"))?;
        comparison.judgment = Some(judgment);
        Ok(())
    }

    pub fn completed(&self) -> usize {
        self.comparisons
            .iter()
            .filter(|comparison| comparison.judgment.is_some())
            .count()
    }

    pub fn total(&self) -> usize {
        self.comparisons.len()
    }

    pub fn pairing_signature(&self) -> Vec<(u64, String, String)> {
        self.comparisons
            .iter()
            .map(|comparison| {
                (
                    comparison.pull_request,
                    comparison.candidate_a_id.clone(),
                    comparison.candidate_b_id.clone(),
                )
            })
            .collect()
    }

    pub fn judgments(&self) -> Vec<(String, String, BlindJudgment)> {
        self.comparisons
            .iter()
            .filter_map(|comparison| {
                comparison.judgment.clone().map(|judgment| {
                    (
                        comparison.candidate_a_id.clone(),
                        comparison.candidate_b_id.clone(),
                        judgment,
                    )
                })
            })
            .collect()
    }

    pub fn judgments_with_cases(&self) -> Vec<(u64, String, String, BlindJudgment)> {
        self.comparisons
            .iter()
            .filter_map(|comparison| {
                comparison.judgment.clone().map(|judgment| {
                    (
                        comparison.pull_request,
                        comparison.candidate_a_id.clone(),
                        comparison.candidate_b_id.clone(),
                        judgment,
                    )
                })
            })
            .collect()
    }

    pub fn reveal(&self) -> BTreeMap<String, String> {
        self.outputs
            .values()
            .map(|output| (output.candidate_id.clone(), output.model.clone()))
            .collect()
    }

    pub fn category_labels(&self) -> Vec<String> {
        let mut labels = Vec::new();
        let maps = self
            .outputs
            .values()
            .map(|output| &output.exact_map)
            .collect::<Vec<_>>();
        if maps.iter().any(|map| map.totals.migrations > 0) {
            labels.push("migration_present".into());
        }
        if maps
            .iter()
            .any(|map| map.totals.generated.saturating_mul(2) >= map.totals.files.max(1))
        {
            labels.push("generated_heavy".into());
        }
        if maps
            .iter()
            .any(|map| map.totals.additions.saturating_add(map.totals.deletions) >= 1_000)
        {
            labels.push("large_change".into());
        }
        if maps.iter().any(|map| map.totals.tests > 0) {
            labels.push("tests_present".into());
        }
        if labels.is_empty() {
            labels.push("authored_changes".into());
        }
        labels
    }

    pub fn save(&self, path: &Path) -> Result<(), ReviewMapFailure> {
        let bytes = serde_json::to_vec_pretty(&SavedBlindSession {
            seed: self.seed,
            comparisons: self.comparisons.clone(),
        })
        .map_err(|error| benchmark_io("Could not serialize blind judgments", error))?;
        super::corpus::write_private(path, &bytes)
    }

    pub fn load(path: &Path, outputs: Vec<BlindCandidateOutput>) -> Result<Self, ReviewMapFailure> {
        let bytes = std::fs::read(path)
            .map_err(|error| benchmark_io("Could not read blind judgments", error))?;
        let saved = serde_json::from_slice::<SavedBlindSession>(&bytes)
            .map_err(|error| benchmark_io("Could not parse blind judgments", error))?;
        let generated = Self::from_outputs(outputs, saved.seed)?;
        if generated.pairing_signature()
            != saved
                .comparisons
                .iter()
                .map(|comparison| {
                    (
                        comparison.pull_request,
                        comparison.candidate_a_id.clone(),
                        comparison.candidate_b_id.clone(),
                    )
                })
                .collect::<Vec<_>>()
        {
            return Err(invalid(
                "Blind judgment file does not match the private benchmark outputs",
            ));
        }
        Ok(Self {
            seed: saved.seed,
            comparisons: saved.comparisons,
            outputs: generated.outputs,
        })
    }
}

#[derive(serde::Deserialize)]
struct StoredCandidateOutput {
    #[allow(dead_code)]
    input: ramo_core::review_map::ReviewMapInput,
    exact_map: ReviewMap,
    proposal: EnrichmentProposal,
}
