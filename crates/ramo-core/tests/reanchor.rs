use ramo_core::drafts::{DraftAnchor, DraftComment, ReanchorResult, reanchor};
use ramo_core::remote_review::RemoteLineSide;

#[test]
fn moves_only_a_unique_exact_context() {
    let draft = DraftComment {
        id: "id".into(),
        body: "body".into(),
        anchor: DraftAnchor {
            repository: "r/r".into(),
            number: 1,
            captured_revision: "old".into(),
            path: "a.rs".into(),
            side: RemoteLineSide::Right,
            start_line: 2,
            end_line: 2,
            context_before: vec!["before".into()],
            selected_text: vec!["target".into()],
            context_after: vec!["after".into()],
        },
    };
    let lines = vec![
        "x".into(),
        "x".into(),
        "before".into(),
        "target".into(),
        "after".into(),
    ];
    assert!(matches!(
        reanchor(draft, "a.rs", RemoteLineSide::Right, &lines),
        ReanchorResult::Moved {
            old_line: 2,
            new_line: 4,
            ..
        }
    ));
}
