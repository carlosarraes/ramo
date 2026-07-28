use ramo_core::github::{InboxKind, PullRequestKey};

#[test]
fn github_keys_and_filters_have_stable_json() {
    let key = PullRequestKey {
        repository: "owner/repo".into(),
        number: 42,
    };
    assert_eq!(
        serde_json::to_string(&key).unwrap(),
        r#"{"repository":"owner/repo","number":42}"#
    );
    assert_eq!(
        serde_json::to_string(&InboxKind::ReviewRequests).unwrap(),
        r#""review_requests""#
    );
}
