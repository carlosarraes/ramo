use std::time::{Duration, Instant};

use ramo_core::review_map::ReviewMapFailureCode;
use ramo_server::api::{PairingState, ReviewMapClientTokenStore};

#[test]
fn pairing_code_is_short_lived_and_single_use() {
    let tokens = ReviewMapClientTokenStore::default();
    let state = PairingState::new(tokens.clone());
    let now = Instant::now();
    let code = state.issue_at(now, Duration::from_secs(300)).unwrap();

    let credential = state.exchange_at(&code, "Carlos phone", now).unwrap();
    assert!(tokens.authorize(&credential.token));
    assert_eq!(
        state
            .exchange_at(&code, "Carlos phone", now)
            .unwrap_err()
            .code,
        ReviewMapFailureCode::PairingRejected
    );

    let expired = state.issue_at(now, Duration::from_secs(1)).unwrap();
    assert_eq!(
        state
            .exchange_at(&expired, "Carlos phone", now + Duration::from_secs(2))
            .unwrap_err()
            .code,
        ReviewMapFailureCode::PairingRejected
    );
}

#[test]
fn client_tokens_are_individually_revocable_and_debug_redacted() {
    let tokens = ReviewMapClientTokenStore::default();
    let credential = tokens.issue("Carlos phone").unwrap();
    assert!(tokens.authorize(&credential.token));
    assert!(!format!("{tokens:?}").contains(&credential.token));

    assert!(tokens.revoke(&credential.client_id).unwrap());
    assert!(!tokens.authorize(&credential.token));
}

#[test]
fn persistent_client_store_keeps_only_token_digests_across_restarts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("clients.json");
    let store = ReviewMapClientTokenStore::open(&path).unwrap();
    let credential = store.issue("Carlos phone").unwrap();
    drop(store);

    let persisted = std::fs::read_to_string(&path).unwrap();
    assert!(!persisted.contains(&credential.token));
    let reopened = ReviewMapClientTokenStore::open(path).unwrap();
    assert!(reopened.authorize(&credential.token));
}

#[test]
fn persistent_pairing_codes_cross_the_cli_server_process_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pairing.json");
    let issuer = PairingState::open(ReviewMapClientTokenStore::default(), &path);
    let code = issuer.issue(Duration::from_secs(300)).unwrap();
    let tokens = ReviewMapClientTokenStore::default();
    let server = PairingState::open(tokens.clone(), &path);

    let credential = server.exchange(&code, "Android").unwrap();

    assert!(tokens.authorize(&credential.token));
    assert!(!std::fs::read_to_string(path).unwrap().contains(&code));
}
