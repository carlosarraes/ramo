use std::sync::Arc;

use pdiff::markup::{MIN_STML_LAYOUT_WIDTH, StmlLine, layout_stml, layout_stml_cached};

fn text(line: &StmlLine) -> String {
    line.spans.iter().map(|span| span.text.as_str()).collect()
}

#[test]
fn wraps_by_words_honors_breaks_and_hard_slices_long_words() {
    assert_eq!(
        frame("one two three four five", 10).0,
        ["one two", "three four", "five"]
    );
    assert_eq!(frame("first<br>second", 40).0, ["first", "second"]);
    assert_eq!(frame("abcdefghijklmnop", 8).0, ["abcdefgh", "ijklmnop"]);
}

fn frame(markup: &str, width: u16) -> (Vec<String>, Vec<String>) {
    let result = layout_stml(markup, width);
    (result.lines.iter().map(text).collect(), result.errors)
}

#[test]
fn wraps_terminal_cells_and_preserves_inline_styles_and_entities() {
    let result = layout_stml("<b>one two 三 four &rarr; five</b>", 10);
    assert!(result.lines.len() > 1);
    assert!(
        result
            .lines
            .iter()
            .flat_map(|line| &line.spans)
            .all(|span| span.bold)
    );
    assert!(
        result
            .lines
            .iter()
            .map(text)
            .collect::<String>()
            .contains('→')
    );
    assert!(
        result
            .lines
            .iter()
            .all(|line| unicode_width::UnicodeWidthStr::width(text(line).as_str()) <= 10)
    );
}

#[test]
fn cards_rows_lists_code_and_spacers_have_deterministic_exact_geometry() {
    let (card, errors) = frame("<card title=Plan>hi</card>", 20);
    assert!(errors.is_empty());
    assert!(card[0].starts_with("╭─ Plan "));
    assert_eq!(card.last().unwrap(), &format!("╰{}╯", "─".repeat(18)));
    assert!(card.iter().all(|line| line.chars().count() == 20));
    assert_eq!(card.len(), 5);

    let (frameless, _) = frame("<box>hi</box>", 20);
    assert_eq!(frameless, [format!("hi{}", " ".repeat(18))]);
    let (double, _) = frame("<box border border-style=double>x</box>", 10);
    assert_eq!(double[0], format!("╔{}╗", "═".repeat(8)));

    let (row, errors) = frame(
        "<row gap=1><box border>aa</box><box border>bb</box></row>",
        21,
    );
    assert!(errors.is_empty());
    assert!(row[0].contains("┐ ┌"));
    assert!(row[1].contains("aa") && row[1].contains("bb"));

    let (fixed, _) = frame(
        "<row gap=1><box border width=6>a</box><box border>b</box></row>",
        20,
    );
    assert!(fixed[0].starts_with(&format!("┌{}┐ ┌", "─".repeat(4))));
    assert_eq!(unicode_width::UnicodeWidthStr::width(fixed[0].as_str()), 20);
    let (percent, _) = frame(
        "<row gap=1><box border width=50%>a</box><box border>b</box></row>",
        20,
    );
    assert_eq!(
        unicode_width::UnicodeWidthStr::width(percent[0].as_str()),
        20
    );

    let (list, _) = frame(
        "<ol><item>first item that wraps around</item><item>second</item></ol>",
        16,
    );
    assert!(list[0].starts_with("1. first"));
    assert!(list[1].starts_with("   "));
    assert!(list.last().unwrap().starts_with("2. second"));

    let (code, _) = frame(
        "<code title=out>const value = 1;\na_very_long_line_is_clipped;</code>",
        18,
    );
    assert_eq!(code.len(), 4);
    assert!(code[0].contains("out"));
    assert!(
        code.iter()
            .all(|line| unicode_width::UnicodeWidthStr::width(line.as_str()) <= 18)
    );

    let (spaced, _) = frame("a<spacer size=2/>b", 10);
    assert_eq!(spaced, ["a", "", "", "b"]);
}

#[test]
fn headings_badges_rules_backgrounds_and_unknown_tags_degrade_predictably() {
    let result = layout_stml(
        "<h1>Title</h1><badge color=success>OK</badge><hr><box bg=subtle padding=1>x</box><wat>content</wat>",
        20,
    );
    assert!(result.lines[0].spans[0].bold);
    assert!(result.lines[0].spans[0].underline);
    assert_eq!(result.lines[0].spans[0].fg.as_deref(), Some("heading"));
    let badge = result
        .lines
        .iter()
        .find(|line| text(line).contains(" OK "))
        .unwrap();
    assert!(
        badge
            .spans
            .iter()
            .all(|span| span.bg.as_deref() == Some("success"))
    );
    assert!(result.lines.iter().any(|line| text(line) == "─".repeat(20)));
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.contains("unknown tag <wat>"))
    );

    let background = layout_stml("<box bg=subtle padding=1>x</box>", 12);
    assert!(background.lines.iter().all(|line| {
        unicode_width::UnicodeWidthStr::width(text(line).as_str()) == 12
            && line
                .spans
                .iter()
                .all(|span| span.bg.as_deref() == Some("subtle"))
    }));
}

#[test]
fn narrow_rows_stack_and_layout_below_the_minimum_returns_a_note() {
    let result = layout_stml(
        &format!("<row>{}</row>", "<box border>x</box>".repeat(6)),
        9,
    );
    assert!(
        result
            .errors
            .iter()
            .any(|error| error.contains("too narrow"))
    );
    assert!(result.lines.len() >= 3);

    let result = layout_stml("hello", MIN_STML_LAYOUT_WIDTH - 1);
    assert!(result.lines.is_empty());
    assert!(!result.errors.is_empty());
}

#[test]
fn repeated_layout_is_value_deterministic() {
    let markup = "<card title=t><list><item>alpha beta</item></list></card>";
    assert_eq!(layout_stml(markup, 30), layout_stml(markup, 30));
    let first = layout_stml_cached(markup, 30);
    let second = layout_stml_cached(markup, 30);
    let other_width = layout_stml_cached(markup, 31);
    assert!(Arc::ptr_eq(&first, &second));
    assert!(!Arc::ptr_eq(&first, &other_width));
}
