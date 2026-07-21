use pdiff::markup::{StmlElement, StmlNode, StmlParseLimits, decode_stml_entities, parse_stml};

fn first_element(markup: &str) -> StmlElement {
    parse_stml(markup, StmlParseLimits::default())
        .nodes
        .into_iter()
        .find_map(|node| match node {
            StmlNode::Element(element) => Some(element),
            StmlNode::Text(_) => None,
        })
        .expect("expected an element")
}

#[test]
fn parses_nested_elements_attributes_void_tags_and_comments() {
    let element = first_element(
        "<!-- hidden --><box border-style=rounded title=\"Auth flow\"><text>hi<br>there</text></box>",
    );
    assert_eq!(element.tag, "box");
    assert_eq!(element.attrs["border-style"], "rounded");
    assert_eq!(element.attrs["title"], "Auth flow");
    let StmlNode::Element(text) = &element.children[0] else {
        panic!("expected nested text element");
    };
    assert!(matches!(text.children[1], StmlNode::Element(ref item) if item.tag == "br"));
}

#[test]
fn keeps_bare_angles_and_raw_code_content_verbatim() {
    let result = parse_stml("a < b and 3<4", StmlParseLimits::default());
    assert!(result.errors.is_empty());
    assert_eq!(result.nodes, [StmlNode::Text("a < b and 3<4".into())]);

    let code = first_element("<code>const a = <b>1</b>;\n  keep spaces</CODE>");
    assert_eq!(
        code.children,
        [StmlNode::Text("const a = <b>1</b>;\n  keep spaces".into())]
    );
}

#[test]
fn malformed_markup_returns_best_effort_nodes_and_bounded_diagnostics() {
    let result = parse_stml(
        "hello</box><box><text>hi</box><b>open",
        StmlParseLimits {
            max_errors: 3,
            ..StmlParseLimits::default()
        },
    );
    assert!(!result.nodes.is_empty());
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.contains("stray closing tag"))
    );
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.contains("implicitly closed"))
    );
    assert!(result.errors.len() <= 3);
}

#[test]
fn sanitizes_terminal_controls_in_text_and_attributes() {
    let element =
        first_element("<text fg=\"\u{1b}[31mred\">danger\u{1b}[2Jzone\u{1b}]8;;bad\u{1b}\\</text>");
    assert_eq!(element.attrs["fg"], "red");
    assert_eq!(element.children, [StmlNode::Text("dangerzone".into())]);
}

#[test]
fn enforces_utf8_byte_node_and_depth_limits_without_panicking() {
    let truncated = parse_stml(
        "ééééé",
        StmlParseLimits {
            max_input_bytes: 7,
            ..StmlParseLimits::default()
        },
    );
    assert_eq!(truncated.nodes, [StmlNode::Text("ééé".into())]);
    assert!(truncated.errors[0].contains("input truncated"));

    let nodes = parse_stml(
        &"<b>x</b>".repeat(50),
        StmlParseLimits {
            max_nodes: 10,
            ..StmlParseLimits::default()
        },
    );
    assert!(
        nodes
            .errors
            .iter()
            .any(|error| error.contains("node limit"))
    );

    let deep = parse_stml(
        &format!("{}x{}", "<box>".repeat(40), "</box>".repeat(40)),
        StmlParseLimits {
            max_depth: 5,
            ..StmlParseLimits::default()
        },
    );
    assert!(
        deep.errors
            .iter()
            .any(|error| error.contains("depth limit"))
    );
}

#[test]
fn decodes_known_named_and_numeric_entities_but_keeps_unknown_values() {
    assert_eq!(
        decode_stml_entities("&lt;a&gt; &amp; &#65;&#x42; &rarr; &check;"),
        "<a> & AB → ✓"
    );
    assert_eq!(
        decode_stml_entities("&unknown; &#x110000;"),
        "&unknown; &#x110000;"
    );
}
