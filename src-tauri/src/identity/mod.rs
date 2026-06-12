//! Identity core — the capability/handle model.
//!
//! Phase 1 (tuxlink-d4wp) of the multiple/tactical-callsigns feature. Pure
//! types + unit tests, wired to nothing. Later phases consume these verbatim.
//!
//! Spec: docs/superpowers/specs/2026-06-10-multiple-tactical-callsigns-design.md
//! Master plan: docs/superpowers/plans/2026-06-10-tactical-callsigns-master-plan.md
//!
//! The handle model makes unauthorized transmit a TYPE error: `IdentityHandle`
//! is non-`Serialize`, in-memory only, and constructible ONLY inside
//! `service::IdentityService::authenticate` after a keyring activation-secret
//! constant-time compare. Transmit/listen APIs (later phases) take a handle,
//! never a raw `Config` callsign.

pub mod address;
pub mod cms_verify;
pub mod commands;
pub mod handle;
pub mod keyring_keys;
pub mod service;
pub mod store;

pub use address::{Address, Callsign};
pub use cms_verify::{cms_gate_decision, CmsGateDecision, RefuseReason, TacticalRegistrationVerifier, VerifyError};
pub use handle::{IdentityHandle, SessionIdentity};
pub use service::IdentityService;
pub use store::{FullIdentity, IdentityStore, TacticalCmsState, TacticalIdentity};

/// Errors surfaced by the identity core. Variant names are the canonical
/// interface-contract names (master plan §"Canonical interface contract").
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdentityError {
    /// A `Callsign::parse` input failed the loose-callsign validator. Carries the
    /// first-violated-rule slug from `config::validate_identity_describe`.
    #[error("invalid callsign: {0}")]
    InvalidCallsign(String),
    /// A tactical label failed validation (empty, >24 chars, or not ASCII-printable).
    #[error("invalid tactical label: {0}")]
    InvalidTactical(String),
    /// No FULL or tactical identity matched the requested `Address`.
    #[error("unknown identity")]
    UnknownIdentity,
    /// `add_tactical` referenced a parent callsign that is not a known FULL identity.
    #[error("parent FULL identity not found")]
    ParentNotFound,
    /// `remove` targeted a FULL identity that still has tactical children.
    #[error("cannot remove a FULL identity that still has tactical labels")]
    RemoveHasTacticals,
    /// `authenticate` found no activation secret stored for the callsign.
    #[error("no activation secret set for this identity")]
    NoSecretSet,
    /// `authenticate`'s entered credential did not match the stored activation secret.
    #[error("credential does not match")]
    CredentialMismatch,
    /// The OS keyring backend returned an unexpected error.
    #[error("keyring backend error: {0}")]
    Keyring(String),
    /// A filesystem error reading/writing the identity store.
    #[error("identity store io error: {0}")]
    Io(String),
}
