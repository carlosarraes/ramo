use pdiff::cli::{Action, parse_from};
use pdiff::core::input::ReviewInput;
use pdiff::session::{
    CommentListType, CommentRevealMode, SessionCommand, SessionOutput, SessionSelector,
};

#[test]
fn list_get_context_and_review_normalize_output_selectors_and_flags() {
    assert!(matches!(
        parse_from(["pdiff", "session", "list", "--json"], true)
            .unwrap()
            .action,
        Action::Session(SessionCommand::List {
            output: SessionOutput::Json
        })
    ));
    let get = parse_from(["pdiff", "session", "get", "abc"], true).unwrap();
    assert!(matches!(
        get.action,
        Action::Session(SessionCommand::Get {
            selector: SessionSelector { session_id: Some(id), .. },
            output: SessionOutput::Text,
        }) if id == "abc"
    ));
    assert!(matches!(
        parse_from(
            ["pdiff", "session", "context", "--repo", ".", "--json"],
            true
        )
        .unwrap()
        .action,
        Action::Session(SessionCommand::Context {
            selector: SessionSelector {
                repo_root: Some(_),
                ..
            },
            output: SessionOutput::Json,
        })
    ));
    assert!(matches!(
        parse_from(
            [
                "pdiff",
                "session",
                "review",
                "abc",
                "--include-patch",
                "--include-notes"
            ],
            true,
        )
        .unwrap()
        .action,
        Action::Session(SessionCommand::Review {
            include_patch: true,
            include_notes: true,
            ..
        })
    ));
}

#[test]
fn navigation_requires_exactly_one_valid_target_mode() {
    assert!(matches!(
        parse_from(
            [
                "pdiff", "session", "navigate", "abc", "--file", "src/lib.rs", "--new-line", "7"
            ],
            true,
        )
        .unwrap()
        .action,
        Action::Session(SessionCommand::Navigate { file_path: Some(path), line: Some(7), side: Some(side), .. })
            if path == "src/lib.rs" && side.as_str() == "new"
    ));
    assert!(matches!(
        parse_from(
            ["pdiff", "session", "navigate", "abc", "--next-comment"],
            true,
        )
        .unwrap()
        .action,
        Action::Session(SessionCommand::Navigate { comment_direction: Some(direction), .. })
            if direction.as_str() == "next"
    ));
    for args in [
        vec!["pdiff", "session", "navigate", "abc"],
        vec![
            "pdiff",
            "session",
            "navigate",
            "abc",
            "--file",
            "x",
            "--old-line",
            "1",
            "--new-line",
            "2",
        ],
        vec![
            "pdiff",
            "session",
            "navigate",
            "abc",
            "--next-comment",
            "--prev-comment",
        ],
    ] {
        assert!(parse_from(args, true).is_err());
    }
}

#[test]
fn reload_reuses_the_existing_review_parser_and_rejects_stdin_or_nested_actions() {
    let invocation = parse_from(
        [
            "pdiff", "session", "reload", "abc", "--source", "/tmp", "--", "show", "HEAD~1", "--",
            "src",
        ],
        true,
    )
    .unwrap();
    assert!(matches!(
        invocation.action,
        Action::Session(SessionCommand::Reload {
            next_input: ReviewInput::Show { reference, pathspecs, .. },
            source_path: Some(_),
            ..
        }) if reference.as_deref() == Some("HEAD~1") && pathspecs == ["src"]
    ));
    assert!(
        parse_from(
            ["pdiff", "session", "reload", "abc", "--", "patch", "-"],
            true,
        )
        .is_err()
    );
    assert!(
        parse_from(
            ["pdiff", "session", "reload", "abc", "--", "session", "list"],
            true,
        )
        .is_err()
    );
}

#[test]
fn comment_commands_validate_targets_batches_types_and_destructive_confirmation() {
    assert!(matches!(
        parse_from(
            [
                "pdiff",
                "session",
                "comment",
                "add",
                "abc",
                "--file",
                "src/lib.rs",
                "--old-line",
                "4",
                "--summary",
                "check",
                "--focus",
                "--json"
            ],
            true,
        )
        .unwrap()
        .action,
        Action::Session(SessionCommand::CommentAdd {
            line: 4,
            reveal: true,
            output: SessionOutput::Json,
            ..
        })
    ));
    assert!(
        parse_from(
            [
                "pdiff",
                "session",
                "comment",
                "add",
                "abc",
                "--file",
                "x",
                "--old-line",
                "1",
                "--new-line",
                "1",
                "--summary",
                "bad",
            ],
            true,
        )
        .is_err()
    );
    assert!(matches!(
        parse_from(
            [
                "pdiff", "session", "comment", "apply", "abc", "--stdin", "--focus"
            ],
            true,
        )
        .unwrap()
        .action,
        Action::Session(SessionCommand::CommentApply {
            reveal_mode: CommentRevealMode::First,
            ..
        })
    ));
    assert!(parse_from(["pdiff", "session", "comment", "apply", "abc"], true,).is_err());
    assert!(matches!(
        parse_from(
            [
                "pdiff", "session", "comment", "list", "abc", "--type", "agent"
            ],
            true,
        )
        .unwrap()
        .action,
        Action::Session(SessionCommand::CommentList {
            note_type: Some(CommentListType::Agent),
            ..
        })
    ));
    assert!(parse_from(["pdiff", "session", "comment", "clear", "abc"], true,).is_err());
    assert!(matches!(
        parse_from(
            [
                "pdiff", "session", "comment", "clear", "abc", "--all", "--yes"
            ],
            true,
        )
        .unwrap()
        .action,
        Action::Session(SessionCommand::CommentClear {
            include_user: true,
            ..
        })
    ));
}

#[test]
fn selector_conflicts_and_daemon_aliases_are_explicit() {
    assert!(parse_from(["pdiff", "session", "get", "abc", "--repo", "."], true,).is_err());
    assert!(matches!(
        parse_from(["pdiff", "daemon", "serve"], true)
            .unwrap()
            .action,
        Action::DaemonServe
    ));
    assert!(matches!(
        parse_from(["pdiff", "mcp", "serve"], true).unwrap().action,
        Action::DaemonServe
    ));
    assert!(matches!(
        parse_from(["pdiff", "skill", "path"], true).unwrap().action,
        Action::SkillPath
    ));
}
