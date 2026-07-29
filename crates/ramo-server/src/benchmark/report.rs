use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ReviewMapFailure;

use super::{
    BenchmarkRun, BlindChoice, BlindSession, CandidateMeasurement, CompletionState,
    DimensionScores, invalid,
};

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateAggregate {
    pub model: String,
    pub model_digest: String,
    pub mean_blind_usefulness: f64,
    pub completion_ratio: f64,
    pub schema_validity_ratio: f64,
    pub semantic_validity_ratio: f64,
    pub unknown_reference_count: usize,
    pub median_wall_time_ms: u64,
    pub peak_rss_bytes: Option<u64>,
    pub pairwise_wins: usize,
    pub pairwise_losses: usize,
    pub pairwise_ties: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkDecision {
    pub model: String,
    pub model_digest: String,
    pub rationale: String,
}

pub fn select_default(
    candidates: &[CandidateAggregate],
) -> Result<BenchmarkDecision, ReviewMapFailure> {
    let mut eligible = candidates
        .iter()
        .filter(|candidate| {
            candidate.completion_ratio == 1.0
                && candidate.schema_validity_ratio == 1.0
                && candidate.semantic_validity_ratio == 1.0
                && candidate.unknown_reference_count == 0
        })
        .collect::<Vec<_>>();
    eligible.sort_by(|left, right| compare_candidates(left, right));
    let winner = eligible
        .first()
        .ok_or_else(|| invalid("No benchmark candidate passed every hard gate"))?;
    Ok(BenchmarkDecision {
        model: winner.model.clone(),
        model_digest: winner.model_digest.clone(),
        rationale: format!(
            "Passed every validity gate; mean blind usefulness {:.2}, median wall time {} ms",
            winner.mean_blind_usefulness, winner.median_wall_time_ms
        ),
    })
}

pub fn aggregate_candidates(run: &BenchmarkRun, session: &BlindSession) -> Vec<CandidateAggregate> {
    let mut latest = BTreeMap::<(String, u64), &CandidateMeasurement>::new();
    for measurement in &run.measurements {
        latest.insert(
            (measurement.model.clone(), measurement.case.pull_request),
            measurement,
        );
    }
    let case_count = run
        .measurements
        .iter()
        .map(|measurement| measurement.case.pull_request)
        .collect::<BTreeSet<_>>()
        .len()
        .max(1);
    let candidate_models = session.reveal();
    let candidate_ids = candidate_models
        .iter()
        .map(|(candidate, model)| (model.clone(), candidate.clone()))
        .collect::<HashMap<_, _>>();
    let mut blind_scores = HashMap::<String, Vec<f64>>::new();
    let mut outcomes = HashMap::<String, (usize, usize, usize)>::new();
    for (candidate_a, candidate_b, judgment) in session.judgments() {
        blind_scores
            .entry(candidate_a.clone())
            .or_default()
            .push(mean_score(judgment.candidate_a));
        blind_scores
            .entry(candidate_b.clone())
            .or_default()
            .push(mean_score(judgment.candidate_b));
        match judgment.overall {
            BlindChoice::CandidateA => {
                outcomes.entry(candidate_a).or_default().0 += 1;
                outcomes.entry(candidate_b).or_default().1 += 1;
            }
            BlindChoice::CandidateB => {
                outcomes.entry(candidate_b).or_default().0 += 1;
                outcomes.entry(candidate_a).or_default().1 += 1;
            }
            BlindChoice::Tie => {
                outcomes.entry(candidate_a).or_default().2 += 1;
                outcomes.entry(candidate_b).or_default().2 += 1;
            }
        }
    }

    run.candidates
        .iter()
        .map(|model| {
            let measurements = latest
                .iter()
                .filter_map(|((candidate, _), measurement)| {
                    (candidate == model).then_some(*measurement)
                })
                .collect::<Vec<_>>();
            let ratio = |predicate: fn(&CandidateMeasurement) -> bool| {
                measurements
                    .iter()
                    .filter(|measurement| predicate(measurement))
                    .count() as f64
                    / case_count as f64
            };
            let mut wall_times = measurements
                .iter()
                .map(|measurement| measurement.wall_time_ms)
                .collect::<Vec<_>>();
            wall_times.sort_unstable();
            let candidate_id = candidate_ids.get(model);
            let scores = candidate_id
                .and_then(|candidate| blind_scores.get(candidate))
                .cloned()
                .unwrap_or_default();
            let (wins, losses, ties) = candidate_id
                .and_then(|candidate| outcomes.get(candidate))
                .copied()
                .unwrap_or_default();
            CandidateAggregate {
                model: model.clone(),
                model_digest: measurements
                    .iter()
                    .rev()
                    .find(|measurement| !measurement.model_digest.is_empty())
                    .map_or_else(String::new, |measurement| measurement.model_digest.clone()),
                mean_blind_usefulness: if scores.is_empty() {
                    0.0
                } else {
                    scores.iter().sum::<f64>() / scores.len() as f64
                },
                completion_ratio: ratio(|measurement| {
                    measurement.completion == CompletionState::Completed
                }),
                schema_validity_ratio: ratio(|measurement| measurement.schema_valid),
                semantic_validity_ratio: ratio(|measurement| measurement.semantic_valid),
                unknown_reference_count: measurements
                    .iter()
                    .map(|measurement| measurement.unknown_reference_count)
                    .sum(),
                median_wall_time_ms: median(&wall_times),
                peak_rss_bytes: measurements
                    .iter()
                    .filter_map(|measurement| measurement.peak_rss_bytes)
                    .max(),
                pairwise_wins: wins,
                pairwise_losses: losses,
                pairwise_ties: ties,
            }
        })
        .collect()
}

pub fn sanitized_report(
    run_id: &str,
    decision: &BenchmarkDecision,
    candidates: &[CandidateAggregate],
    category_labels: &[String],
    hardware_summary: &str,
) -> String {
    let mut report = format!(
        "# Ramo local model benchmark\n\nRun: {run_id}  \nHardware: {hardware_summary}  \nCorpus categories: {}\n\n## Decision\n\n**{}** — {}\n\n## Aggregate metrics\n\n| Model | Digest | Complete | Schema | Semantic | Unknown refs | Blind usefulness | Median ms | Peak RSS | W/L/T |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
        category_labels.join(", "),
        decision.model,
        decision.rationale
    );
    for candidate in candidates {
        report.push_str(&format!(
            "| {} | {} | {:.0}% | {:.0}% | {:.0}% | {} | {:.2} | {} | {} | {}/{}/{} |\n",
            candidate.model,
            candidate.model_digest,
            candidate.completion_ratio * 100.0,
            candidate.schema_validity_ratio * 100.0,
            candidate.semantic_validity_ratio * 100.0,
            candidate.unknown_reference_count,
            candidate.mean_blind_usefulness,
            candidate.median_wall_time_ms,
            candidate
                .peak_rss_bytes
                .map_or_else(|| "n/a".into(), |bytes| bytes.to_string()),
            candidate.pairwise_wins,
            candidate.pairwise_losses,
            candidate.pairwise_ties
        ));
    }
    report
}

fn compare_candidates(left: &CandidateAggregate, right: &CandidateAggregate) -> Ordering {
    right
        .mean_blind_usefulness
        .total_cmp(&left.mean_blind_usefulness)
        .then_with(|| left.median_wall_time_ms.cmp(&right.median_wall_time_ms))
        .then_with(|| match (left.peak_rss_bytes, right.peak_rss_bytes) {
            (Some(left), Some(right)) => left.cmp(&right),
            _ => Ordering::Equal,
        })
        .then_with(|| left.model.cmp(&right.model))
}

fn mean_score(scores: DimensionScores) -> f64 {
    f64::from(scores.grouping + scores.accuracy + scores.order + scores.risks + scores.noise) / 5.0
}

fn median(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        values[middle - 1].saturating_add(values[middle]) / 2
    } else {
        values[middle]
    }
}
