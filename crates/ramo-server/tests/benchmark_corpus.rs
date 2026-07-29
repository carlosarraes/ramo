use std::path::PathBuf;

use ramo_server::benchmark::{
    BenchmarkCase, BenchmarkManifest, BenchmarkRun, CandidateMeasurement, CompletionState,
};

fn candidates() -> Vec<String> {
    ["qwen3:8b", "qwen3-coder:30b", "qwen2.5-coder:7b"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
fn manifest_contains_identity_but_never_patch_content() {
    let manifest = BenchmarkManifest::new(
        PathBuf::from("/home/carraes/mondrio/mondrio-platform"),
        "Mondrio-App/mondrio-platform".into(),
        vec![291, 292, 293, 294, 295, 296],
        candidates(),
    )
    .unwrap();

    let json = serde_json::to_string(&manifest).unwrap();

    assert!(json.contains("mondrio-platform"));
    assert!(!json.contains("\"patch\":"));
    assert!(!json.contains("\"prompt\":"));
}

#[test]
fn manifest_rejects_invalid_corpus_shapes() {
    let make = |pull_requests| {
        BenchmarkManifest::new(
            PathBuf::from("/tmp/repository"),
            "owner/repository".into(),
            pull_requests,
            candidates(),
        )
    };

    assert!(make(vec![1, 2, 3, 4, 5]).is_err());
    assert!(make((1..=11).collect()).is_err());
    assert!(make(vec![1, 2, 3, 4, 5, 5]).is_err());
    assert!(make(vec![0, 1, 2, 3, 4, 5]).is_err());
}

#[test]
fn completed_resume_keys_skip_only_the_exact_candidate_revision() {
    let manifest = BenchmarkManifest::new(
        PathBuf::from("/tmp/repository"),
        "owner/repository".into(),
        vec![1, 2, 3, 4, 5, 6],
        candidates(),
    )
    .unwrap();
    let mut run = BenchmarkRun::new("run-1".into(), &manifest, 42);
    run.record(CandidateMeasurement {
        case: BenchmarkCase::new(1),
        candidate_id: "candidate-1".into(),
        model: "qwen3:8b".into(),
        model_digest: "digest-a".into(),
        prompt_version: manifest.prompt_version,
        request_digest: "request".into(),
        wall_time_ms: 10,
        ollama_total_duration_ns: 9,
        prompt_eval_count: 2,
        eval_count: 3,
        schema_valid: true,
        semantic_valid: true,
        repair_count: 0,
        unknown_reference_count: 0,
        peak_rss_bytes: None,
        completion: CompletionState::Completed,
        failure_code: None,
    });

    assert!(run.is_completed(1, "qwen3:8b", "digest-a", manifest.prompt_version));
    assert!(!run.is_completed(1, "qwen3:8b", "digest-b", manifest.prompt_version));
    assert!(!run.is_completed(1, "qwen3-coder:30b", "digest-a", manifest.prompt_version));
}

#[cfg(unix)]
#[test]
fn saved_manifest_and_run_directory_are_private() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let manifest = BenchmarkManifest::new(
        root.path().to_path_buf(),
        "owner/repository".into(),
        vec![1, 2, 3, 4, 5, 6],
        candidates(),
    )
    .unwrap();
    let path = root.path().join(".ramo-benchmark/manifest.json");

    manifest.save(&path).unwrap();

    assert_eq!(
        std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
