use ramo::margem::build_margem_document;
use ramo_core::{
    changeset::Changeset,
    diff::model::{DiffFile, FileChangeKind},
};

#[test]
fn changeset_conversion_is_lossless_for_review_fields() {
    let input = Changeset::new(
        "working-tree",
        "Working tree",
        vec![DiffFile::for_test(
            "src/lib.rs",
            FileChangeKind::Modified,
            2,
            1,
        )],
    );
    let output = build_margem_document(&input).unwrap();
    let diff = output.as_diff().unwrap();

    assert_eq!(diff.title(), "Working tree");
    assert_eq!(diff.files()[0].id, input.files[0].id);
    assert_eq!(diff.files()[0].path, "src/lib.rs");
    assert_eq!(diff.totals().additions, 2);
    assert_eq!(diff.totals().deletions, 1);
}
