pub(crate) mod context;
mod model;

pub use context::{
    AgentContextError, AgentContextSource, MAX_AGENT_ANNOTATIONS, MAX_AGENT_CONTEXT_BYTES,
    MAX_AGENT_FILES, load_agent_context, parse_agent_context,
};
pub use model::{
    AgentContext, AgentFileContext, LineRange, NoteConfidence, NoteSource, ReviewNote,
};
