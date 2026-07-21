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
fn non_linux_pty_tests_run_serially() {
    assert!(
        CI_WORKFLOW.contains("if: runner.os != 'Linux'"),
        "portable macOS and Windows PTY tests must use the serial test command"
    );
    assert!(
        CI_WORKFLOW.contains("if: runner.os == 'macOS'"),
        "the native macOS PTY job must select its serial test command explicitly"
    );
    assert_eq!(
        CI_WORKFLOW.matches("--test-threads=1").count(),
        2,
        "portable non-Linux and native macOS PTY jobs must serialize tests"
    );
}
