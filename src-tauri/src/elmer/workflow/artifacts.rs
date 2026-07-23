//! Shared value types for Elmer's routine-authoring support: the affordance
//! catalog projection ([`Affordances`] / [`AffordanceAction`], produced by
//! [`super::catalog::build_affordance_catalog`]) and the deterministic
//! validator's report ([`CiReport`] / [`CiVerdict`] / [`CiFinding`], produced
//! by [`super::ci::run_routine_ci`]).
//!
//! Convention mirrors the `ActionInfo` DTO pattern in
//! `crate::routines::commands`: `Debug, Clone, PartialEq, Serialize,
//! Deserialize` on every type, `#[serde(rename_all = "camelCase")]` on structs,
//! `#[serde(rename_all = "lowercase")]` on simple-tag enums.
//!
//! History: this module previously also held the typed phase artifacts of the
//! discarded multi-phase "Routine CI" workflow engine (Intent / Draft / Present
//! / WorkflowRun / …). Those were removed with the engine (bd tuxlink-t3jci);
//! only the two genuinely reused shapes — the affordance projection and the CI
//! report — remain.

use serde::{Deserialize, Serialize};

/// One catalog action projected down to the compact fields the affordance
/// catalog needs — a subset of `crate::routines::commands::ActionInfo` (which
/// carries UI-only fields like `label`/`example_params` this projection does
/// not need).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffordanceAction {
    pub name: String,
    pub transmits: bool,
    pub needs_radio: bool,
    pub writes_config: bool,
    pub params: Vec<String>,
    pub outputs: Vec<String>,
}

/// The affordance catalog: what the routine action registry can currently do,
/// plus any primitives a caller flagged as missing. Produced deterministically
/// by [`super::catalog::build_affordance_catalog`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Affordances {
    pub actions: Vec<AffordanceAction>,
    pub missing_primitives: Vec<String>,
}

/// Pass/fail verdict for a [`CiReport`] — a simple tag, no payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiVerdict {
    Green,
    Red,
}

/// One finding against a routine — an owned mirror of
/// `tuxlink_routines::validate::findings::Finding`. Owned rather than borrowing
/// `Finding` directly because `Finding.code` is `&'static str` (tied to the
/// validator's static finding vocabulary), which this report does not want to
/// be coupled to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiFinding {
    pub code: String,
    pub severity: String,
    pub message: String,
}

/// The deterministic validator's output: whether a routine passed static
/// validation, and why not if it didn't. Produced by
/// [`super::ci::run_routine_ci`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiReport {
    pub verdict: CiVerdict,
    pub findings: Vec<CiFinding>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_report_green_when_no_errors() {
        let report = CiReport {
            verdict: CiVerdict::Green,
            findings: vec![],
        };
        assert!(matches!(report.verdict, CiVerdict::Green));
    }

    #[test]
    fn affordances_roundtrip_through_json() {
        let aff = Affordances {
            actions: vec![AffordanceAction {
                name: "radio.connect".into(),
                transmits: true,
                needs_radio: true,
                writes_config: false,
                params: vec!["bands".into()],
                outputs: vec!["connected".into()],
            }],
            missing_primitives: vec![],
        };
        let json = serde_json::to_string(&aff).unwrap();
        let back: Affordances = serde_json::from_str(&json).unwrap();
        assert_eq!(aff, back);
    }
}
