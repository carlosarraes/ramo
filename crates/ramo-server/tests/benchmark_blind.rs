use ramo_core::review_map::{
    EnrichmentCoverage, EnrichmentProposal, ProposedGroup, ReviewFileKind, ReviewMap,
    ReviewMapGroup, ReviewMapIdentity, ReviewMapStatus, ReviewMapTotals,
};
use ramo_server::benchmark::{
    BlindCandidateOutput, BlindChoice, BlindJudgment, BlindSession, DimensionScores,
};

#[test]
fn judging_payload_never_exposes_model_names() {
    let session = BlindSession::from_outputs(outputs(), 42).unwrap();

    let payload = serde_json::to_string(&session.next().unwrap()).unwrap();

    assert!(!payload.contains("qwen"));
    assert!(payload.contains("Candidate A"));
    assert!(payload.contains("Candidate B"));
}

#[test]
fn deterministic_pairings_balance_candidate_sides() {
    let session = BlindSession::from_outputs(outputs(), 42).unwrap();
    let first = session.pairing_signature();
    let repeated = BlindSession::from_outputs(outputs(), 42)
        .unwrap()
        .pairing_signature();

    assert_eq!(first, repeated);
    for candidate in ["candidate-1", "candidate-2", "candidate-3"] {
        let a = first
            .iter()
            .filter(|(_, left, _)| left == candidate)
            .count();
        let b = first
            .iter()
            .filter(|(_, _, right)| right == candidate)
            .count();
        assert_eq!(a, b);
    }
}

#[test]
fn scores_must_be_complete_and_within_the_five_point_scale() {
    let mut session = BlindSession::from_outputs(outputs(), 42).unwrap();
    let invalid = BlindJudgment {
        candidate_a: DimensionScores {
            grouping: 0,
            accuracy: 5,
            order: 5,
            risks: 5,
            noise: 5,
        },
        candidate_b: DimensionScores::all(5),
        overall: BlindChoice::Tie,
    };

    assert!(session.submit(invalid).is_err());
    assert!(session.submit(valid_judgment()).is_ok());
    assert_eq!(session.completed(), 1);
}

#[test]
fn saved_judging_session_resumes_without_model_identity() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("judgments.json");
    let mut session = BlindSession::from_outputs(outputs(), 42).unwrap();
    session.submit(valid_judgment()).unwrap();

    session.save(&path).unwrap();
    let resumed = BlindSession::load(&path, outputs()).unwrap();
    let json = std::fs::read_to_string(path).unwrap();

    assert_eq!(resumed.completed(), 1);
    assert!(!json.contains("qwen"));
}

fn valid_judgment() -> BlindJudgment {
    BlindJudgment {
        candidate_a: DimensionScores::all(4),
        candidate_b: DimensionScores::all(5),
        overall: BlindChoice::CandidateB,
    }
}

fn outputs() -> Vec<BlindCandidateOutput> {
    let models = [
        ("candidate-1", "qwen3:8b"),
        ("candidate-2", "qwen3-coder:30b"),
        ("candidate-3", "qwen2.5-coder:7b"),
    ];
    (1..=6)
        .flat_map(|pull_request| {
            models.map(move |(candidate_id, model)| BlindCandidateOutput {
                pull_request,
                candidate_id: candidate_id.into(),
                model: model.into(),
                exact_map: exact_map(pull_request),
                proposal: proposal(candidate_id),
            })
        })
        .collect()
}

fn exact_map(pull_request: u64) -> ReviewMap {
    ReviewMap {
        schema_version: 1,
        identity: ReviewMapIdentity {
            repository: "owner/repository".into(),
            pull_request,
            base_sha: "base".into(),
            head_sha: format!("head-{pull_request}"),
        },
        status: ReviewMapStatus::Ready,
        totals: ReviewMapTotals {
            files: 1,
            additions: 2,
            deletions: 1,
            authored: 1,
            ..ReviewMapTotals::default()
        },
        groups: vec![ReviewMapGroup {
            id: "exact".into(),
            label: "Authored".into(),
            kind: ReviewFileKind::Authored,
            file_ids: vec!["file".into()],
            additions: 2,
            deletions: 1,
            collapsed_by_default: false,
            insight: None,
        }],
        files: Vec::new(),
        analysis: None,
    }
}

fn proposal(candidate_id: &str) -> EnrichmentProposal {
    EnrichmentProposal {
        groups: vec![ProposedGroup {
            label: format!("Group {candidate_id}"),
            summary: "Useful summary".into(),
            risk: Some("Bounded risk".into()),
            review_priority: 1,
            paths: Vec::new(),
        }],
        files: Vec::new(),
        review_order: Vec::new(),
        coverage: EnrichmentCoverage::default(),
    }
}
