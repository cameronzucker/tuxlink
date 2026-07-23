//! Task 10 end-to-end integration test: proves the SHIPPED
//! `resources/workflows/build-routine.manifest.json` resource file (not an
//! inline test fixture) both parses via [`super::manifest::load_manifest`]
//! and drives [`super::engine::run_workflow`] through a full happy path.
//!
//! `manifest.rs`'s and `engine.rs`'s own unit tests already cover
//! `load_manifest` and `run_workflow` in isolation against inline fixture
//! JSON/structs; neither proves the on-disk manifest this binary actually
//! ships is itself well-formed and wired correctly end to end. This module
//! closes that gap with a single test that loads the real resource file by
//! its packaged path (`$CARGO_MANIFEST_DIR/resources/workflows/build-routine.manifest.json`)
//! and threads it through a scripted [`super::model::StubModel`] the same
//! way `engine.rs`'s `full_depth_happy_path_saves_routine_and_builds_present`
//! test does — same store-seed test seam (Emit's real job is to drive real
//! `routines_*` tool calls; here the store is pre-seeded "as if" Emit had
//! already saved the routine, and the Emit turn's tool call just names it —
//! see `engine.rs`'s own doc comment on that seam for why).

use std::path::Path;

use tuxlink_agent_runner::{RunOutcome, ToolCall};
use tuxlink_routines::types::{
    OnInterrupted, RoutineDef, Track, TransmitMode, Trigger, SUPPORTED_SCHEMA_VERSION,
};
use tuxlink_routines::validate::StaticContext;

use crate::routines::store::DefinitionStore;

use super::artifacts::{AffordanceAction, Affordances, Draft, DraftNode, Intent};
use super::engine::{run_workflow, WorkflowInputs};
use super::manifest::load_manifest;
use super::model::{PhaseTurn, StubModel};

fn fixture_intent() -> Intent {
    Intent {
        outcome: "connect nearest 20m gateway hourly".to_string(),
        trigger: "schedule: hourly at :00".to_string(),
        success: "mail pulled".to_string(),
        failure: "log and retry next cycle".to_string(),
        side_effects: vec!["radio TX".to_string()],
        persisted_values: vec![],
    }
}

fn fixture_affordances() -> Affordances {
    Affordances {
        actions: vec![AffordanceAction {
            name: "radio.connect".to_string(),
            transmits: true,
            needs_radio: true,
            writes_config: false,
            params: vec!["bands".to_string()],
            outputs: vec!["connected".to_string()],
        }],
        missing_primitives: vec![],
    }
}

fn fixture_draft() -> Draft {
    Draft {
        nodes: vec![DraftNode {
            id: "s1".to_string(),
            action: "radio.connect".to_string(),
            params: serde_json::json!({ "bands": ["20m"] }),
            branch: None,
        }],
    }
}

/// A clean, CI-Green routine the store is pre-seeded with — see this
/// module's doc comment on the store-seed test seam.
fn clean_routine(name: &str) -> RoutineDef {
    RoutineDef {
        routine: name.to_string(),
        schema_version: SUPPORTED_SCHEMA_VERSION,
        transmit_mode: TransmitMode::Attended,
        transmit_ack: None,
        write_ack: None,
        on_interrupted: OnInterrupted::Stay,
        inputs: vec![],
        triggers: vec![Trigger::Manual],
        tracks: vec![Track {
            name: "t".to_string(),
            steps: vec![],
        }],
    }
}

/// An Emit-phase turn whose only tool call names `routine_name` via the
/// top-level `routine` param every edit verb except `routines_save` uses —
/// see `phases.rs::routine_name_from_tool_calls`.
fn emit_turn_naming(routine_name: &str, prompt_tokens: u64) -> PhaseTurn {
    PhaseTurn {
        outcome: RunOutcome::Completed(String::new()),
        tool_calls: vec![ToolCall::new(
            "routines_meta_set",
            serde_json::json!({ "routine": routine_name, "patch": {} }),
        )],
        final_text: String::new(),
        prompt_tokens,
    }
}

/// Loads the real `build-routine.manifest.json` resource this binary
/// ships, drives a scripted Full-depth happy path through `run_workflow`,
/// and confirms the run reaches a saved routine + a built `Present` with no
/// `stopped_reason` — proving the shipped manifest file both parses and
/// drives the engine end to end.
#[tokio::test]
async fn build_routine_manifest_drives_a_full_happy_path_end_to_end() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/workflows/build-routine.manifest.json");
    let manifest = load_manifest(&manifest_path)
        .expect("shipped build-routine manifest must load and parse");

    let dir = tempfile::tempdir().expect("tempdir");
    let store = DefinitionStore::open(dir.path().to_path_buf());

    // Test seam: pre-seed the store with the routine "as if" the Emit
    // phase's real tool-call dispatch had already saved it (see this
    // module's doc comment + `engine.rs`'s own tests for why).
    store
        .save(&clean_routine("hourly-20m-vara-cms"))
        .expect("seed store");

    let stub = StubModel::new(vec![
        PhaseTurn::text("full", 5),
        PhaseTurn::text(
            &serde_json::to_string(&fixture_intent()).expect("serialize"),
            10,
        ),
        PhaseTurn::text(
            &serde_json::to_string(&fixture_affordances()).expect("serialize"),
            10,
        ),
        PhaseTurn::text(
            &serde_json::to_string(&fixture_draft()).expect("serialize"),
            10,
        ),
        emit_turn_naming("hourly-20m-vara-cms", 3),
    ]);

    let run = run_workflow(
        &manifest,
        WorkflowInputs {
            intent_text: "connect nearest 20m gateway hourly".to_string(),
        },
        &stub,
        &StaticContext::new(),
        &store,
    )
    .await;

    assert!(
        run.stopped_reason.is_none(),
        "unexpected stop: {:?}",
        run.stopped_reason
    );
    assert!(run.saved_routine.is_some(), "expected a saved routine name");
    assert!(run.present.is_some(), "expected a built Present artifact");
}
