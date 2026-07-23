//! Present-artifact builder for Elmer's workflow (Routine CI slice 1a, Task
//! 9) — the `Present` phase (see [`super::artifacts::PhaseName::Present`]).
//!
//! [`build_present`] is a **template builder, not a model call**: it
//! deterministically fills a [`Present`] from the already-computed
//! [`CiReport`] (Task 5), the [`Draft`] graph (Task 1), and the
//! caller-supplied inferred-decision list (accumulated by earlier phases,
//! not this one). Keeping this phase template-only for slice 1a avoids an
//! extra model round trip on every run; an LLM present phase that writes a
//! richer human summary is noted as a later refinement in the design's open
//! questions.
//!
//! ## `acks_required` heuristic
//!
//! [`Draft`]/[`DraftNode`] carries only `action: String` — no
//! `transmits`/`writes_config` booleans (those live on
//! `tuxlink_routines::action::ActionDescriptor`, which this workflow-artifact
//! layer deliberately does not depend on; see `catalog.rs`'s family-prefix
//! precedent). So the ack heuristic here is a **name-prefix guess**, not a
//! catalog lookup: an action name starting with `"radio."` implies a
//! transmit-capable step and adds `"transmit"`; an action name starting with
//! `"config."` implies a config-writing step and adds `"writes_config"`.
//! **Flagged as approximate for slice 1a**: a real action whose family
//! doesn't follow this convention, or a `radio.*`/`config.*` action that
//! doesn't actually transmit/write, would be mis-tagged. A later refinement
//! could thread the real `AffordanceAction` list (Task 4's output) through
//! so this phase checks the actual `transmits`/`writes_config` flags instead
//! of guessing from the name.

use super::artifacts::{CiReport, Draft, Present};

/// Name-prefix families that imply a transmit-capable or config-writing
/// step, per the module-level "acks_required heuristic" note. Not the same
/// mechanism as `catalog::build_affordance_catalog`'s family filter (which
/// reads real `ActionInfo` families); this is a cheaper guess made from the
/// draft's action-name strings alone.
const TRANSMIT_FAMILY_PREFIX: &str = "radio.";
const WRITES_CONFIG_FAMILY_PREFIX: &str = "config.";

/// Template-fill a [`Present`] from a completed [`CiReport`], the [`Draft`]
/// it was run against, and the inferred-decision list earlier phases
/// accumulated.
///
/// - `built` names the routine shape: step count plus the first step's
///   action, or an explicit "empty routine" message when `draft.nodes` is
///   empty.
/// - `inferred_decisions` is `inferred` copied verbatim — this phase does
///   not add or filter decisions, only carries them into the artifact.
/// - `gaps` is every [`CiReport::findings`] entry whose `severity` is not
///   `"error"` (i.e. warnings, or any future non-error severity), so a
///   `Green`-with-warnings run still surfaces them to the operator. Findings
///   with `severity == "error"` are excluded from `gaps` because a report
///   carrying one is `Red` by [`super::ci::run_routine_ci`]'s own contract,
///   and an error belongs in a failure path, not a "you can still ship this
///   but note" gap list.
/// - `failure_behavior` is a fixed slice-1a default string (no per-routine
///   customization yet — the routine's own `on_interrupted`/failure fields
///   aren't threaded through this artifact layer).
/// - `acks_required` is derived from `draft` per the module-level heuristic.
pub fn build_present(ci: &CiReport, draft: &Draft, inferred: &[String]) -> Present {
    let built = match draft.nodes.first() {
        Some(first) => format!(
            "{}-step routine starting with `{}`",
            draft.nodes.len(),
            first.action
        ),
        None => "empty routine (no steps drafted)".to_string(),
    };

    let gaps = ci
        .findings
        .iter()
        .filter(|f| f.severity != "error")
        .map(|f| f.message.clone())
        .collect();

    Present {
        built,
        inferred_decisions: inferred.to_vec(),
        failure_behavior: "Stops on the first failed step and leaves the routine \
            paused for operator review; no automatic retry in slice 1a."
            .to_string(),
        gaps,
        acks_required: acks_required_for(draft),
    }
}

/// See the module-level "`acks_required` heuristic" doc.
fn acks_required_for(draft: &Draft) -> Vec<String> {
    let needs_transmit_ack = draft
        .nodes
        .iter()
        .any(|node| node.action.starts_with(TRANSMIT_FAMILY_PREFIX));
    let needs_config_ack = draft
        .nodes
        .iter()
        .any(|node| node.action.starts_with(WRITES_CONFIG_FAMILY_PREFIX));

    let mut acks = Vec::new();
    if needs_transmit_ack {
        acks.push("transmit".to_string());
    }
    if needs_config_ack {
        acks.push("writes_config".to_string());
    }
    acks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elmer::workflow::artifacts::{CiFinding, CiVerdict, DraftNode};

    fn draft_with(actions: &[&str]) -> Draft {
        Draft {
            nodes: actions
                .iter()
                .enumerate()
                .map(|(i, action)| DraftNode {
                    id: format!("s{i}"),
                    action: (*action).to_string(),
                    params: serde_json::Value::Null,
                    branch: None,
                })
                .collect(),
        }
    }

    #[test]
    fn green_report_yields_named_routine_with_no_gaps() {
        let ci = CiReport {
            verdict: CiVerdict::Green,
            findings: vec![],
        };
        let draft = draft_with(&["radio.connect", "local.log"]);

        let present = build_present(&ci, &draft, &[]);

        assert!(present.gaps.is_empty());
        assert_eq!(present.built, "2-step routine starting with `radio.connect`");
    }

    #[test]
    fn warning_finding_surfaces_in_gaps_even_on_green() {
        let ci = CiReport {
            verdict: CiVerdict::Green,
            findings: vec![CiFinding {
                code: "NO_RIG_CONFIGURED".to_string(),
                severity: "warning".to_string(),
                message: "no rig configured for radio.connect".to_string(),
            }],
        };
        let draft = draft_with(&["radio.connect"]);

        let present = build_present(&ci, &draft, &[]);

        assert!(present
            .gaps
            .iter()
            .any(|g| g == "no rig configured for radio.connect"));
    }

    #[test]
    fn error_finding_is_excluded_from_gaps() {
        let ci = CiReport {
            verdict: CiVerdict::Red,
            findings: vec![CiFinding {
                code: "UNKNOWN_ACTION".to_string(),
                severity: "error".to_string(),
                message: "radio.mystery is not a registered action".to_string(),
            }],
        };
        let draft = draft_with(&["radio.mystery"]);

        let present = build_present(&ci, &draft, &[]);

        assert!(present.gaps.is_empty());
    }

    #[test]
    fn empty_draft_names_itself_empty() {
        let ci = CiReport {
            verdict: CiVerdict::Green,
            findings: vec![],
        };
        let draft = Draft { nodes: vec![] };

        let present = build_present(&ci, &draft, &[]);

        assert_eq!(present.built, "empty routine (no steps drafted)");
        assert!(present.acks_required.is_empty());
    }

    #[test]
    fn inferred_decisions_pass_through_verbatim() {
        let ci = CiReport {
            verdict: CiVerdict::Green,
            findings: vec![],
        };
        let draft = draft_with(&["local.log"]);
        let inferred = vec!["defaulted retry to 3 attempts".to_string()];

        let present = build_present(&ci, &draft, &inferred);

        assert_eq!(present.inferred_decisions, inferred);
    }

    #[test]
    fn radio_and_config_actions_derive_both_ack_tags() {
        let ci = CiReport {
            verdict: CiVerdict::Green,
            findings: vec![],
        };
        let draft = draft_with(&["radio.connect", "config.set_ardop", "local.log"]);

        let present = build_present(&ci, &draft, &[]);

        assert!(present.acks_required.iter().any(|a| a == "transmit"));
        assert!(present.acks_required.iter().any(|a| a == "writes_config"));
        assert_eq!(present.acks_required.len(), 2);
    }
}
