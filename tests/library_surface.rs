use pdiff::diff::parser::parse_unified_diff;

#[test]
fn parser_is_available_from_the_library_crate() {
    let files = parse_unified_diff(
        "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old\n+new\n",
    );
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "a.txt");
}
