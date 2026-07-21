use ramo::startup_notice::{resolve_skill_refresh_notice, select_remote_update_notice};

#[test]
fn copied_skill_refresh_notice_is_local_one_time_and_failure_tolerant() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("ramo/state.json");

    assert_eq!(resolve_skill_refresh_notice(&state, "0.0.5", false), None);
    assert_eq!(resolve_skill_refresh_notice(&state, "0.0.5", false), None);
    assert_eq!(
        resolve_skill_refresh_notice(&state, "0.0.6", false),
        Some(
            "ramo 0.0.6 installed • If your agent copied ramo's skill, run ramo skill path"
                .into()
        )
    );
    assert_eq!(resolve_skill_refresh_notice(&state, "0.0.6", false), None);

    let disabled = temp.path().join("disabled/state.json");
    assert_eq!(resolve_skill_refresh_notice(&disabled, "0.0.6", true), None);
    assert!(!disabled.exists());

    let unwritable = temp.path().join("not-a-directory/state.json");
    std::fs::write(unwritable.parent().unwrap(), "file").unwrap();
    assert_eq!(
        resolve_skill_refresh_notice(&unwritable, "0.0.6", false),
        None
    );
}

#[test]
fn remote_update_selection_matches_hunks_stable_and_prerelease_priority() {
    let tags = concat!(
        "a\trefs/tags/v0.0.6\n",
        "b\trefs/tags/v0.0.7-beta.1\n",
        "c\trefs/tags/v0.0.7\n",
        "d\trefs/tags/not-a-version\n",
        "e\trefs/heads/v99.0.0\n",
    );

    assert_eq!(
        select_remote_update_notice("0.0.6", tags),
        Some("Update available: 0.0.7 • install the latest ramo release".into())
    );
    assert_eq!(select_remote_update_notice("0.0.7", tags), None);
    assert_eq!(
        select_remote_update_notice(
            "0.0.7",
            "a\trefs/tags/v0.0.8-beta.2\nb\trefs/tags/v0.0.8-beta.10\n",
        ),
        Some("Update available: 0.0.8-beta.10 • install the latest ramo release".into())
    );
    assert_eq!(
        select_remote_update_notice("0.0.7-beta.1", tags),
        Some("Update available: 0.0.7 • install the latest ramo release".into())
    );
    assert_eq!(select_remote_update_notice("development", tags), None);
}
