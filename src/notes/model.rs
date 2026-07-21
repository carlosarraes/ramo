#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

impl LineRange {
    pub fn inclusive(self) -> (u32, u32) {
        (self.start, self.end)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteConfidence {
    Low,
    Medium,
    High,
}

impl NoteConfidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteSource {
    Ai,
    Agent,
    User,
    Named(String),
}

impl NoteSource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Ai => "ai",
            Self::Agent => "agent",
            Self::User => "user",
            Self::Named(value) => value,
        }
    }

    pub(crate) fn from_raw(value: Option<String>) -> Self {
        match value.as_deref() {
            None | Some("") | Some("ai") => Self::Ai,
            Some("agent" | "mcp") => Self::Agent,
            Some("user") => Self::User,
            Some(_) => Self::Named(value.expect("a named source has a value")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewNote {
    pub id: Option<String>,
    pub old_range: Option<LineRange>,
    pub new_range: Option<LineRange>,
    pub summary: String,
    pub rationale: Option<String>,
    pub markup: Option<String>,
    pub tags: Vec<String>,
    pub confidence: Option<NoteConfidence>,
    pub source: NoteSource,
    pub title: Option<String>,
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub editable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFileContext {
    pub path: String,
    pub summary: Option<String>,
    pub annotations: Vec<ReviewNote>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContext {
    pub version: u32,
    pub summary: Option<String>,
    pub files: Vec<AgentFileContext>,
}
