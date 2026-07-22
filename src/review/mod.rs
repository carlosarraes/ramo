mod anchor;
mod context;
pub(crate) mod emphasis;
pub(crate) mod geometry;
mod navigation;
pub(crate) mod row;
mod selection;
pub(crate) mod state;

pub use context::{
    CollapsedGap, ContextLine, ContextSourceLoader, GapKey, GapPosition, NativeContextSourceLoader,
    SourceFailure, SourceSide, derive_collapsed_gaps, expand_gap_lines, select_gap_for_toggle,
    source_for_context,
};
pub use selection::{
    SelectionPoint, SelectionRow, cell_slice, line_cell_range, project_selection, word_cell_range,
};
pub use state::{
    HunkTarget, ReviewAction, ReviewController, ReviewEffect, ReviewFileSnapshot, ReviewFileStatus,
    ReviewHit, ReviewOptions, ReviewPoint, ReviewPosition, ReviewSide, ReviewSnapshot,
    ReviewViewPreferences, ScrollUnit, SidebarEntrySnapshot, Viewport,
};
