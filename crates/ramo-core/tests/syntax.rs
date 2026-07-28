use ramo_core::syntax::SyntaxHighlighter;

#[test]
fn rust_keywords_and_strings_return_distinct_rgb_spans() {
    let mut highlighter = SyntaxHighlighter::tokyo_night();
    let spans = highlighter.highlight_line("src/lib.rs", None, "let value = \"ramo\";");
    assert_eq!(
        spans
            .iter()
            .map(|span| span.text.as_str())
            .collect::<String>(),
        "let value = \"ramo\";"
    );
    assert!(
        spans
            .windows(2)
            .any(|pair| pair[0].foreground != pair[1].foreground)
    );
}

#[test]
fn plain_text_preserves_content() {
    let mut highlighter = SyntaxHighlighter::tokyo_night();
    let spans = highlighter.highlight_line("README.unknown", None, "ramo ∆");
    assert_eq!(
        spans.into_iter().map(|span| span.text).collect::<String>(),
        "ramo ∆"
    );
}
