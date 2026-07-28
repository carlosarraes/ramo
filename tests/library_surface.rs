use ramo::diff::parser::parse_unified_diff;

#[test]
fn parser_is_available_from_the_library_crate() {
    let files = parse_unified_diff(
        "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
    );
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "a.txt");
}

#[test]
fn terminal_reexports_core_types_without_wrappers() {
    fn takes_core(_: ramo_core::remote_review::ReviewVerdict) {}
    takes_core(ramo::remote_review::ReviewVerdict::Approve);

    let files: Vec<ramo_core::diff::model::DiffFile> = ramo::diff::parser::parse_unified_diff("");
    assert!(files.is_empty());
}
