use ramo::remote_review::{
    InlineCommentTarget, RemoteLineSide, RemoteReviewComment, RemoteReviewRequest, ReviewVerdict,
};

#[test]
fn verdicts_have_stable_provider_neutral_values() {
    assert_eq!(ReviewVerdict::Comment.event_name(), "COMMENT");
    assert_eq!(ReviewVerdict::Approve.event_name(), "APPROVE");
    assert_eq!(
        ReviewVerdict::RequestChanges.event_name(),
        "REQUEST_CHANGES"
    );
}

#[test]
fn inline_targets_are_inclusive_and_one_sided() {
    let target = InlineCommentTarget {
        path: "src/lib.rs".into(),
        side: RemoteLineSide::Right,
        start_line: 42,
        end_line: 44,
    };
    assert_eq!(target.range(), 42..=44);
    assert_eq!(target.display_label(), "src/lib.rs RIGHT:42-44");
    let comment = RemoteReviewComment {
        target,
        body: "Please extract this branch.".into(),
    };
    let request = RemoteReviewRequest {
        commit_id: "abc123".into(),
        body: "Review submitted from Ramo with 1 inline comment.".into(),
        verdict: ReviewVerdict::Comment,
        comments: vec![comment],
    };
    assert_eq!(request.comments.len(), 1);
}
