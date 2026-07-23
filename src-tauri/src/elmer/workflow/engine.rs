//! Workflow engine (Routine CI slice 1a, Task 6): the linear
//! phase-orchestration spine — the integration point that ties the manifest
//! (Task 2), the model port (Task 3), phase prompt-building + artifact
//! capture (Task 7), Routine CI (Task 5), the router (Task 8), and the
//! template `Present` builder (Task 9) into one `run_workflow` call.
//!
//! **The context-bound invariant.** Every phase's prompt is built via
//! [`super::phases::build_prompt`], which — of the PRIOR-PHASE ARTIFACTS —
//! renders ONLY the ones that phase's `declared_inputs` row names, never a
//! prior phase's raw `PhaseTurn::final_text`, never the full artifact set.
//! (It also renders `inputs.intent_text`, the operator's raw request, on
//! every phase; that is the workflow's ground-truth input, not a prior-phase
//! artifact, so it is outside the invariant.) This module's job is to thread
//! [`super::phases::CapturedArtifact`]s through `collected` and hand
//! `build_prompt` that list on every call; it never concatenates a prior
//! turn's raw text into a later prompt itself. The engine test module
//! below proves the WIRING holds (that this file actually calls
//! `build_prompt` with the right `collected` slice on every phase), not the
//! rendering rule itself — `phases.rs`'s own tests already cover that a
//! phase's `declared_inputs` row is honored by `build_prompt` in isolation.
//!
//! **Linear, no repair.** A Red CI verdict or a phase-capture failure stops
//! the run immediately (`stopped_reason` set, run returned as-is) — slice
//! 1a has no CI-repair loop (that is slice 1b, explicitly out of scope
//! here). A Red verdict additionally quarantines the dirty draft: the Emit
//! phase already saved SOMETHING to the store (that is how `capture_artifact`
//! confirms Emit succeeded), and a routine that failed Routine CI must not
//! be left addressable in the store as a normal saved routine.
//!
//! **Router-token recording — the choice made here.** [`super::router::select_depth`]
//! returns only a [`super::artifacts::Depth`], with nowhere to hand back the
//! router turn's `prompt_tokens` for this module's `PhaseRecord`. The Task 6
//! brief offered two ways to close that gap: (a) have the engine rebuild the
//! router prompt itself via `build_prompt(PhaseName::Router, ..)` and parse
//! the answer with a new `router::parse_depth`, or (b) add a router variant
//! that returns `(Depth, u64)`. This module uses (b) —
//! [`super::router::select_depth_with_tokens`] — because (a) has no way to
//! hand back the router turn's `prompt_tokens` (`build_prompt` returns only a
//! prompt) and because the router classifies from `router::classification_prompt`'s
//! own wording, not the generic per-phase instruction `build_prompt` leads
//! with. (Historical note: `build_prompt` used to render nothing but the
//! phase's static instruction for Router — `declared_inputs(Router)` is empty
//! — so path (a) would also have dropped the operator's ask; the F1 fix now
//! renders `intent_text` on every phase, so that hazard is gone, but the two
//! reasons above still favor (b).) `router.rs` still gained `parse_depth` as
//! the extracted, pure parsing half (used internally by both `select_depth`
//! and `select_depth_with_tokens`), since the brief also asked for it and it
//! is a harmless, purely-additive refactor — but the engine calls the
//! token-aware variant, not `build_prompt(Router, ..)`.

use tuxlink_routines::validate::ValidationContext;

use crate::routines::store::DefinitionStore;

use super::artifacts::{CiVerdict, Depth, Draft, PhaseName, PhaseRecord, WorkflowRun};
use super::ci::run_routine_ci;
use super::manifest::WorkflowManifest;
use super::model::PhaseModel;
use super::phases::{build_prompt, capture_artifact, tools_for, CapturedArtifact};
use super::present::build_present;
use super::router::select_depth_with_tokens;

/// The workflow's per-run input: the operator's (or agent's) raw stated
/// intent, before any phase has captured it into the typed [`super::artifacts::Intent`]
/// artifact. This is what seeds the Router phase's classification prompt;
/// once the Intent phase runs, the typed artifact — not this string — is
/// what every later phase's prompt actually renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowInputs {
    pub intent_text: String,
}

/// `Depth::Full`'s model-turn phase list: every artifact phase runs.
/// `Router`, `Ci`, and `Present` are handled specially by [`run_workflow`]
/// (Router before this list, Ci/Present after it) — neither is a
/// `PhaseModel` call in slice 1a (Ci is deterministic; Present is a
/// template builder, not a model call — see [`super::present`]'s module
/// doc), so neither belongs in a "phases to run through `model.run_phase`"
/// list.
const FULL_PHASES: &[PhaseName] = &[
    PhaseName::Intent,
    PhaseName::Feasibility,
    PhaseName::Draft,
    PhaseName::Emit,
];

/// `Depth::Minimal`'s model-turn phase list: collapses `Feasibility` +
/// `Draft` — the router judged the ask simple enough that the drafting
/// phases add no value, so Emit goes straight off the captured `Intent`
/// alone (per [`super::artifacts::Depth`]'s own doc comment: "Minimal
/// collapses the front phases ... and runs Author -> Ci -> Present").
/// `Emit`'s `declared_inputs` still name `affordances`/`draft` (Task 7's
/// `phases::declared_inputs`), but `build_prompt` only renders an artifact
/// that is actually present in `collected` — under `Minimal` those two
/// simply never populate, so the Emit prompt renders `intent` alone. No
/// change to `phases.rs` was needed to support this collapse.
const MINIMAL_PHASES: &[PhaseName] = &[PhaseName::Intent, PhaseName::Emit];

/// Run one workflow end to end: select a depth, walk that depth's phase
/// list (each phase's prompt built from only the artifacts it declares),
/// run Routine CI against whatever the Emit phase saved, and — on a Green
/// verdict — build the template `Present` artifact. Stops and reports
/// `stopped_reason` on the first phase-capture failure or a Red CI verdict;
/// slice 1a never repairs and retries.
pub async fn run_workflow(
    manifest: &WorkflowManifest,
    inputs: WorkflowInputs,
    model: &dyn PhaseModel,
    ctx: &dyn ValidationContext,
    store: &DefinitionStore,
) -> WorkflowRun {
    let mut phases_run: Vec<PhaseRecord> = Vec::new();
    let mut collected: Vec<CapturedArtifact> = Vec::new();

    // 1. Router: selects the depth this run walks. Uses the token-aware
    // variant so the Router phase gets a real `PhaseRecord` like every
    // other phase — see the module doc's "Router-token recording" note.
    let (depth, router_prompt_tokens) =
        select_depth_with_tokens(&inputs.intent_text, model).await;
    phases_run.push(PhaseRecord {
        name: PhaseName::Router,
        prompt_tokens: router_prompt_tokens,
        outcome: format!("selected {depth:?} depth"),
    });

    let phase_list: &[PhaseName] = match depth {
        Depth::Full => FULL_PHASES,
        Depth::Minimal => MINIMAL_PHASES,
    };

    // 2. Walk the model-turn phases for this depth. Each iteration builds
    // its prompt from ONLY `collected` (the context-bound invariant) —
    // never from a prior `PhaseTurn`'s raw text.
    for &phase in phase_list {
        let prompt = build_prompt(phase, manifest, &inputs.intent_text, &collected);
        let tools = tools_for(phase, manifest);
        let turn = model.run_phase(prompt, &tools).await;

        match capture_artifact(phase, &turn, store) {
            Ok(artifact) => {
                phases_run.push(PhaseRecord {
                    name: phase,
                    prompt_tokens: turn.prompt_tokens,
                    outcome: format!("captured {}", artifact.kind()),
                });
                // Feasibility gate (Task 1 honesty mechanism): a non-empty
                // `missing_primitives` means the intent needs something the
                // live catalog does not offer, so the intent is NOT
                // expressible. Stop here with an honest gap report rather than
                // drafting/emitting a routine against a capability that does
                // not exist (the silent-stub / confabulated-proxy failure the
                // honesty task measures against). The affordance catalog is
                // deterministically enumerated from the live surface (Task 4),
                // so `missing_primitives` is a checkable set-difference, not a
                // model guess.
                let capability_gap = if phase == PhaseName::Feasibility {
                    match &artifact {
                        CapturedArtifact::Affordances(aff)
                            if !aff.missing_primitives.is_empty() =>
                        {
                            Some(aff.missing_primitives.join(", "))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                collected.push(artifact);
                if let Some(missing) = capability_gap {
                    return WorkflowRun {
                        depth,
                        phases_run,
                        saved_routine: None,
                        present: None,
                        stopped_reason: Some(format!(
                            "capability gap: the intent needs primitives the catalog \
                             does not offer: {missing}"
                        )),
                    };
                }
            }
            Err(err) => {
                phases_run.push(PhaseRecord {
                    name: phase,
                    prompt_tokens: turn.prompt_tokens,
                    outcome: format!("failed: {err}"),
                });
                return WorkflowRun {
                    depth,
                    phases_run,
                    saved_routine: None,
                    present: None,
                    stopped_reason: Some(format!(
                        "{phase:?} phase did not produce a usable artifact: {err}"
                    )),
                };
            }
        }
    }

    // 3. Ci: deterministic, no model turn. Reads the routine the Emit
    // phase's tool calls saved back off `store` (never trusts a value
    // threaded through `collected` — `CapturedArtifact::Emitted` carries
    // only the name, by design; see `phases.rs`'s doc comment on it).
    let Some(routine_name) = collected.iter().find_map(|a| match a {
        CapturedArtifact::Emitted { routine_name } => Some(routine_name.clone()),
        _ => None,
    }) else {
        // Unreachable in practice: `capture_artifact(Emit, ..)` itself
        // errors (handled above) whenever the Emit phase's tool calls
        // named no routine or the store has no such routine, so every
        // `Ok` `Emitted` artifact already carries a name. Guarded anyway
        // rather than `.expect`-ing, since Emit is always the last entry
        // in both `FULL_PHASES` and `MINIMAL_PHASES` and a future phase
        // reorder should not be able to turn this into a panic.
        return WorkflowRun {
            depth,
            phases_run,
            saved_routine: None,
            present: None,
            stopped_reason: Some(
                "emit phase completed but produced no routine name to run CI against"
                    .to_string(),
            ),
        };
    };

    let Some(def) = store.get(&routine_name) else {
        return WorkflowRun {
            depth,
            phases_run,
            saved_routine: None,
            present: None,
            stopped_reason: Some(format!(
                "routine {routine_name:?} vanished from the store between emit and CI"
            )),
        };
    };

    let ci_report = run_routine_ci(&def, ctx);
    phases_run.push(PhaseRecord {
        name: PhaseName::Ci,
        prompt_tokens: 0,
        outcome: format!("{:?}", ci_report.verdict),
    });

    if ci_report.verdict == CiVerdict::Red {
        // Quarantine: Emit already saved the dirty draft under
        // `routine_name` (that is how capture_artifact confirmed Emit
        // succeeded); a routine that failed Routine CI must not be left
        // addressable in the store as an ordinary saved routine. Slice 1a
        // has no repair loop, so the only sound move is to remove it.
        // Deletion failure (routine already gone, disk error) is not itself
        // grounds for a different `stopped_reason` — the CI-red finding is
        // the actionable fact either way — so it is swallowed here rather
        // than escalated to a second failure mode.
        let _ = store.delete(&routine_name);

        let error_summary: Vec<String> = ci_report
            .findings
            .iter()
            .filter(|f| f.severity == "error")
            .map(|f| format!("{}: {}", f.code, f.message))
            .collect();
        return WorkflowRun {
            depth,
            phases_run,
            saved_routine: None,
            present: None,
            stopped_reason: Some(format!(
                "routine CI red for {routine_name:?}, draft quarantined: {}",
                error_summary.join("; ")
            )),
        };
    }

    // 4. Present: a template build (Task 9), not a model call — see
    // `present.rs`'s module doc. `draft` comes from whatever Draft artifact
    // this run actually collected; a `Minimal` run never collects one (the
    // depth collapses the drafting phases), so this falls back to an empty
    // Draft, matching `build_present`'s own "empty routine" rendering for
    // an empty node list.
    let draft = collected
        .iter()
        .find_map(|a| match a {
            CapturedArtifact::Draft(d) => Some(d.clone()),
            _ => None,
        })
        .unwrap_or(Draft { nodes: Vec::new() });

    // Slice 1a keeps no running log of "decisions the model inferred along
    // the way" distinct from the artifacts themselves — no phase in this
    // pipeline produces that list as a side channel. Left empty rather than
    // invented; a later slice that wants to surface inferred decisions has
    // a natural place to thread them through (this call site).
    let inferred_decisions: Vec<String> = Vec::new();
    let present = build_present(&ci_report, &draft, &inferred_decisions);
    phases_run.push(PhaseRecord {
        name: PhaseName::Present,
        prompt_tokens: 0,
        outcome: "presented".to_string(),
    });

    WorkflowRun {
        depth,
        phases_run,
        saved_routine: Some(routine_name),
        present: Some(present),
        stopped_reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tuxlink_agent_runner::{RunOutcome, ToolCall};
    use tuxlink_routines::types::{
        ActionStep, BusyPolicy, OnInterrupted, RoutineDef, Step, StepId, Track, TransmitMode,
        Trigger, SUPPORTED_SCHEMA_VERSION,
    };
    use tuxlink_routines::validate::StaticContext;

    use super::super::artifacts::{AffordanceAction, Affordances, DraftNode, Intent};
    use super::super::manifest::WorkflowProvenance;
    use super::super::model::{PhaseTurn, StubModel};

    fn fixture_manifest() -> WorkflowManifest {
        WorkflowManifest {
            schema_version: super::super::manifest::WORKFLOW_MANIFEST_SCHEMA_VERSION,
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

    /// A tool-call turn whose `final_text` is empty (Emit narrates via
    /// tools, not text — see `phases::phase_instruction(Emit)`) and whose
    /// single tool call names `routine_name` the way every edit verb except
    /// `routines_save` does (a top-level `routine` param) — the same shape
    /// `phases.rs`'s own Emit-capture tests use.
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

    /// A routine with a step whose action is never registered in the
    /// `ValidationContext` — fires `refs::check`'s `Severity::Error`
    /// `UNKNOWN_ACTION` finding, the same Red-path fixture `ci.rs`'s own
    /// tests use (see `ci.rs::tests::routine_with_unregistered_action_is_red`'s
    /// doc comment for why this stands in for the brief's original
    /// same-rig-parallel-lanes fixture).
    fn routine_that_fails_ci(name: &str) -> RoutineDef {
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
                steps: vec![Step::Action(ActionStep {
                    id: StepId("s1".to_string()),
                    action: "radio.mystery".to_string(),
                    params: serde_json::json!({}),
                    timeout_s: None,
                    on_radio_busy: BusyPolicy::Wait,
                })],
            }],
        }
    }

    // --- THE CRITICAL INVARIANT -------------------------------------------
    //
    // Proves the ENGINE (not `build_prompt` in isolation, which `phases.rs`
    // already covers) actually threads only `collected` artifacts through
    // each phase's prompt: the Draft phase's prompt must render the
    // declared Intent + Affordances artifacts, but must never contain (a)
    // the Router turn's raw commentary (Router produces no artifact at
    // all — this text has no `declared_inputs` row to be declared under,
    // so it structurally cannot appear if the engine is wired correctly),
    // or (b) the Feasibility phase's raw `final_text` verbatim (it is
    // re-rendered through `serde_json::to_string_pretty` on the parsed
    // artifact, not carried through as the original turn string).
    #[tokio::test]
    async fn engine_passes_only_declared_artifacts_to_each_phase_prompt_never_raw_transcript() {
        let manifest = fixture_manifest();
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());

        let intent = fixture_intent();
        let affordances = fixture_affordances();

        // The router's raw answer carries spurious commentary a buggy
        // engine might mistake for "prior transcript" to thread forward.
        // `select_depth`'s parsing only needs "full" as a substring, so
        // this is still a valid depth-selection answer.
        let router_final_text = "full — PHASE-1-INTERNAL-MARKER (spurious router commentary)";
        // Compact (non-pretty) JSON, so it differs byte-for-byte from what
        // `build_prompt` re-renders via `to_string_pretty`.
        let feasibility_final_text = serde_json::to_string(&affordances).expect("serialize");

        let stub = StubModel::new(vec![
            PhaseTurn::text(router_final_text, 5),
            PhaseTurn::text(&serde_json::to_string(&intent).expect("serialize"), 10),
            PhaseTurn::text(&feasibility_final_text, 10),
            PhaseTurn::text(
                &serde_json::to_string(&fixture_draft()).expect("serialize"),
                10,
            ),
            // Emit: no tool calls, so `capture_artifact` errors and the run
            // stops right here — fine, this test only needs prompts up
            // through Draft.
            PhaseTurn::text("", 1),
        ]);

        let _run = run_workflow(
            &manifest,
            WorkflowInputs {
                intent_text: "connect nearest 20m gateway hourly".to_string(),
            },
            &stub,
            &StaticContext::new(),
            &store,
        )
        .await;

        let prompts = stub.prompts_seen();
        assert_eq!(
            prompts.len(),
            5,
            "router + intent + feasibility + draft + emit, in that order: {prompts:?}"
        );
        let draft_prompt = &prompts[3];

        // Declared: Draft's declared_inputs are ["intent", "affordances"].
        assert!(
            draft_prompt.contains(&intent.outcome),
            "draft prompt must render the declared Intent artifact"
        );
        assert!(
            draft_prompt.contains("radio.connect"),
            "draft prompt must render the declared Affordances artifact"
        );

        // Never leaked: Router produces no artifact at all, so its raw
        // commentary has no declared_inputs row to ride in on.
        assert!(!draft_prompt.contains("PHASE-1-INTERNAL-MARKER"));

        // Never leaked verbatim: build_prompt re-serializes the parsed
        // Affordances artifact (pretty-printed), it does not carry the raw
        // turn text through.
        assert!(!draft_prompt.contains(&feasibility_final_text));
    }

    // --- F1 regression: the operator request reaches the phase prompts -----
    //
    // The pilot's Full arm died at Intent capture ("missing field `outcome`")
    // because the Intent phase's prompt never contained the operator's
    // request — `declared_inputs(Intent)` is empty, so `build_prompt` had
    // nothing to render but the static instruction. This proves the engine
    // now threads `inputs.intent_text` into every phase prompt, Intent
    // included, so the model has the request to capture FROM.
    #[tokio::test]
    async fn engine_threads_operator_request_into_intent_and_downstream_prompts() {
        let manifest = fixture_manifest();
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());

        let operator_request =
            "OPERATOR-REQUEST-SENTINEL: pull VARA mail from the nearest 20m gateway every hour";

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
            // Emit produces no tool calls, so capture errors and the run
            // stops — fine, this test only needs the prompts through Draft.
            PhaseTurn::text("", 1),
        ]);

        let _run = run_workflow(
            &manifest,
            WorkflowInputs {
                intent_text: operator_request.to_string(),
            },
            &stub,
            &StaticContext::new(),
            &store,
        )
        .await;

        let prompts = stub.prompts_seen();
        // Router(0), Intent(1), Feasibility(2), Draft(3), Emit(4).
        assert!(
            prompts[1].contains(operator_request),
            "Intent prompt must carry the operator request: {}",
            prompts[1]
        );
        assert!(
            prompts[3].contains(operator_request),
            "Draft prompt must also carry the operator request: {}",
            prompts[3]
        );
    }

    // --- Happy path: Full depth, Green CI ----------------------------------
    #[tokio::test]
    async fn full_depth_happy_path_saves_routine_and_builds_present() {
        let manifest = fixture_manifest();
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());

        // Test seam (documented per the task brief's fallback): the Emit
        // phase's real job is to drive real `routines_*` tool calls that
        // persist a routine; wiring that dispatch is the MCP/tool-invoker
        // layer's concern, not this engine unit test's. Here the store is
        // pre-seeded with the routine "as if" Emit's tool calls had already
        // saved it, and the Emit turn's tool call just NAMES that routine —
        // `capture_artifact(Emit, ..)` only reads the name off the tool
        // call and confirms the store has it (see `phases.rs`), it does not
        // itself dispatch the call.
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

        assert_eq!(run.depth, Depth::Full);
        assert!(
            run.stopped_reason.is_none(),
            "unexpected stop: {:?}",
            run.stopped_reason
        );
        assert_eq!(run.saved_routine, Some("hourly-20m-vara-cms".to_string()));
        assert!(run.present.is_some());
        assert!(store.get("hourly-20m-vara-cms").is_some());

        // Every phase in the Full pipeline (Router, Intent, Feasibility,
        // Draft, Emit, Ci, Present) recorded a PhaseRecord.
        let names: Vec<PhaseName> = run.phases_run.iter().map(|r| r.name).collect();
        assert_eq!(
            names,
            vec![
                PhaseName::Router,
                PhaseName::Intent,
                PhaseName::Feasibility,
                PhaseName::Draft,
                PhaseName::Emit,
                PhaseName::Ci,
                PhaseName::Present,
            ]
        );
    }

    // --- Red build: CI fails, dirty draft is quarantined -------------------
    #[tokio::test]
    async fn red_ci_verdict_stops_the_run_and_quarantines_the_dirty_draft() {
        let manifest = fixture_manifest();
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());

        // Same test seam as the happy-path test: pre-seed the store with
        // the routine "as if" Emit had saved it — this one fails Routine
        // CI (references an unregistered action).
        store
            .save(&routine_that_fails_ci("broken-routine"))
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
            emit_turn_naming("broken-routine", 3),
        ]);

        // No actions registered — `radio.mystery` resolves to
        // `UNKNOWN_ACTION` (Severity::Error), the same fixture shape as
        // `ci.rs::tests::routine_with_unregistered_action_is_red`.
        let ctx = StaticContext::new();

        let run = run_workflow(
            &manifest,
            WorkflowInputs {
                intent_text: "connect nearest 20m gateway hourly".to_string(),
            },
            &stub,
            &ctx,
            &store,
        )
        .await;

        assert!(run.stopped_reason.is_some());
        assert!(run
            .stopped_reason
            .as_ref()
            .expect("just asserted is_some")
            .contains("broken-routine"));
        assert!(run.saved_routine.is_none());
        assert!(run.present.is_none());
        assert!(
            store.get("broken-routine").is_none(),
            "a Red-CI draft must be quarantined out of the store, not left addressable"
        );
    }

    // --- Feasibility gate: capability gap stops with an honest report ------
    //
    // Task 1's honesty mechanism: when the Feasibility phase's Affordances
    // name a primitive the catalog lacks (`missing_primitives` non-empty),
    // the run must STOP right there with a gap report that names the missing
    // primitive — never draft/emit a routine against a fabricated capability,
    // never save anything.
    #[tokio::test]
    async fn capability_gap_stops_at_feasibility_with_an_honest_report() {
        let manifest = fixture_manifest();
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());

        let affordances_with_gap = Affordances {
            actions: vec![],
            missing_primitives: vec!["propagation.predict".to_string()],
        };
        let stub = StubModel::new(vec![
            PhaseTurn::text("full", 5),
            PhaseTurn::text(
                &serde_json::to_string(&fixture_intent()).expect("serialize"),
                10,
            ),
            PhaseTurn::text(
                &serde_json::to_string(&affordances_with_gap).expect("serialize"),
                10,
            ),
            // Draft/Emit turns scripted but must never be consumed — the
            // feasibility gate stops the run before they run.
            PhaseTurn::text("SHOULD-NOT-RUN", 99),
        ]);

        let run = run_workflow(
            &manifest,
            WorkflowInputs {
                intent_text: "predict tomorrow's 20m opening and pre-stage a beacon".to_string(),
            },
            &stub,
            &StaticContext::new(),
            &store,
        )
        .await;

        assert!(run.saved_routine.is_none());
        assert!(run.present.is_none());
        let reason = run.stopped_reason.expect("must stop on a capability gap");
        assert!(
            reason.contains("propagation.predict"),
            "gap report must name the missing primitive: {reason}"
        );
        // Stopped BEFORE Draft/Emit: only Router, Intent, Feasibility ran.
        let names: Vec<PhaseName> = run.phases_run.iter().map(|r| r.name).collect();
        assert_eq!(
            names,
            vec![PhaseName::Router, PhaseName::Intent, PhaseName::Feasibility]
        );
        // The Draft turn was never consumed.
        assert_eq!(stub.prompts_seen().len(), 3);
    }
}
