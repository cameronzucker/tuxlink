//! Error types for parsing and executing routines.

/// Errors turning JSON into a [`crate::types::RoutineDef`].
#[derive(Debug, thiserror::Error)]
pub enum RoutineParseError {
    #[error("routine JSON is malformed: {0}")]
    Json(#[from] serde_json::Error),
    /// A parse failure we could localise to a JSON path before falling back to
    /// serde's field-only message. Same user-visible prefix as [`Self::Json`]
    /// so consumers matching on it are unaffected.
    ///
    /// serde reports the missing FIELD but not WHERE: a step object placed in
    /// `tracks[]` deserialises as a `Track`, so the error reads
    /// "missing field `name`" with a byte offset that is meaningless for the
    /// single-line JSON an agent sends. Observed 2026-07-26: a builder read
    /// that, renamed its correct top-level `routine` key to `name`, and
    /// resent the identical payload 23 times.
    #[error("routine JSON is malformed: {0}")]
    Structural(String),
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

/// Errors resolving `@`-references in a run-start snapshot.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("unresolved reference {0}")]
    UnresolvedRef(String),
    #[error("resolver I/O failure: {0}")]
    Io(String),
}

/// Errors surfaced by the engine facade (`engine.rs`): starting a run,
/// journaling it, and recovering interrupted journals.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("snapshot: {0}")]
    Snapshot(#[from] SnapshotError),
    #[error("journal: {0}")]
    Journal(#[from] std::io::Error),
    #[error("routine '{0}' not found")]
    UnknownRoutine(String),
    #[error("resolved snapshot does not deserialize back to a routine definition: {0}")]
    SnapshotShape(String),
}
