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
//! ## The consent enforcement (slice 5b, spec §4)
//!
//! [`RoutinesState::start_routine_def`] is the single start chokepoint, and
//! slice 5b installs consent enforcement THERE:
//!
//! * **Start gate.** [`closure_transmits`] walks the definition + the store's
//!   routine lookup; a transmitting `automatic` routine with no recorded
//!   [`ack_is_recorded`] acknowledgment is refused with the typed,
//!   operator-facing [`RoutineStartError::UnacknowledgedAutomatic`]. Attended
//!   and non-transmitting routines start.
//! * **Attended pause.** [`build_routines_state`] installs the
//!   [`ConsentRegistry`] as the engine's [`tuxlink_routines::consent::ConsentPort`]
//!   (via `EngineConfig.consent`). When an attended run reaches a `transmits`
//!   step, the executor parks on the registry — BEFORE the step timeout, so the
//!   parked wait is a true waiting state, not charged against the timeout — and
//!   the step resumes on [`RoutinesState::grant_consent`]. The engine computes
//!   the per-run effective attended flag (`start_run_ext`), so automatic and dry
//!   runs never park.
//!
//! Recovery's resume path also flows through `start_routine_def`, so both the
//! gate and the attended pause cover resumed runs for free.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use tuxlink_routines::action::ActionRegistry;
use tuxlink_routines::consent::ConsentPort;
use tuxlink_routines::dryrun::DryRunScript;
use tuxlink_routines::engine::{Engine, EngineConfig, RunHandle};
use tuxlink_routines::error::EngineError;
use tuxlink_routines::journal::{read_journal, JournalEntry, RunEvent, RunState};
use tuxlink_routines::snapshot::EntityResolver;
use tuxlink_routines::types::{OnInterrupted, RoutineDef, TransmitMode};

use super::actions::cat::MonolithRigService;
use super::actions::data::MonolithDataService;
use super::actions::local::MonolithLocalService;
use super::actions::radio::{MonolithAprsService, MonolithConnectService, MonolithListenService};
use super::actions::{build_registry, ActionDeps};
use super::arbiter::RadioArbiter;
use super::consent::{closure_transmits, transmit_action_names, ConsentRegistry};
use super::events::{RoutinesEvent, RoutinesEventSink, TauriRoutinesEventSink};
use super::presets::RadioPresetStore;
use super::resolver::MonolithEntityResolver;
use super::station_sets::StationSetStore;
use super::store::DefinitionStore;

/// The engine's `default_timeout_s` (spec §6): a step with no explicit
/// `timeout_s` gets this ceiling. 5 minutes — long enough for a real HF dial
/// cycle, short enough that a wedged action doesn't hang a run forever.
const DEFAULT_TIMEOUT_S: u64 = 300;

/// How many TERMINAL run entries the in-memory `runs` registry keeps (Task 5a's
/// carried Low finding: the map grew without bound — one entry per run for the
/// life of the process). Live (non-terminal) runs are NEVER evicted, however
/// many there are: cancelling a run requires its token. Terminal entries are a
/// read-through cache only — the journal on disk is the durable record, and
/// [`RoutinesState::run_status`] falls back to it for an evicted run, so
/// eviction costs a file read, never an answer.
const MAX_TERMINAL_RUNS: usize = 100;

/// A run id must be safe to interpolate into a journal filename. Engine-minted
/// ids are `run-<unix>-<nnnn>`; this is the chokepoint that stops a
/// caller-supplied `"../config"` from escaping the journal directory when a
/// command reads `<journal_dir>/<run_id>.jsonl` (same discipline as
/// `store::valid_name`).
pub(crate) fn valid_run_id(run_id: &str) -> bool {
    !run_id.is_empty()
        && run_id.len() <= 64
        && run_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Has the run reached a terminal state (spec §8)? Terminal entries are
/// evictable from the in-memory registry; non-terminal ones are not.
fn is_terminal(state: RunState) -> bool {
    matches!(
        state,
        RunState::Completed | RunState::Failed | RunState::Cancelled | RunState::Interrupted
    )
}

/// Wall-clock unix seconds — the engine + arbiter `now` source, and the command
/// layer's `now` for the enable-time fleet check (`validate_fleet` takes an
/// explicit `now_unix` rather than reading a hidden clock).
pub(crate) fn unix_now_secs() -> i64 {
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
    /// The consent gate (spec §4): a routine whose call-graph closure transmits
    /// and declares `transmit_mode: automatic` cannot start without a recorded
    /// acknowledgment (callsign + timestamp, both non-empty). The message is
    /// operator-facing — it names the routine and the fix.
    #[error(
        "routine '{routine}' transmits under automatic control but has no recorded \
         acknowledgment — open its Settings and acknowledge automatic-transmission \
         responsibility (Part 97 automatic-control rules) before running it"
    )]
    UnacknowledgedAutomatic { routine: String },
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
    /// Monotonic insertion order, used only by [`prune_terminal_runs`] to evict
    /// the OLDEST terminal entries first. A `HashMap` has no order of its own
    /// and `run_id`'s embedded timestamp has 1-second granularity (two runs
    /// started in the same second would be untotally-ordered), so the sequence
    /// is carried explicitly rather than derived from the id.
    seq: u64,
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
    /// The real action catalog, shared with the engine. Held here (not only
    /// inside the engine's private config) so the Task-6 command layer can build
    /// a [`super::validation::MonolithValidationContext`] whose
    /// `action_descriptor()` answers come from the SAME registry the executor
    /// resolves against — the validator can never disagree with runtime about
    /// what an action is or what it declares.
    pub registry: Arc<ActionRegistry>,
    /// Where the engine writes run journals. Held here (the engine keeps its
    /// `EngineConfig` private) so `routines_journal` can read a run's durable
    /// record back, and so [`Self::run_status`] can answer for a run whose
    /// in-memory entry was evicted.
    pub journal_dir: PathBuf,
    /// Identity store path (`identities.json`), needed by the command layer's
    /// validation context to answer `@identity:` reference existence.
    pub identity_store_path: PathBuf,
    /// Live + recently-finished runs, keyed by run_id. Bounded: at most
    /// [`MAX_TERMINAL_RUNS`] terminal entries are retained (plus every live
    /// run). See [`prune_terminal_runs`].
    runs: Mutex<HashMap<String, RunEntry>>,
    /// Monotonic insertion counter for `runs` (see [`RunEntry::seq`]).
    run_seq: AtomicU64,
    sink: Arc<dyn RoutinesEventSink>,
    /// The attended-mode parking desk (spec §4). Transmit steps in attended
    /// runs park here; [`RoutinesState::grant_consent`] releases them.
    consent: Arc<ConsentRegistry>,
    /// Catalog action names that transmit (from the engine registry's
    /// descriptors), used by the start gate's [`closure_transmits`] predicate.
    transmit_names: HashSet<String>,
}

impl RoutinesState {
    /// Assemble the facade from already-resolved parts. This is the injectable
    /// seam: production supplies the real engine (built by
    /// [`build_routines_state`]); tests supply an engine over a fake registry
    /// plus a recording sink.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        engine: Arc<Engine>,
        store: Arc<DefinitionStore>,
        presets: Arc<RadioPresetStore>,
        station_sets: Arc<StationSetStore>,
        arbiter: Arc<RadioArbiter>,
        registry: Arc<ActionRegistry>,
        journal_dir: PathBuf,
        identity_store_path: PathBuf,
        sink: Arc<dyn RoutinesEventSink>,
        consent: Arc<ConsentRegistry>,
        transmit_names: HashSet<String>,
    ) -> Self {
        Self {
            engine,
            store,
            presets,
            station_sets,
            arbiter,
            registry,
            journal_dir,
            identity_store_path,
            runs: Mutex::new(HashMap::new()),
            run_seq: AtomicU64::new(0),
            sink,
            consent,
            transmit_names,
        }
    }

    /// Emit a routines event on the state's sink. The command layer (Task 6)
    /// uses this to announce library mutations
    /// ([`RoutinesEvent::LibraryChanged`]) so every open window re-reads the
    /// list it is showing.
    pub fn emit(&self, event: &RoutinesEvent) {
        self.sink.emit(event);
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
        self.start_routine_def(&def, args, None).await
    }

    /// Start a DRY run of a routine by name (spec §10 layer 3). Routes through
    /// the engine's [`Engine::start_dry_run`] — the canonical fake-world entry
    /// point, which swaps EVERY real action for a capability-mirroring
    /// `FakeAction` before the executor ever resolves one. Nothing real is
    /// touched: no rig, no transmitter, no CMS, no disk beyond the journal.
    ///
    /// `script` (default: optimistic) is what an unscripted action returns; see
    /// [`DryRunScript`].
    pub async fn start_dry_run(
        self: &Arc<Self>,
        name: &str,
        args: serde_json::Value,
        script: DryRunScript,
    ) -> Result<String, RoutineStartError> {
        let def = self
            .store
            .get(name)
            .ok_or_else(|| RoutineStartError::UnknownRoutine(name.to_string()))?;
        self.start_routine_def(&def, args, Some(script)).await
    }

    /// The single start chokepoint (the consent seam — see the module doc).
    /// Asks the engine for a [`RunHandle`], registers the run, emits
    /// [`RoutinesEvent::RunStarted`], and spawns the watcher that emits
    /// [`RoutinesEvent::RunFinished`] on the run's terminus.
    async fn start_routine_def(
        self: &Arc<Self>,
        def: &RoutineDef,
        args: serde_json::Value,
        dry_run: Option<DryRunScript>,
    ) -> Result<String, RoutineStartError> {
        let dry_run_flag = dry_run.is_some();
        // ── CONSENT GATE (slice 5b, spec §4) ─────────────────────────────
        // Compute the transmit closure over the definition + the store's
        // routine lookup (the snapshot resolver inlines @refs but never inlines
        // Call steps — those resolve by name at runtime — so the closure walk
        // is over the definition, mirroring the validator's future walk). A
        // transmitting automatic routine needs a recorded acknowledgment; an
        // attended one is allowed to start and pauses at each transmit step
        // (the executor parks on the ConsentRegistry installed as the engine's
        // ConsentPort in `build_routines_state`). A non-transmitting routine
        // starts unconditionally, whatever its mode.
        //
        // Dry-runs (plan 3) bypass the gate entirely: a dry-run mocks the radio
        // boundary and cannot key a transmitter, so authorization to transmit
        // is not the question a dry-run asks — you dry-run precisely to rehearse
        // an as-yet-unacknowledged routine. `Engine::start_dry_run` also forces
        // the engine's effective attended flag to false, so a dry-run never
        // parks either.
        if !dry_run_flag {
            let lookup = |name: &str| self.store.get(name);
            let transmits = |name: &str| self.transmit_names.contains(name);
            if closure_transmits(def, &lookup, &transmits)
                && def.transmit_mode == TransmitMode::Automatic
                && !ack_is_recorded(def)
            {
                return Err(RoutineStartError::UnacknowledgedAutomatic {
                    routine: def.routine.clone(),
                });
            }
        }

        // A dry run goes through the engine's OWN dry-run entry point, which is
        // what swaps the registry for capability-mirroring fakes. Passing
        // `dry_run: true` to `start_run_ext` would only have suppressed consent
        // parking while STILL executing the real action catalog — a fake-world
        // run that keys a real transmitter. The two paths are kept adjacent here
        // so that mistake cannot be made silently again.
        let RunHandle {
            run_id,
            cancel,
            done,
        } = match dry_run {
            Some(script) => self.engine.start_dry_run(def, args, script).await?,
            None => {
                self.engine
                    .start_run_ext(def, args, 0, false, false)
                    .await?
            }
        };

        {
            let mut runs = lock(&self.runs);
            let seq = self.run_seq.fetch_add(1, Ordering::Relaxed);
            runs.insert(
                run_id.clone(),
                RunEntry {
                    routine: def.routine.clone(),
                    dry_run: dry_run_flag,
                    cancel,
                    state: RunState::Running,
                    seq,
                },
            );
            prune_terminal_runs(&mut runs);
        }

        self.sink.emit(&RoutinesEvent::RunStarted {
            run_id: run_id.clone(),
            routine: def.routine.clone(),
            dry_run: dry_run_flag,
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
                // This run just became terminal — it may have pushed the
                // terminal-entry count over the cap.
                prune_terminal_runs(&mut runs);
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

    /// Grant per-transmission consent for a parked attended-mode transmit step
    /// (spec §4). Returns `false` if no step with that `(run_id, step_id)` is
    /// currently parked (already granted, cancelled, or never parked). Task 6
    /// wires the `routines_consent_grant` command to this.
    ///
    /// **Operator-only (spec §13).** This is reachable solely from the UI
    /// command; the MCP surface has NO parameter that can supply consent — the
    /// design-time acknowledgment is the only consent envelope agents touch, and
    /// it is recorded by a UI act. Do not expose a grant path to the MCP tools.
    pub fn grant_consent(&self, run_id: &str, step_id: &str) -> bool {
        self.consent.grant(run_id, step_id)
    }

    /// Run status (Task 6's `routines_run_status`). Answers from the in-memory
    /// registry when the run is there, and falls back to the run's JOURNAL when
    /// it is not — a terminal entry evicted by [`prune_terminal_runs`], or a run
    /// from a previous process lifetime, still reports honestly rather than
    /// vanishing. `None` means no such run has ever existed on this station.
    pub fn run_status(&self, run_id: &str) -> Option<RunStatusSnapshot> {
        {
            let runs = lock(&self.runs);
            if let Some(e) = runs.get(run_id) {
                return Some(RunStatusSnapshot {
                    run_id: run_id.to_string(),
                    routine: e.routine.clone(),
                    dry_run: e.dry_run,
                    state: e.state,
                });
            }
        }
        self.run_status_from_journal(run_id)
    }

    /// Reconstruct a run's status from its journal: the routine name +
    /// dry-run stamp come from `RunStarted`, the state from the terminal
    /// `RunFinished` (a journal with no `RunFinished` is a run this process
    /// never finished — recovery will mark it `Interrupted`; until then it
    /// reads as `Running`, which is what it was when the record was written).
    fn run_status_from_journal(&self, run_id: &str) -> Option<RunStatusSnapshot> {
        let entries = self.journal_entries(run_id)?;
        let mut routine = None;
        let mut dry_run = false;
        let mut state = RunState::Running;
        for entry in entries {
            match entry.event {
                RunEvent::RunStarted {
                    routine: name,
                    dry_run: dry,
                    ..
                } => {
                    routine = Some(name);
                    dry_run = dry;
                }
                RunEvent::RunFinished { state: s, .. } => state = s,
                _ => {}
            }
        }
        Some(RunStatusSnapshot {
            run_id: run_id.to_string(),
            routine: routine?,
            dry_run,
            state,
        })
    }

    /// A run's full journal, verbatim (Task 6's `routines_journal`). `None` if
    /// `run_id` is not a well-formed run id, or no journal exists for it.
    /// Every `StepErr.cause` in the returned entries is the underlying
    /// VARA/CAT/HTTP text (spec §11) — the command layer passes them through
    /// untouched.
    pub fn journal_entries(&self, run_id: &str) -> Option<Vec<JournalEntry>> {
        if !valid_run_id(run_id) {
            return None;
        }
        let path = self.journal_dir.join(format!("{run_id}.jsonl"));
        read_journal(&path).ok()
    }

    /// How many runs the in-memory registry currently holds (live + retained
    /// terminal). Exists for the eviction test — the cap is an invariant worth
    /// asserting, and the `runs` map itself is private.
    pub fn registered_run_count(&self) -> usize {
        lock(&self.runs).len()
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
                        .start_routine_def(&def, serde_json::json!({}), None)
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

/// Bound the `runs` registry (Task 5a's carried Low finding). Retains every
/// LIVE run — a live run's `CancellationToken` is the only way to cancel it, so
/// evicting one would strand it uncancellable — and the [`MAX_TERMINAL_RUNS`]
/// most-recent TERMINAL runs, dropping the oldest terminal entries beyond that.
///
/// Dropping a terminal entry loses nothing: the journal on disk is the durable
/// record, and [`RoutinesState::run_status`] falls back to it, so an evicted
/// run still answers a status query (one file read instead of a map hit).
fn prune_terminal_runs(runs: &mut HashMap<String, RunEntry>) {
    let mut terminal: Vec<(u64, String)> = runs
        .iter()
        .filter(|(_, e)| is_terminal(e.state))
        .map(|(id, e)| (e.seq, id.clone()))
        .collect();
    if terminal.len() <= MAX_TERMINAL_RUNS {
        return;
    }
    // Oldest first, then drop from the front until the cap is met.
    terminal.sort_by_key(|(seq, _)| *seq);
    let excess = terminal.len() - MAX_TERMINAL_RUNS;
    for (_, run_id) in terminal.into_iter().take(excess) {
        runs.remove(&run_id);
    }
}

/// Is a routine's automatic-transmit acknowledgment recorded (spec §4)? True
/// iff `transmit_ack` is present AND both its `by` (callsign) and `at`
/// (timestamp) are non-empty after trimming — the exact rule plan 3's validator
/// applies for "unacknowledged auto-TX cannot be enabled", kept identical here
/// so enforcement and validation never disagree.
fn ack_is_recorded(def: &RoutineDef) -> bool {
    def.transmit_ack
        .as_ref()
        .is_some_and(|a| !a.by.trim().is_empty() && !a.at.trim().is_empty())
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
    let identity_store_path = config_dir.join("identities.json");
    let journal_dir = config_dir.join("routines-runs");

    let resolver: Arc<dyn EntityResolver> = Arc::new(MonolithEntityResolver::new(
        presets.clone(),
        station_sets.clone(),
        identity_store_path.clone(),
    ));

    // Consent wiring (slice 5b, spec §4): capture the transmitting action names
    // from the registry descriptors (the start gate's closure predicate), and
    // install the ConsentRegistry as the engine's ConsentPort. The executor
    // parks an attended run's transmit steps on it BEFORE the step timeout — no
    // action-wrapping and no per-run registry swap; the per-run park decision is
    // the engine's effective attended flag applied in `run_action_step_shared`.
    let consent = Arc::new(ConsentRegistry::new(sink.clone()));
    let transmit_names = transmit_action_names(&registry);

    // ONE registry `Arc`, shared by the engine (which resolves actions from it)
    // and the state (whose command layer builds the validation context's
    // `action_descriptor()` from it). Two copies could drift; one cannot.
    let registry = Arc::new(registry);

    let engine = Arc::new(Engine::new(EngineConfig {
        journal_dir: journal_dir.clone(),
        registry: registry.clone(),
        resolver,
        now: unix_now_secs,
        default_timeout_s: DEFAULT_TIMEOUT_S,
        lookup: Some(store.lookup_fn()),
        consent: Some(consent.clone() as Arc<dyn ConsentPort>),
    }));

    RoutinesState::new(
        engine,
        store,
        presets,
        station_sets,
        arbiter,
        registry,
        journal_dir,
        identity_store_path,
        sink,
        consent,
        transmit_names,
    )
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
        ActionStep, BusyPolicy, Control, ControlStep, Step, StepId, Track, TransmitAck,
        TransmitMode, Trigger, SUPPORTED_SCHEMA_VERSION,
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
                dry_run: false,
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
                dry_run: false,
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

    // ========================================================================
    // Task 6 — the runs registry is BOUNDED (5a's carried Low finding)
    // ========================================================================

    #[tokio::test]
    async fn terminal_runs_are_evicted_past_the_cap_and_still_answer_from_the_journal() {
        let dir = tempfile::tempdir().unwrap();
        let (state, _sink) = test_state(
            dir.path().to_path_buf(),
            FakeAction::new("local.log").ok(json!({})),
        );
        state
            .store
            .save(&minimal_def("quick", OnInterrupted::Stay))
            .unwrap();

        // Run more routines than the cap allows to retain.
        let overflow = MAX_TERMINAL_RUNS + 10;
        let mut first_run_id = None;
        for i in 0..overflow {
            let run_id = state.start_routine("quick", json!({})).await.unwrap();
            if i == 0 {
                first_run_id = Some(run_id.clone());
            }
            // Let each run reach its terminus before starting the next, so the
            // registry is full of TERMINAL entries (live runs are never evicted).
            wait_until(|| state.run_status(&run_id).map(|s| s.state) == Some(RunState::Completed))
                .await;
        }

        // The map is bounded — it did not grow to `overflow` entries.
        let count = state.registered_run_count();
        assert!(
            count <= MAX_TERMINAL_RUNS,
            "the runs registry must be bounded at {MAX_TERMINAL_RUNS}, holds {count}"
        );

        // And the OLDEST run — evicted from memory — still answers honestly,
        // read back from its journal. Eviction costs a file read, not an answer.
        let first = first_run_id.unwrap();
        let status = state
            .run_status(&first)
            .expect("an evicted run still answers from its journal");
        assert_eq!(status.routine, "quick");
        assert_eq!(status.state, RunState::Completed);
        assert!(!status.dry_run);
    }

    #[tokio::test]
    async fn a_live_run_is_never_evicted() {
        let dir = tempfile::tempdir().unwrap();
        // A hanging action: this run stays live for the whole test.
        let (state, _sink) = test_state(
            dir.path().to_path_buf(),
            FakeAction::new("local.log").hang(),
        );
        state
            .store
            .save(&minimal_def("hangs", OnInterrupted::Stay))
            .unwrap();

        // Every one of these runs stays LIVE (the action hangs), so none is
        // evictable — the cap applies only to terminal entries.
        let mut ids = Vec::new();
        for _ in 0..(MAX_TERMINAL_RUNS + 5) {
            ids.push(state.start_routine("hangs", json!({})).await.unwrap());
        }
        assert_eq!(
            state.registered_run_count(),
            ids.len(),
            "live runs must never be evicted — their cancel token is the only way to stop them"
        );
        // Each is still cancellable, which is the reason they must be retained.
        for id in &ids {
            assert!(
                state.cancel_run(id),
                "a retained live run stays cancellable"
            );
        }
    }

    #[test]
    fn valid_run_id_rejects_a_traversing_id_before_any_journal_path_is_built() {
        assert!(valid_run_id("run-1752400000-0000"));
        assert!(!valid_run_id("../../etc/passwd"));
        assert!(!valid_run_id("run/../escape"));
        assert!(!valid_run_id(""));
        assert!(!valid_run_id("RUN-UPPER"));
    }

    #[tokio::test]
    async fn journal_entries_refuses_a_traversing_run_id() {
        let dir = tempfile::tempdir().unwrap();
        let (state, _sink) = test_state(
            dir.path().to_path_buf(),
            FakeAction::new("local.log").ok(json!({})),
        );
        assert!(state.journal_entries("../../etc/passwd").is_none());
        assert!(state.journal_entries("run-nonexistent").is_none());
    }

    // ========================================================================
    // Task 6 — dry runs go through the engine's registry swap
    // ========================================================================

    /// The bug this test locks down: `start_routine_def` used to pass its
    /// `dry_run` flag to `start_run_ext`, which only suppresses consent parking
    /// — the REAL action catalog still executed. A "dry run" that keys a real
    /// transmitter is worse than no dry run at all.
    #[tokio::test]
    async fn a_dry_run_executes_no_real_action() {
        let dir = tempfile::tempdir().unwrap();
        let real = Arc::new(
            FakeAction::new("radio.tx")
                .with_capabilities(true, true, false)
                .ok(json!({"sent": true})),
        );
        let (state, sink) = state_with(dir.path().to_path_buf(), std::slice::from_ref(&real));

        // Automatic + transmitting + unacked: a REAL run is refused outright.
        state
            .store
            .save(&tx_def("auto", TransmitMode::Automatic, None))
            .unwrap();
        assert!(state.start_routine("auto", json!({})).await.is_err());

        // The DRY run proceeds — and completes — without touching the real
        // action, because the engine replaced the whole registry with fakes.
        let run_id = state
            .start_dry_run("auto", json!({}), DryRunScript::new())
            .await
            .expect("a dry run is never consent-gated");
        wait_until(|| count_finished(&sink.events(), RunState::Completed) == 1).await;

        assert!(
            real.calls().is_empty(),
            "a dry run must never execute the real transmit action"
        );
        assert!(
            state.run_status(&run_id).map(|s| s.dry_run) == Some(true),
            "the run is stamped as a dry run"
        );
        // The journal's own dry_run stamp is the durable proof.
        let entries = state.journal_entries(&run_id).unwrap();
        assert!(
            entries
                .iter()
                .any(|e| matches!(&e.event, RunEvent::RunStarted { dry_run: true, .. })),
            "the journal records that no real action was invoked for this run"
        );
    }

    // ========================================================================
    // Slice 5b — transmit-consent enforcement (spec §4)
    // ========================================================================

    /// A `radio.tx` FakeAction flagged `transmits: true` — the wrapper wraps it.
    fn tx_action() -> Arc<FakeAction> {
        Arc::new(
            FakeAction::new("radio.tx")
                .with_capabilities(true, true, false)
                .ok(json!({"sent": true})),
        )
    }

    fn ack(by: &str, at: &str) -> TransmitAck {
        TransmitAck {
            by: by.into(),
            at: at.into(),
        }
    }

    /// Build a state whose registry holds the given (already-built) actions.
    /// `build_routines_state` installs the ConsentRegistry as the engine's
    /// ConsentPort, so an attended run's transmit steps park in the executor.
    fn state_with(
        config_dir: PathBuf,
        actions: &[Arc<FakeAction>],
    ) -> (Arc<RoutinesState>, Arc<RecordingSink>) {
        let mut reg = ActionRegistry::default();
        for a in actions {
            reg.register(a.clone());
        }
        let arbiter = Arc::new(RadioArbiter::new(fixed_now));
        let sink = Arc::new(RecordingSink::default());
        let sink_dyn: Arc<dyn RoutinesEventSink> = sink.clone();
        let state = Arc::new(build_routines_state(config_dir, reg, arbiter, sink_dyn));
        (state, sink)
    }

    /// A one-transmit-step routine (`radio.tx`), with the given mode + ack.
    fn tx_def(name: &str, mode: TransmitMode, ack: Option<TransmitAck>) -> RoutineDef {
        RoutineDef {
            routine: name.into(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: mode,
            transmit_ack: ack,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t".into(),
                steps: vec![Step::Action(ActionStep {
                    id: StepId("s1".into()),
                    action: "radio.tx".into(),
                    params: json!({}),
                    timeout_s: None,
                    on_radio_busy: BusyPolicy::Wait,
                })],
            }],
        }
    }

    /// Block until an `AwaitingConsent` event appears; return its (run, step).
    async fn wait_awaiting(sink: &RecordingSink) -> (String, String) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            for e in sink.events() {
                if let RoutinesEvent::AwaitingConsent { run_id, step_id } = e {
                    return (run_id, step_id);
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no AwaitingConsent event"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    // ── Start-gate matrix ────────────────────────────────────────────────

    #[tokio::test]
    async fn gate_attended_tx_is_allowed_to_start() {
        let dir = tempfile::tempdir().unwrap();
        let (state, sink) = state_with(dir.path().to_path_buf(), &[tx_action()]);
        state
            .store
            .save(&tx_def("att", TransmitMode::Attended, None))
            .unwrap();
        // Attended + transmitting starts even with NO ack — it pauses at the
        // transmit step (spec §4).
        let run_id = state.start_routine("att", json!({})).await.unwrap();
        // It parks rather than transmitting; release it so the task ends cleanly.
        wait_awaiting(&sink).await;
        state.cancel_run(&run_id);
    }

    #[tokio::test]
    async fn gate_automatic_acked_starts() {
        let dir = tempfile::tempdir().unwrap();
        let action = tx_action();
        let (state, _sink) = state_with(dir.path().to_path_buf(), std::slice::from_ref(&action));
        state
            .store
            .save(&tx_def(
                "auto",
                TransmitMode::Automatic,
                Some(ack("KK7ABC", "2026-07-13T20:00:00Z")),
            ))
            .unwrap();
        let run_id = state.start_routine("auto", json!({})).await.unwrap();
        // Automatic + acked does NOT park: the transmit action runs unattended.
        wait_until(|| !action.calls().is_empty()).await;
        wait_until(|| state.run_status(&run_id).map(|s| s.state) == Some(RunState::Completed))
            .await;
    }

    #[tokio::test]
    async fn gate_automatic_unacked_is_refused_with_operator_message() {
        let dir = tempfile::tempdir().unwrap();
        let action = tx_action();
        let (state, sink) = state_with(dir.path().to_path_buf(), std::slice::from_ref(&action));
        state
            .store
            .save(&tx_def("auto", TransmitMode::Automatic, None))
            .unwrap();
        let err = state.start_routine("auto", json!({})).await.unwrap_err();
        match &err {
            RoutineStartError::UnacknowledgedAutomatic { routine } => {
                assert_eq!(routine, "auto");
                // Operator-facing message names the routine and the fix.
                let msg = err.to_string();
                assert!(msg.contains("auto"), "message names the routine: {msg}");
                assert!(
                    msg.contains("acknowledge") || msg.contains("acknowledgment"),
                    "message tells the operator to acknowledge: {msg}"
                );
            }
            other => panic!("expected UnacknowledgedAutomatic, got {other:?}"),
        }
        // Refused BEFORE any run started: no run, no transmit, no event.
        assert!(action.calls().is_empty(), "the transmit action never ran");
        assert!(
            !sink
                .events()
                .iter()
                .any(|e| matches!(e, RoutinesEvent::RunStarted { .. })),
            "no run was started"
        );
    }

    /// An ack whose fields are whitespace-only is NOT a recorded ack (same rule
    /// the validator applies): the automatic routine is still refused.
    #[tokio::test]
    async fn gate_automatic_blank_ack_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (state, _sink) = state_with(dir.path().to_path_buf(), &[tx_action()]);
        state
            .store
            .save(&tx_def(
                "auto",
                TransmitMode::Automatic,
                Some(ack("   ", "  ")),
            ))
            .unwrap();
        let err = state.start_routine("auto", json!({})).await.unwrap_err();
        assert!(matches!(
            err,
            RoutineStartError::UnacknowledgedAutomatic { .. }
        ));
    }

    #[tokio::test]
    async fn gate_non_transmitting_starts_regardless_of_mode() {
        let dir = tempfile::tempdir().unwrap();
        // Automatic mode + NO ack, but the routine's only action does not
        // transmit → the closure does not transmit → it starts unconditionally.
        let log = Arc::new(FakeAction::new("local.log").ok(json!({})));
        let (state, _sink) = state_with(dir.path().to_path_buf(), &[log]);
        state
            .store
            .save(&minimal_def("quiet-auto", OnInterrupted::Stay))
            .unwrap();
        // minimal_def is attended; make an automatic, unacked, non-TX variant:
        let mut def = minimal_def("quiet-auto2", OnInterrupted::Stay);
        def.transmit_mode = TransmitMode::Automatic;
        def.transmit_ack = None;
        state.store.save(&def).unwrap();
        let run_id = state.start_routine("quiet-auto2", json!({})).await.unwrap();
        wait_until(|| state.run_status(&run_id).map(|s| s.state) == Some(RunState::Completed))
            .await;
    }

    // ── Attended pause: park / grant / cancel ────────────────────────────

    #[tokio::test]
    async fn attended_run_parks_at_tx_step_and_grant_resumes_it() {
        let dir = tempfile::tempdir().unwrap();
        let action = tx_action();
        let (state, sink) = state_with(dir.path().to_path_buf(), std::slice::from_ref(&action));
        state
            .store
            .save(&tx_def("att", TransmitMode::Attended, None))
            .unwrap();

        let run_id = state.start_routine("att", json!({})).await.unwrap();
        let (ev_run, ev_step) = wait_awaiting(&sink).await;
        assert_eq!(ev_run, run_id);
        assert_eq!(ev_step, "s1");
        // Parked: the transmit action has NOT executed.
        assert!(
            action.calls().is_empty(),
            "must not transmit before consent"
        );

        // Operator grants: the transmit action now runs, and the run completes.
        assert!(state.grant_consent(&run_id, "s1"), "a step was parked");
        wait_until(|| !action.calls().is_empty()).await;
        wait_until(|| count_finished(&sink.events(), RunState::Completed) == 1).await;
    }

    #[tokio::test]
    async fn cancel_while_parked_ends_cancelled_and_never_transmits() {
        let dir = tempfile::tempdir().unwrap();
        let action = tx_action();
        let (state, sink) = state_with(dir.path().to_path_buf(), std::slice::from_ref(&action));
        state
            .store
            .save(&tx_def("att", TransmitMode::Attended, None))
            .unwrap();

        let run_id = state.start_routine("att", json!({})).await.unwrap();
        wait_awaiting(&sink).await;
        assert!(action.calls().is_empty());

        assert!(state.cancel_run(&run_id));
        wait_until(|| count_finished(&sink.events(), RunState::Cancelled) == 1).await;
        // The carrier was never keyed.
        assert!(
            action.calls().is_empty(),
            "cancel while parked must never execute the transmit action"
        );
    }

    // ── Closure-through-call under an attended parent ────────────────────

    #[tokio::test]
    async fn closure_through_call_parks_under_attended_parent() {
        let dir = tempfile::tempdir().unwrap();
        let action = tx_action();
        let (state, sink) = state_with(dir.path().to_path_buf(), std::slice::from_ref(&action));

        // Child transmits; child's OWN mode is automatic (acked) — but called by
        // an attended parent, its transmit step must still pause (spec §10).
        state
            .store
            .save(&tx_def(
                "tx-child",
                TransmitMode::Automatic,
                Some(ack("KK7ABC", "2026-07-13T20:00:00Z")),
            ))
            .unwrap();
        // Parent is attended and only calls the child (no TX step of its own).
        let parent = RoutineDef {
            routine: "att-parent".into(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![Track {
                name: "t".into(),
                steps: vec![Step::Control(ControlStep {
                    id: StepId("c1".into()),
                    control: Control::Call {
                        routine: "tx-child".into(),
                        args: json!({}),
                        sync: true,
                    },
                })],
            }],
        };
        state.store.save(&parent).unwrap();

        let _parent_run = state.start_routine("att-parent", json!({})).await.unwrap();
        // The CHILD run parks at its transmit step even though the child itself
        // is automatic — attended-ness propagated down the call.
        let (child_run, child_step) = wait_awaiting(&sink).await;
        assert_eq!(child_step, "s1");
        assert!(
            action.calls().is_empty(),
            "child must not transmit unattended"
        );

        // Grant the child's transmit; the whole chain then completes.
        assert!(state.grant_consent(&child_run, &child_step));
        wait_until(|| !action.calls().is_empty()).await;
    }

    // ── Recovery-resume routes through the same gate ─────────────────────

    #[tokio::test]
    async fn recovery_resume_of_attended_tx_parks_not_auto_transmits() {
        let dir = tempfile::tempdir().unwrap();
        let action = tx_action();
        let (state, sink) = state_with(dir.path().to_path_buf(), std::slice::from_ref(&action));

        // A dead journal for an attended, resume-policy, transmitting routine.
        let journal_dir = dir.path().join("routines-runs");
        let mut def = tx_def("att-resume", TransmitMode::Attended, None);
        def.on_interrupted = OnInterrupted::Resume;
        {
            let mut w = JournalWriter::create(&journal_dir, "run-dead-tx", fixed_now).unwrap();
            w.append(RunEvent::RunStarted {
                routine: "att-resume".into(),
                snapshot: serde_json::to_value(&def).unwrap(),
                dry_run: false,
            })
            .unwrap();
        }

        let report = state.recover().await.unwrap();
        assert_eq!(report.interrupted, 1);
        assert_eq!(report.resumed, 1, "resume policy re-invokes from snapshot");

        // The resumed run flows through start_routine_def → the SAME consent
        // gate → it PARKS at the transmit step; it does NOT auto-transmit on
        // boot (spec §8's "this may key the radio shortly after boot" is exactly
        // what attended mode prevents).
        wait_awaiting(&sink).await;
        assert!(
            action.calls().is_empty(),
            "a resumed attended-TX run must park, never auto-transmit"
        );
    }
}
