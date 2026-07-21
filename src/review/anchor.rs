use super::geometry::ReviewGeometry;
use super::row::ReviewRowKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewportAnchor {
    pub row_key: Option<ReviewRowKey>,
    pub intra_row: usize,
    pub file_id: Option<String>,
    pub hunk_index: Option<usize>,
    pub absolute_offset: usize,
}

pub(crate) fn capture_viewport_anchor(
    geometry: &ReviewGeometry,
    scroll_top: usize,
    selected_file_id: Option<&str>,
    selected_hunk_index: Option<usize>,
) -> ViewportAnchor {
    let row = geometry.row_at_offset(scroll_top);
    ViewportAnchor {
        row_key: row.map(|row| row.key.clone()),
        intra_row: row.map_or(0, |row| scroll_top.saturating_sub(row.top)),
        file_id: row
            .and_then(|row| geometry.sections.get(row.file_index))
            .map(|section| section.file_id.clone())
            .or_else(|| selected_file_id.map(str::to_owned)),
        hunk_index: row.and_then(|row| row.hunk_index).or(selected_hunk_index),
        absolute_offset: scroll_top,
    }
}

pub(crate) fn restore_viewport_anchor(geometry: &ReviewGeometry, anchor: &ViewportAnchor) -> usize {
    let top = anchor
        .row_key
        .as_ref()
        .and_then(|key| geometry.row_by_key(key))
        .map(|row| {
            row.top
                .saturating_add(anchor.intra_row.min(row.height.saturating_sub(1)))
        })
        .or_else(|| {
            Some((anchor.file_id.as_deref()?, anchor.hunk_index?))
                .and_then(|(file_id, hunk)| geometry.hunk_anchor(file_id, hunk))
                .map(|row| row.top)
        })
        .or_else(|| {
            anchor
                .file_id
                .as_deref()
                .and_then(|file_id| geometry.file_section(file_id))
                .map(|section| section.body_top)
        })
        .unwrap_or(anchor.absolute_offset);
    top.min(geometry.max_scroll_top())
}

#[cfg(test)]
mod tests {
    use crate::diff::model::{DiffFile, FileChangeKind};
    use crate::review::geometry::{GeometryOptions, PlannedFile, build_review_geometry};
    use crate::review::row::{EffectiveLayout, build_row_plan};

    use super::{ViewportAnchor, capture_viewport_anchor, restore_viewport_anchor};

    fn geometry(
        layout: EffectiveLayout,
        wrap_lines: bool,
        content_width: u16,
    ) -> super::ReviewGeometry {
        let mut first = DiffFile::for_test("src/a.rs", FileChangeKind::Modified, 30, 3);
        first.hunks[0].lines[12].content = "a long line that wraps at narrow widths 界界界".into();
        let second = DiffFile::for_test("src/b.rs", FileChangeKind::Modified, 20, 2);
        let files = [first, second]
            .into_iter()
            .map(|file| {
                let plan = build_row_plan(&file, layout, true);
                PlannedFile::new(file.id, plan)
            })
            .collect::<Vec<_>>();
        build_review_geometry(
            &files,
            GeometryOptions {
                content_width,
                viewport_height: 12,
                show_line_numbers: true,
                wrap_lines,
            },
        )
    }

    #[test]
    fn semantic_anchor_survives_layout_and_wrap_changes() {
        let wide = geometry(EffectiveLayout::Split, true, 50);
        let selected = wide
            .rows
            .iter()
            .find(|row| row.height > 1)
            .expect("wrapped row");
        let scroll_top = selected.top + 1;
        let anchor = capture_viewport_anchor(&wide, scroll_top, None, None);

        let narrow = geometry(EffectiveLayout::Stack, true, 36);
        let restored = restore_viewport_anchor(&narrow, &anchor);
        let restored_row = narrow.row_at_offset(restored).unwrap();
        assert_eq!(restored_row.key, selected.key);
        assert_eq!(
            restored.saturating_sub(restored_row.top),
            anchor.intra_row.min(restored_row.height - 1)
        );
    }

    #[test]
    fn missing_row_falls_back_to_hunk_then_file_then_absolute_offset() {
        let geometry = geometry(EffectiveLayout::Stack, false, 80);
        let missing = ViewportAnchor {
            row_key: None,
            intra_row: 0,
            file_id: Some("file:src/b.rs".into()),
            hunk_index: Some(0),
            absolute_offset: 7,
        };
        assert_eq!(
            restore_viewport_anchor(&geometry, &missing),
            geometry.sections[1].body_top
        );

        let absolute = ViewportAnchor {
            file_id: Some("missing".into()),
            hunk_index: None,
            absolute_offset: usize::MAX,
            ..missing
        };
        assert_eq!(
            restore_viewport_anchor(&geometry, &absolute),
            geometry.max_scroll_top()
        );
    }
}
