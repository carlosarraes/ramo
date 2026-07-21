use std::io::Cursor;
use std::path::PathBuf;
use std::time::Instant;

use pdiff::config::ResolvedConfig;
use pdiff::core::changeset::Changeset;
use pdiff::core::input::{CommonOptions, PatchSource, ReviewInput};
use pdiff::diff::model::DiffFile;
use pdiff::diff::parser::parse_unified_diff;
use pdiff::input::{LoadContext, LoadError, ReviewLoader};
use pdiff::notes::{AgentContextError, AgentContextSource, parse_agent_context};
use pdiff::vcs::SystemCommandRunner;
use pdiff::watch::{WatchRuntime, WatchUpdate};

fn file(path: &str) -> DiffFile {
    parse_unified_diff(&format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n"
    ))
    .remove(0)
}

#[test]
fn normalizes_metadata_filters_tags_and_sanitizes_terminal_controls() {
    let context = parse_agent_context(
        "agent.json",
        br#"{
          "summary":"review\u001b[2J summary",
          "files":[{
            "path":"src/lib.rs",
            "summary":"file summary",
            "annotations":[{
              "id":"note-1",
              "oldRange":[2,3],
              "newRange":[4,8],
              "summary":"added helper\u001b]8;;https://bad\u001b\\",
              "rationale":"because",
              "markup":"<b>safe</b>",
              "tags":["review",7,"security"],
              "confidence":"high",
              "source":"agent",
              "title":"Finding",
              "author":"pi",
              "createdAt":"2026-07-21T00:00:00Z",
              "updatedAt":"2026-07-21T00:01:00Z",
              "editable":true
            }]
          }]
        }"#,
    )
    .unwrap();

    assert_eq!(context.version, 1);
    assert_eq!(context.summary.as_deref(), Some("review summary"));
    let note = &context.files[0].annotations[0];
    assert_eq!(note.old_range.unwrap().inclusive(), (2, 3));
    assert_eq!(note.new_range.unwrap().inclusive(), (4, 8));
    assert_eq!(note.summary, "added helper");
    assert_eq!(note.tags, ["review", "security"]);
    assert_eq!(note.confidence.as_ref().unwrap().as_str(), "high");
    assert_eq!(note.source.as_str(), "agent");
    assert!(note.editable);
}

#[test]
fn context_order_leads_and_previous_paths_match_without_reordering_unmatched_files() {
    let context = parse_agent_context(
        "agent.json",
        br#"{
          "version":1,
          "summary":"ordered",
          "files":[
            {"path":"old/b.rs","annotations":[{"summary":"rename note"}]},
            {"path":"d.rs","annotations":[]}
          ]
        }"#,
    )
    .unwrap();
    let mut renamed = file("new/b.rs");
    renamed.previous_path = Some("old/b.rs".into());
    let mut changeset = Changeset::new(
        "test",
        "test",
        vec![file("a.rs"), renamed, file("c.rs"), file("d.rs")],
    );

    changeset.apply_agent_context(&context);

    assert_eq!(
        changeset
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["new/b.rs", "d.rs", "a.rs", "c.rs"]
    );
    assert_eq!(changeset.agent_summary.as_deref(), Some("ordered"));
    assert_eq!(
        changeset.files[0].agent.as_ref().unwrap().annotations[0].summary,
        "rename note"
    );
}

#[test]
fn malformed_files_summaries_and_ranges_are_operation_specific() {
    let cases = [
        (
            br#"{"files":[{"annotations":[]}]}"#.as_slice(),
            "file entries require a non-empty path",
        ),
        (
            br#"{"files":[{"path":"a","annotations":[{}]}]}"#.as_slice(),
            "annotations require a non-empty summary",
        ),
        (
            br#"{"files":[{"path":"a","annotations":[{"summary":"x","newRange":[1,"two"]}]}]}"#
                .as_slice(),
            "ranges must be integer pairs",
        ),
        (
            br#"{"files":[{"path":"a","annotations":[{"summary":"x","newRange":[0,2]}]}]}"#
                .as_slice(),
            "ranges use positive 1-based lines",
        ),
        (
            br#"{"files":[{"path":"a","annotations":[{"summary":"x","newRange":[4,2]}]}]}"#
                .as_slice(),
            "ranges must be ordered",
        ),
    ];

    for (json, message) in cases {
        let error = parse_agent_context("broken.json", json).unwrap_err();
        assert!(error.to_string().contains("broken.json"));
        assert!(error.to_string().contains(message), "{error}");
    }
}

#[test]
fn sidecar_and_collection_limits_are_enforced() {
    let oversized = vec![b' '; pdiff::notes::MAX_AGENT_CONTEXT_BYTES + 1];
    assert!(matches!(
        parse_agent_context("large.json", &oversized),
        Err(AgentContextError::TooLarge { .. })
    ));

    let mut files = Vec::new();
    for index in 0..=pdiff::notes::MAX_AGENT_FILES {
        files.push(format!(r#"{{"path":"{index}","annotations":[]}}"#));
    }
    let json = format!(r#"{{"files":[{}]}}"#, files.join(","));
    let error = parse_agent_context("many.json", json.as_bytes()).unwrap_err();
    assert!(error.to_string().contains("file limit"));
}

#[test]
fn file_backed_agent_context_is_reloaded_with_the_review() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("before.rs");
    let right = temp.path().join("after.rs");
    let context_path = temp.path().join("agent.json");
    std::fs::write(&left, "fn before() {}\n").unwrap();
    std::fs::write(&right, "fn after() {}\n").unwrap();
    std::fs::write(
        &context_path,
        r#"{"files":[{"path":"after.rs","annotations":[{"summary":"first"}]}]}"#,
    )
    .unwrap();
    let input = ReviewInput::FilePair {
        left: left.clone(),
        right: right.clone(),
        display_path: Some(PathBuf::from("after.rs")),
        options: CommonOptions {
            agent_context: Some(context_path.clone()),
            ..CommonOptions::default()
        },
    };
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    let load_context = LoadContext {
        cwd: temp.path(),
        config: &config,
        runner: &runner,
    };
    let loaded = ReviewLoader
        .load_with_context(&input, &mut Cursor::new([]), &load_context)
        .unwrap();
    assert_eq!(
        loaded.changeset.files[0]
            .agent
            .as_ref()
            .unwrap()
            .annotations[0]
            .summary,
        "first"
    );
    assert!(matches!(loaded.agent_context, AgentContextSource::File(_)));

    std::fs::write(
        &context_path,
        r#"{"files":[{"path":"after.rs","annotations":[{"summary":"second"}]}]}"#,
    )
    .unwrap();
    let reloaded = ReviewLoader
        .reload_with_agent(&loaded.reload_plan, &loaded.agent_context, &load_context)
        .unwrap();
    assert_eq!(
        reloaded.changeset.files[0]
            .agent
            .as_ref()
            .unwrap()
            .annotations[0]
            .summary,
        "second"
    );
}

#[test]
fn watch_replaces_the_review_when_only_agent_context_changes() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("before.rs");
    let right = temp.path().join("after.rs");
    let context_path = temp.path().join("agent.json");
    std::fs::write(&left, "fn before() {}\n").unwrap();
    std::fs::write(&right, "fn after() {}\n").unwrap();
    std::fs::write(
        &context_path,
        r#"{"files":[{"path":"after.rs","annotations":[{"summary":"first"}]}]}"#,
    )
    .unwrap();
    let input = ReviewInput::FilePair {
        left,
        right,
        display_path: Some(PathBuf::from("after.rs")),
        options: CommonOptions {
            agent_context: Some(context_path.clone()),
            ..CommonOptions::default()
        },
    };
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    let load_context = LoadContext {
        cwd: temp.path(),
        config: &config,
        runner: &runner,
    };
    let loaded = ReviewLoader
        .load_with_context(&input, &mut Cursor::new([]), &load_context)
        .unwrap();
    let start = Instant::now();
    let mut runtime = WatchRuntime::new(&loaded, temp.path().to_path_buf(), config, false, start);

    std::fs::write(
        context_path,
        r#"{"files":[{"path":"after.rs","annotations":[{"summary":"second"}]}]}"#,
    )
    .unwrap();
    runtime.manual_reload(start);
    let WatchUpdate::Replaced { files, .. } = runtime.poll(start) else {
        panic!("agent-only reload was suppressed as unchanged");
    };
    assert_eq!(
        files[0].agent.as_ref().unwrap().annotations[0].summary,
        "second"
    );
}

#[test]
fn patch_and_agent_context_cannot_both_consume_stdin() {
    let input = ReviewInput::Patch {
        source: PatchSource::Stdin,
        options: CommonOptions {
            agent_context: Some(PathBuf::from("-")),
            ..CommonOptions::default()
        },
    };
    let config = ResolvedConfig::default();
    let runner = SystemCommandRunner;
    let cwd = std::env::current_dir().unwrap();
    let error = ReviewLoader
        .load_with_context(
            &input,
            &mut Cursor::new(include_str!("fixtures/simple.patch")),
            &LoadContext {
                cwd: &cwd,
                config: &config,
                runner: &runner,
            },
        )
        .unwrap_err();
    assert!(matches!(
        error,
        LoadError::AgentContext(AgentContextError::ConflictingStdin)
    ));
}
