//! Engine facade: creates runs (snapshot -> journal -> execute), recovers
//! interrupted journals at launch, and implements `RoutineInvoker` so calls
//! between routines are just runs with provenance.
//!
//! `Engine` methods that spawn children (`start_run` / `start_run_ext`)
//! take `self: &Arc<Self>`: a spawned run's `ExecCtx::invoker` needs to hand
//! its own children a real, depth-incrementing invoker bound to the engine
//! itself, so the call depth cap (executor's `Control::Call` arm) actually
//! triggers for real recursive routine chains, not just the direct-`ExecCtx`
//! unit tests in `compose.rs`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::action::ActionRegistry;
use crate::compose::{Provenance, RoutineInvoker};
use crate::consent::ConsentPort;
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
}

pub struct RunHandle {
    pub run_id: String,
    pub cancel: CancellationToken,
    pub done: tokio::sync::oneshot::Receiver<RunOutcome>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterruptedRun {
    pub run_id: String,
    pub journal_path: PathBuf,
}

impl Engine {
    pub fn new(cfg: EngineConfig) -> Self {
        Engine { cfg, counter: AtomicU64::new(0) }
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
        self.start_run_ext(def, args, 0, false, false).await
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
        depth: u32,
        parent_attended: bool,
        dry_run: bool,
    ) -> Result<RunHandle, EngineError> {
        let attended =
            !dry_run && (def.transmit_mode == TransmitMode::Attended || parent_attended);
        let run_id = self.next_run_id();
        let snapshot = resolve_snapshot(def, self.cfg.resolver.as_ref()).await?;
        let mut journal = JournalWriter::create(&self.cfg.journal_dir, &run_id, self.cfg.now)?;
        journal.append(RunEvent::RunStarted { routine: def.routine.clone(), snapshot: snapshot.clone() })?;

        // The executor runs the SNAPSHOT (spec §7), not the live definition.
        let resolved: RoutineDef = serde_json::from_value(snapshot)
            .map_err(|e| EngineError::SnapshotShape(e.to_string()))?;

        let mut vars = RunVars::default();
        if let serde_json::Value::Object(map) = args {
            for (k, v) in map {
                vars.set_input(&k, v);
            }
        }

        let cancel = CancellationToken::new();
        let invoker: Arc<dyn RoutineInvoker> = match &self.cfg.lookup {
            Some(lookup) => Arc::new(EngineChildInvoker {
                engine: self.clone(),
                lookup: lookup.clone(),
                child_depth: depth + 1,
                // A child inherits THIS run's effective attended flag as its
                // `parent_attended`, so consent closure propagates (spec §10).
                parent_attended: attended,
                dry_run,
            }),
            None => Arc::new(NoInvoker),
        };
        let ctx = ExecCtx {
            registry: self.cfg.registry.clone(),
            journal: Arc::new(Mutex::new(journal)),
            cancel: cancel.clone(),
            default_timeout_s: self.cfg.default_timeout_s,
            now: self.cfg.now,
            invoker,
            run_id: run_id.clone(),
            depth,
            attended,
            consent: self.cfg.consent.clone(),
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tracks = resolved.tracks.clone();
        let vars = Arc::new(tokio::sync::Mutex::new(vars));
        let journal_arc = ctx.journal.clone();
        tokio::spawn(async move {
            let result = run_tracks(&tracks, vars, &ctx).await;
            let (state, reason) = match &result {
                Ok(o) => (o.state, None),
                Err(StepError::Cancelled) => (RunState::Cancelled, None),
                Err(e) => (RunState::Failed, Some(e.to_string())),
            };
            let outcome = RunOutcome { state };
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
        Ok(RunHandle { run_id, cancel, done: rx })
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
            out.push(InterruptedRun { run_id, journal_path: path });
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
    async fn invoke(
        &self,
        routine: &str,
        _args: serde_json::Value,
        _provenance: Provenance,
    ) -> Result<serde_json::Value, StepError> {
        Err(StepError::Action {
            action: format!("call:{routine}"),
            cause: "no routine invoker is mounted (engine configured without a definition lookup)".into(),
        })
    }
}

/// The real invoker: an `Arc<Engine>` + a definition lookup, bound to a
/// fixed child depth at construction time (spec §7's runtime depth cap
/// backstop). `Engine::start_run_ext` builds one of these per run
/// whenever `cfg.lookup` is `Some`, so every real child run — not just the
/// direct-`ExecCtx` tests in `compose.rs` — increments depth on the way down.
struct EngineChildInvoker {
    engine: Arc<Engine>,
    lookup: RoutineLookup,
    child_depth: u32,
    /// This (parent) run's own effective attended flag — passed to the child
    /// as its `parent_attended` so consent closure propagates down the call
    /// chain (spec §10). See [`Engine::start_run_ext`].
    parent_attended: bool,
    /// Whether the whole run tree is a fake-world dry-run (plan 3): a dry-run
    /// parent's children are dry-runs too, so none of them park for consent.
    dry_run: bool,
}

#[async_trait]
impl RoutineInvoker for EngineChildInvoker {
    async fn invoke(
        &self,
        routine: &str,
        args: serde_json::Value,
        provenance: Provenance,
    ) -> Result<serde_json::Value, StepError> {
        let def = (self.lookup)(routine).ok_or_else(|| StepError::Action {
            action: format!("call:{routine}"),
            cause: format!(
                "routine '{routine}' not found (invoked by {} step {})",
                provenance.parent_run_id, provenance.parent_step.0
            ),
        })?;
        let handle = self
            .engine
            .start_run_ext(&def, args, self.child_depth, self.parent_attended, self.dry_run)
            .await
            .map_err(|e| StepError::Action { action: format!("call:{routine}"), cause: e.to_string() })?;
        let outcome = handle.done.await.map_err(|_| StepError::Action {
            action: format!("call:{routine}"),
            cause: "child run task dropped without an outcome".into(),
        })?;
        match outcome.state {
            RunState::Completed => Ok(serde_json::json!({"completed": true, "run_id": handle.run_id})),
            other => Err(StepError::Action {
                action: format!("call:{routine}"),
                cause: format!("child run {} ended {:?}", handle.run_id, other),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::ActionRegistry;
    use crate::fakes::{FakeAction, FakeResolver};
    use crate::journal::{read_journal, RunEvent, RunState};
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
            RunEvent::RunStarted { routine, snapshot } if routine == "quick" && snapshot.is_object()));
        assert!(matches!(&entries.last().unwrap().event,
            RunEvent::RunFinished { state: RunState::Completed, .. }));
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
            let mut w = crate::journal::JournalWriter::create(dir.path(), "run-dead", fixed_now).unwrap();
            w.append(RunEvent::RunStarted { routine: "quick".into(), snapshot: json!({}) }).unwrap();
        }
        let eng = engine(dir.path());
        let recovered = eng.recover().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].run_id, "run-dead");
        // The journal is now terminal — a second scan finds nothing.
        assert!(eng.recover().unwrap().is_empty());
        let entries = read_journal(&dir.path().join("run-dead.jsonl")).unwrap();
        assert!(matches!(&entries.last().unwrap().event,
            RunEvent::RunFinished { state: RunState::Interrupted, .. }));
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
            let mut w = crate::journal::JournalWriter::create(dir.path(), "run-dead-2", fixed_now).unwrap();
            w.append(RunEvent::RunStarted { routine: "quick".into(), snapshot: json!({}) }).unwrap();
            w.append(RunEvent::StepIntent {
                step: crate::types::StepId("s1".into()),
                action: "local.log".into(),
                resolved_params: json!({}),
            }).unwrap();
            w.append(RunEvent::StepOk { step: crate::types::StepId("s1".into()), output: json!({}) }).unwrap();
        }
        let prior_count = read_journal(&dir.path().join("run-dead-2.jsonl")).unwrap().len();

        let eng = engine(dir.path());
        let recovered = eng.recover().unwrap();
        assert_eq!(recovered.len(), 1);

        let entries = read_journal(&dir.path().join("run-dead-2.jsonl")).unwrap();
        let last = entries.last().unwrap();
        assert!(matches!(last.event, RunEvent::RunFinished { state: RunState::Interrupted, .. }));
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
                if let RunEvent::StepErr { error: crate::error::StepError::Action { cause, .. }, .. } = &e.event {
                    if cause.contains("depth") {
                        found_depth_err = true;
                    }
                }
            }
        }
        assert!(found_depth_err, "expected some journal in the recursion chain to record a depth-cap StepErr");
    }
}
