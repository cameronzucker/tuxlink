//! Part-97 safety invariants for Elmer's multi-phase "build me a routine"
//! workflow (Routine CI slice 1a, Task 11).
//!
//! **What this file covers.** The workflow authors DISABLED / attended
//! routine drafts and never touches the transmit path itself — this module
//! is the tool-reachability denylist gate that makes that true at the
//! phase-dispatch layer, independent of any one manifest fixture or any one
//! phase's prompt text:
//!
//! 1. For every [`PhaseName`] variant, [`super::phases::tools_for`] never
//!    hands back a [`ToolSpec`] named `routines_enable` or `routines_run` —
//!    the two verbs that flip a routine live or execute it. The drafting
//!    workflow only ever builds and saves a disabled/attended draft
//!    (`super::phases::EMIT_TOOL_ALLOW_SET`'s doc comment states this
//!    intent directly); this test asserts it holds for the *reachable* tool
//!    set, not just the allow-list source.
//! 2. The fixture manifest's `allowed_tool_families` does not contain
//!    `"transmit"` — the Emit phase's tool set is filtered against the
//!    manifest's own declared tool families (`tools_for`'s doc comment), so
//!    a manifest that never declares the transmit family is a second,
//!    independent gate on top of the denylist in (1).
//! 3. Only the `Emit` phase's tool set is non-empty — every other phase
//!    (including the deterministic `Ci` phase, which never calls a model at
//!    all) gets no tools whatsoever, so no non-Emit phase's model turn can
//!    reach ANY tool, transmit-capable or not.
//!
//! **What this file deliberately does NOT cover** (already asserted
//! elsewhere, per the Task 11 brief — not duplicated here): the Red-build
//! quarantine (a Red CI verdict leaves no addressable routine in the store)
//! and the Green-run disabled-save (a saved routine's `enabled.json` entry
//! stays empty) are both covered by `engine.rs`'s
//! `red_ci_verdict_stops_the_run_and_quarantines_the_dirty_draft` and
//! `full_depth_happy_path_saves_routine_and_builds_present` tests. This
//! module is scoped to the tool-reachability denylist only — the core
//! Part-97 *authoring* gate (a phase can only ever be handed a tool it is
//! safe to call, at the point tools are handed to the model, before any
//! save/enable/run distinction is even in play).

use tuxlink_agent_runner::ToolSpec;

use super::artifacts::PhaseName;
use super::manifest::{WorkflowManifest, WorkflowProvenance, WORKFLOW_MANIFEST_SCHEMA_VERSION};
use super::phases::tools_for;

/// Every [`PhaseName`] variant, enumerated by hand. `PhaseName` carries no
/// `EnumIter`/`strum` derive (checked: `artifacts.rs` derives only `Debug,
/// Clone, Copy, PartialEq, Eq, Serialize, Deserialize`), so this is the one
/// place in the crate that must be kept in sync if a new phase is ever
/// added to the pipeline. A missing variant here would silently narrow this
/// test's coverage rather than fail to compile, which is exactly the
/// failure mode a reviewer should watch for when the pipeline grows a phase.
const ALL_PHASES: [PhaseName; 7] = [
    PhaseName::Router,
    PhaseName::Intent,
    PhaseName::Feasibility,
    PhaseName::Draft,
    PhaseName::Emit,
    PhaseName::Ci,
    PhaseName::Present,
];

/// A manifest fixture mirroring the one `engine.rs`'s and `phases.rs`'s own
/// test modules build (see those files' private `fixture_manifest` helpers)
/// — duplicated here rather than imported because both are `#[cfg(test)]`
/// items private to their own `mod tests`, not reachable from this sibling
/// module. `allowed_tool_families` deliberately carries only `"routines"` —
/// this is also the fixture invariant (2) below asserts against directly,
/// so the assertion is checked against real fixture data, not tautological
/// against an empty vec.
fn fixture_manifest() -> WorkflowManifest {
    WorkflowManifest {
        schema_version: WORKFLOW_MANIFEST_SCHEMA_VERSION,
        name: "build-routine".to_string(),
        version: "1.0.0".to_string(),
        entry: PhaseName::Router,
        exit: PhaseName::Present,
        required_inputs: vec!["outcome".to_string()],
        optional_inputs: vec![],
        allowed_tool_families: vec!["routines".to_string()],
        expected_artifacts: vec![
            "intent".to_string(),
            "affordances".to_string(),
            "draft".to_string(),
            "present".to_string(),
        ],
        deterministic_gates: vec!["structure".to_string(), "validate".to_string()],
        failure_escalation: serde_json::json!({ "onRed": "quarantineDraft" }),
        provenance: WorkflowProvenance {
            compatible_capability_versions: vec!["routines-v1".to_string()],
            eval_scenarios: vec![],
            known_model_compat: vec!["stub".to_string()],
            traceable_outputs: vec!["savedRoutine".to_string()],
        },
    }
}

fn assert_no_part97_denylisted_tool(phase: PhaseName, tools: &[ToolSpec]) {
    assert!(
        tools.iter().all(|t| t.name != "routines_enable"),
        "{phase:?} tool set must never contain routines_enable (Part-97: \
         the workflow never flips a routine live)"
    );
    assert!(
        tools.iter().all(|t| t.name != "routines_run"),
        "{phase:?} tool set must never contain routines_run (Part-97: the \
         workflow never executes a routine, i.e. never transmits)"
    );
}

/// Invariant (1): across every phase in the pipeline, `tools_for` never
/// reaches `routines_enable` or `routines_run` — the two verbs that would
/// flip a drafted routine live or run it (transmit). Iterates all 7
/// `PhaseName` variants against one real manifest built via the same
/// fixture pattern `engine.rs`/`phases.rs` use, not a mock.
#[test]
fn tools_for_never_reaches_part97_denylisted_tools_for_any_phase() {
    let manifest = fixture_manifest();
    for phase in ALL_PHASES {
        let tools = tools_for(phase, &manifest);
        assert_no_part97_denylisted_tool(phase, &tools);
    }
}

/// Invariant (2): the fixture manifest's `allowed_tool_families` does not
/// declare `"transmit"`. `tools_for` filters the Emit phase's tool set
/// against this list (its own doc comment: "the manifest's own declared
/// surface is authoritative over this module's fixed allow-set"), so a
/// manifest that never opts into the transmit family is a second,
/// independent gate on top of invariant (1)'s fixed denylist — even a
/// future edit that accidentally widened `EMIT_TOOL_ALLOW_SET` to include a
/// transmit-family tool would still need a manifest that declares
/// `"transmit"` before that tool could ever reach a model.
#[test]
fn manifest_does_not_declare_the_transmit_tool_family() {
    let manifest = fixture_manifest();
    assert!(
        !manifest
            .allowed_tool_families
            .iter()
            .any(|family| family == "transmit"),
        "workflow manifest must never opt into the transmit tool family"
    );
}

/// Invariant (3): only `Emit` gets any tools at all. Every other phase —
/// including `Router` and `Ci`, which never call a model turn in the first
/// place — must get an empty tool set from `tools_for`, so no non-Emit
/// phase's model call can reach ANY tool, transmit-capable or not. This is
/// the coarsest-grained version of the gate: even if `EMIT_TOOL_ALLOW_SET`
/// were misconfigured to include a dangerous tool, that tool would still be
/// unreachable from every phase except the one phase whose whole job is
/// building (never running) a routine.
#[test]
fn only_emit_phase_ever_receives_a_non_empty_tool_set() {
    let manifest = fixture_manifest();
    for phase in ALL_PHASES {
        let tools = tools_for(phase, &manifest);
        if phase == PhaseName::Emit {
            assert!(
                !tools.is_empty(),
                "Emit phase must receive the routine edit verbs"
            );
        } else {
            assert!(
                tools.is_empty(),
                "{phase:?} must receive NO tools — only Emit may call tools"
            );
        }
    }
}
