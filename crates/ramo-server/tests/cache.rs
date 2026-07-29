use std::time::Duration;

use ramo_core::review_map::{
    ClassifierConfig, REVIEW_MAP_SCHEMA_VERSION, ReviewMapCacheIdentity, ReviewMapInput,
    ReviewMapInputFile, build_review_map,
};
use ramo_server::cache::{CacheLimits, ReviewMapCache};

#[test]
fn cache_round_trips_only_the_structured_map_and_replaces_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let cache = ReviewMapCache::new(
        directory.path(),
        CacheLimits {
            max_bytes: 1024 * 1024,
            max_age: Duration::from_secs(3600),
        },
    )
    .unwrap();
    let identity = identity();
    let map = exact_map("head");

    cache.put(&identity, &map).unwrap();
    cache.put(&identity, &map).unwrap();

    assert_eq!(cache.get(&identity).unwrap(), Some(map));
    let bytes = std::fs::read(cache.entry_path(&identity)).unwrap();
    let persisted = String::from_utf8(bytes).unwrap();
    assert!(!persisted.contains("@@ secret patch body"));
    assert!(!persisted.contains("You organize a pull request"));
    assert!(!directory.path().read_dir().unwrap().any(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "tmp")
    }));
}

#[test]
fn corrupt_and_oversized_entries_are_removed() {
    let directory = tempfile::tempdir().unwrap();
    let identity = identity();
    let cache = ReviewMapCache::new(
        directory.path(),
        CacheLimits {
            max_bytes: 1024 * 1024,
            max_age: Duration::from_secs(3600),
        },
    )
    .unwrap();
    std::fs::write(cache.entry_path(&identity), b"not json").unwrap();
    assert_eq!(cache.get(&identity).unwrap(), None);
    assert!(!cache.entry_path(&identity).exists());

    let tiny = ReviewMapCache::new(
        directory.path(),
        CacheLimits {
            max_bytes: 1,
            max_age: Duration::from_secs(3600),
        },
    )
    .unwrap();
    tiny.put(&identity, &exact_map("head")).unwrap();
    assert_eq!(tiny.get(&identity).unwrap(), None);
}

fn identity() -> ReviewMapCacheIdentity {
    ReviewMapCacheIdentity {
        repository: "owner/repo".into(),
        pull_request: 7,
        head_sha: "head".into(),
        model: "qwen3:8b".into(),
        model_digest: "sha256:model".into(),
        prompt_version: 1,
        schema_version: REVIEW_MAP_SCHEMA_VERSION,
        classifier_version: 1,
        generation_parameters: vec![("temperature".into(), "0".into())],
    }
}

fn exact_map(head_sha: &str) -> ramo_core::review_map::ReviewMap {
    build_review_map(
        &ReviewMapInput {
            identity: ramo_core::review_map::ReviewMapIdentity {
                repository: "owner/repo".into(),
                pull_request: 7,
                base_sha: "base".into(),
                head_sha: head_sha.into(),
            },
            files: vec![ReviewMapInputFile {
                path: "src/lib.rs".into(),
                previous_path: None,
                status: "modified".into(),
                additions: 3,
                deletions: 1,
                patch: Some("@@ secret patch body".into()),
                binary: false,
            }],
            codeowners: None,
        },
        &ClassifierConfig::default(),
    )
    .unwrap()
}
