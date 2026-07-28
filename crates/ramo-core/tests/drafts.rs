use ramo_core::drafts::{DraftAnchor, DraftError, create_draft};
use ramo_core::remote_review::RemoteLineSide;

fn anchor() -> DraftAnchor {
    DraftAnchor {
        repository: "ramo/ramo".into(),
        number: 7,
        captured_revision: "sha".into(),
        path: "src/lib.rs".into(),
        side: RemoteLineSide::Right,
        start_line: 9,
        end_line: 7,
        context_before: vec![],
        selected_text: vec!["nine".into(), "eight".into(), "seven".into()],
        context_after: vec![],
    }
}

#[test]
fn normalizes_reversed_ranges_and_rejects_invalid_drafts() {
    let draft = create_draft(
        "id".into(),
        anchor(),
        "body".into(),
        RemoteLineSide::Right,
        1,
        1,
    )
    .unwrap();
    assert_eq!((draft.anchor.start_line, draft.anchor.end_line), (7, 9));
    assert_eq!(draft.anchor.selected_text, ["seven", "eight", "nine"]);
    assert_eq!(
        create_draft(
            "id".into(),
            anchor(),
            " ".into(),
            RemoteLineSide::Right,
            1,
            1
        ),
        Err(DraftError::BlankBody)
    );
    assert_eq!(
        create_draft(
            "id".into(),
            anchor(),
            "x".into(),
            RemoteLineSide::Left,
            1,
            1
        ),
        Err(DraftError::CrossSide)
    );
    assert_eq!(
        create_draft(
            "id".into(),
            anchor(),
            "x".into(),
            RemoteLineSide::Right,
            1,
            2
        ),
        Err(DraftError::CrossHunk)
    );
}
