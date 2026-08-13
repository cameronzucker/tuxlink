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

/// Whose doing a step failure was — the one vocabulary every failure at this
/// boundary answers in, as DATA rather than prose (mutation-contract epic,
/// mirroring the bench's `UnitClass` discipline).
///
/// # Why this exists
///
/// `StepError::Action`'s `cause` pooled four different facts into one string:
/// the authored step asking for something illegal, the radio being busy, the
/// backend being offline, and our own code breaking. Distinguishing them
/// after the fact meant reading prose — the `tuxlink-0rc3h` attribution took
/// reading 2274 tool calls by hand; with this field it is a group-by over
/// journals.
///
/// # The classes
///
/// - [`Invalid`](Self::Invalid): the authored step violates the contract —
///   malformed params, an unknown form, a reference to nothing, a value out
///   of range. Attributable to the AUTHOR (operator or agent); retrying the
///   same step yields the same refusal.
/// - [`Unavailable`](Self::Unavailable): the step is fine and the product
///   cannot do it right now — backend offline, radio busy, modem not
///   running. Environment and timing; retrying later can succeed.
/// - [`Service`](Self::Service): a backing service seam failed with a cause
///   this boundary has NOT classified deeper (the seam returns a bare
///   string). Truthfully unattributed rather than guessed — the bench kept
///   `ProviderError` as its own class for exactly this reason. Refining a
///   seam's errors into `Unavailable`/`Internal` retires its uses of this.
/// - [`Denied`](Self::Denied): the step is well-formed and AUTHORITY for it
///   is absent — the consent gate refused an automatic child whose closure
///   digest no longer binds ("callee changed after acknowledgment"). Neither
///   the author's defect nor the environment: the fix is an operator act
///   (re-acknowledge). ATTENDED runs never mint this — their gates PARK
///   (`AwaitingConsent`) instead of failing; only the automatic-child path,
///   where there is nobody to park for, refuses. Mirrors
///   `WritePortError::Denied` at the MCP boundary.
/// - [`Internal`](Self::Internal): provably our bug — a poisoned lock, a
///   panic, serialization of our own types failing. Never the author's.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Invalid,
    Denied,
    Unavailable,
    Service,
    Internal,
}

impl Disposition {
    /// Whether this failure is a statement about the AUTHOR of the step
    /// (their step violated the contract), as opposed to the environment or
    /// us. The scoring predicate: everything else must never be counted
    /// against the author.
    pub fn is_author_attributable(self) -> bool {
        matches!(self, Self::Invalid)
    }

    /// Whether retrying the same step later can plausibly succeed without
    /// anyone changing anything. Deliberately conservative: [`Service`]
    /// (unattributed) does NOT auto-qualify — deciding a seam's failures are
    /// retryable is exactly the per-seam refinement that retires the class.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Unavailable)
    }
}

/// How a single step fails. The `cause` strings are VERBATIM underlying
/// errors (spec §11): the actual VARA disconnect reason, the actual CAT
/// timeout — never a paraphrase.
///
/// Construct `Action` through [`StepError::invalid`] / [`unavailable`](StepError::unavailable)
/// / [`service`](StepError::service) / [`internal`](StepError::internal) so
/// every mint site states whose doing the failure was. The struct form stays
/// public for matching; `disposition` is `Option` ONLY because journals
/// written before the vocabulary existed must still parse (`None` =
/// "predates classification", the same additive-field pattern as
/// `RunStarted::dry_run`) — new code always sets it.
#[derive(Debug, Clone, PartialEq, thiserror::Error, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum StepError {
    #[error("variable '{0}' is not set on any path reaching this step")]
    UnsetVariable(String),
    #[error("step timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("action '{action}' failed: {cause}")]
    Action {
        action: String,
        cause: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        disposition: Option<Disposition>,
    },
    #[error("run cancelled")]
    Cancelled,
}

impl StepError {
    fn action_with(
        disposition: Disposition,
        action: impl Into<String>,
        cause: impl Into<String>,
    ) -> Self {
        Self::Action {
            action: action.into(),
            cause: cause.into(),
            disposition: Some(disposition),
        }
    }

    /// The authored step violates the contract. Author-attributable.
    pub fn invalid(action: impl Into<String>, cause: impl Into<String>) -> Self {
        Self::action_with(Disposition::Invalid, action, cause)
    }

    /// Authority for a well-formed step is absent. Operator action fixes it.
    pub fn denied(action: impl Into<String>, cause: impl Into<String>) -> Self {
        Self::action_with(Disposition::Denied, action, cause)
    }

    /// The product cannot do it right now. Environment; retryable.
    pub fn unavailable(action: impl Into<String>, cause: impl Into<String>) -> Self {
        Self::action_with(Disposition::Unavailable, action, cause)
    }

    /// A backing service failed with a cause not classified deeper.
    pub fn service(action: impl Into<String>, cause: impl Into<String>) -> Self {
        Self::action_with(Disposition::Service, action, cause)
    }

    /// Provably our bug.
    pub fn internal(action: impl Into<String>, cause: impl Into<String>) -> Self {
        Self::action_with(Disposition::Internal, action, cause)
    }

    /// Whose doing this failure was, when the variant answers at all.
    ///
    /// `UnsetVariable` is BY NATURE the authored definition's doing, and
    /// `Timeout` is by nature the environment not answering within the
    /// budget, so both answer statically. `Cancelled` is an act, not a
    /// fault — it answers `None`. An `Action` from an old journal answers
    /// `None` ("predates classification").
    pub fn disposition(&self) -> Option<Disposition> {
        match self {
            Self::UnsetVariable(_) => Some(Disposition::Invalid),
            Self::Timeout { .. } => Some(Disposition::Unavailable),
            Self::Action { disposition, .. } => *disposition,
            Self::Cancelled => None,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── The disposition contract (mutation-contract epic, slice a) ─────────

    /// New entries journal the disposition as data.
    #[test]
    fn a_classified_error_serializes_its_disposition() {
        let e = StepError::invalid("local.compose", "to must have at least one recipient");
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["kind"], "action");
        assert_eq!(json["detail"]["disposition"], "invalid");
        assert_eq!(
            json["detail"]["cause"],
            "to must have at least one recipient"
        );
    }

    /// A journal written before the vocabulary existed still parses; its
    /// errors answer "predates classification," never a guessed class.
    #[test]
    fn a_pre_vocabulary_journal_entry_parses_with_no_disposition() {
        let legacy = serde_json::json!({
            "kind": "action",
            "detail": { "action": "radio.connect", "cause": "VARA: BUSY channel occupied" }
        });
        let e: StepError = serde_json::from_value(legacy).unwrap();
        assert_eq!(e.disposition(), None);
        match e {
            StepError::Action { action, cause, disposition } => {
                assert_eq!(action, "radio.connect");
                assert_eq!(cause, "VARA: BUSY channel occupied");
                assert_eq!(disposition, None);
            }
            other => panic!("expected Action, got {other:?}"),
        }
    }

    /// The operator-facing message is UNCHANGED by classification: the
    /// disposition is data beside the message, never inside it. This string
    /// is the exact pre-vocabulary Display output.
    #[test]
    fn display_is_byte_identical_with_and_without_a_disposition() {
        let classified = StepError::unavailable("radio.connect", "backend offline");
        let legacy = StepError::Action {
            action: "radio.connect".into(),
            cause: "backend offline".into(),
            disposition: None,
        };
        assert_eq!(
            classified.to_string(),
            "action 'radio.connect' failed: backend offline"
        );
        assert_eq!(classified.to_string(), legacy.to_string());
    }

    /// The scoring predicate: only an INVALID step counts against its author.
    #[test]
    fn only_invalid_is_author_attributable() {
        assert!(Disposition::Invalid.is_author_attributable());
        assert!(!Disposition::Denied.is_author_attributable());
        assert!(!Disposition::Unavailable.is_author_attributable());
        assert!(!Disposition::Service.is_author_attributable());
        assert!(!Disposition::Internal.is_author_attributable());
    }

    /// Retry policy is deliberately conservative: Unavailable only. Service
    /// stays unretried until a seam's failures are classified deeper.
    #[test]
    fn only_unavailable_is_retryable() {
        assert!(Disposition::Unavailable.is_retryable());
        assert!(!Disposition::Invalid.is_retryable());
        assert!(
            !Disposition::Denied.is_retryable(),
            "a denied step needs an operator act, not a retry"
        );
        assert!(!Disposition::Service.is_retryable());
        assert!(!Disposition::Internal.is_retryable());
    }

    /// The variants whose nature answers the question answer it statically.
    #[test]
    fn static_variants_answer_by_nature() {
        assert_eq!(
            StepError::UnsetVariable("x".into()).disposition(),
            Some(Disposition::Invalid),
            "an unset variable is the authored definition's doing"
        );
        assert_eq!(
            StepError::Timeout { seconds: 30 }.disposition(),
            Some(Disposition::Unavailable),
            "a timeout is the environment not answering within the budget"
        );
        assert_eq!(StepError::Cancelled.disposition(), None);
    }
}
