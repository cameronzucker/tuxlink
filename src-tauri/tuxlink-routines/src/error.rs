//! Error types for parsing and executing routines.

/// Errors turning JSON into a [`crate::types::RoutineDef`].
#[derive(Debug, thiserror::Error)]
pub enum RoutineParseError {
    #[error("routine JSON is malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported schema_version {0} (this build supports 1)")]
    UnsupportedSchemaVersion(u32),
}
