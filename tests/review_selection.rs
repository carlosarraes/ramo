use pdiff::review::{
    SelectionPoint, SelectionRow, cell_slice, line_cell_range, project_selection, word_cell_range,
};

#[test]
fn cell_ranges_never_split_wide_characters_or_include_the_next_ascii_cell() {
    let text = "a界b";
    assert_eq!(cell_slice(text, 1..3), "界");
    assert_eq!(cell_slice(text, 2..3), "界");
    assert_eq!(cell_slice(text, 3..4), "b");
    assert_eq!(cell_slice("e\u{301}x", 0..1), "e\u{301}");
}

#[test]
fn split_projection_chooses_one_side_from_terminal_x() {
    let rows = [SelectionRow::split("old value", "new value", 12)];
    assert_eq!(
        project_selection(
            &rows,
            SelectionPoint::new(0, 13),
            SelectionPoint::new(0, 16),
        ),
        "new"
    );
    assert_eq!(
        project_selection(&rows, SelectionPoint::new(0, 0), SelectionPoint::new(0, 3),),
        "old"
    );
}

#[test]
fn stack_projection_preserves_displayed_semantic_order() {
    let rows = [SelectionRow::stack("old"), SelectionRow::stack("new")];
    assert_eq!(
        project_selection(&rows, SelectionPoint::new(0, 0), SelectionPoint::new(1, 3),),
        "old\nnew"
    );
}

#[test]
fn word_and_line_expansion_return_terminal_cell_ranges() {
    let text = "let 变量 = value";
    assert_eq!(word_cell_range(text, 5), 4..8);
    assert_eq!(cell_slice(text, word_cell_range(text, 5)), "变量");
    assert_eq!(line_cell_range(text), 0..16);
}
