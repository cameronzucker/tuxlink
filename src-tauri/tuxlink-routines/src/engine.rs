//! Engine facade: creates runs (snapshot -> journal -> execute), recovers
//! interrupted journals at launch, and implements `RoutineInvoker` so calls
//! between routines are just runs with provenance.
//!
//! `Engine` methods that spawn children (`start_run` / `start_run_ext` /
//! `start_dry_run`) take `self: &Arc<Self>`: a spawned run's
//! `ExecCtx::invoker` needs to hand its own children a real,
//! depth-incrementing invoker bound to the engine itself, so the call depth
//! cap (executor's `Control::Call` arm) actually triggers for real
//! recursive routine chains, not just the direct-`ExecCtx` unit tests in
//! `compose.rs`.
//!
//! `start_run_ext` and `start_dry_run_with_depth` both bottom out in
//! `run_internal`, parameterized over the run's effective `attended` flag
//! (spec §4 consent closure — forced `false` for dry runs), which
//! `ActionRegistry` the run's `ExecCtx` resolves against, and which invoker
//! flavor mounts onto `Control::Call` steps — a normal run always uses
//! `cfg.registry` (the real actions) and `EngineChildInvoker` (real child
//! runs); a dry run always uses a swapped-in `dryrun::build_dryrun_registry`
//! result and `DryRunChildInvoker` (child runs that are ALSO dry runs, so
//! composition never crosses back into real actions partway through, and no
//! descendant ever parks for consent). `run_internal`'s
//! `dry_run_script.is_some()` is the single source of truth for whether a run
//! is a dry run — it drives both the `RunStarted.dry_run` stamp and the
//! invoker choice, so the two can never disagree.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::action::ActionRegistry;
use crate::compose::{CallCtx, ChildHandle, RootConsent, RoutineInvoker};
use crate::consent::ConsentPort;
use crate::dryrun::{build_dryrun_registry, DryRunScript};
use crate::error::{EngineError, StepError};
use crate::executor::{run_tracks, ExecCtx, RunOutcome};
use crate::journal::{scan_interrupted, JournalWriter, RunEvent, RunState};
use crate::snapshot::{resolve_snapshot, EntityResolver};
use crate::types::{RoutineDef, TransmitMode};
use crate::vars::RunVars;

/// Definition lookup for child `Control::Call` runs. This is a callback
/// signature (not a data structure to simplify), so clippy's type-complexity
/// threshold is a false positive here — allow it with this justification.
#[allow(clippy::type_complexity)]
pub type RoutineLookup = Arc<dyn Fn(&str) -> Option<RoutineDef> + Send + Sync>;

pub struct EngineConfig {
    pub journal_dir: PathBuf,
    pub registry: Arc<ActionRegistry>,
    pub resolver: Arc<dyn EntityResolver>,
    pub now: fn() -> i64,
    pub default_timeout_s: u64,
    /// Definition store lookup for child runs spawned by `Control::Call`
    /// (spec §7). `None` in tests that don't compose routines; plan 2's
    /// monolith supplies the real definition store.
    pub lookup: Option<RoutineLookup>,
    /// The attended-consent parking desk (spec §4, §8). `None` in leaf tests;
    /// the monolith supplies its `ConsentRegistry`. Threaded onto every run's
    /// `ExecCtx` so the executor can park an attended run's transmit steps for
    /// operator consent BEFORE the per-step timeout (see
    /// [`crate::executor::ExecCtx::consent`]).
    pub consent: Option<Arc<dyn ConsentPort>>,
}

pub struct Engine {
    cfg: EngineConfig,
    counter: AtomicU64,
    /// Post-construction child-run invoker (O3). The monolith builds the engine
    /// before its `RoutinesState` exists (construction cycle), then installs a
    /// `SessionChildInvoker` here via [`Engine::install_child_invoker`]; when
    /// set, `run_internal`'s non-dry arm prefers it over the internally-built
    /// [`EngineChildInvoker`]. Dry runs never consult it.
    child_invoker: OnceLock<Arc<dyn RoutineInvoker>>,
}

pub struct RunHandle {
    pub run_id: String,
    pub cancel: CancellationToken,
    pub done: tokio::sync::oneshot::Receiver<RunOutcome>,
}

/// Options for [`Engine::start_run_ext`], pinned ONCE for both O3 (B2) and the
/// consent binding (C3) so the signature does not churn twice. B2 always passes
/// `root: None`; C3 fills it with the started run's [`RootConsent`].
pub struct StartOpts {
    /// Call depth (root = 0; each `Control::Call` increments by one).
    pub depth: u32,
    /// The spawning run's effective attended flag (`false` for a root run).
    pub parent_attended: bool,
    /// Forces the effective attended flag `false` (legacy dry-suppression path).
    pub dry_run: bool,
    /// The child's cancellation parent, when this run is a child (so a parent
    /// cancel propagates). `None` mints a fresh root token.
    pub cancel: Option<CancellationToken>,
    /// The root consent context (C3); `None` in B2.
    pub root: Option<RootConsent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterruptedRun {
    pub run_id: String,
    pub journal_path: PathBuf,
}

impl Engine {
    pub fn new(cfg: EngineConfig) -> Self {
        Engine {
            cfg,
            counter: AtomicU64::new(0),
            child_invoker: OnceLock::new(),
        }
    }

    /// Install the post-construction child-run invoker (O3). Idempotent-ish:
    /// a second install is ignored (the `OnceLock` keeps the first). The
    /// monolith calls this at the end of `build_routines_state`.
    pub fn install_child_invoker(&self, invoker: Arc<dyn RoutineInvoker>) {
        let _ = self.child_invoker.set(invoker);
    }

    fn next_run_id(&self) -> String {
        // now() + counter: unique within a process lifetime and sortable;
        // uuid is unnecessary for on-disk journal names.
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("run-{}-{n:04}", (self.cfg.now)())
    }

    /// Start a root run (call depth 0), non-dry, with no attended ancestor.
    /// The run's own `transmit_mode` still governs whether ITS transmit steps
    /// pause (spec §4); this is the plain entry point for a real, top-level run.
    pub async fn start_run(
        self: &Arc<Self>,
        def: &RoutineDef,
        args: serde_json::Value,
    ) -> Result<RunHandle, EngineError> {
        self.start_run_ext(
            def,
            args,
            StartOpts {
                depth: 0,
                parent_attended: false,
                dry_run: false,
                cancel: None,
                root: None,
            },
        )
        .await
    }

    /// Start a run at an explicit call depth (root = 0; each `Control::Call`
    /// a child run's invoker spawns increments it by one). `self: &Arc<Self>`
    /// so the child invoker mounted onto this run's `ExecCtx` can hold its
    /// own `Arc<Engine>` clone and call back into `start_run_ext` for
    /// grandchildren, real recursion depth included.
    ///
    /// `parent_attended` is the effective attended flag of the run that spawned
    /// this one (`false` for a root run); `dry_run` marks a fake-world run
    /// (plan 3) that must never pause for consent. This run's own effective
    /// attended flag — the value the executor reads (with the step's
    /// `transmits` descriptor and `ExecCtx::consent`) to park a transmit step
    /// on the [`ConsentPort`] BEFORE its timed execute (plan 2 Task 5b) — is
    /// the sticky OR of the run's own `transmit_mode == attended` with
    /// `parent_attended`, forced `false` under `dry_run`. This is what makes
    /// consent closure propagate down a call chain: an attended parent's
    /// transmitting callee pauses too, and an attended callee of an automatic
    /// parent still pauses (spec §4, §10 consent closure).
    pub async fn start_run_ext(
        self: &Arc<Self>,
        def: &RoutineDef,
        args: serde_json::Value,
        opts: StartOpts,
    ) -> Result<RunHandle, EngineError> {
        let attended = !opts.dry_run
            && (def.transmit_mode == TransmitMode::Attended || opts.parent_attended);
        // C3: the root consent context threads onto this run's `ExecCtx` and, from
        // there, unchanged down every `Control::Call` so a registry invoker
        // re-verifies the root's digests at each child start.
        self.run_internal(
            def,
            args,
            opts.depth,
            attended,
            self.cfg.registry.clone(),
            None,
            opts.cancel,
            opts.root,
        )
        .await
    }

    /// Dry-run entry point (plan-3 task 5): start a root dry run (call
    /// depth 0). Builds a registry where every action `cfg.registry`
    /// declares is replaced by a scripted `FakeAction` mirroring its
    /// capability flags (`dryrun::build_dryrun_registry`), stamps this
    /// run's `RunStarted.dry_run: true`, and NEVER touches `cfg.registry`'s
    /// real actions — including through `Control::Call` composition, which
    /// mounts `DryRunChildInvoker` so a called routine's run is ALSO a dry
    /// run, not a real one wearing the parent's dry-run stamp.
    pub async fn start_dry_run(
        self: &Arc<Self>,
        def: &RoutineDef,
        args: serde_json::Value,
        script: DryRunScript,
    ) -> Result<RunHandle, EngineError> {
        self.start_dry_run_with_depth(def, args, 0, script, None)
            .await
    }

    /// Dry-run counterpart to `start_run_ext`: same depth-cap
    /// bookkeeping, but the registry is rebuilt fresh from `cfg.registry`'s
    /// descriptors at this depth level too (a called routine's dry run gets
    /// its own scripted `FakeAction`s replaying `script` from the start,
    /// not a continuation of the parent's queues — the script is not
    /// consumed cross-run).
    async fn start_dry_run_with_depth(
        self: &Arc<Self>,
        def: &RoutineDef,
        args: serde_json::Value,
        depth: u32,
        script: DryRunScript,
        cancel: Option<CancellationToken>,
    ) -> Result<RunHandle, EngineError> {
        let registry = Arc::new(build_dryrun_registry(
            &self.cfg.registry.descriptors(),
            script.clone(),
        ));
        // A dry run NEVER parks for consent (this branch's invariant, spec §4):
        // the fake-world run has no real transmit, so `attended` is forced
        // `false` for the entire dry-run tree — `DryRunChildInvoker` recurses
        // back through here, so every descendant is likewise non-attended.
        // A dry run never binds a consent digest (it cannot key a transmitter or
        // write config), so it threads no root context.
        self.run_internal(def, args, depth, false, registry, Some(script), cancel, None)
            .await
    }

    /// Shared implementation behind `start_run_ext` and
    /// `start_dry_run_with_depth`: everything about starting a run
    /// (snapshot -> journal -> spawn the track executor) is identical
    /// between a real run and a dry run except WHICH `ActionRegistry` the
    /// spawned `ExecCtx` resolves actions against and which `RoutineInvoker`
    /// mounts onto `Control::Call` steps. `dry_run_script.is_some()` is the
    /// single source of truth for "is this run a dry run" — it drives both
    /// the `RunStarted.dry_run` stamp and the invoker choice, so the two
    /// can never disagree.
    #[allow(clippy::too_many_arguments)]
    async fn run_internal(
        self: &Arc<Self>,
        def: &RoutineDef,
        args: serde_json::Value,
        depth: u32,
        attended: bool,
        registry: Arc<ActionRegistry>,
        dry_run_script: Option<DryRunScript>,
        cancel: Option<CancellationToken>,
        root: Option<RootConsent>,
    ) -> Result<RunHandle, EngineError> {
        let dry_run = dry_run_script.is_some();
        let run_id = self.next_run_id();
        let snapshot = resolve_snapshot(def, self.cfg.resolver.as_ref()).await?;

        // Orphan-journal fix (O3): parse the snapshot into the executable
        // `RoutineDef` BEFORE creating the journal file. A SnapshotShape
        // failure returns `Err` with NO journal on disk — the old order
        // created the journal + `RunStarted` first, so a shape failure left an
        // orphaned RunStarted-only journal that recovery then reported as an
        // interrupted run that never actually started.
        let resolved: RoutineDef = serde_json::from_value(snapshot.clone())
            .map_err(|e| EngineError::SnapshotShape(e.to_string()))?;

        let mut journal = JournalWriter::create(&self.cfg.journal_dir, &run_id, self.cfg.now)?;
        journal.append(RunEvent::RunStarted {
            routine: def.routine.clone(),
            snapshot,
            dry_run,
        })?;

        let mut vars = RunVars::default();
        if let serde_json::Value::Object(map) = args {
            for (k, v) in map {
                vars.set_input(&k, v);
            }
        }

        // The run's cancellation token: a child derives from its parent (so a
        // parent cancel propagates); a root mints a fresh one.
        let cancel = cancel.unwrap_or_default();
        // Non-dry runs prefer an installed `child_invoker` (the monolith's
        // `SessionChildInvoker`, which registers children for cancellation);
        // dry runs always recurse into `DryRunChildInvoker`.
        let invoker: Arc<dyn RoutineInvoker> = if let Some(script) = &dry_run_script {
            match &self.cfg.lookup {
                Some(lookup) => Arc::new(DryRunChildInvoker {
                    engine: self.clone(),
                    lookup: lookup.clone(),
                    script: script.clone(),
                }),
                None => Arc::new(NoInvoker),
            }
        } else if let Some(installed) = self.child_invoker.get() {
            installed.clone()
        } else {
            match &self.cfg.lookup {
                Some(lookup) => Arc::new(EngineChildInvoker {
                    engine: self.clone(),
                    lookup: lookup.clone(),
                }),
                None => Arc::new(NoInvoker),
            }
        };
        let ctx = ExecCtx {
            registry,
            journal: Arc::new(Mutex::new(journal)),
            cancel: cancel.clone(),
            default_timeout_s: self.cfg.default_timeout_s,
            now: self.cfg.now,
            invoker,
            run_id: run_id.clone(),
            depth,
            attended,
            consent: self.cfg.consent.clone(),
            root,
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tracks = resolved.tracks.clone();
        let vars = Arc::new(tokio::sync::Mutex::new(vars));
        let journal_arc = ctx.journal.clone();
        tokio::spawn(async move {
            let result = run_tracks(&tracks, vars, &ctx).await;
            // O4: an End-terminated run carries its authored reason + end step
            // through `RunOutcome`; propagate it into `RunFinished` (the old
            // code always wrote `None` here, dropping every End reason). A
            // propagated `StepErr` still wins — `run_tracks` returns `Err` for
            // it, so the error string is used ahead of any End reason.
            let (state, reason, end_step) = match &result {
                Ok(o) => (o.state, o.reason.clone(), o.end_step.clone()),
                Err(StepError::Cancelled) => (RunState::Cancelled, None, None),
                Err(e) => (RunState::Failed, Some(e.to_string()), None),
            };
            let outcome = RunOutcome {
                state,
                reason: reason.clone(),
                end_step,
            };
            // RunFinished is appended exactly once, on every path (success,
            // failure, cancel) — this is the single point that does it. Use
            // `unwrap_or_else` rather than `unwrap` on the lock: if a
            // sibling track's journal write panicked and poisoned this
            // mutex mid-run, we must still append the terminal event rather
            // than panicking here ourselves and leaving the run a zombie
            // (no RunFinished ever recorded, `handle.done` never resolved).
            let mut journal_guard = journal_arc.lock().unwrap_or_else(|e| e.into_inner());
            let _ = journal_guard.append(RunEvent::RunFinished { state, reason });
            drop(journal_guard);
            let _ = tx.send(outcome);
        });
        Ok(RunHandle {
            run_id,
            cancel,
            done: rx,
        })
    }

    /// Launch-time recovery (spec §8): make every dead journal explicitly,
    /// terminally Interrupted. `on_interrupted: resume` handling is plan 5's
    /// command layer (it re-invokes from the journal snapshot).
    pub fn recover(&self) -> Result<Vec<InterruptedRun>, EngineError> {
        let mut out = Vec::new();
        for (run_id, path) in scan_interrupted(&self.cfg.journal_dir)? {
            let mut w = JournalWriter::create(&self.cfg.journal_dir, &run_id, self.cfg.now)?;
            w.append(RunEvent::RunFinished {
                state: RunState::Interrupted,
                reason: Some("process terminated underneath this run".into()),
            })?;
            out.push(InterruptedRun {
                run_id,
                journal_path: path,
            });
        }
        Ok(out)
    }
}

/// Invoker mounted when the engine has no definition lookup configured
/// (`EngineConfig.lookup == None`) — e.g. unit tests exercising a single
/// routine with no composition. Any `Control::Call` step fails loudly rather
/// than silently no-opping.
struct NoInvoker;

#[async_trait]
impl RoutineInvoker for NoInvoker {
    async fn start(
        &self,
        routine: &str,
        _args: serde_json::Value,
        _call: CallCtx,
        _parent_cancel: &CancellationToken,
    ) -> Result<ChildHandle, StepError> {
        Err(StepError::Action {
            action: format!("call:{routine}"),
            cause: "no routine invoker is mounted (engine configured without a definition lookup)"
                .into(),
        })
    }

    async fn await_outcome(
        &self,
        _handle: ChildHandle,
    ) -> Result<serde_json::Value, StepError> {
        // Unreachable: `start` always errors, so no handle is ever produced.
        Err(StepError::Action {
            action: "call".into(),
            cause: "no routine invoker is mounted".into(),
        })
    }
}

/// The real invoker: an `Arc<Engine>` + a definition lookup. Per-call depth,
/// parent-attended, and cancellation now ride the [`CallCtx`] +
/// `parent_cancel` the executor passes to `start`, so a SINGLE invoker mounts
/// for the engine's whole lifetime (the OnceLock-installed monolith invoker
/// works the same way) instead of one per run bound at construction.
struct EngineChildInvoker {
    engine: Arc<Engine>,
    lookup: RoutineLookup,
}

#[async_trait]
impl RoutineInvoker for EngineChildInvoker {
    async fn start(
        &self,
        routine: &str,
        args: serde_json::Value,
        call: CallCtx,
        parent_cancel: &CancellationToken,
    ) -> Result<ChildHandle, StepError> {
        let def = (self.lookup)(routine).ok_or_else(|| StepError::Action {
            action: format!("call:{routine}"),
            cause: format!(
                "routine '{routine}' not found (invoked by {} step {})",
                call.provenance.parent_run_id, call.provenance.parent_step.0
            ),
        })?;
        let handle = self
            .engine
            .start_run_ext(
                &def,
                args,
                StartOpts {
                    depth: call.child_depth,
                    // A child inherits THIS run's effective attended flag as its
                    // `parent_attended`, so consent closure propagates (spec §10).
                    parent_attended: call.parent_attended,
                    dry_run: false,
                    // The child's cancellation derives from the parent's, so a
                    // parent cancel propagates down the call chain.
                    cancel: Some(parent_cancel.child_token()),
                    // C3: propagate the SAME root context unchanged so every
                    // descendant re-verifies against the top-level ack.
                    root: call.root,
                },
            )
            .await
            .map_err(|e| StepError::Action {
                action: format!("call:{routine}"),
                cause: e.to_string(),
            })?;
        let RunHandle {
            run_id,
            cancel,
            done,
        } = handle;
        Ok(ChildHandle::from_parts(run_id, cancel, done))
    }

    async fn await_outcome(
        &self,
        handle: ChildHandle,
    ) -> Result<serde_json::Value, StepError> {
        await_child_handle(handle).await
    }
}

/// Dry-run counterpart to `EngineChildInvoker` (plan-3 task 5): a
/// `Control::Call` step reached while dry-running recurses into
/// `Engine::start_dry_run_with_depth` — ANOTHER dry run, one level deeper —
/// instead of `start_run_ext`'s real-registry path, so a routine's
/// entire call closure stays inside the fake-action world a dry run
/// promises, not just its own top-level steps.
struct DryRunChildInvoker {
    engine: Arc<Engine>,
    lookup: RoutineLookup,
    script: DryRunScript,
}

#[async_trait]
impl RoutineInvoker for DryRunChildInvoker {
    async fn start(
        &self,
        routine: &str,
        args: serde_json::Value,
        call: CallCtx,
        parent_cancel: &CancellationToken,
    ) -> Result<ChildHandle, StepError> {
        let def = (self.lookup)(routine).ok_or_else(|| StepError::Action {
            action: format!("call:{routine}"),
            cause: format!(
                "routine '{routine}' not found (invoked by {} step {})",
                call.provenance.parent_run_id, call.provenance.parent_step.0
            ),
        })?;
        let handle = self
            .engine
            .start_dry_run_with_depth(
                &def,
                args,
                call.child_depth,
                self.script.clone(),
                Some(parent_cancel.child_token()),
            )
            .await
            .map_err(|e| StepError::Action {
                action: format!("call:{routine}"),
                cause: e.to_string(),
            })?;
        let RunHandle {
            run_id,
            cancel,
            done,
        } = handle;
        Ok(ChildHandle::from_parts(run_id, cancel, done))
    }

    async fn await_outcome(
        &self,
        handle: ChildHandle,
    ) -> Result<serde_json::Value, StepError> {
        await_child_handle(handle).await
    }
}

/// Shared by `EngineChildInvoker` and `DryRunChildInvoker`: await a spawned
/// child run's outcome and translate it into the `Control::Call` step's
/// result value or a verbatim `StepError`.
async fn await_child_handle(handle: ChildHandle) -> Result<serde_json::Value, StepError> {
    let (run_id, _cancel, done) = handle.into_parts();
    let outcome = done.await.map_err(|_| StepError::Action {
        action: "call".into(),
        cause: format!("child run {run_id} task dropped without an outcome"),
    })?;
    match outcome.state {
        RunState::Completed => Ok(serde_json::json!({"completed": true, "run_id": run_id})),
        other => Err(StepError::Action {
            action: "call".into(),
            cause: format!("child run {run_id} ended {other:?}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionRegistry;
    use crate::fakes::{FakeAction, FakeInvoker, FakeResolver};
    use crate::journal::{read_journal, JournalEntry, RunEvent, RunState};
    use crate::types::RoutineDef;
    use serde_json::json;
    use std::sync::Arc;

    fn fixed_now() -> i64 {
        1_752_400_000
    }

    const DEF: &str = r#"{
      "routine": "quick", "schema_version": 1, "transmit_mode": "attended",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "local.log", "params": {} }
      ]}]
    }"#;

    /// A routine that calls itself synchronously — used to prove the depth
    /// cap actually triggers against real, engine-spawned child runs (not
    /// just the direct-`ExecCtx` `compose.rs` test that sets `ctx.depth`
    /// by hand).
    const LOOP_DEF: &str = r#"{
      "routine": "loop-a", "schema_version": 1, "transmit_mode": "attended",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "c1", "control": "call", "routine": "loop-a", "args": {}, "sync": true }
      ]}]
    }"#;

    /// A one-step attended routine whose only action transmits — used to prove
    /// `EngineConfig.consent` threads through to the executor's parking.
    const TX_DEF: &str = r#"{
      "routine": "tx", "schema_version": 1, "transmit_mode": "attended",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "radio.tx", "params": {} }
      ]}]
    }"#;

    fn engine(dir: &std::path::Path) -> Arc<Engine> {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("local.log").ok(json!({}))));
        Arc::new(Engine::new(EngineConfig {
            journal_dir: dir.to_path_buf(),
            registry: Arc::new(reg),
            resolver: Arc::new(FakeResolver::new()),
            now: fixed_now,
            default_timeout_s: 30,
            lookup: None,
            consent: None,
        }))
    }

    #[tokio::test]
    async fn start_run_journals_started_snapshot_and_finishes_completed() {
        let dir = tempfile::tempdir().unwrap();
        let eng = engine(dir.path());
        let def = RoutineDef::parse(DEF).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Completed);

        let jpath = dir.path().join(format!("{}.jsonl", handle.run_id));
        let entries = read_journal(&jpath).unwrap();
        assert!(matches!(&entries.first().unwrap().event,
            RunEvent::RunStarted { routine, snapshot, dry_run } if routine == "quick" && snapshot.is_object() && !dry_run));
        assert!(matches!(
            &entries.last().unwrap().event,
            RunEvent::RunFinished {
                state: RunState::Completed,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn attended_transmit_run_parks_on_the_configured_consent_port() {
        // Proves the EngineConfig.consent → ExecCtx.consent → executor thread:
        // an attended run with a transmitting action parks on the configured
        // port before the action runs, and completes once the port grants.
        let dir = tempfile::tempdir().unwrap();
        let mut reg = ActionRegistry::default();
        let tx = Arc::new(
            FakeAction::new("radio.tx")
                .with_capabilities(true, true, false)
                .ok(json!({"sent": true})),
        );
        reg.register(tx.clone());
        let consent = Arc::new(crate::fakes::FakeConsent::granting_after(
            std::time::Duration::ZERO,
        ));
        let eng = Arc::new(Engine::new(EngineConfig {
            journal_dir: dir.path().to_path_buf(),
            registry: Arc::new(reg),
            resolver: Arc::new(FakeResolver::new()),
            now: fixed_now,
            default_timeout_s: 30,
            lookup: None,
            consent: Some(consent.clone()),
        }));
        let def = RoutineDef::parse(TX_DEF).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Completed);
        // The transmit step parked on the port (keyed by its step id), then ran.
        let parked = consent.parked();
        assert_eq!(parked.len(), 1, "the transmit step parked exactly once");
        assert_eq!(parked[0].1, "s1");
        assert_eq!(tx.calls().len(), 1, "the transmit ran after the grant");
    }

    #[tokio::test]
    async fn cancel_produces_a_cancelled_terminal_state() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("local.log").hang()));
        let eng = Arc::new(Engine::new(EngineConfig {
            journal_dir: dir.path().to_path_buf(),
            registry: Arc::new(reg),
            resolver: Arc::new(FakeResolver::new()),
            now: fixed_now,
            default_timeout_s: 3600,
            lookup: None,
            consent: None,
        }));
        let def = RoutineDef::parse(DEF).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        handle.cancel.cancel();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Cancelled);
    }

    #[tokio::test]
    async fn recover_marks_dead_journals_interrupted_terminally() {
        let dir = tempfile::tempdir().unwrap();
        // Fabricate a dead journal (crash: no RunFinished).
        {
            let mut w =
                crate::journal::JournalWriter::create(dir.path(), "run-dead", fixed_now).unwrap();
            w.append(RunEvent::RunStarted {
                routine: "quick".into(),
                snapshot: json!({}),
                dry_run: false,
            })
            .unwrap();
        }
        let eng = engine(dir.path());
        let recovered = eng.recover().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].run_id, "run-dead");
        // The journal is now terminal — a second scan finds nothing.
        assert!(eng.recover().unwrap().is_empty());
        let entries = read_journal(&dir.path().join("run-dead.jsonl")).unwrap();
        assert!(matches!(
            &entries.last().unwrap().event,
            RunEvent::RunFinished {
                state: RunState::Interrupted,
                ..
            }
        ));
        // FINDING 2 regression: recover() reopens an EXISTING journal via
        // `JournalWriter::create` to append the terminal entry. seq must
        // resume from the prior entry count (1 pre-existing entry here), not
        // restart at 0 and collide with `RunStarted`'s own seq 0.
        let seqs: Vec<u64> = entries.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![0, 1]);
        assert_eq!(entries.last().unwrap().seq, (entries.len() - 1) as u64);
    }

    #[tokio::test]
    async fn recover_appends_monotonic_seq_across_multiple_prior_entries() {
        // Same regression as above, generalized to a journal with several
        // pre-crash entries: the appended RunFinished's seq must equal the
        // count of entries that existed before recovery, for any prior
        // entry count — not just the single-entry case.
        let dir = tempfile::tempdir().unwrap();
        {
            let mut w =
                crate::journal::JournalWriter::create(dir.path(), "run-dead-2", fixed_now).unwrap();
            w.append(RunEvent::RunStarted {
                routine: "quick".into(),
                snapshot: json!({}),
                dry_run: false,
            })
            .unwrap();
            w.append(RunEvent::StepIntent {
                step: crate::types::StepId("s1".into()),
                action: "local.log".into(),
                resolved_params: json!({}),
            })
            .unwrap();
            w.append(RunEvent::StepOk {
                step: crate::types::StepId("s1".into()),
                output: json!({}),
            })
            .unwrap();
        }
        let prior_count = read_journal(&dir.path().join("run-dead-2.jsonl"))
            .unwrap()
            .len();

        let eng = engine(dir.path());
        let recovered = eng.recover().unwrap();
        assert_eq!(recovered.len(), 1);

        let entries = read_journal(&dir.path().join("run-dead-2.jsonl")).unwrap();
        let last = entries.last().unwrap();
        assert!(matches!(
            last.event,
            RunEvent::RunFinished {
                state: RunState::Interrupted,
                ..
            }
        ));
        assert_eq!(last.seq, prior_count as u64);
        let seqs: Vec<u64> = entries.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![0, 1, 2, 3]);
    }

    #[tokio::test]
    async fn real_child_runs_hit_the_depth_cap() {
        let dir = tempfile::tempdir().unwrap();
        let reg = ActionRegistry::default(); // no actions needed: only Call steps
        let lookup: RoutineLookup = Arc::new(|name: &str| {
            if name == "loop-a" {
                Some(RoutineDef::parse(LOOP_DEF).unwrap())
            } else {
                None
            }
        });
        let eng = Arc::new(Engine::new(EngineConfig {
            journal_dir: dir.path().to_path_buf(),
            registry: Arc::new(reg),
            resolver: Arc::new(FakeResolver::new()),
            now: fixed_now,
            default_timeout_s: 30,
            lookup: Some(lookup),
            consent: None,
        }));
        let def = RoutineDef::parse(LOOP_DEF).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Failed);

        // The chain of real child runs — one journal file per depth level —
        // must include a StepErr whose cause names the depth cap explicitly.
        let mut found_depth_err = false;
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let entries = read_journal(&path).unwrap();
            for e in &entries {
                if let RunEvent::StepErr {
                    error: crate::error::StepError::Action { cause, .. },
                    ..
                } = &e.event
                {
                    if cause.contains("depth") {
                        found_depth_err = true;
                    }
                }
            }
        }
        assert!(
            found_depth_err,
            "expected some journal in the recursion chain to record a depth-cap StepErr"
        );
    }

    // --- start_dry_run (plan-3 task 5) --------------------------------------

    /// Mirrors `executor.rs`'s own `branch_follows_the_variable_and_jumps`
    /// fixture: s1 connects, s2 branches on `s1.connected`, then-arm (s3)
    /// ends the run; else-arm (s4) is only reached via the jump, never
    /// sequentially (s3's explicit End stops the run before falling
    /// through to s4).
    const BRANCH_DEF: &str = r#"{
      "routine": "branchy", "schema_version": 1, "transmit_mode": "attended",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "radio.connect", "params": {} },
        { "id": "s2", "control": "branch", "on": "s1.connected", "then": ["s3"], "else": ["s4"] },
        { "id": "s3", "action": "local.log_then", "params": {} },
        { "id": "end1", "control": "end", "failed": false },
        { "id": "s4", "action": "local.log_else", "params": {} }
      ]}]
    }"#;

    /// The "real" engine + registry a dry run must never touch: `connect`
    /// is the canary the task's test contract asks for — after every dry
    /// run below, `connect.calls()` must still be empty. Scripted
    /// `connected: false` so the one test that DOES exercise the real
    /// registry (`start_run_is_unaffected_by_dry_run_support...`) has a
    /// deterministic real-run branch outcome to assert against.
    fn branch_engine_with_canary(dir: &std::path::Path) -> (Arc<Engine>, Arc<FakeAction>) {
        let connect = Arc::new(
            FakeAction::new("radio.connect")
                .with_capabilities(true, false, false)
                .ok(json!({"connected": false})),
        );
        let mut reg = ActionRegistry::default();
        reg.register(connect.clone());
        reg.register(Arc::new(FakeAction::new("local.log_then").ok(json!({}))));
        reg.register(Arc::new(FakeAction::new("local.log_else").ok(json!({}))));
        let eng = Arc::new(Engine::new(EngineConfig {
            journal_dir: dir.to_path_buf(),
            registry: Arc::new(reg),
            resolver: Arc::new(FakeResolver::new()),
            now: fixed_now,
            default_timeout_s: 30,
            lookup: None,
            consent: None,
        }));
        (eng, connect)
    }

    fn executed_step_ids(entries: &[crate::journal::JournalEntry]) -> Vec<String> {
        entries
            .iter()
            .filter_map(|e| match &e.event {
                RunEvent::StepIntent { step, .. } => Some(step.0.clone()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn dry_run_scripted_error_fails_the_run_stamps_dry_run_and_never_touches_the_real_action()
    {
        use crate::dryrun::{DryRunOutcome, DryRunScript};

        let dir = tempfile::tempdir().unwrap();
        let (eng, connect) = branch_engine_with_canary(dir.path());
        let def = RoutineDef::parse(BRANCH_DEF).unwrap();

        let script = DryRunScript::new().with_outcomes(
            "radio.connect",
            vec![DryRunOutcome::Err("VARA: BUSY channel occupied".into())],
        );
        let handle = eng.start_dry_run(&def, json!({}), script).await.unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Failed);

        let jpath = dir.path().join(format!("{}.jsonl", handle.run_id));
        let entries = read_journal(&jpath).unwrap();
        assert!(matches!(
            &entries.first().unwrap().event,
            RunEvent::RunStarted { routine, dry_run, .. }
                if routine == "branchy" && *dry_run
        ));
        // The scripted error stopped the run at s1 — the branch was never
        // reached, so neither s3 nor s4 executed.
        assert_eq!(executed_step_ids(&entries), vec!["s1"]);
        assert!(
            connect.calls().is_empty(),
            "real radio.connect must never be called by a dry run"
        );
    }

    #[tokio::test]
    async fn dry_run_scripted_connect_true_takes_the_then_arm_and_completes() {
        use crate::dryrun::{DryRunOutcome, DryRunScript};

        let dir = tempfile::tempdir().unwrap();
        let (eng, connect) = branch_engine_with_canary(dir.path());
        let def = RoutineDef::parse(BRANCH_DEF).unwrap();

        let script = DryRunScript::new().with_outcomes(
            "radio.connect",
            vec![DryRunOutcome::Ok(json!({"connected": true}))],
        );
        let handle = eng.start_dry_run(&def, json!({}), script).await.unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Completed);

        let jpath = dir.path().join(format!("{}.jsonl", handle.run_id));
        let entries = read_journal(&jpath).unwrap();
        assert!(matches!(
            &entries.first().unwrap().event,
            RunEvent::RunStarted { dry_run, .. } if *dry_run
        ));
        assert_eq!(executed_step_ids(&entries), vec!["s1", "s3"]);
        assert!(connect.calls().is_empty());
    }

    #[tokio::test]
    async fn dry_run_scripted_connect_false_takes_the_else_arm() {
        use crate::dryrun::{DryRunOutcome, DryRunScript};

        let dir = tempfile::tempdir().unwrap();
        let (eng, connect) = branch_engine_with_canary(dir.path());
        let def = RoutineDef::parse(BRANCH_DEF).unwrap();

        let script = DryRunScript::new().with_outcomes(
            "radio.connect",
            vec![DryRunOutcome::Ok(json!({"connected": false}))],
        );
        let handle = eng.start_dry_run(&def, json!({}), script).await.unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Completed);

        let jpath = dir.path().join(format!("{}.jsonl", handle.run_id));
        let entries = read_journal(&jpath).unwrap();
        assert_eq!(executed_step_ids(&entries), vec!["s1", "s4"]);
        assert!(connect.calls().is_empty());
    }

    #[tokio::test]
    async fn dry_run_optimistic_default_drives_connected_true_through_the_branch() {
        use crate::dryrun::DryRunScript;

        let dir = tempfile::tempdir().unwrap();
        let (eng, connect) = branch_engine_with_canary(dir.path());
        let def = RoutineDef::parse(BRANCH_DEF).unwrap();

        // No scripted outcome for "radio.connect" at all: the default
        // policy (Optimistic, DryRunScript's own Default) must supply
        // `connected: true` for a needs_radio descriptor, per dryrun.rs's
        // documented rule — so the then-arm executes without the caller
        // having to script every radio step by hand.
        let handle = eng
            .start_dry_run(&def, json!({}), DryRunScript::new())
            .await
            .unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Completed);

        let jpath = dir.path().join(format!("{}.jsonl", handle.run_id));
        let entries = read_journal(&jpath).unwrap();
        assert_eq!(executed_step_ids(&entries), vec!["s1", "s3"]);
        assert!(connect.calls().is_empty());
    }

    #[tokio::test]
    async fn start_run_is_unaffected_by_dry_run_support_and_stamps_dry_run_false() {
        // Regression guard for the task-5 refactor that threaded `run_internal`
        // through both `start_run_ext` and `start_dry_run_with_depth`:
        // a REAL run must still touch the real registry and stamp `dry_run: false`.
        let dir = tempfile::tempdir().unwrap();
        let (eng, connect) = branch_engine_with_canary(dir.path());
        let def = RoutineDef::parse(BRANCH_DEF).unwrap();

        let handle = eng.start_run(&def, json!({})).await.unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Completed);

        let jpath = dir.path().join(format!("{}.jsonl", handle.run_id));
        let entries = read_journal(&jpath).unwrap();
        assert!(matches!(
            &entries.first().unwrap().event,
            RunEvent::RunStarted { dry_run, .. } if !*dry_run
        ));
        // The canary is scripted `connected: false` (see
        // `branch_engine_with_canary`), so the real run takes the else-arm.
        assert_eq!(executed_step_ids(&entries), vec!["s1", "s4"]);
        assert_eq!(
            connect.calls().len(),
            1,
            "a real run DOES call the real action"
        );
    }

    #[tokio::test]
    async fn dry_run_call_step_spawns_dry_child_and_never_touches_real_actions() {
        use crate::dryrun::DryRunScript;

        // Child routine: one track with one action step (radio.connect)
        const CHILD_DEF: &str = r#"{
          "routine": "child-routine", "schema_version": 1, "transmit_mode": "attended",
          "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
          "tracks": [{ "name": "t", "steps": [
            { "id": "s1", "action": "radio.connect", "params": {} }
          ]}]
        }"#;

        // Parent routine: one track with one Control::Call step to child-routine
        const PARENT_DEF: &str = r#"{
          "routine": "parent", "schema_version": 1, "transmit_mode": "attended",
          "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
          "tracks": [{ "name": "t", "steps": [
            { "id": "c1", "control": "call", "routine": "child-routine", "args": {}, "sync": true }
          ]}]
        }"#;

        let dir = tempfile::tempdir().unwrap();

        // Canary action (the real one in cfg.registry that must never be touched)
        let connect = Arc::new(
            FakeAction::new("radio.connect")
                .with_capabilities(true, false, false)
                .ok(json!({"connected": true})),
        );

        // Build registry with canary
        let mut reg = ActionRegistry::default();
        reg.register(connect.clone());

        // Lookup that resolves "child-routine" to the child definition
        let lookup: RoutineLookup = Arc::new(|name: &str| {
            if name == "child-routine" {
                Some(RoutineDef::parse(CHILD_DEF).unwrap())
            } else {
                None
            }
        });

        let eng = Arc::new(Engine::new(EngineConfig {
            journal_dir: dir.path().to_path_buf(),
            registry: Arc::new(reg),
            resolver: Arc::new(FakeResolver::new()),
            now: fixed_now,
            default_timeout_s: 30,
            lookup: Some(lookup),
            consent: None,
        }));

        // Run parent with dry_run
        let parent_def = RoutineDef::parse(PARENT_DEF).unwrap();
        let handle = eng
            .start_dry_run(&parent_def, json!({}), DryRunScript::new())
            .await
            .unwrap();
        let outcome = handle.done.await.unwrap();

        // Assert: parent run completed
        assert_eq!(outcome.state, RunState::Completed);

        // Assert: canary real action was never called (dry run never touched registry)
        assert!(
            connect.calls().is_empty(),
            "real radio.connect must never be called by a dry run or its children"
        );

        // Find and verify parent journal has dry_run: true
        let parent_jpath = dir.path().join(format!("{}.jsonl", handle.run_id));
        let parent_entries = read_journal(&parent_jpath).unwrap();
        assert!(matches!(
            &parent_entries.first().unwrap().event,
            RunEvent::RunStarted { routine, dry_run, .. }
                if routine == "parent" && *dry_run
        ));

        // Find child journal: scan all .jsonl files, find one with
        // RunStarted { routine: "child-routine", dry_run: true }
        let mut child_found = false;
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if path == parent_jpath {
                continue; // Skip parent journal
            }
            let child_entries = read_journal(&path).unwrap();
            if let RunEvent::RunStarted {
                routine, dry_run, ..
            } = &child_entries.first().unwrap().event
            {
                if routine == "child-routine" && *dry_run {
                    child_found = true;
                    break;
                }
            }
        }
        assert!(
            child_found,
            "child routine should have its own journal with dry_run: true"
        );
    }

    // --- Task B2: two-phase invoker, CallChild emission, cancellation,
    //     orphan-journal fix (O3) -----------------------------------------

    const C_OK: &str = r#"{
      "routine": "child-ok", "schema_version": 1, "transmit_mode": "automatic",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "cs1", "action": "local.log", "params": {} }
      ]}]
    }"#;

    const C_FAIL: &str = r#"{
      "routine": "child-fail", "schema_version": 1, "transmit_mode": "automatic",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "cs1", "action": "boom", "params": {} }
      ]}]
    }"#;

    const C_HANG: &str = r#"{
      "routine": "child-hang", "schema_version": 1, "transmit_mode": "automatic",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "cs1", "action": "hold", "params": {} }
      ]}]
    }"#;

    /// Its declared `routine` name is an `@ref` that the resolver maps to a
    /// NON-string — so `resolve_snapshot` succeeds but `from_value::<RoutineDef>`
    /// fails (SnapshotShape). This is the orphan-journal test lever.
    const C_BAD: &str = r#"{
      "routine": "@shape:bad", "schema_version": 1, "transmit_mode": "automatic",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "cs1", "action": "local.log", "params": {} }
      ]}]
    }"#;

    fn call_parent(routine: &str, sync: bool) -> String {
        format!(
            r#"{{
              "routine": "parent", "schema_version": 1, "transmit_mode": "automatic",
              "on_interrupted": "stay", "inputs": [], "triggers": [{{"type": "manual"}}],
              "tracks": [{{ "name": "t", "steps": [
                {{ "id": "c1", "control": "call", "routine": "{routine}", "args": {{}}, "sync": {sync} }}
              ]}}]
            }}"#
        )
    }

    /// Fire-and-forget call to a hanging child, THEN a hanging own step so the
    /// parent stays alive to be cancelled while the detached child runs on.
    const P_FF_HANG: &str = r#"{
      "routine": "parent", "schema_version": 1, "transmit_mode": "automatic",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "c1", "control": "call", "routine": "child-hang", "args": {}, "sync": false },
        { "id": "s2", "action": "hold", "params": {} }
      ]}]
    }"#;

    /// An engine wired for routine composition: `local.log` completes, `boom`
    /// errors, `hold` hangs; the lookup resolves the child fixtures; the
    /// resolver maps `@shape:bad` to a non-string for the orphan test.
    fn compose_engine(dir: &std::path::Path) -> Arc<Engine> {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("local.log").ok(json!({}))));
        reg.register(Arc::new(FakeAction::new("boom").err("child boom: CAT stuck")));
        reg.register(Arc::new(FakeAction::new("hold").hang()));
        let lookup: RoutineLookup = Arc::new(|name: &str| match name {
            "child-ok" => Some(RoutineDef::parse(C_OK).unwrap()),
            "child-fail" => Some(RoutineDef::parse(C_FAIL).unwrap()),
            "child-hang" => Some(RoutineDef::parse(C_HANG).unwrap()),
            "badchild" => Some(RoutineDef::parse(C_BAD).unwrap()),
            _ => None,
        });
        Arc::new(Engine::new(EngineConfig {
            journal_dir: dir.to_path_buf(),
            registry: Arc::new(reg),
            resolver: Arc::new(FakeResolver::new().entity("shape", "bad", json!(123))),
            now: fixed_now,
            default_timeout_s: 3600,
            lookup: Some(lookup),
            consent: None,
        }))
    }

    fn jsonl_files(dir: &std::path::Path) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| {
                let p = e.unwrap().path();
                (p.extension().and_then(|x| x.to_str()) == Some("jsonl")).then_some(p)
            })
            .collect();
        out.sort();
        out
    }

    fn call_child_id(entries: &[JournalEntry]) -> Option<String> {
        entries.iter().find_map(|e| match &e.event {
            RunEvent::CallChild { child_run_id, .. } => Some(child_run_id.clone()),
            _ => None,
        })
    }

    fn seq_of<F: Fn(&RunEvent) -> bool>(entries: &[JournalEntry], pred: F) -> Option<u64> {
        entries.iter().find(|e| pred(&e.event)).map(|e| e.seq)
    }

    // (a) sync success: intent -> call_child -> step_ok, output carries
    // {"completed":true,"run_id":...} where run_id matches the call_child edge.
    #[tokio::test]
    async fn b2_sync_call_journals_call_child_then_completed_step_ok() {
        let dir = tempfile::tempdir().unwrap();
        let eng = compose_engine(dir.path());
        let def = RoutineDef::parse(&call_parent("child-ok", true)).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        assert_eq!(handle.done.await.unwrap().state, RunState::Completed);

        let entries =
            read_journal(&dir.path().join(format!("{}.jsonl", handle.run_id))).unwrap();
        let s_intent = seq_of(&entries, |e| {
            matches!(e, RunEvent::StepIntent { action, .. } if action == "call:child-ok")
        })
        .unwrap();
        let s_call = seq_of(&entries, |e| matches!(e, RunEvent::CallChild { .. })).unwrap();
        let ok_output = entries
            .iter()
            .find_map(|e| match &e.event {
                RunEvent::StepOk { output, .. } if output.get("completed") == Some(&json!(true)) => {
                    Some(output.clone())
                }
                _ => None,
            })
            .unwrap();
        let s_ok = seq_of(&entries, |e| {
            matches!(e, RunEvent::StepOk { output, .. } if output.get("completed") == Some(&json!(true)))
        })
        .unwrap();
        assert!(s_intent < s_call && s_call < s_ok, "intent -> call_child -> step_ok");
        assert_eq!(
            ok_output.get("run_id").and_then(|v| v.as_str()),
            call_child_id(&entries).as_deref(),
            "step output run_id matches the call_child edge"
        );
    }

    // (b) sync failure: call_child BEFORE step_err, id matches the child journal.
    #[tokio::test]
    async fn b2_sync_call_failure_call_child_precedes_step_err_and_id_matches_child_journal() {
        let dir = tempfile::tempdir().unwrap();
        let eng = compose_engine(dir.path());
        let def = RoutineDef::parse(&call_parent("child-fail", true)).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        assert_eq!(handle.done.await.unwrap().state, RunState::Failed);

        let entries =
            read_journal(&dir.path().join(format!("{}.jsonl", handle.run_id))).unwrap();
        let s_call = seq_of(&entries, |e| matches!(e, RunEvent::CallChild { .. })).unwrap();
        let s_err = seq_of(&entries, |e| matches!(e, RunEvent::StepErr { .. })).unwrap();
        assert!(s_call < s_err, "call_child before step_err");

        let child_id = call_child_id(&entries).unwrap();
        let child_entries =
            read_journal(&dir.path().join(format!("{child_id}.jsonl"))).unwrap();
        assert!(matches!(
            &child_entries.first().unwrap().event,
            RunEvent::RunStarted { routine, .. } if routine == "child-fail"
        ));
        assert!(matches!(
            &child_entries.last().unwrap().event,
            RunEvent::RunFinished { state: RunState::Failed, .. }
        ));
    }

    // (c) F&F: call_child + step_ok{dispatched:true} strictly before the parent
    // run_finished (assert seq).
    #[tokio::test]
    async fn b2_fire_and_forget_call_child_and_dispatched_precede_run_finished() {
        let dir = tempfile::tempdir().unwrap();
        let eng = compose_engine(dir.path());
        let def = RoutineDef::parse(&call_parent("child-ok", false)).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        assert_eq!(handle.done.await.unwrap().state, RunState::Completed);

        let entries =
            read_journal(&dir.path().join(format!("{}.jsonl", handle.run_id))).unwrap();
        let s_call = seq_of(&entries, |e| matches!(e, RunEvent::CallChild { .. })).unwrap();
        let s_ok = seq_of(&entries, |e| {
            matches!(e, RunEvent::StepOk { output, .. } if output.get("dispatched") == Some(&json!(true)))
        })
        .unwrap();
        let s_fin = seq_of(&entries, |e| matches!(e, RunEvent::RunFinished { .. })).unwrap();
        assert!(
            s_call < s_ok && s_ok < s_fin,
            "call_child < dispatched step_ok < run_finished (got {s_call},{s_ok},{s_fin})"
        );
    }

    // (d) F&F start failure: step_err, NOT dispatched:true, AND no call_child.
    #[tokio::test]
    async fn b2_fire_and_forget_start_failure_step_errs_without_dispatched_or_call_child() {
        let dir = tempfile::tempdir().unwrap();
        let eng = compose_engine(dir.path());
        // "nope" is not in the lookup: the child never starts.
        let def = RoutineDef::parse(&call_parent("nope", false)).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        assert_eq!(handle.done.await.unwrap().state, RunState::Failed);

        let entries =
            read_journal(&dir.path().join(format!("{}.jsonl", handle.run_id))).unwrap();
        assert!(
            entries.iter().any(|e| matches!(&e.event, RunEvent::StepErr { .. })),
            "a start failure is a verbatim step error"
        );
        assert!(
            !entries.iter().any(|e| matches!(&e.event, RunEvent::CallChild { .. })),
            "a failed start journals NO call_child edge"
        );
        assert!(
            !entries.iter().any(|e| matches!(&e.event,
                RunEvent::StepOk { output, .. } if output.get("dispatched") == Some(&json!(true)))),
            "the silent dispatched:true lie must not be journaled on a failed dispatch"
        );
    }

    // (e) parent cancel mid-sync-child: child journal terminal Cancelled AND
    // parent step_err Cancelled.
    #[tokio::test]
    async fn b2_parent_cancel_mid_sync_child_cancels_child_and_step_errs_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let eng = compose_engine(dir.path());
        let def = RoutineDef::parse(&call_parent("child-hang", true)).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        let parent_jpath = dir.path().join(format!("{}.jsonl", handle.run_id));

        // Wait until the sync child has been started (the call_child edge is
        // journaled), then cancel the parent.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let child_id = loop {
            if let Ok(entries) = read_journal(&parent_jpath) {
                if let Some(id) = call_child_id(&entries) {
                    break id;
                }
            }
            assert!(std::time::Instant::now() < deadline, "child never started");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        handle.cancel.cancel();
        assert_eq!(handle.done.await.unwrap().state, RunState::Cancelled);

        let parent = read_journal(&parent_jpath).unwrap();
        assert!(
            parent.iter().any(|e| matches!(&e.event,
                RunEvent::StepErr { error: StepError::Cancelled, .. })),
            "the parent's call step errs Cancelled"
        );
        // The child, whose token derives from the parent's, terminates Cancelled.
        let child_jpath = dir.path().join(format!("{child_id}.jsonl"));
        let cdeadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let child = read_journal(&child_jpath).unwrap();
            if let Some(last) = child.last() {
                if let RunEvent::RunFinished { state, .. } = &last.event {
                    assert_eq!(*state, RunState::Cancelled, "child ends Cancelled");
                    break;
                }
            }
            assert!(std::time::Instant::now() < cdeadline, "child never terminated");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    // (f) F&F child survives parent cancel: the detached child is NOT cancelled
    // when the parent is.
    #[tokio::test]
    async fn b2_fire_and_forget_child_survives_parent_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let eng = compose_engine(dir.path());
        let def = RoutineDef::parse(P_FF_HANG).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        let parent_jpath = dir.path().join(format!("{}.jsonl", handle.run_id));

        // Wait until the F&F child has been dispatched (call_child journaled),
        // then cancel the parent (which is now hanging on its own `hold` step).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let child_id = loop {
            if let Ok(entries) = read_journal(&parent_jpath) {
                if let Some(id) = call_child_id(&entries) {
                    break id;
                }
            }
            assert!(std::time::Instant::now() < deadline, "child never dispatched");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        handle.cancel.cancel();
        assert_eq!(handle.done.await.unwrap().state, RunState::Cancelled);

        // Grace period: had the child been (wrongly) tied to the parent's
        // cancellation, it would have written RunFinished{Cancelled} by now.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let child = read_journal(&dir.path().join(format!("{child_id}.jsonl"))).unwrap();
        assert!(matches!(
            &child.first().unwrap().event,
            RunEvent::RunStarted { routine, .. } if routine == "child-hang"
        ));
        assert!(
            !child.iter().any(|e| matches!(&e.event, RunEvent::RunFinished { .. })),
            "the detached F&F child is still running — a parent cancel did not reach it"
        );
    }

    // (g) snapshot-shape failure leaves no child journal file (orphan-journal
    // fix): the child's `from_value` fails BEFORE its journal is created.
    #[tokio::test]
    async fn b2_child_snapshot_shape_failure_leaves_no_child_journal() {
        let dir = tempfile::tempdir().unwrap();
        let eng = compose_engine(dir.path());
        let def = RoutineDef::parse(&call_parent("badchild", true)).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        assert_eq!(handle.done.await.unwrap().state, RunState::Failed);

        let files = jsonl_files(dir.path());
        assert_eq!(
            files.len(),
            1,
            "exactly one journal (the parent) — the bad child left no orphan: {files:?}"
        );
        assert_eq!(
            files[0].file_stem().unwrap().to_str().unwrap(),
            handle.run_id,
            "the sole journal is the parent's"
        );
        let entries = read_journal(&files[0]).unwrap();
        assert!(
            entries.iter().any(|e| matches!(&e.event, RunEvent::StepErr { .. })),
            "the parent's call step errs on the child's snapshot-shape failure"
        );
        assert!(
            !entries.iter().any(|e| matches!(&e.event, RunEvent::CallChild { .. })),
            "no call_child edge: the child never returned a run id"
        );
    }

    // (h) an installed child_invoker (OnceLock) is preferred over the internal
    // EngineChildInvoker in the non-dry arm.
    #[tokio::test]
    async fn b2_installed_child_invoker_is_preferred_over_engine_child_invoker() {
        let dir = tempfile::tempdir().unwrap();
        let eng = compose_engine(dir.path());
        let fake = Arc::new(FakeInvoker::new().result("child-ok", json!({"from": "fake"})));
        eng.install_child_invoker(fake.clone());

        let def = RoutineDef::parse(&call_parent("child-ok", true)).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        assert_eq!(handle.done.await.unwrap().state, RunState::Completed);

        // The installed invoker handled the call (not EngineChildInvoker).
        let invs = fake.invocations();
        assert_eq!(invs.len(), 1);
        assert_eq!(invs[0].routine, "child-ok");
        // Its scripted value is the call step's output.
        let entries =
            read_journal(&dir.path().join(format!("{}.jsonl", handle.run_id))).unwrap();
        assert!(entries.iter().any(|e| matches!(&e.event,
            RunEvent::StepOk { output, .. } if output.get("from") == Some(&json!("fake")))));
        // The fake never spawned a real child run: only the parent journal exists.
        assert_eq!(jsonl_files(dir.path()).len(), 1);
    }

    // --- Task B3: End reason threads into run_finished; ordering; precedence
    //     (O4) ---------------------------------------------------------------

    /// A single-track routine that ends mid-track (a step after the End is
    /// swept). Used to prove (a) `run_finished.reason` carries the End reason
    /// (the old code always wrote `None`), and (b) the journal ordering
    /// end_reached < step_skipped < run_finished.
    const B3_ENDER_DEF: &str = r#"{
      "routine": "ender", "schema_version": 1, "transmit_mode": "automatic",
      "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
      "tracks": [{ "name": "t", "steps": [
        { "id": "s1", "action": "local.log", "params": {} },
        { "id": "e1", "control": "end", "failed": true, "reason": "why" },
        { "id": "s2", "action": "local.log", "params": {} }
      ]}]
    }"#;

    // (a, engine half) + (b): the End reason reaches run_finished, and the
    // journal order is end_reached -> step_skipped -> run_finished.
    #[tokio::test]
    async fn b3_end_reason_threads_into_run_finished_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let eng = engine(dir.path());
        let def = RoutineDef::parse(B3_ENDER_DEF).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Failed);
        assert_eq!(outcome.reason.as_deref(), Some("why"));

        let jpath = dir.path().join(format!("{}.jsonl", handle.run_id));
        let entries = read_journal(&jpath).unwrap();

        // run_finished carries the End reason (the dropped-reason defect fix).
        let finished = entries
            .iter()
            .find_map(|e| match &e.event {
                RunEvent::RunFinished { state, reason } => Some((*state, reason.clone())),
                _ => None,
            })
            .expect("a run_finished entry");
        assert_eq!(finished, (RunState::Failed, Some("why".into())));

        let s_end = seq_of(&entries, |e| matches!(e, RunEvent::EndReached { .. })).unwrap();
        let s_skip = seq_of(&entries, |e| {
            matches!(e, RunEvent::StepSkipped { step, .. } if step.0 == "s2")
        })
        .unwrap();
        let s_fin = seq_of(&entries, |e| matches!(e, RunEvent::RunFinished { .. })).unwrap();
        assert!(
            s_end < s_skip && s_skip < s_fin,
            "ordering end_reached({s_end}) < step_skipped({s_skip}) < run_finished({s_fin})"
        );
    }

    /// (d) A propagated StepErr outranks a failed-End reason: the erroring
    /// track finishes first (no await point) and cancels the End track (parked
    /// on a delay), so `run_finished.reason` is the verbatim error string, not
    /// the End's authored "end reason".
    #[tokio::test(start_paused = true)]
    async fn b3_propagated_step_err_wins_over_end_reason() {
        const CLASH_DEF: &str = r#"{
          "routine": "clash", "schema_version": 1, "transmit_mode": "automatic",
          "on_interrupted": "stay", "inputs": [], "triggers": [{"type": "manual"}],
          "tracks": [
            { "name": "err", "steps": [ { "id": "b1", "action": "boom", "params": {} } ] },
            { "name": "end", "steps": [
                { "id": "d1", "control": "delay", "delay": "+1h" },
                { "id": "e1", "control": "end", "failed": true, "reason": "end reason" }
            ] }
          ]
        }"#;
        let dir = tempfile::tempdir().unwrap();
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(FakeAction::new("boom").err("child boom: CAT stuck")));
        let eng = Arc::new(Engine::new(EngineConfig {
            journal_dir: dir.path().to_path_buf(),
            registry: Arc::new(reg),
            resolver: Arc::new(FakeResolver::new()),
            now: fixed_now,
            default_timeout_s: 3600,
            lookup: None,
            consent: None,
        }));
        let def = RoutineDef::parse(CLASH_DEF).unwrap();
        let handle = eng.start_run(&def, json!({})).await.unwrap();
        let outcome = handle.done.await.unwrap();
        assert_eq!(outcome.state, RunState::Failed);

        let jpath = dir.path().join(format!("{}.jsonl", handle.run_id));
        let entries = read_journal(&jpath).unwrap();
        let reason = entries
            .iter()
            .find_map(|e| match &e.event {
                RunEvent::RunFinished { reason, .. } => Some(reason.clone()),
                _ => None,
            })
            .flatten()
            .expect("run_finished carries a reason");
        assert!(
            reason.contains("child boom: CAT stuck"),
            "the StepErr string wins, got {reason:?}"
        );
        assert!(
            !reason.contains("end reason"),
            "the End reason must NOT win over a propagated StepErr, got {reason:?}"
        );
    }
}
