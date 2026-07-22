//! The model port for Elmer's multi-phase "build me a routine" workflow
//! (Routine CI slice 1a, Task 3).
//!
//! `PhaseModel` is the workflow engine's (Task 6) seam onto a model: one call
//! = one phase's worth of model reasoning, given a rendered prompt and the
//! tool schemas available to that phase. It is deliberately a NARROWER port
//! than `tuxlink_agent_runner::Provider` — the workflow engine does not run
//! the bounded agent loop's tool-call dispatch itself; each phase issues (at
//! most) one model call and gets back a [`PhaseTurn`] carrying the raw
//! `final_text` for the phase to parse against its own artifact schema.
//!
//! **Dyn-compatibility.** The engine holds this as `&dyn PhaseModel`, and a
//! native `async fn` in a trait is not dyn-compatible on this crate's MSRV
//! (1.75). We use `#[async_trait::async_trait]` to match how
//! `tuxlink_agent_runner::traits::ToolInvoker` (also held as
//! `Box<dyn ToolInvoker>` / `&dyn ToolInvoker`) declares its own async
//! methods — same crate (`async-trait`) already in `src-tauri/Cargo.toml`,
//! no new dependency.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tuxlink_agent_runner::{
    run_with_conversation_with_transcript, CallAuthority, Conversation, EgressStatus, Limits,
    NullTranscript, Provider, RunEvent, RunOutcome, ToolCall, ToolInvoker, ToolOutcome, ToolSpec,
};

/// One model turn within a workflow phase.
///
/// Distinct from `tuxlink_agent_runner::types::ModelTurn` (a `Provider`'s
/// per-turn output inside the bounded agent loop): `PhaseTurn` is the
/// workflow engine's own unit of model output, carrying the loop-style
/// [`RunOutcome`] the phase decides against plus the raw text the phase
/// parses, so a phase can inspect both without the engine reimplementing the
/// agent loop's outcome vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseTurn {
    /// How the phase's model call resolved.
    pub outcome: RunOutcome,
    /// Tool calls the model requested during this turn, if any.
    pub tool_calls: Vec<ToolCall>,
    /// The model's raw final text (e.g. the JSON a phase parses into its
    /// artifact). Populated from `outcome`'s `Completed` payload for the
    /// common case; kept as a separate field so a phase can read it without
    /// re-matching on `outcome`.
    pub final_text: String,
    /// Prompt tokens the model call consumed, as reported by the provider.
    pub prompt_tokens: u64,
}

impl PhaseTurn {
    /// Convenience constructor for the common case: a plain completed text
    /// turn with no tool calls. `outcome` is `RunOutcome::Completed(s)`.
    pub fn text(s: &str, toks: u64) -> Self {
        Self {
            outcome: RunOutcome::Completed(s.to_string()),
            tool_calls: Vec::new(),
            final_text: s.to_string(),
            prompt_tokens: toks,
        }
    }
}

/// The model port a workflow phase runs its reasoning through.
///
/// Object-safe (`#[async_trait]`) and `Send + Sync` so the engine (Task 6)
/// can hold it as `&dyn PhaseModel`, mirroring `ToolInvoker` /
/// `Provider` in `tuxlink_agent_runner::traits`.
#[async_trait]
pub trait PhaseModel: Send + Sync {
    /// Run one phase's model call: `prompt` is the fully-rendered phase
    /// prompt, `tools` are the tool schemas available to this phase (may be
    /// empty for phases that never call tools).
    async fn run_phase(&self, prompt: String, tools: &[ToolSpec]) -> PhaseTurn;
}

/// A scripted [`PhaseModel`] for tests: returns pre-loaded [`PhaseTurn`]s in
/// FIFO order and records every prompt it was given, so a test (including the
/// workflow-engine invariant tests in Task 6) can assert what the engine
/// actually sent.
pub struct StubModel {
    turns: Mutex<VecDeque<PhaseTurn>>,
    prompts_seen: Mutex<Vec<String>>,
}

impl StubModel {
    /// Build a stub scripted to return `turns` in order, one per
    /// `run_phase` call.
    pub fn new(turns: Vec<PhaseTurn>) -> Self {
        Self {
            turns: Mutex::new(turns.into_iter().collect()),
            prompts_seen: Mutex::new(Vec::new()),
        }
    }

    /// The prompts passed to `run_phase`, in call order.
    pub fn prompts_seen(&self) -> Vec<String> {
        self.prompts_seen.lock().expect("prompts_seen mutex poisoned").clone()
    }
}

#[async_trait]
impl PhaseModel for StubModel {
    async fn run_phase(&self, prompt: String, _tools: &[ToolSpec]) -> PhaseTurn {
        self.prompts_seen
            .lock()
            .expect("prompts_seen mutex poisoned")
            .push(prompt);
        self.turns
            .lock()
            .expect("turns mutex poisoned")
            .pop_front()
            .expect("StubModel called more times than it was scripted for")
    }
}

// ---------------------------------------------------------------------------
// SessionPhaseModel — the production `PhaseModel` adapter (Task 13a)
// ---------------------------------------------------------------------------

/// The production [`PhaseModel`]: drives each workflow phase through a real
/// [`Provider`] (the model) and, for the Emit phase, a real [`ToolInvoker`]
/// bound to the routines edit verbs writing to the run's `DefinitionStore`.
/// This is the adapter [`super::engine::run_workflow`] runs against when it is
/// driven by a real model instead of [`StubModel`]; Task 13b wires it into the
/// battery binary.
///
/// ## The two phase modes (the load-bearing contract)
///
/// [`Self::run_phase`] switches on whether `tools` is empty — the mode the
/// engine encodes by passing `phases::tools_for(phase, manifest)`:
///
/// * **Artifact phases (`tools` EMPTY)** — Intent / Feasibility / Draft /
///   Present. One model completion, NO tool dispatch: the bounded loop runs
///   against a [`NoToolsInvoker`], so the model is handed an empty tool surface
///   and can only answer in text. That answer becomes [`PhaseTurn::final_text`]
///   for the phase to parse against its own artifact schema, with
///   `tool_calls = []`.
/// * **Emit phase (`tools` NON-EMPTY)** — the routine edit verbs. The loop runs
///   against the injected edit-verb invoker, wrapped in [`EmitDispatch`], which
///   (a) exposes to the model EXACTLY the `tools` the engine passed (already
///   Part-97-filtered by `phases::tools_for` / `PART97_DENYLIST`), (b)
///   allow-list-gates every `invoke` to that set — a belt to `tools_for`'s
///   suspenders, so `routines_enable` / `routines_run` / any transmit verb is
///   never forwarded to the inner invoker even if the injected invoker could
///   reach one — and (c) records the [`ToolCall`]s the model issued.
///   `run_phase` returns those recorded calls as `tool_calls`, which is what
///   `phases::routine_name_from_tool_calls` reads the saved routine's name off;
///   the inner invoker's `invoke` is what actually persists the routine to the
///   store the engine then confirms with `store.get` in
///   `phases::capture_artifact(Emit, ..)`.
///
/// `prompt_tokens` is summed from the provider's per-turn
/// [`RunEvent::ContextUsage`] events — the same metering path
/// `elmer_battery::make_battery_sink` feeds its `Meters` from (there via the
/// bridged `ElmerEvent::Context`; here read straight off the runner's
/// `RunEvent`, since this adapter drives the runner directly rather than
/// through `ElmerSession::send`).
///
/// ## Injection (13b supplies the real parts; the unit tests supply fakes)
///
/// The adapter hardcodes NEITHER a model NOR a keyring: it holds an
/// already-vetted `Arc<dyn Provider>` and an `Arc<dyn ToolInvoker>` (the
/// edit-verb invoker bound to the run's temp store) — exactly the parts
/// `elmer_battery::run_cell` already constructs (`ElmerProvider::new_vetted`
/// for the provider; the in-process routines invoker for the store). Task 13b
/// builds it from those; the tests below build it from a scripted fake provider
/// plus a temp-dir store-writing invoker.
pub struct SessionPhaseModel {
    /// The vetted model provider for this run (one per cell, as in `run_cell`).
    provider: Arc<dyn Provider>,
    /// The Emit-phase edit-verb invoker, bound to the run's `DefinitionStore`.
    /// Only reached for the Emit phase (non-empty `tools`).
    emit_invoker: Arc<dyn ToolInvoker>,
    /// Bounds on each phase's bounded agent loop.
    limits: Limits,
}

impl SessionPhaseModel {
    /// Build with default [`Limits`].
    pub fn new(provider: Arc<dyn Provider>, emit_invoker: Arc<dyn ToolInvoker>) -> Self {
        Self::with_limits(provider, emit_invoker, Limits::default())
    }

    /// Build with explicit per-phase loop [`Limits`] (13b may tighten these to
    /// the cell's budget).
    pub fn with_limits(
        provider: Arc<dyn Provider>,
        emit_invoker: Arc<dyn ToolInvoker>,
        limits: Limits,
    ) -> Self {
        Self {
            provider,
            emit_invoker,
            limits,
        }
    }
}

#[async_trait]
impl PhaseModel for SessionPhaseModel {
    async fn run_phase(&self, prompt: String, tools: &[ToolSpec]) -> PhaseTurn {
        // Sum prompt tokens across the phase's provider turns from the runner's
        // fire-and-forget ContextUsage events (mirrors `make_battery_sink`).
        let prompt_tokens = AtomicU64::new(0);
        let on_event = |ev: RunEvent| {
            if let RunEvent::ContextUsage {
                prompt_tokens: pt, ..
            } = ev
            {
                prompt_tokens.fetch_add(u64::from(pt), Ordering::SeqCst);
            }
        };

        let mut conversation = Conversation::new(prompt);
        // A fresh, never-fired token: the workflow engine bounds the run via
        // `Limits`, not via mid-phase cancellation.
        let cancel = CancellationToken::new();

        let (outcome, tool_calls) = if tools.is_empty() {
            // Artifact phase: single completion against an empty tool surface.
            let invoker = NoToolsInvoker;
            let outcome = run_with_conversation_with_transcript(
                &mut conversation,
                &*self.provider,
                &invoker,
                EgressStatus::default(),
                self.limits,
                cancel,
                &on_event,
                &NullTranscript,
            )
            .await;
            (outcome, Vec::new())
        } else {
            // Emit phase: dispatch the model's edit-verb calls against the
            // injected store-bound invoker, gated to the engine-passed allow-set
            // and recorded for the returned `tool_calls`.
            let dispatch = EmitDispatch::new(tools.to_vec(), &*self.emit_invoker);
            let outcome = run_with_conversation_with_transcript(
                &mut conversation,
                &*self.provider,
                &dispatch,
                EgressStatus::default(),
                self.limits,
                cancel,
                &on_event,
                &NullTranscript,
            )
            .await;
            let recorded = dispatch.into_recorded();
            (outcome, recorded)
        };

        // `final_text` is the model's final answer for the artifact phases; a
        // non-`Completed` outcome (bound hit, provider error, ...) has no answer
        // text, so it is empty and the phase's own capture surfaces the error.
        let final_text = match &outcome {
            RunOutcome::Completed(text) => text.clone(),
            _ => String::new(),
        };

        PhaseTurn {
            outcome,
            tool_calls,
            final_text,
            prompt_tokens: prompt_tokens.load(Ordering::SeqCst),
        }
    }
}

/// The artifact-phase tool surface: NONE. The model is handed an empty tool set,
/// so it can only answer in text; `invoke` is never reached on the happy path
/// (an artifact-phase model that tries to call a tool hits the runner's
/// unknown-tool validation, not this method).
struct NoToolsInvoker;

#[async_trait]
impl ToolInvoker for NoToolsInvoker {
    fn tools(&self) -> &[ToolSpec] {
        &[]
    }

    async fn invoke(
        &self,
        _call: &ToolCall,
        _authority: CallAuthority,
        _cancel: &CancellationToken,
    ) -> ToolOutcome {
        ToolOutcome::InvalidArgs(
            "artifact phases expose no tools; the model must answer in text".to_string(),
        )
    }
}

/// The Emit-phase dispatch wrapper around the injected edit-verb invoker.
///
/// It presents to the runner EXACTLY the engine-passed `tools` (so the model
/// sees the Part-97-filtered edit verbs and nothing else), forwards allowed
/// calls to the store-bound inner invoker, DENIES any call outside the allow-set
/// (Part-97 belt-and-suspenders), and records every attempted call so
/// [`SessionPhaseModel::run_phase`] can return them for
/// `phases::routine_name_from_tool_calls`.
struct EmitDispatch<'a> {
    /// The tool surface the model sees — EXACTLY the engine-passed `tools`.
    allowed: Vec<ToolSpec>,
    /// The real edit-verb invoker, bound to the run's store.
    inner: &'a dyn ToolInvoker,
    /// Every call the model issued, in order (for the returned `tool_calls`).
    recorded: Mutex<Vec<ToolCall>>,
}

impl<'a> EmitDispatch<'a> {
    fn new(allowed: Vec<ToolSpec>, inner: &'a dyn ToolInvoker) -> Self {
        Self {
            allowed,
            inner,
            recorded: Mutex::new(Vec::new()),
        }
    }

    /// Consume the wrapper and yield the recorded calls (call order preserved).
    fn into_recorded(self) -> Vec<ToolCall> {
        self.recorded.into_inner().expect("recorded mutex poisoned")
    }
}

#[async_trait]
impl<'a> ToolInvoker for EmitDispatch<'a> {
    fn tools(&self) -> &[ToolSpec] {
        &self.allowed
    }

    async fn invoke(
        &self,
        call: &ToolCall,
        authority: CallAuthority,
        cancel: &CancellationToken,
    ) -> ToolOutcome {
        self.recorded
            .lock()
            .expect("recorded mutex poisoned")
            .push(call.clone());
        // Part-97 belt to `tools_for`'s suspenders: never forward a verb outside
        // the engine-passed allow-set, no matter what the injected invoker
        // exposes. (In practice the runner's own COR-3 validation already
        // rejects an out-of-set name before dispatch; this is defense in depth.)
        if !self.allowed.iter().any(|spec| spec.name == call.name) {
            return ToolOutcome::Denied(format!(
                "tool {:?} is not in the Emit edit-verb allow-set (Part-97)",
                call.name
            ));
        }
        self.inner.invoke(call, authority, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_model_returns_scripted_turn_and_records_prompt() {
        let stub = StubModel::new(vec![PhaseTurn::text("{\"outcome\":\"x\"}", 12)]);
        let turn = stub.run_phase("PROMPT-A".into(), &[]).await;
        assert_eq!(turn.final_text, "{\"outcome\":\"x\"}");
        assert_eq!(turn.prompt_tokens, 12);
        assert_eq!(stub.prompts_seen(), vec!["PROMPT-A".to_string()]);
    }

    // -----------------------------------------------------------------------
    // SessionPhaseModel (Task 13a)
    // -----------------------------------------------------------------------

    use tuxlink_agent_runner::{ModelTurn, ProviderError};

    use super::super::artifacts::PhaseName;
    use super::super::manifest::{
        WorkflowManifest, WorkflowProvenance, WORKFLOW_MANIFEST_SCHEMA_VERSION,
    };
    use super::super::phases::{capture_artifact, tools_for, CapturedArtifact};
    use crate::routines::store::DefinitionStore;

    /// A minimal fake [`Provider`]: replays `(prompt_tokens, ModelTurn)` in FIFO
    /// order and emits a [`RunEvent::ContextUsage`] carrying the per-turn prompt
    /// tokens before each turn — the metering path `run_phase` reads. Running
    /// past the script yields a transport error (an under-supplied script is a
    /// loud test bug, not a hang).
    struct FakeProvider {
        turns: Mutex<VecDeque<(u32, ModelTurn)>>,
    }

    impl FakeProvider {
        fn new(turns: Vec<(u32, ModelTurn)>) -> Self {
            Self {
                turns: Mutex::new(turns.into_iter().collect()),
            }
        }
    }

    #[async_trait]
    impl Provider for FakeProvider {
        async fn turn(
            &self,
            _conversation: &Conversation,
            _tools: &[ToolSpec],
            on_event: &(dyn Fn(RunEvent) + Sync),
        ) -> Result<ModelTurn, ProviderError> {
            let next = self.turns.lock().expect("turns mutex poisoned").pop_front();
            match next {
                Some((tokens, turn)) => {
                    on_event(RunEvent::ContextUsage {
                        prompt_tokens: tokens,
                        eval_tokens: 0,
                        num_ctx: None,
                    });
                    Ok(turn)
                }
                None => Err(ProviderError::Transport(
                    "FakeProvider exhausted: the test script supplied too few turns".to_string(),
                )),
            }
        }
    }

    /// A fake edit-verb invoker: on `routines_save` it parses the `def` arg into
    /// a [`RoutineDef`](tuxlink_routines::types::RoutineDef) and persists it to a
    /// real temp-dir [`DefinitionStore`] (so `capture_artifact(Emit, ..)`'s
    /// `store.get` finds it), and returns `Ok` so the loop re-prompts. Any other
    /// verb is a harmless `Ok({})`. Its own `tools()` is unused by the runner —
    /// [`EmitDispatch`] overrides the tool surface with the engine-passed set.
    struct StoreWritingInvoker {
        store: DefinitionStore,
    }

    #[async_trait]
    impl ToolInvoker for StoreWritingInvoker {
        fn tools(&self) -> &[ToolSpec] {
            &[]
        }

        async fn invoke(
            &self,
            call: &ToolCall,
            _authority: CallAuthority,
            _cancel: &CancellationToken,
        ) -> ToolOutcome {
            if call.name == "routines_save" {
                let Some(def_val) = call.args.get("def") else {
                    return ToolOutcome::InvalidArgs("routines_save missing `def`".to_string());
                };
                match serde_json::from_value::<tuxlink_routines::types::RoutineDef>(def_val.clone())
                {
                    Ok(def) => match self.store.save(&def) {
                        Ok(rev) => ToolOutcome::Ok(
                            serde_json::json!({ "saved": def.routine, "revision": rev }),
                        ),
                        Err(e) => ToolOutcome::InvalidArgs(format!("store save failed: {e}")),
                    },
                    Err(e) => ToolOutcome::InvalidArgs(format!("def did not parse: {e}")),
                }
            } else {
                ToolOutcome::Ok(serde_json::json!({}))
            }
        }
    }

    fn fixture_manifest(allowed_tool_families: Vec<String>) -> WorkflowManifest {
        WorkflowManifest {
            schema_version: WORKFLOW_MANIFEST_SCHEMA_VERSION,
            name: "build-routine".to_string(),
            version: "1.0.0".to_string(),
            entry: PhaseName::Router,
            exit: PhaseName::Present,
            required_inputs: vec!["outcome".to_string()],
            optional_inputs: vec![],
            allowed_tool_families,
            expected_artifacts: vec![],
            deterministic_gates: vec![],
            failure_escalation: serde_json::json!({}),
            provenance: WorkflowProvenance {
                compatible_capability_versions: vec![],
                eval_scenarios: vec![],
                known_model_compat: vec![],
                traceable_outputs: vec![],
            },
        }
    }

    // (1) Artifact-phase call (empty `tools`): returns the scripted `final_text`
    // as a `Completed` outcome, dispatches no tools, and reports the non-zero
    // prompt tokens the provider metered.
    #[tokio::test]
    async fn artifact_phase_returns_scripted_text_and_nonzero_prompt_tokens() {
        let provider: Arc<dyn Provider> = Arc::new(FakeProvider::new(vec![(
            137,
            ModelTurn::Text("{\"outcome\":\"connect 20m hourly\"}".to_string()),
        )]));
        // The Emit invoker is irrelevant to an artifact phase; supply a harmless
        // store-backed one.
        let dir = tempfile::tempdir().expect("tempdir");
        let emit_invoker: Arc<dyn ToolInvoker> = Arc::new(StoreWritingInvoker {
            store: DefinitionStore::open(dir.path().to_path_buf()),
        });
        let model = SessionPhaseModel::new(provider, emit_invoker);

        let turn = model.run_phase("intent prompt".to_string(), &[]).await;

        assert_eq!(turn.final_text, "{\"outcome\":\"connect 20m hourly\"}");
        assert_eq!(
            turn.outcome,
            RunOutcome::Completed(turn.final_text.clone()),
            "artifact phase is a plain completed text turn"
        );
        assert!(
            turn.tool_calls.is_empty(),
            "artifact phase must dispatch no tools"
        );
        assert_eq!(turn.prompt_tokens, 137);
        assert!(turn.prompt_tokens > 0, "prompt tokens must be metered");
    }

    // (2) Emit-phase call (non-empty `tools`): the scripted model turn issues a
    // `routines_save`, which actually writes the routine into a temp-dir
    // `DefinitionStore`, and `run_phase` returns `tool_calls` such that
    // `capture_artifact(PhaseName::Emit, &turn, &store)` resolves to
    // `Emitted { .. }` — exactly the engine's Emit contract.
    #[tokio::test]
    async fn emit_phase_saves_routine_and_returns_locatable_tool_calls() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Two handles on the SAME dir: the invoker writes; the engine-side
        // handle (what `capture_artifact` reads) sees the same on-disk state —
        // the file-backed store IS the shared channel, exactly as in the
        // battery (routines port + engine store both resolve to the config dir).
        let invoker_store = DefinitionStore::open(dir.path().to_path_buf());
        let engine_store = DefinitionStore::open(dir.path().to_path_buf());

        let def_json = serde_json::json!({
            "routine": "hourly-20m-vara-cms",
            "schema_version": 1,
            "transmit_mode": "attended",
            "triggers": [],
            "tracks": []
        });
        let provider: Arc<dyn Provider> = Arc::new(FakeProvider::new(vec![
            (
                200,
                ModelTurn::ToolCalls(vec![ToolCall::new(
                    "routines_save",
                    serde_json::json!({ "def": def_json }),
                )]),
            ),
            (40, ModelTurn::Text("Built and saved the routine.".to_string())),
        ]));
        let emit_invoker: Arc<dyn ToolInvoker> = Arc::new(StoreWritingInvoker {
            store: invoker_store,
        });
        let model = SessionPhaseModel::new(provider, emit_invoker);

        let manifest = fixture_manifest(vec!["routines".to_string()]);
        let tools = tools_for(PhaseName::Emit, &manifest);
        assert!(
            !tools.is_empty(),
            "precondition: the Emit phase carries the edit verbs"
        );

        let turn = model.run_phase("emit prompt".to_string(), &tools).await;

        // The routine the model saved is now on disk in the engine's store.
        assert!(
            engine_store.get("hourly-20m-vara-cms").is_some(),
            "the emit dispatch must have persisted the routine to the store"
        );

        // And the returned tool_calls carry the name `capture_artifact` reads
        // off them — so the full Emit contract holds end to end.
        let captured = capture_artifact(PhaseName::Emit, &turn, &engine_store)
            .expect("emit phase resolves to an Emitted artifact");
        assert_eq!(
            captured,
            CapturedArtifact::Emitted {
                routine_name: "hourly-20m-vara-cms".to_string()
            }
        );

        // The recorded save call is present, and the final text is the model's
        // narration turn.
        assert_eq!(turn.tool_calls.len(), 1);
        assert_eq!(turn.tool_calls[0].name, "routines_save");
        assert_eq!(
            turn.outcome,
            RunOutcome::Completed("Built and saved the routine.".to_string())
        );
    }

    // (3) Part-97 belt: `EmitDispatch` DENIES a call outside its allow-set and
    // never forwards it to the inner invoker (defense in depth behind the
    // runner's own COR-3 validation, which already blocks an out-of-set name).
    #[tokio::test]
    async fn emit_dispatch_denies_calls_outside_allow_set_without_forwarding() {
        struct PanicInvoker;
        #[async_trait]
        impl ToolInvoker for PanicInvoker {
            fn tools(&self) -> &[ToolSpec] {
                &[]
            }
            async fn invoke(
                &self,
                _call: &ToolCall,
                _authority: CallAuthority,
                _cancel: &CancellationToken,
            ) -> ToolOutcome {
                panic!("inner invoker must not be reached for a denied verb");
            }
        }

        let inner = PanicInvoker;
        let dispatch = EmitDispatch::new(
            vec![ToolSpec::new(
                "routines_save",
                serde_json::json!({ "type": "object" }),
            )],
            &inner,
        );
        let cancel = CancellationToken::new();

        let outcome = dispatch
            .invoke(
                &ToolCall::new("routines_run", serde_json::json!({})),
                CallAuthority::Agent,
                &cancel,
            )
            .await;

        assert!(
            matches!(outcome, ToolOutcome::Denied(_)),
            "a verb outside the allow-set must be denied, not forwarded"
        );
        // The attempt is still recorded (visible in the returned tool_calls) but
        // was not forwarded (the inner invoker would have panicked).
        assert_eq!(dispatch.into_recorded().len(), 1);
    }
}
