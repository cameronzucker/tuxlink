//! `RoutinesScheduler` — the tick loop that actually FIRES schedule-triggered
//! routines (plan 2 Task 6b, spec §8).
//!
//! Before this module, an enabled `Trigger::Schedule` routine validated,
//! enabled, and fleet-checked — and then nothing happened, ever. The schedule
//! MATH has been in the leaf crate all along (`tuxlink_routines::scheduler`:
//! [`next_fire`], [`missed_fires_windowed`], `within_window`, `every_seconds` — pure
//! functions over unix seconds); what it deliberately did NOT own was the tick
//! loop, because firing means *creating a run*, and run creation is the app
//! layer's job (the consent gate, the event sink, and the run registry all live
//! here). This module is that loop.
//!
//! ## Shape
//!
//! ONE tokio task, holding an `Arc<RoutinesState>`:
//!
//! 1. **At launch** — reconcile missed fires (below), then enter the loop.
//! 2. **Compute** the next fire instant across every enabled, schedule-triggered
//!    routine (the earliest [`next_fire`] over the fleet).
//! 3. **Sleep** until that instant — cancellably, and interruptibly: the
//!    library-changed notify (§"Reacting to library changes") wakes it early.
//! 4. **Fire** every routine due at that instant (there can be several — an
//!    aligned fleet collides on the hour by construction; the arbiter serializes
//!    their radio steps, and the fleet check warned the operator at enable time).
//! 5. **Recompute**, forever.
//!
//! ## The anchor — why cadence is measured from the last fire, not from `now`
//!
//! Each routine's next fire is computed from its **anchor**: the last instant
//! its schedule was EVALUATED (fired, skipped, or refused — see
//! [`LastFireStore`]), or the instant it was first seen enabled. Not from
//! "now".
//!
//! This matters because [`next_fire`] for an UNALIGNED schedule is
//! `anchor + interval`. Had the loop re-anchored to `now` on every recompute,
//! every library change would push the next fire out by however long the loop
//! had been sleeping — an operator who saves a routine every 25 minutes would
//! keep a 30-minute schedule from EVER firing. Anchoring to the last evaluation
//! makes the cadence stable under arbitrary recomputes. (Aligned schedules snap
//! to the epoch grid inside `next_fire` and are anchor-independent, so they were
//! never exposed to that bug — but one rule for both is simpler than two.)
//!
//! The anchor advances to the **evaluation instant** (`now` at fire time), not
//! to the scheduled instant. The difference is the loop's wake latency — under a
//! second — and it buys a property that matters: after a long suspend (laptop
//! lid, `SIGSTOP`, a 3-hour freeze) a 30-minute schedule fires exactly ONCE on
//! resume, not six times back-to-back. A burst of catch-up runs on a station
//! radio is precisely the pile-up this module exists to prevent.
//!
//! ## Missed fires at launch (spec §8)
//!
//! Schedules pause when the app is closed — Tuxlink does not run headless
//! (spec §3: the transmit consent gates are GUI elements, so unsupervised
//! operation is not a thing this app does). The gap is reconciled at launch, per
//! the trigger's `if_missed` policy:
//!
//! * **`skip`** (default) — the misses are RECORDED (persisted count + a
//!   [`RoutinesEvent::MissedFires`] event + a `tracing::warn`) and nothing runs.
//!   "Recorded visibly" is the spec's wording, and a count nobody can see is not
//!   visible: [`schedule_status`] is the read path the UI renders.
//! * **`run_once_on_launch`** — the anacron pattern, for the deployment Pi that
//!   rebooted overnight: ONE catch-up run starts now (not one per missed slot),
//!   then normal scheduling resumes.
//!
//! A routine with no recorded last-fire has no misses by definition (it was
//! enabled, but never yet armed at a fire instant) — its anchor is simply seeded
//! at the moment the scheduler first sees it. Nor does a routine the operator
//! just ENABLED: the command layer anchors it at the enable instant
//! ([`anchor_on_enable`]), so "no misses on a first enable" is a structural
//! property rather than an accident of there being no record yet. Without that,
//! a routine re-enabled after a week of being off would take its anchor from its
//! previous enabled period, compute a next fire that had already passed, and run
//! an immediate catch-up — the exact behavior `if_missed: skip` exists to
//! prevent.
//!
//! ## Everything that stops a fire is DURABLE, not just an event
//!
//! There are three ways a scheduled fire fails to happen — the app was closed
//! (missed), the gate said no (refused), the previous run was still going
//! (skipped) — and each emits an event. Events reach only a UI that is already
//! listening, which the operator asleep through five refused 03:00 fires is not.
//! So each is also written to the [`LastFireStore`] (`missed`,
//! `last_refusal{at, reason}`, `last_skip{at, reason}`) and served back by
//! [`schedule_status`], the read path behind the command. The refusal's reason is
//! the gate's verbatim operator-facing text, so the UI can render "last fire
//! refused: <reason>" without the scheduler having to paraphrase a gate it does
//! not own.
//!
//! ## No pile-up: a routine never overlaps itself
//!
//! If a routine's next fire arrives while its PREVIOUS run is still active, the
//! fire is skipped with a [`RoutinesEvent::ScheduleSkipped`] event and a
//! `tracing::warn` naming the routine. It is not queued and it is not started
//! alongside. The radio arbiter (spec §9) would serialize the two runs' radio
//! steps anyway, so nothing would be *unsafe* about the overlap — but a routine
//! that takes 40 minutes on a 30-minute schedule would otherwise accumulate runs
//! without bound, each holding a lease queue slot, and the station would fall
//! further behind forever. Skipping is the honest failure: the operator sees
//! "this routine cannot keep up with its schedule" in the event stream.
//!
//! ## Refusals are events, not crashes and not silent skips
//!
//! A fire goes through [`super::commands::run_routine`] — the SAME path the
//! operator's Run button takes — so a scheduled fire is gated exactly like a
//! manual one: validation errors block it (spec §10: "errors block enable/run"),
//! and the transmit-consent start gate refuses an unacknowledged automatic
//! routine (spec §4). Either refusal is emitted as
//! [`RoutinesEvent::ScheduleRefused`] carrying the gate's verbatim
//! operator-facing message, logged at `warn`, and the loop continues. A refusal
//! never panics the task, and it is never dropped on the floor.
//!
//! ## Reacting to library changes — and never missing one
//!
//! [`RoutinesState::emit`] pings a `tokio::sync::Notify` on every
//! `LibraryChanged{entity: routine}` — which is every save, delete, enable, and
//! disable, since the command layer funnels all of them through that one call.
//! The scheduler selects on that notify while it sleeps, so a disable takes
//! effect immediately rather than at the end of the current sleep. Nothing in
//! the command layer had to change: the chokepoint was already there.
//!
//! A scheduler that misses one of those pings STOPS SCHEDULING, so the wake path
//! is defended three deep — each layer cheap, each independently sufficient:
//!
//! 1. **The ping stores a permit.** `notify_one`, not `notify_waiters`: a change
//!    that lands while the loop is not parked (mid-disk-read, mid-fire) is held
//!    and delivered to the next wait instead of being dropped.
//! 2. **The wait is armed before the world is read.** [`RoutinesScheduler::run`]
//!    creates the `notified()` future BEFORE listing the routines directory, so a
//!    change landing during that read cannot slip between the read and the await.
//! 3. **There is always a timer.** Even with nothing scheduled at all, the loop
//!    re-reads the store every [`MAX_SLEEP_SECS`]. If the ping mechanism failed
//!    entirely, the worst case is a one-minute delay — never a scheduler parked
//!    forever waiting for an event that already happened.
//!
//! Layer 3 is what makes the first-ever enable safe. Before it, an empty library
//! meant a wait with no timer, so a single dropped ping meant the first routine
//! an operator ever enabled would silently never fire.
//!
//! ## A stale due-set never fires a disabled routine
//!
//! The due-set is computed before the sleep and consumed after it, so by the time
//! [`RoutinesScheduler::fire`] runs, it is a HINT about what WAS due — not an
//! authority on what may run. `fire` re-reads the enabled bit from disk and drops
//! the fire if the routine has since been disabled or deleted. The pings above
//! make that rare; the re-read makes it impossible. For an automatic-transmit
//! routine the difference between "rare" and "impossible" is an unscheduled
//! transmission from a routine the operator explicitly turned off.
//!
//! ## Shutdown
//!
//! The task holds a [`CancellationToken`] and stops promptly when it is
//! cancelled ([`SchedulerHandle::stop`]) — that is the seam the tests drive, and
//! the seam a future graceful-quit path would call. The app has **no established
//! global shutdown signal today** (see `lib.rs`'s modem-status broadcaster: "the
//! thread runs for the lifetime of the process; v1 has no shutdown signal"), so
//! in production the handle is managed and the task simply dies with the
//! process. That is acceptable under the supervised posture (spec §3): the
//! scheduler owns no transport state and holds no radio lease of its own — the
//! runs it starts own those, and they have their own cancel tokens and their own
//! journals, which launch recovery reconciles.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::futures::Notified;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use tuxlink_routines::scheduler::{missed_fires_windowed, next_fire};
use tuxlink_routines::types::{IfMissed, Trigger};

use super::atomic_write;
use super::events::RoutinesEvent;
use super::session::RoutinesState;
use crate::ui_commands::UiError;

/// The longest single `sleep` the loop will take, however far away the next fire
/// is. On wake it re-reads the clock and recomputes.
///
/// Capping matters because the fire instants are WALL-CLOCK (unix seconds) while
/// `tokio::time::sleep` is MONOTONIC: an NTP step, a manual clock change, or a
/// suspend/resume desynchronizes the two. A cap bounds that error to one minute
/// — after any clock discontinuity the loop notices within 60 s and re-derives
/// everything from the new wall-clock reading. The cost is one directory listing
/// per minute per idle station.
const MAX_SLEEP_SECS: i64 = 60;

/// The clock seam (mirrors the leaf's `EngineConfig.now`): wall-clock unix
/// seconds. Production passes `session::unix_now_secs`; tests pass a function
/// derived from tokio's paused clock, so the loop's sleeps and its notion of
/// "now" advance together and every timing assertion is deterministic.
pub type NowFn = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The timezone seam (Task 6c, spec §8): the operator's LOCAL UTC offset in
/// seconds (`local - utc`, `chrono`'s `FixedOffset::local_minus_utc`
/// convention), threaded into `tuxlink_routines::scheduler::next_fire` so a
/// `Trigger::Schedule`'s `window` ("22:00-06:00") gates in the clock the
/// operator actually authored it against, not UTC. A closure rather than a
/// cached `i32` because DST changes the answer twice a year — the offset
/// must be read fresh on every evaluation (production reads
/// `session::local_utc_offset_seconds`, which calls `chrono::Local::now()`
/// every time), never memoized for the scheduler's lifetime.
pub type OffsetFn = Arc<dyn Fn() -> i32 + Send + Sync>;

// ============================================================================
// The last-fire map — the durable half of the missed-fire policy
// ============================================================================

/// A fire the gate REFUSED, kept for the read path (spec §8's "recorded
/// visibly": an operator who opens the app at 08:00 must be able to learn that
/// the 03:00 fire was refused, and why, verbatim).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    /// The SCHEDULED instant of the fire that was refused.
    pub at: i64,
    /// The gate's operator-facing message, passed through verbatim (see
    /// [`refusal_reason`]).
    pub reason: String,
}

/// A fire that was SKIPPED because the routine's previous run was still active
/// (the no-pile-up rule). Same read-path rationale as [`Refusal`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skip {
    /// The SCHEDULED instant of the fire that was skipped.
    pub at: i64,
    /// Why it was skipped, in the same words the [`RoutinesEvent::ScheduleSkipped`]
    /// event carried.
    pub reason: String,
}

/// One routine's schedule bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastFire {
    /// The last instant this routine's schedule was EVALUATED — fired, skipped
    /// for overlap, or refused at the gate — or the instant the scheduler first
    /// saw it enabled, or the instant the operator enabled it. This is the
    /// anchor `next_fire` is computed from, and the `last_seen` the launch-time
    /// [`missed_fires_windowed`] reckoning measures against.
    ///
    /// Evaluated, not *fired*, is the load-bearing word: a fire the gate refused
    /// is not a fire the app was CLOSED for, and counting it as a "missed fire"
    /// at the next launch would be a lie.
    pub last_fire_unix: i64,
    /// Fires that elapsed while the app was closed, as computed at the most
    /// recent launch (0 if none). Overwritten each launch; normal fires leave it
    /// alone, so the UI can keep showing "7 fires missed overnight" until the
    /// next launch re-reckons it.
    #[serde(default)]
    pub missed: u64,
    /// The most recent refused fire, if any — the durable half of
    /// [`RoutinesEvent::ScheduleRefused`], which only ever reached a UI that was
    /// already listening.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refusal: Option<Refusal>,
    /// The most recent skipped (overlapping) fire, if any — the durable half of
    /// [`RoutinesEvent::ScheduleSkipped`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_skip: Option<Skip>,
}

/// `{routine: LastFire}`, atomically persisted beside `config.json` (spec §14:
/// every routines-module write goes through [`atomic_write`], never a bare
/// `fs::write`). Sited beside the other routines stores — `radio-presets.json`,
/// `station-sets.json` — rather than inside `routines/`, whose every `*.json` is
/// a portable routine DEFINITION; station-local runtime state does not belong in
/// a directory whose contents are meant to be exportable (spec §14).
///
/// Nearly single-writer: the scheduler task does every write except one — the
/// command layer's enable path anchors a routine's cadence at the instant the
/// operator armed it ([`anchor_on_enable`], and see the "the anchor" module
/// note). The two can in principle interleave, so every mutation here is a
/// narrow read-modify-write of ONE routine's entry, and the seeding path in
/// `next_due` re-reads before it writes and uses `or_insert` rather than
/// clobbering. The worst case a collision can produce is a single routine's
/// anchor landing on one of two instants a few milliseconds apart — which is
/// the loop's ordinary wake latency, and harmless.
pub struct LastFireStore {
    path: PathBuf,
}

impl LastFireStore {
    pub fn open(path: PathBuf) -> Self {
        Self { path }
    }

    /// The whole map. A missing, unreadable, or corrupt file reads as EMPTY
    /// rather than failing the scheduler: an empty map means "no last-fire
    /// known", whose defined behavior (seed the anchor, no misses) is exactly
    /// the safe one — it never invents a miss and never fires a catch-up run it
    /// cannot justify.
    pub fn read(&self) -> BTreeMap<String, LastFire> {
        match std::fs::read_to_string(&self.path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => BTreeMap::new(),
        }
    }

    fn write(&self, map: &BTreeMap<String, LastFire>) {
        let Ok(json) = serde_json::to_vec_pretty(map) else {
            return;
        };
        if let Err(e) = atomic_write(&self.path, &json) {
            // A failed write costs at most a re-reckoning at the next launch
            // (the anchor falls back to "first seen"). It must never take the
            // scheduler down.
            tracing::warn!(
                target: "tuxlink::routines",
                path = %self.path.display(),
                error = %e,
                "failed to persist the routines last-fire map",
            );
        }
    }

    pub fn get(&self, routine: &str) -> Option<LastFire> {
        self.read().get(routine).cloned()
    }

    /// Record that `routine`'s schedule was evaluated at `at` (fired, skipped,
    /// or refused). Leaves the `missed` count alone — it is the launch-time
    /// reckoning's field, and a normal fire does not answer it.
    pub fn record_evaluated(&self, routine: &str, at: i64) {
        let mut map = self.read();
        let entry = map.entry(routine.to_string()).or_default();
        entry.last_fire_unix = at;
        self.write(&map);
    }

    /// Record that `routine` was ENABLED at `at`: anchor its cadence HERE, and
    /// drop the schedule diagnostics from its previous enabled period.
    ///
    /// The anchor is the point. Without it, `next_due` would take the anchor from
    /// the routine's LAST enabled period — a week ago, say — and
    /// `next_fire(anchor)` for an unaligned schedule (`anchor + interval`) would
    /// land in the past, so the very first thing a re-enabled routine did was an
    /// immediate catch-up fire. That directly contradicts `if_missed: skip`,
    /// which the operator chose precisely to say "do not make up for lost time",
    /// and it is worse than a stale count: on an automatic-transmit routine it is
    /// a transmission the operator did not ask for, seconds after a click that
    /// said nothing about firing NOW.
    ///
    /// The diagnostics go with it: `missed`, `last_refusal`, and `last_skip` all
    /// describe the previous enabled period. A refusal from three weeks ago,
    /// still shown under a routine the operator has since fixed and re-armed, is
    /// stale noise on the one surface that is supposed to answer "what is wrong
    /// with this routine RIGHT NOW".
    ///
    /// Idempotent by construction, but the command layer only calls it on a real
    /// disabled → enabled transition, so re-enabling an already-enabled routine
    /// does not disturb its cadence.
    pub fn record_enabled(&self, routine: &str, at: i64) {
        let mut map = self.read();
        map.insert(
            routine.to_string(),
            LastFire {
                last_fire_unix: at,
                missed: 0,
                last_refusal: None,
                last_skip: None,
            },
        );
        self.write(&map);
    }

    /// Persist a REFUSED fire (the gate said no). Leaves the anchor alone — the
    /// fire path already recorded the evaluation instant — and leaves `missed`
    /// alone, which is the launch reckoning's field.
    pub fn record_refusal(&self, routine: &str, at: i64, reason: &str) {
        let mut map = self.read();
        let entry = map.entry(routine.to_string()).or_default();
        entry.last_refusal = Some(Refusal {
            at,
            reason: reason.to_string(),
        });
        self.write(&map);
    }

    /// Persist a SKIPPED fire (the previous run was still active). Same
    /// leave-the-anchor-alone rationale as [`Self::record_refusal`].
    pub fn record_skip(&self, routine: &str, at: i64, reason: &str) {
        let mut map = self.read();
        let entry = map.entry(routine.to_string()).or_default();
        entry.last_skip = Some(Skip {
            at,
            reason: reason.to_string(),
        });
        self.write(&map);
    }

    /// Record the launch-time reckoning: `missed` fires elapsed while the app
    /// was closed, and the schedule is re-anchored at `at` (now) so the loop
    /// does not immediately fire the whole backlog it just decided to skip.
    pub fn record_missed(&self, routine: &str, at: i64, missed: u64) {
        let mut map = self.read();
        let entry = map.entry(routine.to_string()).or_default();
        entry.last_fire_unix = at;
        entry.missed = missed;
        self.write(&map);
    }

    /// Drop entries for routines that no longer exist (deleted from the
    /// library), so the map cannot grow without bound across the station's life.
    /// Run once at launch, against the routine names actually on disk.
    pub fn retain(&self, known: &HashSet<String>) {
        let map = self.read();
        let pruned: BTreeMap<String, LastFire> = map
            .iter()
            .filter(|(name, _)| known.contains(name.as_str()))
            .map(|(name, entry)| (name.clone(), entry.clone()))
            .collect();
        if pruned.len() != map.len() {
            self.write(&pruned);
        }
    }
}

/// What the scheduler has to say about one routine, for the UI (the
/// `routines_missed_fires` command). Only routines with something to REPORT
/// appear (see [`schedule_status`]).
///
/// Named for what it is rather than for the command that carries it: it began
/// as a missed-fires-only record, and missed fires turned out to be one of
/// three ways a scheduled fire can fail to happen — the app was closed
/// (`missed`), the gate said no (`last_refusal`), or the previous run was still
/// going (`last_skip`). All three are things the operator opens the app to find
/// out, and all three used to be answerable only by an event stream that a
/// window opened afterwards had already missed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleStatus {
    pub routine: String,
    /// Fires that elapsed while the app was CLOSED, per the last launch's
    /// reckoning. 0 when the routine has nothing to report on that front but
    /// does carry a refusal or a skip.
    pub missed: u64,
    pub last_fire_unix: i64,
    /// The most recent fire the gate refused, with its verbatim reason — what
    /// the UI renders as "last fire refused: <reason>".
    pub last_refusal: Option<Refusal>,
    /// The most recent fire skipped because the routine was still running.
    pub last_skip: Option<Skip>,
}

/// Every routine the scheduler has something to say about: fires missed while
/// the app was closed, a refused fire, or a skipped one. The read path behind
/// spec §8's "misses are recorded visibly either way" — and behind the same
/// requirement for the OTHER two ways a fire fails to happen.
///
/// The [`RoutinesEvent::MissedFires`] / [`RoutinesEvent::ScheduleRefused`] /
/// [`RoutinesEvent::ScheduleSkipped`] events only ever reach a UI that was
/// already listening. An operator whose station refused five 03:00 fires and
/// who opens the app at 08:00 was not listening — this is how that operator
/// finds out. Empty when there is nothing to say.
pub fn schedule_status(state: &RoutinesState) -> Vec<ScheduleStatus> {
    LastFireStore::open(last_fire_path(state))
        .read()
        .into_iter()
        .filter(|(_, e)| e.missed > 0 || e.last_refusal.is_some() || e.last_skip.is_some())
        .map(|(routine, e)| ScheduleStatus {
            routine,
            missed: e.missed,
            last_fire_unix: e.last_fire_unix,
            last_refusal: e.last_refusal,
            last_skip: e.last_skip,
        })
        .collect()
}

/// Anchor `routine`'s cadence at `at` — the instant the operator ENABLED it.
///
/// The command layer's enable path calls this on a disabled → enabled
/// transition. Without it, a routine re-enabled after a long disable takes its
/// anchor from its PREVIOUS enabled period, `next_fire` lands in the past, and
/// the routine fires immediately — a catch-up run that `if_missed: skip`
/// explicitly told it not to do, and, on an automatic-transmit routine, an
/// unscheduled transmission moments after a click that did not ask for one.
/// See [`LastFireStore::record_enabled`].
pub fn anchor_on_enable(state: &RoutinesState, routine: &str, at: i64) {
    LastFireStore::open(last_fire_path(state)).record_enabled(routine, at);
}

/// Where the last-fire map lives for a given state (beside `config.json`).
fn last_fire_path(state: &RoutinesState) -> PathBuf {
    state.config_dir.join("routines-last-fire.json")
}

// ============================================================================
// The scheduler
// ============================================================================

/// A live scheduler task's control surface.
pub struct SchedulerHandle {
    cancel: CancellationToken,
}

impl SchedulerHandle {
    /// Stop the loop. It observes the token at its next select — i.e. promptly,
    /// even mid-sleep — and the task ends. Idempotent.
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

/// Task 6c, spec §8's graceful-quit path: "2 routines running — stop them
/// and exit?" resolves to cancelling every live run (as CANCELLED, per
/// [`RoutinesState::cancel_all_live_runs`]) and stopping this scheduler loop
/// so nothing schedules a NEW run into the window between the operator's
/// confirmation and the process actually exiting. Returns how many runs were
/// cancelled.
///
/// Deliberately a plain fn, not inlined into the Tauri close handler
/// (`lib.rs`): the close handler's only job is deciding WHETHER to call this
/// — based on the operator's answer to a native dialog it cannot show inside
/// a headless test — while this fn is what actually DOES it, and does not
/// touch Tauri, a window, or a dialog at all. That split is what makes the
/// core cancellation behavior testable headless.
///
/// Idempotent for the same reason [`RoutinesState::cancel_all_live_runs`] and
/// [`SchedulerHandle::stop`] each already are: cancelling an
/// already-cancelled run token, and stopping an already-stopped scheduler,
/// are both no-ops. A second call cancels 0 further runs and returns 0.
pub fn cancel_all_live_runs_and_stop(state: &RoutinesState, scheduler: &SchedulerHandle) -> usize {
    let cancelled = state.cancel_all_live_runs();
    scheduler.stop();
    cancelled
}

/// Why the sleep ended.
#[derive(Debug, PartialEq, Eq)]
enum Wake {
    /// The target instant arrived.
    Due,
    /// The routine library changed; recompute before doing anything.
    LibraryChanged,
    /// Nothing happened for [`MAX_SLEEP_SECS`]; re-read the world anyway. This
    /// is the belt to the notify's braces — see [`RoutinesScheduler::wait`].
    Recheck,
    /// The task was cancelled.
    Cancelled,
}

/// The set of routines due at one instant.
#[derive(Debug, PartialEq, Eq)]
struct Due {
    at: i64,
    routines: Vec<String>,
}

pub struct RoutinesScheduler {
    state: Arc<RoutinesState>,
    last_fire: LastFireStore,
    now: NowFn,
    /// Task 6c (spec §8): the operator's LOCAL UTC offset, re-read on every
    /// use (see [`OffsetFn`]) — a `Trigger::Schedule`'s `window` gates in
    /// this clock, not UTC.
    utc_offset: OffsetFn,
    notify: Arc<Notify>,
    cancel: CancellationToken,
}

impl RoutinesScheduler {
    /// Build a scheduler over `state`, reading `now`/the UTC offset from the
    /// injected clocks.
    pub fn new(state: Arc<RoutinesState>, now: NowFn, utc_offset: OffsetFn) -> Self {
        let notify = state.schedule_notify();
        Self {
            last_fire: LastFireStore::open(last_fire_path(&state)),
            state,
            now,
            utc_offset,
            notify,
            cancel: CancellationToken::new(),
        }
    }

    /// Spawn the loop as a tokio task and return its handle. This is what
    /// `lib.rs` `.setup()` calls.
    pub fn spawn(state: Arc<RoutinesState>, now: NowFn, utc_offset: OffsetFn) -> SchedulerHandle {
        let scheduler = Self::new(state, now, utc_offset);
        let handle = SchedulerHandle {
            cancel: scheduler.cancel.clone(),
        };
        tokio::spawn(scheduler.run());
        handle
    }

    /// The tick loop.
    ///
    /// ## The wake is ARMED before the world is read
    ///
    /// `notified()` is created at the top of every pass — BEFORE [`next_due`]
    /// reads the routines directory off disk — and only awaited afterwards. That
    /// ordering is the whole lost-wakeup fix, and it is the same discipline the
    /// tests' own `wait_for` helper uses: a `LibraryChanged` ping that lands
    /// while the loop is mid-read must be delivered to THIS pass's waiter, not
    /// dropped between the read and the await. Arming after the read leaves a
    /// window in which an operator's enable is observed by neither the read (too
    /// early) nor the wait (too late), and the routine they just armed does not
    /// fire until something else happens to wake the loop.
    ///
    /// Two more layers stand behind that ordering, because a scheduler that
    /// silently stops scheduling is the failure mode this module cannot have:
    ///
    /// * `notify_one` STORES a permit when nobody is waiting, so a ping that
    ///   arrives while the loop is firing a routine (not waiting at all) is
    ///   still delivered — to the next pass. See
    ///   `RoutinesState::notify_schedule_changed`.
    /// * [`wait`] ALWAYS has a timer, even with nothing scheduled, so the loop
    ///   re-reads the store every [`MAX_SLEEP_SECS`] regardless of whether any
    ///   ping was ever delivered. If both mechanisms above somehow failed, the
    ///   worst case degrades to a one-minute delay rather than a scheduler
    ///   parked forever.
    ///
    /// [`next_due`]: Self::next_due
    /// [`wait`]: Self::wait
    async fn run(self) {
        self.reconcile_missed_fires().await;

        loop {
            // Armed BEFORE the disk read — see the doc above. Nothing is awaited
            // until `wait`, so a ping landing during `next_due` is held, not lost.
            let notified = self.notify.notified();
            tokio::pin!(notified);

            let now = (self.now)();
            let due = self.next_due(now);

            match self.wait(due.as_ref().map(|d| d.at), notified.as_mut()).await {
                Wake::Cancelled => return,
                // A save/enable/disable/delete landed (or nothing did, and the
                // periodic re-check came around): the fleet's schedules may be
                // different now. Recompute from scratch rather than firing a set
                // that may no longer be enabled.
                Wake::LibraryChanged | Wake::Recheck => continue,
                Wake::Due => {
                    let Some(due) = due else { continue };
                    for routine in &due.routines {
                        self.fire(routine, due.at).await;
                    }
                }
            }
        }
    }

    /// Wait for the next thing worth waking for: the `target` instant if there is
    /// one, a library change, cancellation, or [`MAX_SLEEP_SECS`] elapsing.
    ///
    /// `notified` is the wake future ARMED BY THE CALLER before it read the store
    /// (see [`run`](Self::run)); waiting on it here rather than creating it here
    /// is what closes the lost-wakeup window.
    ///
    /// `target: None` means nothing is scheduled — an empty library, or one whose
    /// routines are all manual or all disabled. The loop still takes a timer: a
    /// wait with NO timer is a wait that can only ever end on a ping, and one
    /// dropped ping then parks the scheduler until the operator happens to touch
    /// the library again. The first routine an operator ever enables is exactly
    /// the case that lands in that window. One directory listing a minute on an
    /// idle station is a cheap premium against a scheduler that never fires.
    async fn wait(&self, target: Option<i64>, mut notified: Pin<&mut Notified<'_>>) -> Wake {
        loop {
            let now = (self.now)();
            let chunk = match target {
                Some(t) if now >= t => return Wake::Due,
                // Chunked, because fire instants are WALL-CLOCK while
                // `tokio::time::sleep` is MONOTONIC (see MAX_SLEEP_SECS).
                Some(t) => (t - now).min(MAX_SLEEP_SECS),
                None => MAX_SLEEP_SECS,
            };
            tokio::select! {
                biased;
                _ = self.cancel.cancelled() => return Wake::Cancelled,
                _ = &mut notified => return Wake::LibraryChanged,
                _ = tokio::time::sleep(Duration::from_secs(chunk as u64)) => {
                    if target.is_none() {
                        // Nothing to wake FOR: go re-read the world rather than
                        // sleeping another minute against a stale answer.
                        return Wake::Recheck;
                    }
                }
            }
        }
    }

    /// The earliest fire instant across every enabled, schedule-triggered
    /// routine, and every routine due at it. `None` when nothing is scheduled.
    ///
    /// Reads the store fresh (no cache): the enable sidecar and the definition
    /// files are the truth, and the loop recomputes rarely enough (once per fire,
    /// once per library change, once per [`MAX_SLEEP_SECS`]) that a directory
    /// listing is the right cost for never being able to disagree with disk.
    ///
    /// The last-fire map is read ONCE per pass, not once per routine: it is a
    /// single small file holding every routine's anchor, and re-reading it inside
    /// the loop turned one file read into N.
    fn next_due(&self, now: i64) -> Option<Due> {
        let mut due: Option<Due> = None;
        let anchors = self.last_fire.read();
        // Routines the scheduler has never seen. Their anchor is `now` (see
        // below), and it is persisted before the pass ends — an anchor recomputed
        // from a fresh `now` on every pass would push an unaligned schedule's next
        // fire out by the length of every sleep the loop ever took (module doc).
        let mut unseen: Vec<String> = Vec::new();

        for summary in self.state.store.list().into_iter().filter(|s| s.enabled) {
            let anchor = match anchors.get(&summary.routine) {
                Some(entry) => entry.last_fire_unix,
                None => {
                    unseen.push(summary.routine.clone());
                    now
                }
            };
            let Some(at) = earliest_fire(&summary.triggers, anchor, (self.utc_offset)()) else {
                continue; // manual-only, or an unparseable every/window (fail closed)
            };
            match &mut due {
                Some(d) if at < d.at => {
                    *d = Due {
                        at,
                        routines: vec![summary.routine],
                    };
                }
                // Several routines can be due at the same instant — an aligned
                // fleet collides on the hour by construction. All of them fire.
                Some(d) if at == d.at => d.routines.push(summary.routine),
                Some(_) => {}
                None => {
                    due = Some(Due {
                        at,
                        routines: vec![summary.routine],
                    })
                }
            }
        }

        if !unseen.is_empty() {
            self.seed_anchors(&unseen, now);
        }
        due
    }

    /// Persist `now` as the anchor for routines the scheduler has not seen before.
    ///
    /// Re-reads the map first and uses `or_insert`, rather than writing back the
    /// copy `next_due` was working from: the command layer's enable path
    /// ([`anchor_on_enable`]) is a second writer, and a blind write-back could
    /// clobber an anchor it wrote while this pass was computing. `or_insert` lets
    /// the enable-path anchor win, which is the correct outcome — it is the more
    /// specific statement about when the routine was armed.
    fn seed_anchors(&self, routines: &[String], now: i64) {
        let mut map = self.last_fire.read();
        for routine in routines {
            map.entry(routine.clone()).or_insert(LastFire {
                last_fire_unix: now,
                ..LastFire::default()
            });
        }
        self.last_fire.write(&map);
    }

    /// Fire one routine, or say why not. Every branch is visible: an event on the
    /// sink and a `tracing` line naming the routine.
    ///
    /// ## The enabled bit is re-read HERE, against disk, immediately before the run
    ///
    /// The due-set this routine came from was computed BEFORE the sleep. Between
    /// that computation and this instant the operator may have disabled the
    /// routine, or deleted it outright. The library-changed ping normally
    /// interrupts the sleep and forces a recompute — but "normally" is not the
    /// bar for a fire that can key a transmitter. A ping that is dropped, or one
    /// that races the wake, would otherwise have this method start a run of a
    /// routine the operator has explicitly turned OFF; on an automatic-transmit
    /// routine that is an unscheduled transmission, which is the one outcome this
    /// module exists to make impossible.
    ///
    /// So the due-set is treated as a HINT — "these were due" — and the enabled
    /// bit on disk is the authority on whether any of them may actually run. A
    /// routine that has since been disabled or deleted is dropped silently: no
    /// run, and no `ScheduleRefused` event either, because nothing was refused.
    /// The operator turned it off, and it did not fire. That is not an incident to
    /// report; it is the system working.
    async fn fire(&self, routine: &str, at: i64) {
        // The due-set is stale by construction. Disk is the authority.
        if !self.state.store.is_enabled(routine) || self.state.store.get(routine).is_none() {
            tracing::debug!(
                target: "tuxlink::routines",
                routine,
                at,
                "scheduled fire dropped: the routine is no longer enabled",
            );
            return;
        }

        // The schedule was evaluated at this instant whatever happens next — a
        // skip and a refusal are not fires the app was CLOSED for, and recording
        // them keeps the next launch's missed-fire reckoning honest.
        let now = (self.now)();
        self.last_fire.record_evaluated(routine, now);

        // No pile-up: a routine never overlaps itself.
        if self.state.is_routine_running(routine) {
            let reason = "previous run still active".to_string();
            tracing::warn!(
                target: "tuxlink::routines",
                routine,
                at,
                "scheduled fire skipped: {reason}",
            );
            // Durable, not just an event: an operator who was not listening at
            // 03:00 still needs to learn the routine cannot keep up (module doc).
            self.last_fire.record_skip(routine, at, &reason);
            self.state.emit(&RoutinesEvent::ScheduleSkipped {
                routine: routine.to_string(),
                at,
                reason,
            });
            return;
        }

        // The SAME path the operator's Run button takes: validation gate (spec
        // §10) then the transmit-consent start gate (spec §4).
        match super::commands::run_routine(&self.state, routine, serde_json::json!({})).await {
            Ok(run_id) => {
                tracing::info!(
                    target: "tuxlink::routines",
                    routine,
                    at,
                    run_id = %run_id,
                    "scheduled fire started a run",
                );
                self.state.emit(&RoutinesEvent::ScheduledFire {
                    routine: routine.to_string(),
                    run_id,
                    at,
                });
            }
            Err(e) => {
                let reason = refusal_reason(&e);
                tracing::warn!(
                    target: "tuxlink::routines",
                    routine,
                    at,
                    "scheduled fire refused: {reason}",
                );
                // Durable, not just an event (module doc): five refused 03:00
                // fires must still be answerable at 08:00, verbatim, to an
                // operator who was asleep for all of them.
                self.last_fire.record_refusal(routine, at, &reason);
                self.state.emit(&RoutinesEvent::ScheduleRefused {
                    routine: routine.to_string(),
                    at,
                    reason,
                });
            }
        }
    }

    /// Launch-time missed-fire reconciliation (spec §8).
    async fn reconcile_missed_fires(&self) {
        let now = (self.now)();
        let enabled: Vec<_> = self
            .state
            .store
            .list()
            .into_iter()
            .filter(|s| s.enabled)
            .collect();

        // The map only ever holds routines that still exist.
        let known: HashSet<String> = self
            .state
            .store
            .list()
            .into_iter()
            .map(|s| s.routine)
            .collect();
        self.last_fire.retain(&known);

        for summary in enabled {
            let Some(last) = self.last_fire.get(&summary.routine) else {
                // Never seen before: no last-fire, so no misses BY DEFINITION
                // (it was enabled, but never yet armed at a fire instant). Seed
                // the anchor here and let it schedule normally.
                self.last_fire.record_evaluated(&summary.routine, now);
                continue;
            };

            // A routine can carry several schedule triggers. The worst gap is
            // the one that matters, and a single `run_once_on_launch` anywhere
            // in the set is enough to earn the one catch-up run.
            let mut missed = 0u64;
            let mut policy = IfMissed::Skip;
            for trigger in &summary.triggers {
                let n =
                    missed_fires_windowed(trigger, last.last_fire_unix, now, (self.utc_offset)());
                if n == 0 {
                    continue;
                }
                missed = missed.max(n);
                if let Trigger::Schedule { if_missed, .. } = trigger {
                    if *if_missed == IfMissed::RunOnceOnLaunch {
                        policy = IfMissed::RunOnceOnLaunch;
                    }
                }
            }
            if missed == 0 {
                continue;
            }

            // Re-anchor at `now` either way: the backlog is being either skipped
            // or collapsed into one catch-up run, and leaving the anchor back in
            // the past would make the loop fire the whole backlog it just decided
            // not to run.
            self.last_fire.record_missed(&summary.routine, now, missed);

            let ran = policy == IfMissed::RunOnceOnLaunch;
            tracing::warn!(
                target: "tuxlink::routines",
                routine = %summary.routine,
                missed,
                ?policy,
                ran,
                "fires elapsed while the app was closed",
            );
            self.state.emit(&RoutinesEvent::MissedFires {
                routine: summary.routine.clone(),
                missed,
                policy,
                ran,
            });

            if ran {
                // ONE catch-up run (the anacron pattern) — not one per missed
                // slot. `fire` re-records the anchor at `now`, which is where
                // `record_missed` just put it, so the catch-up does not disturb
                // the cadence.
                self.fire(&summary.routine, now).await;
            }
        }
    }
}

/// The earliest fire instant across a routine's triggers, measured from
/// `anchor`. `None` if it has no schedule trigger the leaf can compute a fire
/// for — a manual routine, or one whose `every`/`window` does not parse (the
/// leaf fails CLOSED on a malformed window; a config typo stalls the routine
/// visibly on the dashboard rather than silently opening a quiet-hours TX gate).
///
/// `utc_offset_seconds` (Task 6c, `local - utc`) is threaded straight through
/// to `next_fire` — a `Trigger::Schedule`'s `window` ("22:00-06:00") gates in
/// the operator's LOCAL clock, not UTC.
fn earliest_fire(triggers: &[Trigger], anchor: i64, utc_offset_seconds: i32) -> Option<i64> {
    triggers
        .iter()
        .filter_map(|t| next_fire(t, anchor, utc_offset_seconds))
        .min()
}

/// The operator-facing text out of a [`UiError`]. Every variant carries its own
/// message; the gate's wording is what the operator needs to see (spec §4's
/// "acknowledge automatic-transmission responsibility…", spec §10's verbatim
/// finding messages), so it is passed through rather than paraphrased.
fn refusal_reason(e: &UiError) -> String {
    match e {
        UiError::NotConfigured(m) | UiError::NotFound(m) | UiError::Rejected(m) => m.clone(),
        UiError::AuthFailed { reason }
        | UiError::Transport { reason }
        | UiError::Unavailable { reason } => reason.clone(),
        UiError::Internal { detail } => detail.clone(),
        UiError::Cancelled => "cancelled".to_string(),
    }
}

// ============================================================================
// Tests — a paused tokio clock, a tempdir config, a fake action catalog.
//
// The clock seam is what makes these deterministic: `now` is DERIVED from
// tokio's `Instant`, so under `#[tokio::test(start_paused = true)]` the loop's
// `sleep`s and its notion of wall-clock time advance in lockstep, and the whole
// run of a 30-minute schedule completes in microseconds of real time.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;
    use tokio::time::Instant;
    use tuxlink_routines::action::{Action as _, ActionRegistry};
    use tuxlink_routines::fakes::FakeAction;

    use crate::routines::arbiter::RadioArbiter;
    use crate::routines::commands::{save_routine, set_routine_enabled};
    use crate::routines::events::RoutinesEventSink;
    use crate::routines::session::build_routines_state;
    use tuxlink_routines::journal::RunState;
    use tuxlink_routines::types::RoutineDef;

    /// The base wall-clock reading the tests' virtual clock starts at. Its
    /// calendar value is not load-bearing: every expectation below is derived
    /// arithmetically from BASE, mirroring the leaf scheduler's tests.
    const BASE: i64 = 1_752_400_000;

    /// A `now` fn pinned to tokio's clock: `BASE + elapsed`. Under paused time
    /// this advances exactly as tokio auto-advances through the loop's sleeps,
    /// so "the scheduler thinks it is 30 minutes later" and "tokio has slept 30
    /// virtual minutes" are the same statement.
    fn virtual_now(origin: Instant) -> NowFn {
        Arc::new(move || BASE + origin.elapsed().as_secs() as i64)
    }

    /// A recording sink that can be AWAITED — the test parks on the notify
    /// instead of polling, so tokio's auto-advance is free to jump the virtual
    /// clock straight to the scheduler's next timer. A polling loop would have
    /// been the nearest timer itself and would have crawled the clock forward
    /// 10 ms at a time.
    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<RoutinesEvent>>,
        notify: Notify,
    }

    impl RecordingSink {
        fn events(&self) -> Vec<RoutinesEvent> {
            self.events.lock().unwrap().clone()
        }

        /// Park until `pred` holds over the recorded events.
        async fn wait_for<F: Fn(&[RoutinesEvent]) -> bool>(&self, pred: F) {
            loop {
                // Arm the wait BEFORE checking, so an event emitted between the
                // check and the await cannot be missed.
                let notified = self.notify.notified();
                if pred(&self.events()) {
                    return;
                }
                notified.await;
            }
        }
    }

    impl RoutinesEventSink for RecordingSink {
        fn emit(&self, event: &RoutinesEvent) {
            self.events.lock().unwrap().push(event.clone());
            self.notify.notify_waiters();
        }
    }

    /// Wait for `pred`, failing the test after `budget` of VIRTUAL time rather
    /// than hanging forever if the scheduler never fires.
    async fn expect_event<F: Fn(&[RoutinesEvent]) -> bool>(
        sink: &RecordingSink,
        budget: Duration,
        pred: F,
        what: &str,
    ) {
        if tokio::time::timeout(budget, sink.wait_for(&pred))
            .await
            .is_err()
        {
            panic!(
                "timed out waiting for {what}; events seen: {:?}",
                sink.events()
            );
        }
    }

    fn fired(events: &[RoutinesEvent], routine: &str) -> usize {
        events
            .iter()
            .filter(
                |e| matches!(e, RoutinesEvent::ScheduledFire { routine: r, .. } if r == routine),
            )
            .count()
    }

    struct Harness {
        _dir: tempfile::TempDir,
        state: Arc<RoutinesState>,
        sink: Arc<RecordingSink>,
        origin: Instant,
        action: Arc<FakeAction>,
    }

    impl Harness {
        /// `action` is the catalog entry the test's routines call. Pass a hanging
        /// one to keep a run alive (the overlap test), a transmitting one to
        /// exercise the consent gate. An inert `local.log` is ALWAYS registered
        /// alongside it, so a test can arm a second, uninteresting routine
        /// (`Harness::arm(.., "local.log")`) whose runs just complete — the
        /// control group that proves the loop survived whatever happened to the
        /// interesting one.
        fn new(action: Arc<FakeAction>) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut reg = ActionRegistry::default();
            if action.descriptor().name != "local.log" {
                reg.register(Arc::new(FakeAction::new("local.log").ok(json!({}))));
            }
            reg.register(action.clone());
            let sink = Arc::new(RecordingSink::default());
            let sink_dyn: Arc<dyn RoutinesEventSink> = sink.clone();
            let state = Arc::new(build_routines_state(
                dir.path().to_path_buf(),
                reg,
                Arc::new(RadioArbiter::new(|| BASE)),
                sink_dyn,
            ));
            Self {
                _dir: dir,
                state,
                sink,
                origin: Instant::now(),
                action,
            }
        }

        fn log() -> Self {
            Self::new(Arc::new(FakeAction::new("local.log").ok(json!({}))))
        }

        fn start(&self) -> SchedulerHandle {
            self.start_with_offset(0)
        }

        /// Task 6c: same as [`Self::start`], but pins the scheduler's UTC
        /// offset to a fixed value instead of 0 — the seam the quiet-hours
        /// timezone tests use to prove the offset is actually threaded
        /// through the live tick loop, not just the pure `earliest_fire`
        /// helper.
        fn start_with_offset(&self, utc_offset_seconds: i32) -> SchedulerHandle {
            RoutinesScheduler::spawn(
                Arc::clone(&self.state),
                virtual_now(self.origin),
                Arc::new(move || utc_offset_seconds),
            )
        }

        fn last_fire(&self) -> LastFireStore {
            LastFireStore::open(last_fire_path(&self.state))
        }

        /// The definition body the arming helpers save.
        fn body(name: &str, every: &str, if_missed: &str, action: &str) -> String {
            json!({
                "routine": name,
                "schema_version": 1,
                "transmit_mode": "attended",
                "triggers": [{
                    "type": "schedule",
                    "every": every,
                    "if_missed": if_missed,
                }],
                "tracks": [{"name": "t", "steps": [
                    {"id": "s1", "action": action, "params": {}},
                    {"id": "e1", "control": "end"}
                ]}]
            })
            .to_string()
        }

        /// Save a scheduled routine WITHOUT enabling it — for the tests that then
        /// enable it through the command layer, which is where the enable-instant
        /// anchor is written.
        fn save(&self, name: &str, every: &str, if_missed: &str, action: &str) {
            save_routine(&self.state, &Self::body(name, every, if_missed, action))
                .expect("a parseable routine saves");
        }

        /// Save + enable a scheduled routine calling the harness's action.
        /// `enable` goes through the store directly, not the enable gate — the
        /// gate is Task 6's, already tested there, and a fleet warning (two
        /// routines colliding on the same aligned instant) is exactly what
        /// several of these tests WANT.
        fn arm(&self, name: &str, every: &str, if_missed: &str, action: &str) {
            self.save(name, every, if_missed, action);
            self.state.store.set_enabled(name, true).unwrap();
        }

        /// Same as [`Self::arm`], plus a `Trigger::Schedule.window` — the Task
        /// 6c timezone tests' seam for arming a quiet-hours-gated routine.
        fn arm_windowed(&self, name: &str, every: &str, window: &str, action: &str) {
            let body = json!({
                "routine": name,
                "schema_version": 1,
                "transmit_mode": "attended",
                "triggers": [{
                    "type": "schedule",
                    "every": every,
                    "window": window,
                    "if_missed": "skip",
                }],
                "tracks": [{"name": "t", "steps": [
                    {"id": "s1", "action": action, "params": {}},
                    {"id": "e1", "control": "end"}
                ]}]
            })
            .to_string();
            save_routine(&self.state, &body).expect("a parseable routine saves");
            self.state.store.set_enabled(name, true).unwrap();
        }

        /// Arm a routine with NO wake ping whatsoever: the definition and the
        /// enabled bit go straight to the store, so `RoutinesState::emit` — the
        /// only thing that pings the scheduler — never runs.
        ///
        /// This is the LOST-PING simulation. It is not a hypothetical: `emit`
        /// used `notify_waiters`, which stores no permit, so any ping arriving
        /// while the loop was between waits (mid-disk-read, mid-fire) was dropped
        /// exactly like this. What the scheduler does NEXT — with a change on disk
        /// that it was never told about — is the thing under test.
        fn arm_silently(&self, name: &str, every: &str, if_missed: &str, action: &str) {
            let def: RoutineDef =
                serde_json::from_str(&Self::body(name, every, if_missed, action)).unwrap();
            self.state.store.save(&def).unwrap();
            self.state.store.set_enabled(name, true).unwrap();
        }
    }

    /// The instant the FIRST `ScheduledFire` for `routine` was stamped with.
    fn fire_at(events: &[RoutinesEvent], routine: &str) -> i64 {
        events
            .iter()
            .find_map(|e| match e {
                RoutinesEvent::ScheduledFire { routine: r, at, .. } if r == routine => Some(*at),
                _ => None,
            })
            .expect("the routine fired")
    }

    // ── the loop fires ───────────────────────────────────────────────────────

    #[tokio::test(start_paused = true)]
    async fn a_schedule_fires_at_the_computed_instant() {
        let h = Harness::log();
        h.arm("half-hourly", "30m", "skip", "local.log");
        let sched = h.start();

        // Nothing has fired yet — the first fire is 30 minutes out, and no time
        // has passed.
        assert_eq!(fired(&h.sink.events(), "half-hourly"), 0);

        expect_event(
            &h.sink,
            Duration::from_secs(3600),
            |evs| fired(evs, "half-hourly") == 1,
            "the 30m schedule to fire",
        )
        .await;

        // It fired at BASE + 30m, on the interval — not early, not late.
        let at = h
            .sink
            .events()
            .iter()
            .find_map(|e| match e {
                RoutinesEvent::ScheduledFire { routine, at, .. } if routine == "half-hourly" => {
                    Some(*at)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            at,
            BASE + 1800,
            "the fire is stamped with the SCHEDULED instant"
        );
        assert!(
            !h.action.calls().is_empty(),
            "the fire started a real run that executed the routine's step"
        );

        sched.stop();
    }

    /// Task 6c (spec §8's window is authored in the OPERATOR'S clock, not
    /// UTC): drives the LIVE tick loop, not just `earliest_fire`'s pure math
    /// — proof that `next_due`/`fire` actually thread the injected
    /// `OffsetFn` end to end. BASE's UTC time-of-day is 09:46, which reads
    /// as ALREADY INSIDE "06:00-22:00" if read as UTC; at UTC-7 (Arizona,
    /// the offset this test pins) BASE's LOCAL time-of-day is 02:46 —
    /// OUTSIDE the window — so the first fire must be deferred all the way
    /// to LOCAL 06:00 (UTC 13:00), not the very next 30-minute grid instant
    /// a UTC-only reading would have produced.
    #[tokio::test(start_paused = true)]
    async fn a_windowed_schedule_gates_in_the_operators_local_clock_not_utc() {
        let h = Harness::log();
        h.arm_windowed("quiet-hours", "30m", "06:00-22:00", "local.log");
        let day_base = BASE - (BASE % 86_400);
        let expected_local_open = day_base + 13 * 3600; // LOCAL 06:00 == UTC 13:00 at -7h
        let sched = h.start_with_offset(-7 * 3600);

        expect_event(
            &h.sink,
            Duration::from_secs(4 * 3600),
            |evs| fired(evs, "quiet-hours") == 1,
            "the windowed schedule to fire once LOCAL window opens",
        )
        .await;

        let at = h
            .sink
            .events()
            .iter()
            .find_map(|e| match e {
                RoutinesEvent::ScheduledFire { routine, at, .. } if routine == "quiet-hours" => {
                    Some(*at)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            at, expected_local_open,
            "expected the fire at LOCAL 06:00 (UTC 13:00 at offset -7h); a \
             UTC-only reading of this same window string would have fired \
             around BASE+30m instead, since BASE's UTC time-of-day (09:46) \
             already looks like it's inside \"06:00-22:00\""
        );

        sched.stop();
    }

    #[tokio::test(start_paused = true)]
    async fn every_routine_due_at_the_same_instant_fires() {
        let h = Harness::log();
        // Three routines on the identical cadence and identical anchor: they all
        // come due at the same instant, and all three must fire.
        h.arm("alpha", "10m", "skip", "local.log");
        h.arm("bravo", "10m", "skip", "local.log");
        h.arm("charlie", "10m", "skip", "local.log");
        let sched = h.start();

        expect_event(
            &h.sink,
            Duration::from_secs(1800),
            |evs| {
                fired(evs, "alpha") >= 1 && fired(evs, "bravo") >= 1 && fired(evs, "charlie") >= 1
            },
            "all three co-scheduled routines to fire",
        )
        .await;

        // All at the same instant — one wake, three fires.
        let instants: HashSet<i64> = h
            .sink
            .events()
            .iter()
            .filter_map(|e| match e {
                RoutinesEvent::ScheduledFire { at, .. } => Some(*at),
                _ => None,
            })
            .collect();
        assert_eq!(
            instants,
            HashSet::from([BASE + 600]),
            "all three fired at the one computed instant"
        );

        sched.stop();
    }

    #[tokio::test(start_paused = true)]
    async fn the_cadence_repeats_rather_than_firing_once() {
        let h = Harness::log();
        h.arm("ticker", "5m", "skip", "local.log");
        let sched = h.start();

        expect_event(
            &h.sink,
            Duration::from_secs(3600),
            |evs| fired(evs, "ticker") >= 3,
            "three successive fires",
        )
        .await;
        sched.stop();
    }

    // ── disable takes effect promptly (the notify) ───────────────────────────

    #[tokio::test(start_paused = true)]
    async fn disabling_a_routine_removes_it_from_the_schedule_before_its_next_fire() {
        let h = Harness::log();
        h.arm("doomed", "30m", "skip", "local.log");
        let sched = h.start();

        // Let the loop reach its sleep, then disable — through the COMMAND layer,
        // which emits LibraryChanged and therefore pings the scheduler's notify.
        tokio::time::sleep(Duration::from_secs(60)).await;
        crate::routines::commands::set_routine_enabled(&h.state, "doomed", false, BASE + 60, 0)
            .unwrap();

        // Sleep well past the instant it WOULD have fired at (BASE + 1800).
        tokio::time::sleep(Duration::from_secs(3600)).await;
        assert_eq!(
            fired(&h.sink.events(), "doomed"),
            0,
            "a disabled routine must never fire: {:?}",
            h.sink.events()
        );

        sched.stop();
    }

    #[tokio::test(start_paused = true)]
    async fn enabling_a_routine_mid_flight_schedules_it_without_a_restart() {
        let h = Harness::log();
        let sched = h.start();
        // The scheduler starts with an EMPTY library and parks on the notify —
        // there is no fire instant to sleep toward. Arming a routine must wake it.
        tokio::time::sleep(Duration::from_secs(120)).await;
        h.arm("late-arrival", "10m", "skip", "local.log");
        h.state.notify_schedule_changed();

        expect_event(
            &h.sink,
            Duration::from_secs(1800),
            |evs| fired(evs, "late-arrival") == 1,
            "a routine armed after the scheduler started to fire",
        )
        .await;
        sched.stop();
    }

    // ── the wake is never lost ───────────────────────────────────────────────

    /// The mechanism, in isolation: a ping that arrives when nobody is parked on
    /// the notify must be HELD, not dropped.
    ///
    /// `notify_waiters` (the original mechanism) stores no permit — it wakes the
    /// waiters that exist at that instant and forgets. The scheduler is not
    /// always waiting: it spends time reading the routines directory off disk and
    /// running fires. A ping landing in either window went nowhere.
    #[tokio::test(start_paused = true)]
    async fn a_wake_ping_delivered_to_nobody_is_held_for_the_next_waiter() {
        let h = Harness::log();
        let notify = h.state.schedule_notify();

        // Nobody is waiting: this is the loop mid-read, or mid-fire.
        h.state.notify_schedule_changed();

        assert!(
            tokio::time::timeout(Duration::from_secs(30), notify.notified())
                .await
                .is_ok(),
            "a ping delivered while the scheduler was not parked must wake its next wait",
        );
    }

    /// The consequence, end to end: an operator's enable lands with its ping LOST
    /// (the store is written directly, so nothing ever pings the scheduler), and
    /// the routine must still fire.
    ///
    /// This is the review's HIGH finding as a test. The scheduler read an empty
    /// library, found nothing to schedule, and — before the fallback timer — had
    /// no instant to wake for, so it awaited a ping that had already been dropped
    /// on the floor. The first routine an operator ever enabled could silently
    /// never fire. The fallback timer means the worst case is now a one-minute
    /// delay.
    #[tokio::test(start_paused = true)]
    async fn a_routine_armed_with_no_wake_ping_at_all_still_fires() {
        let h = Harness::log();
        // An EMPTY library: the scheduler has no fire instant to sleep toward,
        // which is the case that used to park it on a ping-only wait.
        let sched = h.start();
        tokio::time::sleep(Duration::from_secs(5)).await;

        h.arm_silently("orphan", "10m", "skip", "local.log");

        expect_event(
            &h.sink,
            Duration::from_secs(3600),
            |evs| fired(evs, "orphan") == 1,
            "a routine whose wake ping was lost to fire anyway, on the periodic re-check",
        )
        .await;

        sched.stop();
    }

    // ── a stale due-set never fires a disabled routine ───────────────────────

    /// THE TRANSMIT-SAFETY TEST. The due-set is computed before the sleep; the
    /// operator disables the routine during it; the ping that would have forced a
    /// recompute is LOST (the store is written directly, so nothing emits). The
    /// timer then fires on a due-set that says to run a routine the operator has
    /// turned OFF.
    ///
    /// Nothing may run. On an automatic-transmit routine, starting that run keys a
    /// transmitter the operator explicitly disarmed — so `fire` re-reads the
    /// enabled bit from disk and drops the fire, rather than trusting the due-set
    /// it was handed.
    #[tokio::test(start_paused = true)]
    async fn a_routine_disabled_while_the_loop_sleeps_never_fires_even_if_the_ping_is_lost() {
        let h = Harness::log();
        h.arm("doomed", "10m", "skip", "local.log");
        let sched = h.start();

        // The loop is now asleep on a due-set: {doomed @ BASE+600}.
        tokio::time::sleep(Duration::from_secs(60)).await;

        // Disabled straight through the store: no emit, no ping, no recompute.
        // The sleeping loop still believes doomed is due.
        h.state.store.set_enabled("doomed", false).unwrap();

        // Well past the instant it would have fired at.
        tokio::time::sleep(Duration::from_secs(1800)).await;

        assert_eq!(
            fired(&h.sink.events(), "doomed"),
            0,
            "a disabled routine must never fire: {:?}",
            h.sink.events()
        );
        assert!(
            h.action.calls().is_empty(),
            "and not one of its steps may execute — on an automatic-transmit \
             routine this is the carrier"
        );

        // Dropped, not "refused": the operator turned it off and it did not fire.
        // That is the system working, not an incident, and it earns no event and
        // no persisted diagnostic.
        assert!(
            !h.sink.events().iter().any(|e| matches!(
                e,
                RoutinesEvent::ScheduleRefused { .. } | RoutinesEvent::ScheduleSkipped { .. }
            )),
            "a disabled routine is dropped silently, not reported as a refusal: {:?}",
            h.sink.events()
        );
        assert!(schedule_status(&h.state).is_empty());

        sched.stop();
    }

    // ── no pile-up ───────────────────────────────────────────────────────────

    #[tokio::test(start_paused = true)]
    async fn a_fire_while_the_previous_run_is_still_active_is_skipped_visibly() {
        // The action HANGS, so the first run never finishes: every subsequent
        // fire lands on top of a live run.
        let h = Harness::new(Arc::new(FakeAction::new("local.log").hang()));
        h.arm("slowpoke", "5m", "skip", "local.log");
        let sched = h.start();

        // First fire starts a run (which then hangs).
        expect_event(
            &h.sink,
            Duration::from_secs(600),
            |evs| fired(evs, "slowpoke") == 1,
            "the first fire",
        )
        .await;

        // The next fire finds it still running and SKIPS — with an event that
        // says why, not silently.
        expect_event(
            &h.sink,
            Duration::from_secs(1200),
            |evs| {
                evs.iter().any(|e| {
                    matches!(e,
                    RoutinesEvent::ScheduleSkipped { routine, reason, .. }
                        if routine == "slowpoke" && reason.contains("still active"))
                })
            },
            "the overlapping fire to be skipped",
        )
        .await;

        // And it started exactly ONE run — no pile-up.
        assert_eq!(
            fired(&h.sink.events(), "slowpoke"),
            1,
            "an overlapping fire must not start a second run"
        );

        // The skip is DURABLE, not just an event: "this routine cannot keep up
        // with its schedule" has to be answerable to an operator who opens the
        // app tomorrow, long after the event stream moved on.
        let status = schedule_status(&h.state);
        let entry = status
            .iter()
            .find(|s| s.routine == "slowpoke")
            .expect("the skip is persisted for the read path");
        let skip = entry.last_skip.as_ref().expect("with its reason");
        assert!(skip.reason.contains("still active"));
        assert_eq!(skip.at, BASE + 600, "stamped with the scheduled instant");
        assert!(entry.last_refusal.is_none(), "nothing was refused");

        sched.stop();
    }

    // ── refusals do not crash the loop ───────────────────────────────────────

    /// An AUTOMATIC, transmitting, UNACKNOWLEDGED routine (spec §4) on a
    /// schedule: every fire is refused at the gate, and the carrier is never
    /// keyed. The refusal must be VISIBLE (an event carrying the gate's verbatim
    /// message) and must not take the loop down.
    ///
    /// Which gate refuses it: the VALIDATION gate, whose `AUTO_TX_UNACKED`
    /// finding fires before `start_routine`'s consent gate can — a scheduled
    /// fire runs the same `commands::run_routine` path the operator's Run button
    /// does, and that path validates first (spec §10: "errors block enable/run").
    /// The two gates enforce the identical rule; the consent gate remains the
    /// backstop for a start that does not come through the command layer
    /// (recovery's resume path). Either way the outcome asserted here is the one
    /// that matters: no run, no transmission, one honest event.
    #[tokio::test(start_paused = true)]
    async fn an_unacknowledged_automatic_transmit_fire_is_refused_and_the_loop_keeps_running() {
        let tx = Arc::new(
            FakeAction::new("radio.tx")
                .with_capabilities(true, true, false)
                .ok(json!({"sent": true})),
        );
        let h = Harness::new(tx.clone());
        let body = json!({
            "routine": "auto-tx",
            "schema_version": 1,
            "transmit_mode": "automatic",
            "triggers": [{"type": "schedule", "every": "5m", "if_missed": "skip"}],
            "tracks": [{"name": "t", "steps": [
                {"id": "s1", "action": "radio.tx", "params": {}},
                {"id": "e1", "control": "end"}
            ]}]
        })
        .to_string();
        save_routine(&h.state, &body).unwrap();
        h.state.store.set_enabled("auto-tx", true).unwrap();
        // A second, inert routine is the control group: it proves the loop kept
        // running after the refusal rather than dying on it.
        h.arm("innocent", "5m", "skip", "local.log");

        let sched = h.start();

        expect_event(
            &h.sink,
            Duration::from_secs(600),
            |evs| {
                evs.iter().any(|e| {
                    matches!(e,
                    RoutinesEvent::ScheduleRefused { routine, reason, .. }
                        if routine == "auto-tx" && reason.contains("acknowledg"))
                })
            },
            "the gate to refuse the unacknowledged automatic-transmit fire",
        )
        .await;

        assert_eq!(
            fired(&h.sink.events(), "auto-tx"),
            0,
            "a refused fire starts no run"
        );
        assert!(
            tx.calls().is_empty(),
            "the carrier must never be keyed by a refused fire"
        );

        // The loop survived: the sibling keeps firing on its own cadence.
        expect_event(
            &h.sink,
            Duration::from_secs(1800),
            |evs| fired(evs, "innocent") >= 2,
            "the loop to keep firing other routines after a refusal",
        )
        .await;

        // And the refusal is DURABLE, with the gate's own words. An operator
        // whose station refused five 03:00 fires and who opens the app at 08:00
        // was not listening to the event stream; this record is how they find
        // out, and "the gate said no" is useless without WHY.
        let status = schedule_status(&h.state);
        let entry = status
            .iter()
            .find(|s| s.routine == "auto-tx")
            .expect("the refusal is persisted for the read path");
        let refusal = entry
            .last_refusal
            .as_ref()
            .expect("with the gate's verbatim reason");
        assert!(
            refusal.reason.contains("acknowledg"),
            "the gate's own wording, passed through rather than paraphrased: {}",
            refusal.reason
        );
        // NOT BASE + 300 (the FIRST refusal): `record_refusal` overwrites with
        // the MOST RECENT refusal (same "latest wins" contract as every other
        // `LastFireStore` write), and the wait above for "innocent" to have
        // fired TWICE does not resolve until virtual time reaches BASE + 600
        // — "auto-tx" and "innocent" share the identical unaligned 5m cadence
        // and were both first seen in the SAME `next_due` pass, so they stay
        // co-scheduled in lockstep: by the round "innocent" fires its 2nd
        // time, "auto-tx" has ALSO been refused its 2nd time, at the same
        // instant, and that is the refusal this durable record now holds.
        assert_eq!(
            refusal.at,
            BASE + 600,
            "stamped with the MOST RECENT refused fire's scheduled instant"
        );
        assert!(entry.last_skip.is_none(), "nothing was skipped");

        sched.stop();
    }

    // ── enabling anchors the cadence at the enable instant ───────────────────

    /// Re-enabling a routine after a long disable must NOT fire an immediate
    /// catch-up run.
    ///
    /// The routine's anchor used to survive its disabled period, so `next_fire`
    /// measured the cadence from an instant in the PREVIOUS enabled period —
    /// three hours ago here — computed a next fire that had long since passed,
    /// and fired the moment the operator re-armed it. That contradicts
    /// `if_missed: skip`, which is the operator saying "do not make up for lost
    /// time", and on an automatic-transmit routine it is a transmission seconds
    /// after a click that asked for no such thing.
    ///
    /// The enable path now writes the anchor, so the cadence is measured from the
    /// moment the operator armed it.
    #[tokio::test(start_paused = true)]
    async fn re_enabling_after_a_long_disable_does_not_fire_an_immediate_catch_up() {
        let h = Harness::log();
        // Through the COMMAND layer, end to end: the enable path is where the
        // anchor is written, and that is the thing under test.
        h.save("sleeper", "30m", "skip", "local.log");
        set_routine_enabled(&h.state, "sleeper", true, BASE, 0).unwrap();
        let sched = h.start();

        // Disabled a minute in, then left off for three hours — six 30m slots
        // pass with the routine disarmed.
        tokio::time::sleep(Duration::from_secs(60)).await;
        set_routine_enabled(&h.state, "sleeper", false, BASE + 60, 0).unwrap();
        tokio::time::sleep(Duration::from_secs(3 * 3600)).await;
        assert_eq!(
            fired(&h.sink.events(), "sleeper"),
            0,
            "nothing fires while the routine is disabled"
        );

        // Re-enable.
        let reenabled_at = BASE + 60 + 3 * 3600;
        set_routine_enabled(&h.state, "sleeper", true, reenabled_at, 0).unwrap();

        tokio::time::sleep(Duration::from_secs(120)).await;
        assert_eq!(
            fired(&h.sink.events(), "sleeper"),
            0,
            "a re-enable is not a catch-up: the six slots that elapsed while the \
             routine was OFF are not fires it missed: {:?}",
            h.sink.events()
        );

        // It fires on its cadence, measured from the enable instant.
        expect_event(
            &h.sink,
            Duration::from_secs(3600),
            |evs| fired(evs, "sleeper") == 1,
            "the first fire, a full interval after the re-enable",
        )
        .await;
        assert_eq!(
            fire_at(&h.sink.events(), "sleeper"),
            reenabled_at + 1800,
            "the cadence is anchored at the instant the operator enabled it"
        );

        sched.stop();
    }

    // ── missed fires at launch (spec §8) ─────────────────────────────────────

    #[tokio::test(start_paused = true)]
    async fn missed_fires_under_skip_are_recorded_and_nothing_runs() {
        let h = Harness::log();
        h.arm("overnight", "30m", "skip", "local.log");
        // The app was closed for 5 hours: the last fire is BASE - 5h → ten 30m
        // slots elapsed.
        h.last_fire().record_evaluated("overnight", BASE - 5 * 3600);

        let sched = h.start();

        expect_event(
            &h.sink,
            Duration::from_secs(60),
            |evs| {
                evs.iter().any(|e| {
                    matches!(e,
                    RoutinesEvent::MissedFires { routine, missed, policy: IfMissed::Skip, ran }
                        if routine == "overnight" && *missed == 10 && !ran)
                })
            },
            "the missed fires to be recorded",
        )
        .await;

        // NOTHING ran: skip records the misses and runs nothing.
        assert_eq!(
            fired(&h.sink.events(), "overnight"),
            0,
            "the skip policy must not run a catch-up: {:?}",
            h.sink.events()
        );
        assert!(h.action.calls().is_empty());

        // The count is persisted where the UI can read it back — an event only
        // reaches a listener that was already listening.
        let report = schedule_status(&h.state);
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].routine, "overnight");
        assert_eq!(report[0].missed, 10);

        // And the backlog it declined to run is NOT then fired by the loop: the
        // schedule was re-anchored at launch, so the next fire is a full interval
        // out (BASE + 30m), not immediately.
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert_eq!(
            fired(&h.sink.events(), "overnight"),
            0,
            "the skipped backlog must not fire right after launch"
        );

        sched.stop();
    }

    #[tokio::test(start_paused = true)]
    async fn missed_fires_under_run_once_on_launch_fire_exactly_one_catch_up_run() {
        let h = Harness::log();
        h.arm("anacron", "30m", "run_once_on_launch", "local.log");
        h.last_fire().record_evaluated("anacron", BASE - 5 * 3600);

        let sched = h.start();

        expect_event(
            &h.sink,
            Duration::from_secs(60),
            |evs| {
                evs.iter().any(|e| {
                    matches!(e,
                    RoutinesEvent::MissedFires {
                        routine, missed, policy: IfMissed::RunOnceOnLaunch, ran: true
                    } if routine == "anacron" && *missed == 10)
                })
            },
            "the run_once_on_launch reckoning",
        )
        .await;

        // Exactly ONE catch-up run — not ten.
        expect_event(
            &h.sink,
            Duration::from_secs(60),
            |evs| fired(evs, "anacron") == 1,
            "the single catch-up run",
        )
        .await;
        tokio::time::sleep(Duration::from_secs(120)).await;
        assert_eq!(
            fired(&h.sink.events(), "anacron"),
            1,
            "run_once_on_launch means ONE run, not one per missed slot: {:?}",
            h.sink.events()
        );

        sched.stop();
    }

    #[tokio::test(start_paused = true)]
    async fn a_routine_with_no_recorded_last_fire_has_no_misses() {
        let h = Harness::log();
        h.arm("fresh", "30m", "run_once_on_launch", "local.log");
        // No last-fire entry at all: first enable. Even under the anacron policy
        // there is nothing to catch up on — a routine that never had a fire
        // instant cannot have missed one.
        let sched = h.start();

        tokio::time::sleep(Duration::from_secs(120)).await;
        assert!(
            !h.sink
                .events()
                .iter()
                .any(|e| matches!(e, RoutinesEvent::MissedFires { .. })),
            "a first-enable routine has no missed fires: {:?}",
            h.sink.events()
        );
        assert_eq!(fired(&h.sink.events(), "fresh"), 0, "and no catch-up run");
        assert!(schedule_status(&h.state).is_empty());

        // Its anchor was seeded at launch, so it fires one interval later.
        expect_event(
            &h.sink,
            Duration::from_secs(3600),
            |evs| fired(evs, "fresh") == 1,
            "the normal first fire, one interval after arming",
        )
        .await;

        sched.stop();
    }

    // ── missed-fire reconciliation is window-aware (plan-4 amendment task 1b) ──
    //
    // `reconcile_missed_fires` must call the window/align-aware
    // `missed_fires_windowed`, not the window-blind `missed_fires` — otherwise
    // a windowed overnight routine reports phantom misses for hours it was
    // never due to fire in anyway. BASE's UTC time-of-day is 09:46:40 (offset
    // 0 here, so LOCAL == UTC); three hours earlier is 06:46:40. Both tests
    // below use that 3h gap so the window-blind count (3h / 30m == 6) is the
    // same known baseline to contrast against.

    #[tokio::test(start_paused = true)]
    async fn missed_fires_reconciliation_is_zero_when_the_window_was_closed_for_the_whole_gap() {
        // Window "22:00-06:00" is CLOSED for the entire 06:00-22:00 span; both
        // the gap's start (06:46:40) and its end (09:46:40 == BASE) fall
        // inside that closed span, so the window never opens across the gap
        // at all. The window-blind reckoning would report 6 slots; the
        // window-aware reconciler must report 0 — and, per `schedule_status`,
        // a zero-missed routine with nothing else to report does not even
        // appear in the read-path report.
        let h = Harness::log();
        h.arm_windowed("overnight-closed", "30m", "22:00-06:00", "local.log");
        h.last_fire()
            .record_evaluated("overnight-closed", BASE - 3 * 3600);

        let sched = h.start();

        tokio::time::sleep(Duration::from_secs(60)).await;
        assert!(
            !h.sink.events().iter().any(|e| matches!(
                e,
                RoutinesEvent::MissedFires { routine, .. } if routine == "overnight-closed"
            )),
            "a gap the window was closed for in its entirety must not be recorded as missed: {:?}",
            h.sink.events()
        );
        assert_eq!(fired(&h.sink.events(), "overnight-closed"), 0);
        assert!(
            schedule_status(&h.state).is_empty(),
            "missed == 0 means the routine has nothing to report"
        );

        sched.stop();
    }

    #[tokio::test(start_paused = true)]
    async fn missed_fires_reconciliation_reports_the_true_count_when_the_window_stayed_open() {
        // Companion to the closed-window case above: when the window is OPEN
        // for the whole gap, the window-aware count must match the naive
        // window-blind one exactly — window-awareness must not UNDER-count an
        // ordinary open-window miss. Window "06:00-22:00" is open across the
        // whole gap (06:46:40 through 09:46:40 local): 3h / 30m == 6 missed
        // slots.
        let h = Harness::log();
        h.arm_windowed("daytime-open", "30m", "06:00-22:00", "local.log");
        h.last_fire()
            .record_evaluated("daytime-open", BASE - 3 * 3600);

        let sched = h.start();

        expect_event(
            &h.sink,
            Duration::from_secs(60),
            |evs| {
                evs.iter().any(|e| {
                    matches!(e,
                    RoutinesEvent::MissedFires { routine, missed, policy: IfMissed::Skip, ran }
                        if routine == "daytime-open" && *missed == 6 && !ran)
                })
            },
            "the true (window-open) missed count to be recorded",
        )
        .await;
        assert_eq!(fired(&h.sink.events(), "daytime-open"), 0);

        let report = schedule_status(&h.state);
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].routine, "daytime-open");
        assert_eq!(report[0].missed, 6);

        sched.stop();
    }

    // ── graceful quit (Task 6c, spec §8) ────────────────────────────────────

    #[tokio::test(start_paused = true)]
    async fn cancel_all_live_runs_and_stop_cancels_the_live_run_and_stops_the_loop() {
        // The action HANGS, so the fire it starts stays live until cancelled.
        let h = Harness::new(Arc::new(FakeAction::new("local.log").hang()));
        h.arm("slowpoke", "5m", "skip", "local.log");
        let sched = h.start();

        expect_event(
            &h.sink,
            Duration::from_secs(600),
            |evs| fired(evs, "slowpoke") == 1,
            "the fire that starts the hanging run",
        )
        .await;
        assert_eq!(h.state.live_run_count(), 1, "the hung run is live");

        let cancelled = cancel_all_live_runs_and_stop(&h.state, &sched);
        assert_eq!(
            cancelled, 1,
            "cancel_all_live_runs_and_stop must cancel exactly the one live run"
        );
        expect_event(
            &h.sink,
            Duration::from_secs(60),
            |evs| {
                evs.iter().any(|e| {
                    matches!(e, RoutinesEvent::RunFinished { state: RunState::Cancelled, .. })
                })
            },
            "the RunFinished{Cancelled} event for the run cancel_all_live_runs cancelled",
        )
        .await;

        // The loop is STOPPED, not just idle: arm a new routine whose next
        // fire falls well within the time this test advances, and prove it
        // never fires — the only way that holds is if the tick loop task
        // has actually exited, not merely finished sleeping.
        h.arm("after-stop", "5m", "skip", "local.log");
        tokio::time::sleep(Duration::from_secs(3600)).await;
        assert_eq!(
            fired(&h.sink.events(), "after-stop"),
            0,
            "a stopped scheduler must never fire a routine armed after stop()"
        );

        // Idempotent: a second call finds nothing left to cancel.
        assert_eq!(
            cancel_all_live_runs_and_stop(&h.state, &sched),
            0,
            "a second call cancels nothing further"
        );
    }

    // ── stores + pure helpers ────────────────────────────────────────────────

    #[test]
    fn the_last_fire_map_round_trips_and_prunes_deleted_routines() {
        let dir = tempfile::tempdir().unwrap();
        let store = LastFireStore::open(dir.path().join("routines-last-fire.json"));

        assert!(store.get("nothing").is_none(), "an unwritten map is empty");

        store.record_evaluated("alpha", BASE);
        store.record_missed("bravo", BASE, 7);
        assert_eq!(store.get("alpha").unwrap().last_fire_unix, BASE);
        assert_eq!(store.get("bravo").unwrap().missed, 7);

        // A normal fire updates the anchor and LEAVES the missed count alone —
        // the UI keeps showing "7 missed overnight" until the next launch
        // re-reckons it.
        store.record_evaluated("bravo", BASE + 1800);
        let bravo = store.get("bravo").unwrap();
        assert_eq!(bravo.last_fire_unix, BASE + 1800);
        assert_eq!(bravo.missed, 7);

        // A deleted routine's entry is pruned rather than accumulating forever.
        store.retain(&HashSet::from(["alpha".to_string()]));
        assert!(store.get("alpha").is_some());
        assert!(store.get("bravo").is_none());
    }

    #[test]
    fn refusals_and_skips_round_trip_and_leave_the_anchor_alone() {
        let dir = tempfile::tempdir().unwrap();
        let store = LastFireStore::open(dir.path().join("routines-last-fire.json"));

        // The fire path records the evaluation instant; the refusal/skip records
        // only say WHAT happened at it. They must not move the anchor a second
        // time, or a refused fire would drift the cadence by the loop's latency.
        store.record_evaluated("mine", BASE);
        store.record_refusal("mine", BASE + 300, "no acknowledgment on file");
        store.record_skip("mine", BASE + 600, "previous run still active");

        let e = store.get("mine").unwrap();
        assert_eq!(e.last_fire_unix, BASE, "the anchor is untouched");
        let refusal = e.last_refusal.unwrap();
        assert_eq!(refusal.at, BASE + 300);
        assert_eq!(refusal.reason, "no acknowledgment on file");
        let skip = e.last_skip.unwrap();
        assert_eq!(skip.at, BASE + 600);
        assert_eq!(skip.reason, "previous run still active");
    }

    #[test]
    fn record_enabled_reanchors_and_clears_the_previous_periods_diagnostics() {
        let dir = tempfile::tempdir().unwrap();
        let store = LastFireStore::open(dir.path().join("routines-last-fire.json"));

        store.record_missed("mine", BASE, 7);
        store.record_refusal("mine", BASE, "no acknowledgment on file");
        store.record_skip("mine", BASE, "previous run still active");

        // Enabling is a fresh start: the cadence is anchored HERE (so the first
        // fire is a full interval away, not an immediate catch-up), and the
        // diagnostics of the previous enabled period — which the operator has
        // presumably just finished acting on, since they re-armed it — go with it.
        store.record_enabled("mine", BASE + 9000);

        let e = store.get("mine").unwrap();
        assert_eq!(e.last_fire_unix, BASE + 9000);
        assert_eq!(e.missed, 0);
        assert!(e.last_refusal.is_none());
        assert!(e.last_skip.is_none());
    }

    #[test]
    fn a_corrupt_last_fire_file_reads_as_empty_rather_than_failing_the_scheduler() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routines-last-fire.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let store = LastFireStore::open(path);
        assert!(store.read().is_empty());
        // "No last-fire known" is the SAFE default: it seeds an anchor and
        // invents no misses, so a corrupt file never fires a catch-up run.
        assert!(store.get("anything").is_none());
    }

    #[test]
    fn earliest_fire_takes_the_soonest_trigger_and_ignores_manual() {
        let hourly = Trigger::Schedule {
            every: "1h".into(),
            align: None,
            window: None,
            if_missed: IfMissed::Skip,
        };
        let ten_min = Trigger::Schedule {
            every: "10m".into(),
            align: None,
            window: None,
            if_missed: IfMissed::Skip,
        };
        assert_eq!(
            earliest_fire(&[hourly.clone(), ten_min], BASE, 0),
            Some(BASE + 600),
            "the soonest of several triggers wins"
        );
        assert_eq!(earliest_fire(&[Trigger::Manual], BASE, 0), None);
        assert_eq!(earliest_fire(&[], BASE, 0), None);
        assert_eq!(
            earliest_fire(&[Trigger::Manual, hourly], BASE, 0),
            Some(BASE + 3600)
        );
    }

    /// Task 6c: prove `earliest_fire` actually threads the offset through to
    /// `next_fire` at THIS layer (not just the leaf's own pure-fn tests) — a
    /// quiet-hours window at a non-zero UTC offset must gate differently than
    /// the same anchor at offset 0.
    #[test]
    fn earliest_fire_threads_the_utc_offset_into_a_windowed_schedule() {
        let windowed = Trigger::Schedule {
            every: "30m".into(),
            align: Some("hour".into()),
            window: Some("06:00-22:00".into()),
            if_missed: IfMissed::Skip,
        };
        // UTC 09:00 is LOCAL 02:00 at UTC-7 — outside "06:00-22:00" local —
        // so the fire must advance to LOCAL 06:00 (UTC 13:00).
        let day_base = BASE - (BASE % 86_400);
        let nine_am_utc = day_base + 9 * 3600;
        let arizona_offset = -7 * 3600;

        let at_offset =
            earliest_fire(std::slice::from_ref(&windowed), nine_am_utc, arizona_offset);
        let at_utc = earliest_fire(&[windowed], nine_am_utc, 0);
        assert_eq!(at_offset, Some(day_base + 13 * 3600));
        assert_ne!(
            at_offset, at_utc,
            "offset -7h and offset 0 must produce different fire instants from the \
             same anchor — otherwise `next_due` would be silently ignoring the \
             offset it was handed"
        );
    }

    #[test]
    fn refusal_reason_passes_the_gate_message_through_verbatim() {
        let msg = "routine 'auto-tx' transmits under automatic control but has no \
                   recorded acknowledgment";
        assert_eq!(refusal_reason(&UiError::Rejected(msg.into())), msg);
        assert_eq!(
            refusal_reason(&UiError::Internal {
                detail: "disk on fire".into()
            }),
            "disk on fire"
        );
    }
}
