use std::collections::BTreeMap;
use std::io::Cursor;

use pdiff::config::ResolvedConfig;
use pdiff::core::input::{CommonOptions, ReviewInput};
use pdiff::input::{
    LoadContext, LoadOutcome, ReviewLoader, looks_like_patch, sanitize_terminal_text,
};
use pdiff::pager::resolve_text_pager;
use pdiff::vcs::SystemCommandRunner;

#[test]
fn patch_detection_accepts_git_unified_and_hunk_only_inputs_after_ansi_removal() {
    assert!(looks_like_patch("\x1b[31mdiff --git a/a b/a\x1b[0m\n"));
    assert!(looks_like_patch("--- a/a\n+++ b/a\n@@ -1 +1 @@\n"));
    assert!(looks_like_patch("heading\n@@ -1 +1 @@\n-old\n+new\n"));
    assert!(!looks_like_patch("ordinary compiler output\n"));
}

#[test]
fn pager_resolution_never_invokes_a_shell_or_recurses_into_pdiff() {
    let env = BTreeMap::from([("PDIFF_TEXT_PAGER".into(), "env LESS=-FRX 'less' -R".into())]);
    let spec = resolve_text_pager(&env).unwrap();
    assert_eq!(spec.program, "less");
    assert_eq!(spec.args, ["-R"]);
    assert_eq!(spec.env.get("LESS").map(String::as_str), Some("-FRX"));

    let recursive = BTreeMap::from([("PDIFF_TEXT_PAGER".into(), "/usr/bin/pdiff pager".into())]);
    assert_eq!(resolve_text_pager(&recursive).unwrap().display, "less -R");

    for command in ["'C:\\tools\\PDIFF.EXE' pager", "'C:\\tools\\pdiff.cmd'"] {
        let recursive = BTreeMap::from([("PDIFF_TEXT_PAGER".into(), command.into())]);
        assert_eq!(resolve_text_pager(&recursive).unwrap().display, "less -R");
    }

    let empty = BTreeMap::from([("PDIFF_TEXT_PAGER".into(), String::new())]);
    assert_eq!(resolve_text_pager(&empty).unwrap().display, "less -R");
}

#[test]
fn pager_resolution_preserves_literal_operators_and_assignment_precedence() {
    let env = BTreeMap::from([
        ("PAGER".into(), "more".into()),
        (
            "PDIFF_TEXT_PAGER".into(),
            "LESS=-F less '-R;' '$(touch nope)' '|' '>' output".into(),
        ),
    ]);
    let spec = resolve_text_pager(&env).unwrap();
    assert_eq!(spec.program, "less");
    assert_eq!(spec.args, ["-R;", "$(touch nope)", "|", ">", "output"]);
    assert_eq!(spec.env.get("LESS").map(String::as_str), Some("-F"));

    let invalid = BTreeMap::from([("PDIFF_TEXT_PAGER".into(), "'unterminated".into())]);
    assert!(
        resolve_text_pager(&invalid)
            .unwrap_err()
            .to_string()
            .contains("PDIFF_TEXT_PAGER")
    );
}

#[test]
fn sanitizer_removes_osc_controls_but_can_preserve_sgr_styles() {
    let text = "safe\x1b]8;;https://bad\x1b\\link\x1b]8;;\x1b\\\x1b[31m red\x1b[0m\r\n";
    assert_eq!(sanitize_terminal_text(text, false), "safelink red\n");
    assert_eq!(
        sanitize_terminal_text(text, true),
        "safelink\x1b[31m red\x1b[0m\n"
    );
}

#[test]
fn pager_loader_returns_review_or_plain_text_without_terminal_work() {
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    let cwd = std::env::current_dir().unwrap();
    let context = LoadContext {
        cwd: &cwd,
        config: &config,
        runner: &runner,
    };
    let input = ReviewInput::Pager {
        options: CommonOptions::default(),
    };
    let review = ReviewLoader
        .load_outcome_with_context(
            &input,
            &mut Cursor::new(include_str!("fixtures/simple.patch")),
            &context,
        )
        .unwrap();
    assert!(matches!(review, LoadOutcome::Review(_)));

    let plain = ReviewLoader
        .load_outcome_with_context(&input, &mut Cursor::new("ordinary output\n"), &context)
        .unwrap();
    assert!(matches!(plain, LoadOutcome::PlainText(text) if text == "ordinary output\n"));

    let empty = ReviewLoader
        .load_outcome_with_context(&input, &mut Cursor::new(""), &context)
        .unwrap();
    assert!(matches!(empty, LoadOutcome::PlainText(text) if text.is_empty()));
}
