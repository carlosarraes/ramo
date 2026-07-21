use pdiff::startup_notice::resolve_skill_refresh_notice;

#[test]
fn copied_skill_refresh_notice_is_local_one_time_and_failure_tolerant() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("pdiff/state.json");

    assert_eq!(resolve_skill_refresh_notice(&state, "0.0.5", false), None);
    assert_eq!(resolve_skill_refresh_notice(&state, "0.0.5", false), None);
    assert_eq!(
        resolve_skill_refresh_notice(&state, "0.0.6", false),
        Some(
            "pdiff 0.0.6 installed • If your agent copied pdiff's skill, run pdiff skill path"
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
