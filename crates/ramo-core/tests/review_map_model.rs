use ramo_core::review_map::{
    PatchCoverage, REVIEW_MAP_SCHEMA_VERSION, ReviewFileKind, ReviewMapFailureCode,
    ReviewMapIdentity, ReviewMapStatus,
};

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
