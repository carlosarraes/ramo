const CI_WORKFLOW: &str = include_str!("../.github/workflows/ci.yml");

#[test]
fn every_pinned_rust_toolchain_step_selects_stable_explicitly() {
    let steps: Vec<_> = CI_WORKFLOW
        .split("uses: dtolnay/rust-toolchain@")
        .skip(1)
        .map(|tail| tail.split("\n      - ").next().unwrap_or(tail))
        .collect();

    assert_eq!(steps.len(), 3, "unexpected Rust toolchain step count");
    for step in steps {
        assert!(
            step.contains("toolchain: stable"),
            "pinned rust-toolchain action must receive an explicit toolchain:\n{step}"
        );
    }
}

#[test]
fn macos_pty_tests_run_serially() {
    assert_eq!(
        CI_WORKFLOW.matches("runner.os == 'macOS'").count(),
        2,
        "portable and native PTY jobs must select their macOS test commands explicitly"
    );
    assert_eq!(
        CI_WORKFLOW.matches("--test-threads=1").count(),
        2,
        "portable and native PTY jobs must serialize macOS PTY tests"
    );
}
