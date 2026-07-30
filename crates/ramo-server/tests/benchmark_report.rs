use ramo_server::benchmark::{
    BenchmarkDecision, CandidateAggregate, sanitized_report, select_default,
};
use ramo_server::config::{SelectedModelConfig, load_selected_model, save_selected_model};

#[test]
fn invalid_or_unreliable_models_cannot_win_on_quality_alone() {
    let decision = select_default(&[
        candidate("quality", 4.9, 0.80, 20_000),
        candidate("reliable", 4.5, 1.00, 35_000),
    ])
    .unwrap();

    assert_eq!(decision.model, "reliable");
}

#[test]
fn quality_wins_after_hard_gates_then_latency_breaks_a_tie() {
    let quality_candidates = judged_pair(
        candidate("fast", 4.1, 1.0, 10_000),
        candidate("useful", 4.8, 1.0, 30_000),
        3,
    );
    let quality = select_default(&quality_candidates).unwrap();
    let latency_candidates = judged_pair(
        candidate("slow", 4.8, 1.0, 30_000),
        candidate("fast", 4.8, 1.0, 10_000),
        3,
    );
    let latency = select_default(&latency_candidates).unwrap();

    assert_eq!(quality.model, "useful");
    assert_eq!(latency.model, "fast");
}

#[test]
fn selection_refuses_zero_scores_and_incomplete_pair_coverage() {
    let mut zero = candidate("reliable", 0.0, 1.0, 20_000);
    zero.blind_judgment_count = 0;
    assert!(select_default(&[zero.clone()]).is_err());

    zero.mean_blind_usefulness = 4.5;
    zero.blind_judgment_count = 2;
    zero.pairwise_case_counts.insert("peer".into(), 2);
    let mut peer = candidate("peer", 4.4, 1.0, 25_000);
    peer.blind_judgment_count = 2;
    peer.pairwise_case_counts.insert("reliable".into(), 2);
    assert!(select_default(&[zero, peer]).is_err());
}

#[test]
fn selection_requires_usefulness_at_least_three_point_five() {
    let candidate = candidate("valid-but-weak", 3.49, 1.0, 10_000);
    assert!(select_default(&[candidate]).is_err());
}

#[test]
fn sanitized_report_excludes_private_identity_and_review_text() {
    let candidates = vec![candidate("qwen3:8b", 4.5, 1.0, 20_000)];
    let decision = BenchmarkDecision {
        model: "qwen3:8b".into(),
        model_digest: "sha256:model".into(),
        rationale: "Highest blind usefulness after all hard gates.".into(),
    };

    let report = sanitized_report(
        "run-1",
        &decision,
        &candidates,
        &["migration_present".into(), "generated_heavy".into()],
        "Linux x86_64; 32 GiB RAM",
    );

    assert!(report.contains("qwen3:8b"));
    for private in [
        "Mondrio",
        "mondrio-platform",
        "github.com/owner/repo/pull",
        "src/",
        "backend/",
        "private summary sentinel",
    ] {
        assert!(!report.contains(private));
    }
}

#[test]
fn selected_model_configuration_is_atomic_and_round_trips() {
    let directory = tempfile::tempdir().unwrap();
    let selected = SelectedModelConfig {
        selected_model: "qwen3:8b".into(),
        model_digest: "sha256:model".into(),
        prompt_version: 1,
        benchmark_run_id: "run-1".into(),
    };

    save_selected_model(directory.path(), &selected).unwrap();

    assert_eq!(
        load_selected_model(directory.path()).unwrap(),
        Some(selected)
    );
    assert!(!directory.path().join("selected-model.json.tmp").exists());
}

fn candidate(
    model: &str,
    usefulness: f64,
    completion_ratio: f64,
    median_wall_time_ms: u64,
) -> CandidateAggregate {
    CandidateAggregate {
        model: model.into(),
        model_digest: format!("digest:{model}"),
        mean_blind_usefulness: usefulness,
        completion_ratio,
        schema_validity_ratio: completion_ratio,
        semantic_validity_ratio: completion_ratio,
        unknown_reference_count: 0,
        median_wall_time_ms,
        peak_rss_bytes: None,
        blind_judgment_count: 3,
        pairwise_case_counts: std::collections::BTreeMap::new(),
        pairwise_wins: 0,
        pairwise_losses: 0,
        pairwise_ties: 0,
    }
}

fn judged_pair(
    mut left: CandidateAggregate,
    mut right: CandidateAggregate,
    cases: usize,
) -> [CandidateAggregate; 2] {
    left.blind_judgment_count = cases;
    right.blind_judgment_count = cases;
    left.pairwise_case_counts.insert(right.model.clone(), cases);
    right.pairwise_case_counts.insert(left.model.clone(), cases);
    [left, right]
}
