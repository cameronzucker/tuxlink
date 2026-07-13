//! `RoutinesState` — the managed-state facade that mounts the
//! `tuxlink-routines` [`Engine`] in the Tauri backend (plan 2 Task 5a).
//!
//! Mirrors the Elmer pattern (`src/elmer/session.rs` managed-state facade +
//! `src/elmer/events.rs` typed event enum + `EventSink`): one `Arc`-wrapped
//! state object, `.manage()`d in `lib.rs` `.setup()`, holding the engine, the
//! stores, the radio arbiter, a registry of live runs, and the event sink.
//!
//! ## The engine → UI event bridge
//!
//! The `tuxlink-routines` engine is a Tauri-free leaf crate: it *journals*
//! every step but emits no Tauri events. This module is the bridge. It wires
//! events at the RUN boundary only:
//!
//! * [`RoutinesState::start_routine`] resolves the definition, asks the engine
//!   for a [`RunHandle`], registers the run, emits
//!   [`RoutinesEvent::RunStarted`], then spawns a watcher task that awaits the
//!   handle's `done` channel and emits [`RoutinesEvent::RunFinished`].
//!
//! Step-level events (`StepCompleted`/`StateChanged`) are deliberately NOT
//! emitted in v1 — the frontend gets step granularity by polling the journal
//! via the `routines_run_status` / `routines_journal` commands (plan Task 6).
//! See `events.rs`'s module doc for the "keep it simple and honest" rationale.
//!
//! ## The consent seam (slice 5b)
//!
//! [`RoutinesState::start_routine_def`] is the single start chokepoint. Slice
//! 5b installs consent enforcement THERE (spec §4: refuse a run whose snapshot
//! contains `transmits: true` steps unless the definition's `transmit_mode` is
//! declared and, for automatic mode, a `transmit_ack` is recorded; park
//! attended-mode transmits on a consent channel). This slice leaves the seam
//! clean and does NOT build the wrapper — recovery's resume path also flows
//! through `start_routine_def`, so 5b's enforcement will cover resumed runs for
//! free.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use tuxlink_routines::action::ActionRegistry;
use tuxlink_routines::engine::{Engine, EngineConfig, RunHandle};
use tuxlink_routines::error::EngineError;
use tuxlink_routines::journal::{read_journal, RunEvent, RunState};
use tuxlink_routines::snapshot::EntityResolver;
use tuxlink_routines::types::{OnInterrupted, RoutineDef};

use super::actions::cat::MonolithRigService;
use super::actions::data::MonolithDataService;
use super::actions::local::MonolithLocalService;
use super::actions::radio::{MonolithAprsService, MonolithConnectService, MonolithListenService};
use super::actions::{build_registry, ActionDeps};
use super::arbiter::RadioArbiter;
use super::events::{RoutinesEvent, RoutinesEventSink, TauriRoutinesEventSink};
use super::presets::RadioPresetStore;
use super::resolver::MonolithEntityResolver;
use super::station_sets::StationSetStore;
use super::store::DefinitionStore;

/// The engine's `default_timeout_s` (spec §6): a step with no explicit
/// `timeout_s` gets this ceiling. 5 minutes — long enough for a real HF dial
/// cycle, short enough that a wedged action doesn't hang a run forever.
const DEFAULT_TIMEOUT_S: u64 = 300;

/// Wall-clock unix seconds — the engine + arbiter `now` source.
fn unix_now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Errors starting a run through [`RoutinesState::start_routine`].
#[derive(Debug, thiserror::Error)]
pub enum RoutineStartError {
    #[error("routine '{0}' not found")]
    UnknownRoutine(String),
    #[error(transparent)]
    Engine(#[from] EngineError),
}

/// A live run's registry entry: its cancel token (for `routines_cancel`) plus a
/// state snapshot (for `routines_run_status`). Kept after the run finishes so a
/// late `run_status` query still reports the terminal state (the journal is the
/// durable record; this is the fast in-memory answer).
struct RunEntry {
    routine: String,
    dry_run: bool,
    cancel: CancellationToken,
    state: RunState,
}

/// The fast in-memory answer to `routines_run_status` (plan Task 6). The full,
/// step-by-step record is the journal (`routines_journal`).
#[derive(Debug, Clone, PartialEq)]
pub struct RunStatusSnapshot {
    pub run_id: String,
    pub routine: String,
    pub dry_run: bool,
    pub state: RunState,
}

/// Outcome of [`RoutinesState::recover`] — how many interrupted runs were found
/// and how many were resumed (`on_interrupted: resume`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    pub interrupted: usize,
    pub resumed: usize,
}

/// Managed-state facade over the routines [`Engine`] (plan 2 Task 5a).
pub struct RoutinesState {
    pub engine: Arc<Engine>,
    pub store: Arc<DefinitionStore>,
    pub presets: Arc<RadioPresetStore>,
    pub station_sets: Arc<StationSetStore>,
    pub arbiter: Arc<RadioArbiter>,
    /// Live + recently-finished runs, keyed by run_id.
    runs: Mutex<HashMap<String, RunEntry>>,
    sink: Arc<dyn RoutinesEventSink>,
}

impl RoutinesState {
    /// Assemble the facade from already-resolved parts. This is the injectable
    /// seam: production supplies the real engine (built by
    /// [`build_routines_state`]); tests supply an engine over a fake registry
    /// plus a recording sink.
    pub fn new(
        engine: Arc<Engine>,
        store: Arc<DefinitionStore>,
        presets: Arc<RadioPresetStore>,
        station_sets: Arc<StationSetStore>,
        arbiter: Arc<RadioArbiter>,
        sink: Arc<dyn RoutinesEventSink>,
    ) -> Self {
        Self {
            engine,
            store,
            presets,
            station_sets,
            arbiter,
            runs: Mutex::new(HashMap::new()),
            sink,
        }
    }

    /// Start a routine by name. Looks the definition up in the store, then
    /// delegates to [`start_routine_def`](Self::start_routine_def). Returns the
    /// minted run_id.
    pub async fn start_routine(
        self: &Arc<Self>,
        name: &str,
        args: serde_json::Value,
    ) -> Result<String, RoutineStartError> {
        let def = self
            .store
            .get(name)
            .ok_or_else(|| RoutineStartError::UnknownRoutine(name.to_string()))?;
        self.start_routine_def(&def, args, false).await
    }

    /// The single start chokepoint (the consent seam — see the module doc).
    /// Asks the engine for a [`RunHandle`], registers the run, emits
    /// [`RoutinesEvent::RunStarted`], and spawns the watcher that emits
    /// [`RoutinesEvent::RunFinished`] on the run's terminus.
    async fn start_routine_def(
        self: &Arc<Self>,
        def: &RoutineDef,
        args: serde_json::Value,
        dry_run: bool,
    ) -> Result<String, RoutineStartError> {
        // ── CONSENT SEAM (slice 5b) ──────────────────────────────────────
        // 5b inserts the transmit-consent check HERE, before start_run: refuse
        // a run whose snapshot carries `transmits: true` steps unless
        // transmit_mode is declared + (automatic) transmit_ack recorded, and
        // park attended transmits on a consent channel emitting
        // RoutinesEvent::AwaitingConsent. Intentionally absent in this slice.

        let RunHandle {
            run_id,
            cancel,
            done,
        } = self.engine.start_run(def, args).await?;

        {
            let mut runs = lock(&self.runs);
            runs.insert(
                run_id.clone(),
                RunEntry {
                    routine: def.routine.clone(),
                    dry_run,
                    cancel,
                    state: RunState::Running,
                },
            );
        }

        self.sink.emit(&RoutinesEvent::RunStarted {
            run_id: run_id.clone(),
            routine: def.routine.clone(),
            dry_run,
        });

        // Watcher: await the run's terminus, update the registry, emit
        // RunFinished. The engine's RunOutcome carries only the state — the
        // verbatim terminal reason lives in the journal (routines_journal),
        // so the watcher path leaves `reason: None`.
        let this = Arc::clone(self);
        let watch_id = run_id.clone();
        tokio::spawn(async move {
            let state = match done.await {
                Ok(outcome) => outcome.state,
                // The run task was dropped without sending an outcome (should
                // not happen — the engine sends on every path — but be honest
                // rather than hang the registry entry at Running forever).
                Err(_) => RunState::Failed,
            };
            {
                let mut runs = lock(&this.runs);
                if let Some(entry) = runs.get_mut(&watch_id) {
                    entry.state = state;
                }
            }
            this.sink.emit(&RoutinesEvent::RunFinished {
                run_id: watch_id,
                state,
                reason: None,
            });
        });

        Ok(run_id)
    }

    /// Cancel a live run by id. Returns `false` if no run with that id is
    /// registered. The watcher emits `RunFinished{Cancelled}` when the engine's
    /// executor observes the token.
    pub fn cancel_run(&self, run_id: &str) -> bool {
        let runs = lock(&self.runs);
        match runs.get(run_id) {
            Some(entry) => {
                entry.cancel.cancel();
                true
            }
            None => false,
        }
    }

    /// Fast in-memory run status (plan Task 6's `routines_run_status`).
    pub fn run_status(&self, run_id: &str) -> Option<RunStatusSnapshot> {
        let runs = lock(&self.runs);
        runs.get(run_id).map(|e| RunStatusSnapshot {
            run_id: run_id.to_string(),
            routine: e.routine.clone(),
            dry_run: e.dry_run,
            state: e.state,
        })
    }

    /// Launch-time recovery (spec §8). Marks every interrupted journal
    /// terminally `Interrupted` (via [`Engine::recover`]), emits a
    /// `RunFinished{Interrupted}` event for each, then applies the
    /// `on_interrupted` policy: `resume` re-invokes the routine from its journal
    /// snapshot (a fresh run — the engine has no partial step-resume; "resume"
    /// means re-run the resolved definition), `stay` does nothing further.
    pub async fn recover(self: &Arc<Self>) -> Result<RecoveryReport, EngineError> {
        let interrupted = self.engine.recover()?;
        let mut resumed = 0usize;

        for run in &interrupted {
            self.sink.emit(&RoutinesEvent::RunFinished {
                run_id: run.run_id.clone(),
                state: RunState::Interrupted,
                reason: Some("process terminated underneath this run".to_string()),
            });

            // Resume ONLY when the journalled snapshot's on_interrupted says so.
            // Reading the policy from the SNAPSHOT (not the live store def) is
            // deliberate: a library edit after the crash must not change how an
            // in-flight-at-crash run continues (spec §7 snapshot isolation).
            match snapshot_def_from_journal(&run.journal_path) {
                Some(def) if def.on_interrupted == OnInterrupted::Resume => {
                    match self
                        .start_routine_def(&def, serde_json::json!({}), false)
                        .await
                    {
                        Ok(_) => resumed += 1,
                        Err(e) => {
                            tracing::warn!(
                                target: "tuxlink::routines",
                                run_id = %run.run_id,
                                error = %e,
                                "failed to resume interrupted routine from snapshot"
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(RecoveryReport {
            interrupted: interrupted.len(),
            resumed,
        })
    }
}

/// Lock the runs registry, tolerating poison (a panicked run-watcher must not
/// wedge every subsequent status query). The guarded critical sections here are
/// brief, non-`await`, and panic-free, so recovering the inner value is safe.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Read a dead run's journal and deserialize its `RunStarted.snapshot` back
/// into a [`RoutineDef`] (the fully-resolved definition the run was executing).
/// `None` if the journal is unreadable, empty, or its snapshot no longer
/// deserializes — all non-fatal: recovery simply does not resume that run.
fn snapshot_def_from_journal(path: &Path) -> Option<RoutineDef> {
    let entries = read_journal(path).ok()?;
    for entry in entries {
        if let RunEvent::RunStarted { snapshot, .. } = entry.event {
            return serde_json::from_value::<RoutineDef>(snapshot).ok();
        }
    }
    None
}

// ============================================================================
// Production construction (lib.rs .setup())
// ============================================================================

/// Build the real action registry: the Monolith* service adapters (each
/// wrapping the `AppHandle`) plus the shared `arbiter`. Split out of
/// [`build_routines_state`] so session tests can swap in a fake registry
/// without an `AppHandle`.
pub fn build_default_registry(app: &AppHandle, arbiter: Arc<RadioArbiter>) -> ActionRegistry {
    let deps = ActionDeps {
        arbiter,
        connect: Arc::new(MonolithConnectService::new(app.clone())),
        aprs: Arc::new(MonolithAprsService::new(app.clone())),
        listen: Arc::new(MonolithListenService::new()),
        rig: Arc::new(MonolithRigService::new()),
        data: Arc::new(MonolithDataService::new(app.clone())),
        local: Arc::new(MonolithLocalService::new(app.clone())),
    };
    build_registry(deps)
}

/// Core constructor (app-free, so unit-testable): resolves every routines file
/// under `config_dir`, wires the engine over the injected `registry`/`arbiter`,
/// and returns the managed-state facade.
///
/// `config_dir` is the directory holding `config.json` (i.e.
/// `config_path().parent()`), matching where `identity_store_path()` and the
/// preset/station-set stores live. Passed explicitly (not read from the process
/// env) so tests point at a tempdir without racing `TUXLINK_CONFIG_DIR`.
pub fn build_routines_state(
    config_dir: PathBuf,
    registry: ActionRegistry,
    arbiter: Arc<RadioArbiter>,
    sink: Arc<dyn RoutinesEventSink>,
) -> RoutinesState {
    let store = Arc::new(DefinitionStore::open(config_dir.join("routines")));
    let presets = Arc::new(RadioPresetStore::open(
        config_dir.join("radio-presets.json"),
    ));
    let station_sets = Arc::new(StationSetStore::open(config_dir.join("station-sets.json")));

    let resolver: Arc<dyn EntityResolver> = Arc::new(MonolithEntityResolver::new(
        presets.clone(),
        station_sets.clone(),
        config_dir.join("identities.json"),
    ));

    let engine = Arc::new(Engine::new(EngineConfig {
        journal_dir: config_dir.join("routines-runs"),
        registry: Arc::new(registry),
        resolver,
        now: unix_now_secs,
        default_timeout_s: DEFAULT_TIMEOUT_S,
        lookup: Some(store.lookup_fn()),
    }));

    RoutinesState::new(engine, store, presets, station_sets, arbiter, sink)
}

/// The `lib.rs` `.setup()` entry point: resolve the config dir, build the real
/// arbiter + registry + Tauri event sink, and assemble the facade.
pub fn build_routines_state_for_app(app: &AppHandle) -> RoutinesState {
    let config_dir = crate::config::config_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let arbiter = Arc::new(RadioArbiter::new(unix_now_secs));
    let registry = build_default_registry(app, arbiter.clone());
    let sink: Arc<dyn RoutinesEventSink> = Arc::new(TauriRoutinesEventSink::new(app.clone()));
    build_routines_state(config_dir, registry, arbiter, sink)
}

// ============================================================================
// Tests — app-free (no AppHandle, no Tauri runtime): a fake registry of
// FakeActions, a recording sink, and tempdir stores.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;
    use tuxlink_routines::fakes::FakeAction;
    use tuxlink_routines::journal::JournalWriter;
    use tuxlink_routines::types::{
        ActionStep, BusyPolicy, Step, StepId, Track, TransmitMode, Trigger,
        SUPPORTED_SCHEMA_VERSION,
    };

    fn fixed_now() -> i64 {
        1_752_400_000
    }

    /// A recording [`RoutinesEventSink`] — captures every emitted event so a
    /// test can assert the RunStarted → RunFinished stream.
    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<RoutinesEvent>>,
    }

    impl RecordingSink {
        fn events(&self) -> Vec<RoutinesEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    impl RoutinesEventSink for RecordingSink {
        fn emit(&self, event: &RoutinesEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    fn minimal_def(name: &str, on_interrupted: OnInterrupted) -> RoutineDef {
        RoutineDef {
            routine: name.to_string(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t".into(),
                steps: vec![Step::Action(ActionStep {
                    id: StepId("s1".into()),
                    action: "local.log".into(),
                    params: json!({}),
                    timeout_s: None,
                    on_radio_busy: BusyPolicy::Wait,
                })],
            }],
        }
    }

    /// Build a test state over `config_dir`: a fake registry with a
    /// scripted `local.log`, a fresh arbiter, and a recording sink.
    fn test_state(
        config_dir: PathBuf,
        log_action: FakeAction,
    ) -> (Arc<RoutinesState>, Arc<RecordingSink>) {
        let mut reg = ActionRegistry::default();
        reg.register(Arc::new(log_action));
        let arbiter = Arc::new(RadioArbiter::new(fixed_now));
        let sink = Arc::new(RecordingSink::default());
        let sink_dyn: Arc<dyn RoutinesEventSink> = sink.clone();
        let state = Arc::new(build_routines_state(config_dir, reg, arbiter, sink_dyn));
        (state, sink)
    }

    /// Poll `p()` every 10 ms until true, panicking after 5 s.
    async fn wait_until<F: Fn() -> bool>(p: F) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if p() {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "wait_until timed out");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn count_finished(events: &[RoutinesEvent], want: RunState) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, RoutinesEvent::RunFinished { state, .. } if *state == want))
            .count()
    }

    #[tokio::test]
    async fn build_routines_state_constructs_against_a_tempdir_config() {
        let dir = tempfile::tempdir().unwrap();
        let (state, _sink) = test_state(
            dir.path().to_path_buf(),
            FakeAction::new("local.log").ok(json!({})),
        );

        // The stores are wired against the tempdir: a saved def round-trips
        // through the managed store, and the routines dir was created on open.
        let def = minimal_def("quick", OnInterrupted::Stay);
        state.store.save(&def).unwrap();
        assert!(state.store.get("quick").is_some());
        assert!(dir.path().join("routines").is_dir());
    }

    #[tokio::test]
    async fn start_routine_unknown_name_errors() {
        let dir = tempfile::tempdir().unwrap();
        let (state, _sink) = test_state(
            dir.path().to_path_buf(),
            FakeAction::new("local.log").ok(json!({})),
        );
        let err = state.start_routine("nope", json!({})).await.unwrap_err();
        assert!(matches!(err, RoutineStartError::UnknownRoutine(n) if n == "nope"));
    }

    #[tokio::test]
    async fn start_routine_emits_run_started_then_run_finished() {
        let dir = tempfile::tempdir().unwrap();
        let (state, sink) = test_state(
            dir.path().to_path_buf(),
            FakeAction::new("local.log").ok(json!({"logged": true})),
        );

        state
            .store
            .save(&minimal_def("quick", OnInterrupted::Stay))
            .unwrap();
        let run_id = state.start_routine("quick", json!({})).await.unwrap();

        // RunStarted is emitted synchronously, before the watcher spawns.
        let started = sink.events();
        assert!(
            started.iter().any(|e| matches!(e,
                RoutinesEvent::RunStarted { run_id: r, routine, dry_run }
                    if *r == run_id && routine == "quick" && !dry_run)),
            "RunStarted must be emitted before start_routine returns: {started:?}"
        );

        // RunFinished{Completed} arrives once the watcher observes the terminus.
        wait_until(|| count_finished(&sink.events(), RunState::Completed) == 1).await;
        let evs = sink.events();
        assert!(
            evs.iter().any(|e| matches!(e,
                RoutinesEvent::RunFinished { run_id: r, state: RunState::Completed, .. }
                    if *r == run_id)),
            "RunFinished{{Completed}} must name the same run_id: {evs:?}"
        );

        // run_status reflects the terminal state after the watcher updates it.
        wait_until(|| state.run_status(&run_id).map(|s| s.state) == Some(RunState::Completed))
            .await;
    }

    #[tokio::test]
    async fn cancel_run_drives_a_cancelled_terminus() {
        let dir = tempfile::tempdir().unwrap();
        // A hanging action: the run stays live until cancelled.
        let (state, sink) = test_state(
            dir.path().to_path_buf(),
            FakeAction::new("local.log").hang(),
        );
        state
            .store
            .save(&minimal_def("hangs", OnInterrupted::Stay))
            .unwrap();

        let run_id = state.start_routine("hangs", json!({})).await.unwrap();
        // The action is hanging; no terminus yet.
        assert_eq!(count_finished(&sink.events(), RunState::Cancelled), 0);

        assert!(
            state.cancel_run(&run_id),
            "cancel_run must find the live run"
        );
        wait_until(|| count_finished(&sink.events(), RunState::Cancelled) == 1).await;

        assert!(
            !state.cancel_run("does-not-exist"),
            "cancel of an unknown run is false"
        );
    }

    #[tokio::test]
    async fn recovery_marks_and_emits_interrupted_for_stay_policy() {
        let dir = tempfile::tempdir().unwrap();
        let (state, sink) = test_state(
            dir.path().to_path_buf(),
            FakeAction::new("local.log").ok(json!({})),
        );

        // Fabricate a dead journal (RunStarted, no RunFinished) with a
        // stay-policy snapshot, in the engine's journal dir.
        let journal_dir = dir.path().join("routines-runs");
        let def = minimal_def("stayer", OnInterrupted::Stay);
        {
            let mut w = JournalWriter::create(&journal_dir, "run-dead-stay", fixed_now).unwrap();
            w.append(RunEvent::RunStarted {
                routine: "stayer".into(),
                snapshot: serde_json::to_value(&def).unwrap(),
            })
            .unwrap();
        }

        let report = state.recover().await.unwrap();
        assert_eq!(
            report,
            RecoveryReport {
                interrupted: 1,
                resumed: 0
            }
        );

        let evs = sink.events();
        assert!(
            evs.iter().any(|e| matches!(e,
                RoutinesEvent::RunFinished { run_id, state: RunState::Interrupted, reason: Some(_) }
                    if run_id == "run-dead-stay")),
            "recovery must emit RunFinished{{Interrupted}} for the dead run: {evs:?}"
        );
        // Stay policy: no resumed run was started.
        assert!(!evs
            .iter()
            .any(|e| matches!(e, RoutinesEvent::RunStarted { .. })));
    }

    #[tokio::test]
    async fn recovery_resumes_on_interrupted_resume_from_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let (state, sink) = test_state(
            dir.path().to_path_buf(),
            FakeAction::new("local.log").ok(json!({})),
        );

        let journal_dir = dir.path().join("routines-runs");
        let def = minimal_def("resumer", OnInterrupted::Resume);
        {
            let mut w = JournalWriter::create(&journal_dir, "run-dead-resume", fixed_now).unwrap();
            w.append(RunEvent::RunStarted {
                routine: "resumer".into(),
                snapshot: serde_json::to_value(&def).unwrap(),
            })
            .unwrap();
        }

        let report = state.recover().await.unwrap();
        assert_eq!(report.interrupted, 1);
        assert_eq!(
            report.resumed, 1,
            "resume policy must re-invoke from the snapshot"
        );

        // The interrupted run is marked; a fresh run was started AND runs to
        // completion (the fake local.log succeeds).
        let evs = sink.events();
        assert!(
            evs.iter().any(|e| matches!(e,
                RoutinesEvent::RunFinished { run_id, state: RunState::Interrupted, .. }
                    if run_id == "run-dead-resume")),
            "interrupted run must be marked: {evs:?}"
        );
        assert!(
            evs.iter().any(
                |e| matches!(e, RoutinesEvent::RunStarted { routine, .. } if routine == "resumer")
            ),
            "a resumed run must be started: {evs:?}"
        );
        // The resumed run runs to completion on the fake local.log: the ONLY
        // RunFinished{Completed} on the sink is the resumed run's (the
        // interrupted run finished Interrupted, not Completed).
        wait_until(|| count_finished(&sink.events(), RunState::Completed) == 1).await;
    }
}
