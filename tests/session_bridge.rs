use ramo::core::input::LayoutMode;
use ramo::diff::parser::parse_unified_diff;
use ramo::notes::LineRange;
use ramo::review::{ReviewController, ReviewOptions, Viewport};
use ramo::session::{apply_session_request, build_snapshot};

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n tail\n@@ -10,2 +10,2 @@\n-old ten\n+new ten\n end\n";

fn view() -> Viewport {
    Viewport {
        width: 100,
        height: 20,
    }
}

fn controller() -> ReviewController {
    ReviewController::new(
        parse_unified_diff(PATCH),
        ReviewOptions {
            layout: LayoutMode::Stack,
            ..ReviewOptions::default()
        },
    )
}

#[test]
fn navigation_resolves_one_based_hunks_side_lines_and_annotated_directions() {
    let mut controller = controller();
    let result = apply_session_request(
        &mut controller,
        "nav-1",
        &serde_json::json!({"action":"navigate","filePath":"src/lib.rs","hunkNumber":2}),
        view(),
    )
    .unwrap();
    assert_eq!(result["hunkIndex"], 1);
    let result = apply_session_request(
        &mut controller,
        "nav-2",
        &serde_json::json!({"action":"navigate","filePath":"src/lib.rs","side":"new","line":2}),
        view(),
    )
    .unwrap();
    assert_eq!(result["hunkIndex"], 0);

    apply_session_request(
        &mut controller,
        "comment-a",
        &serde_json::json!({"action":"comment-add","filePath":"src/lib.rs","side":"new","line":2,"summary":"first","reveal":false}),
        view(),
    )
    .unwrap();
    apply_session_request(
        &mut controller,
        "comment-b",
        &serde_json::json!({"action":"comment-add","filePath":"src/lib.rs","side":"new","line":10,"summary":"second","reveal":false}),
        view(),
    )
    .unwrap();
    let result = apply_session_request(
        &mut controller,
        "nav-3",
        &serde_json::json!({"action":"navigate","commentDirection":"next"}),
        view(),
    )
    .unwrap();
    assert_eq!(result["hunkIndex"], 1);
}

#[test]
fn add_batch_markup_focus_and_failed_target_isolation_are_deterministic() {
    let mut controller = controller();
    let added = apply_session_request(
        &mut controller,
        "add-1",
        &serde_json::json!({
            "action":"comment-add","filePath":"src/lib.rs","side":"new","line":2,
            "summary":"check","markup":"<wat>fallback</wat>","author":"Pi","reveal":true
        }),
        view(),
    )
    .unwrap();
    assert_eq!(added["commentId"], "mcp:add-1");
    assert!(added["markupWidth"].as_u64().unwrap() >= 8);
    assert!(!added["markupNotes"].as_array().unwrap().is_empty());
    let snapshot = build_snapshot(&mut controller, view(), "now");
    assert!(snapshot.state.show_agent_notes);

    let before = controller.live_notes().len();
    assert!(
        apply_session_request(
            &mut controller,
            "batch-1",
            &serde_json::json!({
                "action":"comment-apply","comments":[
                    {"filePath":"src/lib.rs","side":"new","line":10,"summary":"valid"},
                    {"filePath":"missing.rs","side":"new","line":1,"summary":"invalid"}
                ],"revealMode":"first"
            }),
            view(),
        )
        .is_err()
    );
    assert_eq!(controller.live_notes().len(), before);

    let oversized = "x".repeat(64 * 1024 + 1);
    let error = apply_session_request(
        &mut controller,
        "oversized",
        &serde_json::json!({
            "action":"comment-add","filePath":"src/lib.rs","side":"new","line":2,
            "summary":oversized,"reveal":false
        }),
        view(),
    )
    .unwrap_err();
    assert!(error.contains("exceed 65536 bytes"));
    assert_eq!(controller.live_notes().len(), before);
}

#[test]
fn list_remove_and_clear_keep_live_external_and_human_sources_distinct() {
    let mut controller = controller();
    let human = controller.add_human_note(
        "src/lib.rs",
        None,
        Some(LineRange { start: 2, end: 2 }),
        "human",
        view(),
    );
    apply_session_request(
        &mut controller,
        "live-1",
        &serde_json::json!({"action":"comment-add","filePath":"src/lib.rs","side":"new","line":2,"summary":"live","reveal":false}),
        view(),
    )
    .unwrap();
    let live = apply_session_request(
        &mut controller,
        "list-1",
        &serde_json::json!({"action":"comment-list","type":"live"}),
        view(),
    )
    .unwrap();
    assert_eq!(live.as_array().unwrap().len(), 1);
    let all = apply_session_request(
        &mut controller,
        "list-2",
        &serde_json::json!({"action":"comment-list","type":"all"}),
        view(),
    )
    .unwrap();
    assert_eq!(all.as_array().unwrap().len(), 2);

    let removed = apply_session_request(
        &mut controller,
        "rm-1",
        &serde_json::json!({"action":"comment-rm","commentId":human}),
        view(),
    )
    .unwrap();
    assert_eq!(removed["source"], "user");
    controller.add_human_note("src/lib.rs", None, None, "keep", view());
    let cleared = apply_session_request(
        &mut controller,
        "clear-1",
        &serde_json::json!({"action":"comment-clear","includeUser":false}),
        view(),
    )
    .unwrap();
    assert_eq!(cleared["removedLiveCommentCount"], 1);
    assert_eq!(cleared["removedUserNoteCount"], 0);
    assert_eq!(controller.human_notes().len(), 1);
    apply_session_request(
        &mut controller,
        "clear-2",
        &serde_json::json!({"action":"comment-clear","includeUser":true}),
        view(),
    )
    .unwrap();
    assert!(controller.human_notes().is_empty());
}
