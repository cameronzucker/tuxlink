//! Phase definitions for Elmer's multi-phase "build me a routine" workflow
//! (Routine CI slice 1a, Task 7): the static per-phase prompt text, what
//! prior artifacts each phase's prompt is allowed to see, how a phase's
//! model turn is captured into a typed artifact, and which tools (if any)
//! a phase's model call may use.
//!
//! **Context-bound invariant.** [`build_prompt`] renders ONLY the artifacts
//! [`declared_inputs`] names for a given phase — never the full artifact
//! set, never raw transcript. This is deliberate: a phase that could see
//! every prior artifact (or the whole conversation) could silently start
//! depending on something its manifest contract does not name, which is
//! exactly the failure mode the Task 6 workflow-engine invariant test
//! checks for. `declared_inputs` is the single place that contract lives.
//!
//! **Direct-coercion reuse.** [`capture_artifact`] parses a phase's raw
//! `final_text` through `tuxlink_mcp_core::arg_shape::parse_if_string`
//! directly — the same one-parse compat rule the MCP argument-decode
//! boundary uses for composite tool params (`tuxlink-sq72z`), called here
//! as a plain function, NOT through MCP dispatch. A phase's model call is
//! not a tool call; there is no `ToolSpec` to validate `final_text`
//! against, so nothing else in this codebase already does this coercion
//! for a phase's answer text. Reusing the rule (rather than re-deriving a
//! second stringified-JSON tolerance) keeps exactly one tolerant-decode
//! implementation in the tree.

use tuxlink_agent_runner::{ToolCall, ToolSpec};
use tuxlink_mcp_core::arg_shape::{parse_if_string, CompositeKind};

use crate::routines::store::DefinitionStore;

use super::artifacts::{Affordances, Draft, Intent, PhaseName, Present};
use super::manifest::WorkflowManifest;
use super::model::PhaseTurn;

/// One phase's captured output — the typed artifacts [`capture_artifact`]
/// can produce, plus the [`Self::Emitted`] confirmation that the Emit
/// phase's tool calls actually persisted a routine to the [`DefinitionStore`].
/// Deliberately NOT `PhaseRecord` (that's the engine's per-run bookkeeping,
/// Task 1) and NOT `CiReport` (the Ci phase is deterministic — Task 5's
/// `run_routine_ci` produces that directly, no model turn to capture here).
#[derive(Debug, Clone, PartialEq)]
pub enum CapturedArtifact {
    Intent(Intent),
    Affordances(Affordances),
    Draft(Draft),
    /// The Emit phase's confirmation: the model's tool calls built and saved
    /// a routine under this name, now readable back from the store. Carries
    /// only the name (not the full `RoutineDef`) — later phases that need
    /// the def re-read it from the store themselves rather than trust a
    /// value threaded through the artifact list.
    Emitted { routine_name: String },
    Present(Present),
}

impl CapturedArtifact {
    /// The declared-inputs key this artifact answers to — the vocabulary
    /// [`declared_inputs`] and [`build_prompt`] look artifacts up by.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Intent(_) => "intent",
            Self::Affordances(_) => "affordances",
            Self::Draft(_) => "draft",
            Self::Emitted { .. } => "emitted",
            Self::Present(_) => "present",
        }
    }

    /// The artifact rendered as the JSON value [`build_prompt`] fences into
    /// the phase prompt. Every real artifact variant round-trips through its
    /// own `Serialize` impl; [`Self::Emitted`] has no typed artifact struct
    /// of its own (it is a store-confirmation, not a phase-produced shape),
    /// so it renders the one fact a later phase's prompt needs from it.
    fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Intent(v) => serde_json::to_value(v),
            Self::Affordances(v) => serde_json::to_value(v),
            Self::Draft(v) => serde_json::to_value(v),
            Self::Present(v) => serde_json::to_value(v),
            Self::Emitted { routine_name } => {
                Ok(serde_json::json!({ "routineName": routine_name }))
            }
        }
        .unwrap_or(serde_json::Value::Null)
    }
}

/// Errors [`capture_artifact`] can return: a phase's `final_text` did not
/// parse into the artifact shape the phase expects, or (Emit only) the
/// routine the phase's tool calls should have saved could not be found in
/// the store afterward.
#[derive(Debug, thiserror::Error)]
pub enum PhaseError {
    #[error("phase artifact did not parse: {0}")]
    Parse(String),
    #[error("phase store lookup failed: {0}")]
    Store(String),
}

/// The prior-artifact kinds (by [`CapturedArtifact::kind`]) each phase's
/// prompt is allowed to see, in pipeline order (`Router -> Intent ->
/// Feasibility -> Draft -> Emit -> Ci -> Present`). This is the single
/// source of truth for the context-bound invariant [`build_prompt`] enforces
/// — a phase never sees an artifact its own row here does not name.
///
/// - `Router`/`Intent` run before any artifact exists (Router picks the run
///   depth from the raw ask; Intent is the phase that FIRST produces one).
/// - `Feasibility` needs the captured `Intent` to know what the request is
///   asking for.
/// - `Draft` needs `Intent` + `Affordances` (the brief's own example).
/// - `Emit` needs everything Draft needed plus the `Draft` itself — it is
///   turning the drafted graph into real tool calls.
/// - `Ci` is deterministic (Task 5's `run_routine_ci`); it never calls
///   [`build_prompt`], so its row is empty rather than omitted (keeps the
///   match exhaustive and self-documenting).
/// - `Present` needs the original `Intent` (to name gaps against what was
///   actually asked for), the `Draft` (what got built), and `Emitted` (that
///   it actually saved) to write an honest summary.
fn declared_inputs(phase: PhaseName) -> &'static [&'static str] {
    match phase {
        PhaseName::Router => &[],
        PhaseName::Intent => &[],
        PhaseName::Feasibility => &["intent"],
        PhaseName::Draft => &["intent", "affordances"],
        PhaseName::Emit => &["intent", "affordances", "draft"],
        PhaseName::Ci => &[],
        PhaseName::Present => &["intent", "draft", "emitted"],
    }
}

/// The static, per-phase instruction text [`build_prompt`] leads with.
/// Phrased so each phase's model call knows exactly what shape to answer
/// in — artifact phases are told to answer as a single JSON object matching
/// their schema, `Emit` is told to call tools instead of narrating.
fn phase_instruction(phase: PhaseName) -> &'static str {
    match phase {
        PhaseName::Router => {
            "Decide how much of the workflow this request needs: \"minimal\" \
             (skip straight to drafting) or \"full\" (walk every phase). \
             Answer with the routing decision only."
        }
        PhaseName::Intent => {
            "Capture the operator's intent for the routine they want built: \
             outcome, trigger, success condition, failure behavior, side \
             effects, and any values that must persist across runs. Answer \
             as a single JSON object matching the Intent schema — no prose \
             outside the JSON."
        }
        PhaseName::Feasibility => {
            "Given the captured intent below, determine what the routine \
             catalog can currently do and what the intent needs that the \
             catalog does not yet offer. Answer as a single JSON object \
             matching the Affordances schema — no prose outside the JSON."
        }
        PhaseName::Draft => {
            "Given the intent and the available affordances below, draft a \
             candidate routine graph: steps, actions, params, and any \
             branches. Answer as a single JSON object matching the Draft \
             schema — do not save anything yet, no prose outside the JSON."
        }
        PhaseName::Emit => {
            "Given the intent, affordances, and drafted routine graph below, \
             build the routine for real using the routine edit tools \
             available to you, then save it. Call the tools — do not \
             narrate the build in text."
        }
        PhaseName::Ci => {
            "Routine CI runs deterministically against the saved draft; this \
             phase does not call a model."
        }
        PhaseName::Present => {
            "Given the intent, the drafted routine, and the saved-routine \
             confirmation below, summarize what was built for the operator: \
             what it does, decisions you inferred along the way, how it \
             behaves on failure, any gaps, and anything that needs an \
             explicit acknowledgment before it can run for real. Answer as a \
             single JSON object matching the Present schema."
        }
    }
}

/// Render one phase's model prompt: the phase's static instruction, then the
/// operator's original request (`intent_text`), then each artifact
/// [`declared_inputs`] names for that phase — if present in `artifacts` — as a
/// fenced JSON block. `manifest` names the workflow the phase belongs to
/// (surfaced as a one-line header) so a phase's prompt is self-identifying
/// without threading a second string parameter through every call site.
///
/// `intent_text` is the operator's raw stated request — the GROUND-TRUTH
/// INPUT to the whole workflow, not a prior phase's captured artifact. Every
/// phase (Intent included) needs it: without it the Intent phase is told to
/// "capture the operator's intent" with nothing to capture FROM, and returns
/// an artifact missing its required fields. Rendering it does NOT violate the
/// context-bound invariant, which is about prior phases' captured artifacts
/// never leaking except where `declared_inputs` names them — the operator's
/// own request is not such an artifact.
///
/// Artifacts NOT in `declared_inputs(phase)` are never rendered, even if
/// present in `artifacts` — the context-bound invariant this module exists
/// to enforce (see the module doc).
pub fn build_prompt(
    phase: PhaseName,
    manifest: &WorkflowManifest,
    intent_text: &str,
    artifacts: &[CapturedArtifact],
) -> String {
    let mut prompt = format!(
        "Workflow: {} v{}\n\n{}\n\nOperator request:\n{intent_text}",
        manifest.name,
        manifest.version,
        phase_instruction(phase)
    );
    for kind in declared_inputs(phase) {
        let Some(artifact) = artifacts.iter().find(|a| a.kind() == *kind) else {
            continue;
        };
        let json = serde_json::to_string_pretty(&artifact.to_json())
            .unwrap_or_else(|_| "{}".to_string());
        prompt.push_str("\n\n```json\n");
        prompt.push_str(&json);
        prompt.push_str("\n```");
    }
    prompt
}

/// Parse an artifact-phase's raw model answer into a typed artifact: one
/// JSON parse of `turn.final_text`, then the direct-coercion reuse
/// ([`parse_if_string`]) so a model that stringified its JSON object answer
/// (the same compat shape small models emit for tool params) still parses,
/// then a typed `serde_json::from_value`.
fn parse_artifact<T: serde::de::DeserializeOwned>(turn: &PhaseTurn) -> Result<T, PhaseError> {
    let raw: serde_json::Value =
        serde_json::from_str(&turn.final_text).map_err(|e| PhaseError::Parse(e.to_string()))?;
    let coerced = parse_if_string(raw, CompositeKind::Object);
    serde_json::from_value(coerced).map_err(|e| PhaseError::Parse(e.to_string()))
}

/// The routine name the Emit phase's tool calls acted on, read straight off
/// the calls themselves rather than threaded through as a side parameter:
/// every routine edit verb except `routines_save` carries a top-level
/// `routine` string param, and `routines_save` carries it nested in `def`
/// (or, for the deprecated string form, inside `def_json`). Returns the
/// first name found, in call order — an Emit phase operates on exactly one
/// routine, so the first call that names one settles it. `None` when no
/// call in `tool_calls` names a routine at all (a phase that produced no
/// tool calls, or tool calls this function does not recognize).
fn routine_name_from_tool_calls(tool_calls: &[ToolCall]) -> Option<String> {
    for call in tool_calls {
        if let Some(name) = call.args.get("routine").and_then(|v| v.as_str()) {
            return Some(name.to_string());
        }
        if call.name == "routines_save" {
            if let Some(def) = call.args.get("def") {
                let coerced = parse_if_string(def.clone(), CompositeKind::Object);
                if let Some(name) = coerced.get("routine").and_then(|v| v.as_str()) {
                    return Some(name.to_string());
                }
            }
            if let Some(def_json) = call.args.get("def_json").and_then(|v| v.as_str()) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(def_json) {
                    if let Some(name) = parsed.get("routine").and_then(|v| v.as_str()) {
                        return Some(name.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Capture one phase's model turn into a typed [`CapturedArtifact`].
///
/// `Intent` / `Feasibility` / `Draft` / `Present` parse `turn.final_text`
/// via [`parse_artifact`] (the direct-coercion path). `Emit` does not parse
/// `final_text` at all — the model built the routine by calling tools, so
/// this reads the routine name off `turn.tool_calls`
/// ([`routine_name_from_tool_calls`]) and confirms it landed in `store`
/// (`DefinitionStore::get`, which itself round-trips through
/// `RoutineDef::parse` — the "loads the def via store + `RoutineDef::parse`"
/// the brief calls for). `Router` and `Ci` are not artifact-capturing phases
/// (Router only selects a `Depth`; Ci's `CiReport` comes from Task 5's
/// deterministic `run_routine_ci`, not a model turn) — both fall through to
/// a typed `Parse` error naming the phase, keeping the match exhaustive.
pub fn capture_artifact(
    phase: PhaseName,
    turn: &PhaseTurn,
    store: &DefinitionStore,
) -> Result<CapturedArtifact, PhaseError> {
    match phase {
        PhaseName::Intent => parse_artifact(turn).map(CapturedArtifact::Intent),
        PhaseName::Feasibility => parse_artifact(turn).map(CapturedArtifact::Affordances),
        PhaseName::Draft => parse_artifact(turn).map(CapturedArtifact::Draft),
        PhaseName::Present => parse_artifact(turn).map(CapturedArtifact::Present),
        PhaseName::Emit => {
            let routine_name = routine_name_from_tool_calls(&turn.tool_calls).ok_or_else(|| {
                PhaseError::Store(
                    "emit phase's tool calls named no routine to look up".to_string(),
                )
            })?;
            if store.get(&routine_name).is_none() {
                return Err(PhaseError::Store(format!(
                    "routine {routine_name:?} not found in store after emit"
                )));
            }
            Ok(CapturedArtifact::Emitted { routine_name })
        }
        PhaseName::Router | PhaseName::Ci => Err(PhaseError::Parse(format!(
            "{phase:?} does not capture an artifact"
        ))),
    }
}

/// The Emit phase's tool edit-verb allow-set (spec §"Affordance catalog" /
/// Part-97): every verb a routine can be BUILT with, deliberately excluding
/// `routines_enable` / `routines_run` (the Part-97 denylist — those flip a
/// routine live or execute it, neither of which the drafting workflow ever
/// does) and any transmit-capable tool (there is none in this list; the
/// routines edit surface has no transmit verb of its own).
const EMIT_TOOL_ALLOW_SET: &[&str] = &[
    "routines_step_add",
    "routines_step_update",
    "routines_step_remove",
    "routines_step_move",
    "routines_track_add",
    "routines_trigger_set",
    "routines_meta_set",
    "routines_save",
];

/// The Part-97 denylist: tools the Emit phase must NEVER be handed, no
/// matter how the allow-set above is edited later. `EMIT_TOOL_ALLOW_SET`
/// never lists them to begin with; this filter is the belt to that
/// suspenders — defense in depth against a future edit that adds one of
/// these names to the allow-set by mistake.
const PART97_DENYLIST: &[&str] = &["routines_enable", "routines_run"];

/// The tool schemas available to `phase`'s model call. Only `Emit` gets
/// tools (the routine edit verbs, filtered against [`PART97_DENYLIST`]) —
/// every other phase answers in its final message, so it gets an empty set.
///
/// Also gates on `manifest.allowed_tool_families`: a manifest that does not
/// declare the `"routines"` family (every verb here is a `routines_*` tool)
/// gets no tools even in the Emit phase — the manifest's own declared
/// surface is authoritative over this module's fixed allow-set, never the
/// other way around.
///
/// **Schema placeholder (flagged in the Task 7 report):** each `ToolSpec`
/// carries a minimal `{"type": "object"}` schema rather than the routine
/// edit verbs' real `schemars`-generated schemas
/// (`tuxlink_mcp_core::router::RoutineStepAddParams` and siblings) — pulling
/// those in would require the crate this file lives in to take a new direct
/// dependency on `schemars` (currently only reachable transitively through
/// `tuxlink-mcp-core`), which is outside this task's "write only
/// `phases.rs`" constraint. Task 11 (per the brief) tests only that the
/// Part-97 denylist names are absent from this list, not schema fidelity.
pub fn tools_for(phase: PhaseName, manifest: &WorkflowManifest) -> Vec<ToolSpec> {
    if phase != PhaseName::Emit {
        return Vec::new();
    }
    if !manifest.allowed_tool_families.iter().any(|f| f == "routines") {
        return Vec::new();
    }
    EMIT_TOOL_ALLOW_SET
        .iter()
        .filter(|name| !PART97_DENYLIST.contains(name))
        .map(|name| ToolSpec::new(*name, serde_json::json!({ "type": "object" })))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuxlink_agent_runner::RunOutcome;

    use super::super::artifacts::AffordanceAction;
    use super::super::manifest::WorkflowProvenance;

    fn fixture_manifest(allowed_tool_families: Vec<String>) -> WorkflowManifest {
        WorkflowManifest {
            schema_version: super::super::manifest::WORKFLOW_MANIFEST_SCHEMA_VERSION,
            name: "build-routine".to_string(),
            version: "1.0.0".to_string(),
            entry: PhaseName::Router,
            exit: PhaseName::Present,
            required_inputs: vec!["outcome".to_string()],
            optional_inputs: vec![],
            allowed_tool_families,
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

    // (a) build_prompt(Draft, ..) with an Intent + Affordances in scope
    // contains both artifacts' JSON and the draft-phase instruction, and
    // does NOT contain any artifact the Draft phase does not declare (here:
    // a Present artifact, which Draft never lists in `declared_inputs`).
    #[test]
    fn build_prompt_draft_renders_only_declared_artifacts() {
        let manifest = fixture_manifest(vec!["routines".to_string()]);
        let intent = fixture_intent();
        let affordances = fixture_affordances();
        let present_not_declared_by_draft = Present {
            built: "SENTINEL-not-declared-by-draft".to_string(),
            inferred_decisions: vec![],
            failure_behavior: "n/a".to_string(),
            gaps: vec![],
            acks_required: vec![],
        };
        let artifacts = vec![
            CapturedArtifact::Intent(intent.clone()),
            CapturedArtifact::Affordances(affordances.clone()),
            CapturedArtifact::Present(present_not_declared_by_draft),
        ];

        let prompt = build_prompt(
            PhaseName::Draft,
            &manifest,
            "OPERATOR-ASK-build-a-mail-routine",
            &artifacts,
        );

        assert!(prompt.contains(phase_instruction(PhaseName::Draft)));
        assert!(prompt.contains(&intent.outcome));
        assert!(prompt.contains("radio.connect"));
        assert!(!prompt.contains("SENTINEL-not-declared-by-draft"));
    }

    #[test]
    fn build_prompt_intent_phase_has_no_declared_artifacts_to_render() {
        let manifest = fixture_manifest(vec!["routines".to_string()]);
        let intent = fixture_intent();
        let artifacts = vec![CapturedArtifact::Intent(intent.clone())];

        // Intent is the phase that PRODUCES the Intent artifact — its own
        // `declared_inputs` row is empty, so even though an Intent is in
        // scope (e.g. a re-run), it must not leak into the prompt. The
        // operator request here is deliberately worded to share NO substring
        // with `fixture_intent().outcome`, so the leak assertion below cannot
        // pass merely because `intent_text` echoes the artifact.
        let prompt = build_prompt(
            PhaseName::Intent,
            &manifest,
            "please set something up for me on 40 meters",
            &artifacts,
        );
        assert!(prompt.contains(phase_instruction(PhaseName::Intent)));
        assert!(!prompt.contains(&intent.outcome));
    }

    // F1 regression: the Intent phase's prompt MUST carry the operator's raw
    // request. `declared_inputs(Intent)` is empty (no prior artifact exists
    // yet), so before this fix the Intent model was told "capture the
    // operator's intent" with nothing to capture FROM — it returned JSON
    // missing `outcome` and the whole Full arm died at capture. The operator
    // request is the ground-truth input, not a prior-phase artifact, so it is
    // rendered on every phase including Intent.
    #[test]
    fn build_prompt_intent_phase_renders_the_operator_request() {
        let manifest = fixture_manifest(vec!["routines".to_string()]);
        let operator_request =
            "OPERATOR-REQUEST-SENTINEL: pull VARA mail from the nearest 20m gateway every hour";

        let prompt = build_prompt(PhaseName::Intent, &manifest, operator_request, &[]);

        assert!(prompt.contains(phase_instruction(PhaseName::Intent)));
        assert!(
            prompt.contains(operator_request),
            "Intent prompt must contain the operator's raw request: {prompt}"
        );
    }

    // The operator request rides EVERY phase, not just Intent — a downstream
    // phase (Draft) sees it too, alongside (not instead of) its declared
    // prior-phase artifacts.
    #[test]
    fn build_prompt_downstream_phase_also_renders_the_operator_request() {
        let manifest = fixture_manifest(vec!["routines".to_string()]);
        let operator_request = "OPERATOR-REQUEST-SENTINEL: draft me a morning ICS check-in";
        let artifacts = vec![
            CapturedArtifact::Intent(fixture_intent()),
            CapturedArtifact::Affordances(fixture_affordances()),
        ];

        let prompt = build_prompt(PhaseName::Draft, &manifest, operator_request, &artifacts);

        assert!(
            prompt.contains(operator_request),
            "Draft prompt must also carry the operator's raw request: {prompt}"
        );
        // Declared prior-phase artifacts still render — the operator request
        // is additive, it does not displace the context-bound artifacts.
        assert!(prompt.contains(&fixture_intent().outcome));
        assert!(prompt.contains("radio.connect"));
    }

    // (b) capture_artifact(Intent, turn_with_json_object_as_string, _) uses
    // parse_if_string so a stringified-JSON intent still parses.
    #[test]
    fn capture_artifact_intent_parses_stringified_json_via_parse_if_string() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());

        let intent = fixture_intent();
        let inner_json = serde_json::to_string(&intent).expect("serialize intent");
        // Double-encode: final_text is a JSON STRING whose content is the
        // intent object's JSON — the exact "model stringified its answer"
        // compat shape parse_if_string exists for.
        let final_text =
            serde_json::to_string(&serde_json::Value::String(inner_json)).expect("wrap as string");
        let turn = PhaseTurn {
            outcome: RunOutcome::Completed(final_text.clone()),
            tool_calls: Vec::new(),
            final_text,
            prompt_tokens: 42,
        };

        let captured =
            capture_artifact(PhaseName::Intent, &turn, &store).expect("stringified intent parses");
        assert_eq!(captured, CapturedArtifact::Intent(intent));
    }

    #[test]
    fn capture_artifact_intent_parses_plain_json_object_too() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());

        let intent = fixture_intent();
        let final_text = serde_json::to_string(&intent).expect("serialize intent");
        let turn = PhaseTurn::text(&final_text, 10);

        let captured = capture_artifact(PhaseName::Intent, &turn, &store).expect("plain intent parses");
        assert_eq!(captured, CapturedArtifact::Intent(intent));
    }

    #[test]
    fn capture_artifact_emit_reads_routine_name_off_tool_calls_and_confirms_store() {
        use tuxlink_routines::types::{OnInterrupted, RoutineDef, TransmitMode};

        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        let def = RoutineDef {
            routine: "hourly-20m-vara-cms".to_string(),
            schema_version: 1,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![],
            tracks: vec![],
        };
        store.save(&def).expect("seed store");

        let turn = PhaseTurn {
            outcome: RunOutcome::Completed(String::new()),
            tool_calls: vec![ToolCall::new(
                "routines_meta_set",
                serde_json::json!({ "routine": "hourly-20m-vara-cms", "patch": {} }),
            )],
            final_text: String::new(),
            prompt_tokens: 0,
        };

        let captured = capture_artifact(PhaseName::Emit, &turn, &store).expect("emit resolves");
        assert_eq!(
            captured,
            CapturedArtifact::Emitted {
                routine_name: "hourly-20m-vara-cms".to_string()
            }
        );
    }

    #[test]
    fn capture_artifact_emit_finds_routine_name_nested_in_routines_save_def() {
        use tuxlink_routines::types::{OnInterrupted, RoutineDef, TransmitMode};

        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        let def = RoutineDef {
            routine: "morning-ics-cycle".to_string(),
            schema_version: 1,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![],
            tracks: vec![],
        };
        store.save(&def).expect("seed store");

        // routines_save has no top-level `routine` param — the name is
        // nested in `def.routine` (or, deprecated, inside a `def_json`
        // string). This call also exercises the direct-coercion reuse: the
        // model handed `def` back as a STRING containing the JSON object.
        let stringified_def = serde_json::to_string(&serde_json::json!({
            "routine": "morning-ics-cycle",
            "schema_version": 1,
            "transmit_mode": "attended",
            "triggers": [],
            "tracks": []
        }))
        .expect("stringify def");
        let turn = PhaseTurn {
            outcome: RunOutcome::Completed(String::new()),
            tool_calls: vec![ToolCall::new(
                "routines_save",
                serde_json::json!({ "def": stringified_def }),
            )],
            final_text: String::new(),
            prompt_tokens: 0,
        };

        let captured = capture_artifact(PhaseName::Emit, &turn, &store).expect("emit resolves");
        assert_eq!(
            captured,
            CapturedArtifact::Emitted {
                routine_name: "morning-ics-cycle".to_string()
            }
        );
    }

    #[test]
    fn capture_artifact_emit_errors_when_store_has_no_such_routine() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DefinitionStore::open(dir.path().to_path_buf());
        let turn = PhaseTurn {
            outcome: RunOutcome::Completed(String::new()),
            tool_calls: vec![ToolCall::new(
                "routines_meta_set",
                serde_json::json!({ "routine": "never-saved", "patch": {} }),
            )],
            final_text: String::new(),
            prompt_tokens: 0,
        };

        let err = capture_artifact(PhaseName::Emit, &turn, &store)
            .expect_err("routine was never saved");
        assert!(matches!(err, PhaseError::Store(_)));
    }

    // (c) tools_for(Emit, manifest) excludes routines_enable and routines_run.
    #[test]
    fn tools_for_emit_excludes_part97_denylist() {
        let manifest = fixture_manifest(vec!["routines".to_string()]);
        let tools = tools_for(PhaseName::Emit, &manifest);

        assert!(!tools.is_empty(), "emit phase must get the edit verbs");
        assert!(tools.iter().all(|t| t.name != "routines_enable"));
        assert!(tools.iter().all(|t| t.name != "routines_run"));
        assert!(tools.iter().any(|t| t.name == "routines_save"));
    }

    // (d) tools_for(Intent, ..) is empty.
    #[test]
    fn tools_for_intent_is_empty() {
        let manifest = fixture_manifest(vec!["routines".to_string()]);
        assert!(tools_for(PhaseName::Intent, &manifest).is_empty());
    }

    #[test]
    fn tools_for_emit_is_empty_when_manifest_omits_routines_family() {
        let manifest = fixture_manifest(vec!["some-other-family".to_string()]);
        assert!(tools_for(PhaseName::Emit, &manifest).is_empty());
    }
}
