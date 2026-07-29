use std::sync::Arc;

use ramo_core::review_map::ReviewMapFailureCode;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewMapFailure {
    pub code: ReviewMapFailureCode,
    pub message: String,
    #[serde(skip)]
    source: Option<Arc<dyn std::error::Error + Send + Sync>>,
}

impl ReviewMapFailure {
    pub fn new(code: ReviewMapFailureCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        code: ReviewMapFailureCode,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            source: Some(Arc::new(source)),
        }
    }
}

impl std::fmt::Debug for ReviewMapFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReviewMapFailure")
            .field("code", &self.code)
            .field("message", &self.message)
            .field("source", &self.source.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl std::fmt::Display for ReviewMapFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReviewMapFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}
