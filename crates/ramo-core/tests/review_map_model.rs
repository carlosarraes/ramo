use ramo_core::review_map::{
    PatchCoverage, REVIEW_MAP_SCHEMA_VERSION, ReviewFileKind, ReviewMapCacheIdentity,
    ReviewMapFailureCode, ReviewMapIdentity, ReviewMapStatus, review_map_cache_key,
};

fn cache_identity() -> ReviewMapCacheIdentity {
    ReviewMapCacheIdentity {
        repository: "Mondrio-App/mondrio-platform".into(),
        pull_request: 291,
        head_sha: "head".into(),
        model: "qwen3:8b".into(),
        model_digest: "sha256:model".into(),
        prompt_version: 1,
        schema_version: REVIEW_MAP_SCHEMA_VERSION,
        classifier_version: 1,
        generation_parameters: vec![
            ("temperature".into(), "0".into()),
            ("seed".into(), "42".into()),
        ],
    }
}

#[test]
fn review_map_wire_names_are_stable() {
    assert_eq!(REVIEW_MAP_SCHEMA_VERSION, 1);
    assert_eq!(
        serde_json::to_string(&ReviewMapStatus::Analyzing).unwrap(),
        "\"analyzing\""
    );
    assert_eq!(
        serde_json::to_string(&ReviewMapFailureCode::GithubAuthUnavailable).unwrap(),
        "\"github_auth_unavailable\""
    );
    assert_eq!(
        serde_json::to_string(&ReviewFileKind::Generated).unwrap(),
        "\"generated\""
    );
    assert_eq!(
        serde_json::to_string(&PatchCoverage::MetadataOnly).unwrap(),
        "\"metadata_only\""
    );

    let identity = ReviewMapIdentity {
        repository: "Mondrio-App/mondrio-platform".into(),
        pull_request: 291,
        base_sha: "base".into(),
        head_sha: "head".into(),
    };
    assert_eq!(serde_json::to_value(identity).unwrap()["head_sha"], "head");
}

#[test]
fn every_semantic_version_changes_the_cache_key() {
    let base = cache_identity();
    let original = review_map_cache_key(&base);

    for changed in [
        ReviewMapCacheIdentity {
            head_sha: "new".into(),
            ..base.clone()
        },
        ReviewMapCacheIdentity {
            model: "other".into(),
            ..base.clone()
        },
        ReviewMapCacheIdentity {
            prompt_version: base.prompt_version + 1,
            ..base.clone()
        },
        ReviewMapCacheIdentity {
            schema_version: base.schema_version + 1,
            ..base.clone()
        },
        ReviewMapCacheIdentity {
            classifier_version: base.classifier_version + 1,
            ..base.clone()
        },
    ] {
        assert_ne!(review_map_cache_key(&changed), original);
    }
}

#[test]
fn generation_parameter_order_does_not_change_the_cache_key() {
    let base = cache_identity();
    let mut reordered = base.clone();
    reordered.generation_parameters.reverse();

    assert_eq!(
        review_map_cache_key(&base),
        review_map_cache_key(&reordered)
    );
}
