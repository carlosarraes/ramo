pub(crate) mod context;
mod model;
mod target;

pub use context::{
    AgentContextError, AgentContextSource, MAX_AGENT_ANNOTATIONS, MAX_AGENT_CONTEXT_BYTES,
    MAX_AGENT_FILES, load_agent_context, parse_agent_context,
};
pub use model::{
    AgentContext, AgentFileContext, LineRange, NoteConfidence, NoteSource, ReviewNote,
};
pub use target::{
    ClearedSessionNotes, HumanNote, HumanNoteDraft, LiveNote, LiveNoteInput, NoteAnchorSide,
    NoteBoxLayout, NoteTarget, annotated_hunks, annotation_range_label, note_box_layout,
    note_source, resolve_note_target, resolve_ranges_target, stable_note_id,
};
