//! Error types for parsing and executing routines.

/// Errors turning JSON into a [`crate::types::RoutineDef`].
#[derive(Debug, thiserror::Error)]
pub enum RoutineParseError {
    #[error("routine JSON is malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported schema_version {0} (this build supports 1)")]
    UnsupportedSchemaVersion(u32),
}

/// How a single step fails. The `cause` strings are VERBATIM underlying
/// errors (spec §11): the actual VARA disconnect reason, the actual CAT
/// timeout — never a paraphrase.
#[derive(Debug, Clone, PartialEq, thiserror::Error, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum StepError {
    #[error("variable '{0}' is not set on any path reaching this step")]
    UnsetVariable(String),
    #[error("step timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("action '{action}' failed: {cause}")]
    Action { action: String, cause: String },
    #[error("run cancelled")]
    Cancelled,
}
