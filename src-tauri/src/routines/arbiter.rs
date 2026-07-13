//! `RadioArbiter`: single-owner lease over a rig, shared between the human
//! operator's interactive sessions and routine-run steps (spec §9).
//!
//! **Structural precedent:** mirrors [`crate::position::arbiter::PositionArbiter`]
//! — a `Mutex`-wrapped single-owner state machine with a proptest invariant —
//! but this arbiter is *async*, because a routine step legitimately wants to
//! wait its turn (`BusyPolicy::Wait`) rather than fail immediately.
//!
//! **Lock discipline:** follows the house rule documented at
//! [`crate::modem_status::TransportOwner`] — the internal `std::sync::Mutex`
//! guarding per-rig state is NEVER held across an `.await` point. Every method
//! below either (a) does its work entirely inside one short, synchronous
//! critical section, or (b) computes an outcome inside that critical section,
//! drops the guard, and only then `.await`s (the FIFO wait path in
//! [`RadioArbiter::acquire`]).
//!
//! **FIFO wait queue:** built from `tokio::sync::oneshot` channels, not a
//! `tokio::sync::Semaphore` or a second async mutex — a waiter's slot in
//! [`RigState::waiters`] carries its own `Holder`/`CancellationToken`, which
//! the hand-off logic in [`RadioLease`]'s `Drop` impl (and `promote_next_locked`,
//! its shared free-function core) needs when it promotes the next waiter to
//! active holder. Every state transition (enqueue, grant, give-up,
//! evict) happens inside the ONE lock, so "at most one holder per rig" holds
//! even under concurrent `acquire`/drop/`operator_take` calls — see the
//! `seq`-based staleness check in [`RadioLease`]'s `Drop` impl for the one
//! genuine race this design has to close (a lease that got pre-empted by
//! [`RadioArbiter::interactive_acquire`] while its owner was still mid-flight).
//!
//! **Grant handoff is structurally drop-safe:** the FIFO wait channel carries
//! the [`RadioLease`] itself (`oneshot::Sender<RadioLease>`/`Receiver<RadioLease>`),
//! not a bare `()` wakeup. `promote_next_locked` constructs the lease and
//! attempts to send it to the front waiter, all under the ONE lock; the
//! immediate-grant path in [`RadioArbiter::acquire`] constructs its lease the
//! same way, synchronously, before ever returning it. This means there is no
//! window where the arbiter's bookkeeping (`RigState::active`) can say
//! "holder X has the rig" while no live `RadioLease` exists anywhere to
//! eventually give it back — every reachable interleaving (successful
//! receive, receiver dropped before receiving, receiver dropped after
//! receiving) funnels through `RadioLease::drop`. See the interleaving
//! walk-through in `promote_next_locked`'s doc and in each caller that
//! invokes it.
//!
//! **Pause vs. cancel — two distinct signals per active holder:** each
//! [`ActiveHolder`]/[`RadioLease`] pair carries TWO tokens. `cancel` is the
//! caller's own acquire-side token (shared via `clone()`, so cancelling it
//! also cancels the caller's copy — used for hard eviction, e.g.
//! [`RadioArbiter::interactive_acquire`]). `pause` is a `child_token()` of
//! `cancel`, minted fresh at grant time — used by
//! [`RadioArbiter::operator_take`] to ask a run to yield gracefully without
//! being indistinguishable from an outright cancel. See
//! [`RadioArbiter::operator_take`]'s doc for the full contract.
//!
//! Spec: `docs/superpowers/specs/2026-07-13-routines-design.md` §9. Plan:
//! `docs/superpowers/plans/2026-07-13-routines-02-actions-arbiter-mount.md`
//! Task 2.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use tuxlink_routines::types::BusyPolicy;

/// Who is holding (or asking for) a rig lease.
///
/// `Interactive` is the human operator — a first-class holder per spec §9;
/// a routine step never preempts it (`acquire` for a `Run` holder against a
/// rig held by `Interactive` behaves exactly like contending against another
/// `Run` — `Wait` queues, `Fail` errors — it is `interactive_acquire` and
/// `operator_take`, not `acquire`, that carry the operator's preemption
/// privilege). `Run` identifies the routine run + step asking for or holding
/// the rig, matching the engine's own run/step identifiers so a rendered
/// `held_by` string (see [`render_holder`]) is directly actionable in the UI
/// and journals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Holder {
    Interactive,
    Run { run_id: String, step: String },
}

/// Renders a [`Holder`] EXACTLY as spec §9 / the plan's test contract
/// requires: `"operator (interactive)"` or `"run <id> step <step>"`. Used for
/// every `held_by` field on [`ArbiterError`] and [`HolderInfo`] — callers
/// (journals, UI, `tracing` events) get this string verbatim, never a
/// paraphrase.
pub fn render_holder(holder: &Holder) -> String {
    match holder {
        Holder::Interactive => "operator (interactive)".to_string(),
        Holder::Run { run_id, step } => format!("run {run_id} step {step}"),
    }
}

/// Point-in-time snapshot of who holds a rig, for [`RadioArbiter::status`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HolderInfo {
    pub holder: Holder,
    pub held_for_s: u64,
    /// Whether [`RadioArbiter::operator_take`] has signalled this holder to
    /// pause (its dedicated `pause` token is cancelled). Lets the UI render
    /// "operator is waiting for this run to yield" distinctly from a plain
    /// busy/held state. Always `false` for an `Interactive` holder — the
    /// operator never calls `operator_take` on themself.
    pub pause_requested: bool,
}

/// Errors from [`RadioArbiter::acquire`]. `held_by` is always the
/// [`render_holder`] string of whoever is (or was, at the moment of
/// giving up) blocking the caller — never paraphrased, per Global
/// Constraints.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArbiterError {
    /// `BusyPolicy::Fail` and the rig was already held.
    #[error("rig busy — held by {held_by} for {held_for_s}s")]
    Busy { held_by: String, held_for_s: u64 },
    /// `BusyPolicy::Wait` and the caller's `timeout` elapsed before the rig
    /// became free.
    #[error("timed out after {waited_s}s waiting for rig — held by {held_by}")]
    Timeout { held_by: String, waited_s: u64 },
    /// The caller's `cancel` token fired while queued (`Wait` policy only —
    /// `Fail` policy never queues, so it never observes cancellation).
    #[error("cancelled while waiting for rig")]
    Cancelled,
}

/// The currently-installed holder of a rig.
struct ActiveHolder {
    holder: Holder,
    /// Clone of the acquire-side [`CancellationToken`] the holder passed in.
    /// `CancellationToken::clone` shares the same underlying cancellation
    /// signal (not a `child_token`, which is one-way parent→child), so
    /// cancelling THIS clone also cancels the caller's own token — this is
    /// the HARD-eviction mechanism, used by
    /// [`RadioArbiter::interactive_acquire`] (never by
    /// [`RadioArbiter::operator_take`], which uses `pause` below — cancelling
    /// `cancel` is indistinguishable from "your run itself was cancelled,"
    /// which is a different signal than "please pause and yield").
    cancel: CancellationToken,
    /// Dedicated pause-request signal, `cancel.child_token()` minted at
    /// GRANT time (not enqueue time — see `promote_next_locked`).
    /// [`RadioArbiter::operator_take`] cancels THIS token, never `cancel`.
    /// Being a child of `cancel` means a hard-stop of `cancel` (run
    /// cancellation, or a later `interactive_acquire` eviction) also marks
    /// `pause` cancelled as a harmless side effect — a holder being
    /// hard-stopped does not need to separately notice a pause request.
    /// Cancelling `pause` does NOT propagate up to `cancel` — that one-way
    /// direction is the whole point: it is how a holder tells "pause" apart
    /// from "cancel."
    pause: CancellationToken,
    /// `now()` at the moment this holder was installed (grant time, not
    /// enqueue time) — the basis for `held_for_s`.
    since: i64,
    /// Unique id minted by [`RadioArbiter::next_seq`] at grant time. Lets
    /// [`RadioLease::drop`] tell whether it is STILL the bookkeeping's
    /// notion of the active holder, or was silently pre-empted by
    /// [`RadioArbiter::interactive_acquire`] — see that method's doc.
    seq: u64,
}

/// A queued `Wait`-policy request, FIFO-ordered by `VecDeque` position.
struct Waiter {
    holder: Holder,
    cancel: CancellationToken,
    /// Carries the [`RadioLease`] ITSELF (not a bare `()` wakeup) — minted by
    /// [`promote_next_locked`] when this waiter becomes the active holder.
    /// This is the CRITICAL structural property that closes the
    /// promote-then-drop leak: the receiving `acquire` call never has to
    /// separately "confirm and construct" a lease after being woken (the
    /// prior design's vulnerable step) — it just takes ownership of the
    /// value the channel already delivered. See `promote_next_locked`'s doc
    /// for the full interleaving walk-through.
    tx: oneshot::Sender<RadioLease>,
    /// Matches the `seq` [`promote_next_locked`] will stamp onto the
    /// resulting [`ActiveHolder`] — lets a timed-out/cancelled waiter find
    /// (and remove, or discover-already-granted) its own queue entry without
    /// needing `PartialEq` on `Holder` to disambiguate same-named waiters.
    seq: u64,
}

#[derive(Default)]
struct RigState {
    active: Option<ActiveHolder>,
    waiters: VecDeque<Waiter>,
}

/// Mutex-wrapped single-owner state machine over a rig identifier → lease.
/// Multiple rigs share one arbiter instance (Task 5 `.manage()`s a single
/// `Arc<RadioArbiter>`); each rig's state is independent (a distinct
/// `RigState` entry), so contention on one rig never blocks another.
pub struct RadioArbiter {
    now: fn() -> i64,
    /// `Arc`-wrapped so [`RadioLease`] (returned BY VALUE, with no lifetime
    /// parameter, from `acquire`/kept alive across a run's `.await`s) can
    /// hold its own handle to the shared state, independent of any borrow of
    /// `&RadioArbiter` — matching how `Arc<RadioArbiter>` itself will be
    /// stored in `RoutinesState` (plan Task 5).
    rigs: Arc<Mutex<HashMap<String, RigState>>>,
    seq: AtomicU64,
}

/// Outcome of the synchronous, lock-held prefix of `acquire` — computed and
/// returned BEFORE the lock is dropped, so the subsequent `.await` (FIFO wait
/// path only) never happens with the lock held.
enum AcquireOutcome {
    /// Rig was free; `seq` and `pause` token of the just-installed
    /// [`ActiveHolder`] — passed through so the caller can build its
    /// [`RadioLease`] synchronously, matching how `promote_next_locked`
    /// builds the queued-path's lease.
    Immediate(u64, CancellationToken),
    /// `BusyPolicy::Fail` and the rig was held.
    Busy { held_by: String, held_for_s: u64 },
    /// `BusyPolicy::Wait`; enqueued with this `seq`, waiting on `rx` for the
    /// [`RadioLease`] itself (not a bare wakeup — see `Waiter::tx`'s doc).
    Queued(oneshot::Receiver<RadioLease>, u64),
}

impl RadioArbiter {
    pub fn new(now: fn() -> i64) -> Self {
        Self {
            now,
            rigs: Arc::new(Mutex::new(HashMap::new())),
            seq: AtomicU64::new(0),
        }
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }

    fn lease(&self, rig: &str, seq: u64, pause: CancellationToken) -> RadioLease {
        RadioLease {
            rigs: self.rigs.clone(),
            now: self.now,
            rig: rig.to_string(),
            seq,
            pause,
        }
    }

    /// Acquires `rig` for `holder`. If free, returns immediately. If held:
    /// `BusyPolicy::Fail` errors immediately naming the current holder;
    /// `BusyPolicy::Wait` joins the FIFO queue and resolves when it becomes
    /// the active holder, `timeout` elapses, or `cancel` fires — whichever
    /// happens first.
    ///
    /// The returned [`RadioLease`] releases (and hands off to the next
    /// waiter, if any) on `Drop` — callers do not call a separate `release`.
    pub async fn acquire(
        &self,
        rig: &str,
        holder: Holder,
        policy: BusyPolicy,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<RadioLease, ArbiterError> {
        let enqueued_at = (self.now)();

        let outcome = {
            let mut rigs = self.rigs.lock().unwrap();
            let state = rigs.entry(rig.to_string()).or_default();
            if state.active.is_none() {
                let seq = self.next_seq();
                let since = (self.now)();
                // Minted HERE, at grant time, not stored anywhere before
                // this — the pause/cancel distinction only exists once
                // there is an active holder to distinguish signals for.
                let pause = cancel.child_token();
                state.active = Some(ActiveHolder {
                    holder: holder.clone(),
                    cancel: cancel.clone(),
                    pause: pause.clone(),
                    since,
                    seq,
                });
                AcquireOutcome::Immediate(seq, pause)
            } else {
                // Extract owned data from the current holder BEFORE any
                // further mutation of `state`, so no borrow of
                // `state.active` is still alive once we touch
                // `state.waiters` below (keeps this block trivially
                // borrow-checkable without relying on disjoint-field NLL
                // subtleties).
                let (held_by, since) = match state.active.as_ref() {
                    Some(active) => (render_holder(&active.holder), active.since),
                    // Unreachable: this is the `else` arm of `state.active.is_none()`,
                    // so the holder is present. A `match` (not `.unwrap()`) states
                    // that to clippy without a redundant `is_none`→`unwrap` pattern.
                    None => unreachable!("active holder is Some in the busy branch"),
                };
                match policy {
                    BusyPolicy::Fail => {
                        let held_for_s = ((self.now)() - since).max(0) as u64;
                        AcquireOutcome::Busy {
                            held_by,
                            held_for_s,
                        }
                    }
                    BusyPolicy::Wait => {
                        let seq = self.next_seq();
                        let (tx, rx) = oneshot::channel::<RadioLease>();
                        state.waiters.push_back(Waiter {
                            holder: holder.clone(),
                            cancel: cancel.clone(),
                            tx,
                            seq,
                        });
                        AcquireOutcome::Queued(rx, seq)
                    }
                }
            }
        }; // std Mutex guard dropped here — nothing below holds it across `.await`.

        match outcome {
            AcquireOutcome::Immediate(seq, pause) => {
                tracing::info!(
                    target: "tuxlink::routines::arbiter",
                    rig,
                    holder = %render_holder(&holder),
                    "acquired",
                );
                Ok(self.lease(rig, seq, pause))
            }
            AcquireOutcome::Busy {
                held_by,
                held_for_s,
            } => {
                tracing::info!(
                    target: "tuxlink::routines::arbiter",
                    rig,
                    held_by = %held_by,
                    held_for_s,
                    "busy — fail policy",
                );
                Err(ArbiterError::Busy {
                    held_by,
                    held_for_s,
                })
            }
            AcquireOutcome::Queued(rx, seq) => {
                tracing::info!(
                    target: "tuxlink::routines::arbiter",
                    rig,
                    holder = %render_holder(&holder),
                    "queued — wait policy",
                );
                tokio::select! {
                    biased;
                    res = rx => match res {
                        Ok(lease) => {
                            // `lease` is the SAME `RadioLease` `promote_next_locked`
                            // constructed and installed as `state.active` under
                            // its own lock — we just take ownership of it here,
                            // never reconstruct. This is the fix for the
                            // promote-then-drop leak: there is no gap between
                            // "arbiter says we're the holder" and "a live
                            // RadioLease exists" — they are established
                            // atomically, under the SAME lock, by the promoter.
                            tracing::info!(
                                target: "tuxlink::routines::arbiter",
                                rig,
                                holder = %render_holder(&holder),
                                "acquired from queue",
                            );
                            Ok(lease)
                        }
                        Err(_) => {
                            // Sender dropped without granting (arbiter torn
                            // down mid-wait, or a bug in `promote_next_locked`
                            // that skipped us). Treat as cancelled rather than
                            // hanging.
                            tracing::info!(
                                target: "tuxlink::routines::arbiter",
                                rig,
                                "queue sender dropped without granting",
                            );
                            Err(ArbiterError::Cancelled)
                        }
                    },
                    _ = tokio::time::sleep(timeout) => {
                        let waited_s = ((self.now)() - enqueued_at).max(0) as u64;
                        let held_by = self.reclaim_after_giveup(rig, seq);
                        tracing::info!(
                            target: "tuxlink::routines::arbiter",
                            rig,
                            held_by = %held_by,
                            waited_s,
                            "timed out waiting",
                        );
                        Err(ArbiterError::Timeout { held_by, waited_s })
                    }
                    _ = cancel.cancelled() => {
                        self.reclaim_after_giveup(rig, seq);
                        tracing::info!(target: "tuxlink::routines::arbiter", rig, "cancelled while waiting");
                        Err(ArbiterError::Cancelled)
                    }
                }
            }
        }
    }

    /// Cleans up after a waiter gives up (timeout or cancel): removes it from
    /// the queue if it is still there, returning the CURRENT holder's
    /// rendered name (what was blocking it). If it is NOT in the queue
    /// anymore — a concurrent [`RadioLease::drop`]/`release` promoted it to
    /// active holder in the exact instant it gave up — self-heals by
    /// immediately releasing that just-granted lease (promoting whoever is
    /// next) rather than leaking a held-forever rig. This tie is only
    /// reachable on a multi-thread Tokio runtime (the tests in this module
    /// run current-thread, where it cannot occur); it is handled here purely
    /// as a correctness backstop for Task 5's eventual runtime configuration.
    ///
    /// The self-heal branch reports the holder name AT THE MOMENT OF
    /// TIMEOUT/CANCEL — i.e. what `held_by` describes for the caller's own
    /// `ArbiterError` — captured BEFORE `promote_next_locked` mutates
    /// `state.active` to whoever comes next. (Previously this reported the
    /// POST-release holder, which answers a different, more confusing
    /// question for a "what blocked you" error string.)
    fn reclaim_after_giveup(&self, rig: &str, seq: u64) -> String {
        let mut rejected: Vec<RadioLease> = Vec::new();
        let held_by = {
            let mut rigs = self.rigs.lock().unwrap();
            let Some(state) = rigs.get_mut(rig) else {
                return "unknown (no rig state)".to_string();
            };
            let before = state.waiters.len();
            state.waiters.retain(|w| w.seq != seq);
            if state.waiters.len() < before {
                // Still genuinely queued — we removed our own entry before
                // it was ever promoted. Nothing to self-heal.
                state
                    .active
                    .as_ref()
                    .map(|a| render_holder(&a.holder))
                    .unwrap_or_else(|| "unknown (no active holder)".to_string())
            } else if state.active.as_ref().map(|a| a.seq) == Some(seq) {
                // We were already promoted (multi-thread tie) but our
                // `select!` above discarded that outcome. Capture "who's
                // holding it" (i.e. us) BEFORE the self-heal below hands it
                // to whoever is next.
                let at_timeout_holder = render_holder(&state.active.as_ref().unwrap().holder);
                state.active = None;
                rejected = promote_next_locked(state, &self.rigs, rig, self.now);
                at_timeout_holder
            } else {
                // Neither still-queued nor the current active holder: some
                // OTHER concurrent event already resolved our seq's fate
                // before we got the lock (e.g. our own `rx` was dropped by
                // `select!`'s cleanup, and tokio's oneshot drops any
                // buffered-but-unreceived `RadioLease`, which self-heals via
                // its own `Drop` impl on a different call stack). Reporting
                // "unknown" here is a correctness-safe fallback — reclaiming
                // again would double-release — not a bug. Only reachable on
                // a multi-thread Tokio runtime.
                "unknown".to_string()
            }
        }; // lock guard dropped here.
           // `rejected` leases (if any) are dropped OUTSIDE the lock:
           // `RadioLease::drop` re-locks `self.rigs`, which would deadlock
           // (std::sync::Mutex is not reentrant) while the guard above is held.
        drop(rejected);
        held_by
    }

    /// Synchronous, infallible operator acquire — the human operator is a
    /// first-class holder (spec §9) and is never made to wait behind a
    /// routine. If `rig` is currently held by a `Run`, this forcibly evicts
    /// it — cancelling its acquire-side `cancel` token (a HARD stop, distinct
    /// from [`Self::operator_take`]'s graceful `pause` signal — see that
    /// method's doc for the full contract) — before installing `Interactive`.
    /// If it is held by a STALE `Interactive` record (e.g. a prior session
    /// that never called `interactive_release`), that record is silently
    /// replaced too — two operator sessions never contend with each other.
    ///
    /// **This is the immediate/unconditional half of a UI "take the radio"
    /// flow.** The graceful half is [`Self::operator_take`]: ask the run to
    /// yield at its own step boundary, and wait. The two compose: a UI
    /// button can call `operator_take` first and give the run a beat to
    /// release cooperatively, then fall back to `interactive_acquire` — which
    /// seizes the rig outright, per spec §9 — if the operator does not want
    /// to wait, or the run never yields. Calling `interactive_acquire`
    /// directly, with no prior `operator_take`, is also valid — it always
    /// eventually succeeds specifically because it does not wait for
    /// cooperation.
    ///
    /// Unlike `acquire`, this does NOT return a [`RadioLease`] — the existing
    /// UI session code this is wired into (plan Task 5: VARA/ARDOP/Direwolf
    /// connect paths) already manages its own connect/disconnect lifecycle
    /// imperatively; forcing it to thread an RAII guard through would be a
    /// bigger, riskier change than pairing this with
    /// [`Self::interactive_release`] at the existing disconnect call site.
    ///
    /// Queued `Wait`-policy waiters are left untouched — they keep waiting
    /// (and will time out on their own budget, or get their turn after
    /// [`Self::interactive_release`]); this method only evicts the ACTIVE
    /// holder, it does not flush the queue.
    pub fn interactive_acquire(&self, rig: &str) {
        let mut rigs = self.rigs.lock().unwrap();
        let state = rigs.entry(rig.to_string()).or_default();
        if let Some(prev) = state.active.take() {
            if let Holder::Run { run_id, step } = &prev.holder {
                prev.cancel.cancel();
                tracing::info!(
                    target: "tuxlink::routines::arbiter",
                    rig,
                    run_id = %run_id,
                    step = %step,
                    "interactive_acquire evicted run holder",
                );
            }
            // `prev` (and its `cancel`/`pause`/`seq`) is dropped here. Any
            // `RadioLease` the evicted run is still holding will find its
            // `seq` no longer matches `state.active` on its own `Drop`, so
            // it correctly no-ops instead of releasing OUR new hold.
        }
        let seq = self.next_seq();
        let since = (self.now)();
        // Interactive holders never route through `acquire`'s RadioLease
        // channel and `operator_take` never targets `Interactive` (see that
        // method's doc), so this `pause` token is never observed by
        // anything — a fresh, standalone token is correct (never cancelled).
        state.active = Some(ActiveHolder {
            holder: Holder::Interactive,
            cancel: CancellationToken::new(),
            pause: CancellationToken::new(),
            since,
            seq,
        });
        tracing::info!(target: "tuxlink::routines::arbiter", rig, holder = "operator (interactive)", "acquired");
    }

    /// Releases an interactive hold installed by [`Self::interactive_acquire`]
    /// and promotes the next FIFO waiter, if any. A no-op if `rig` is not
    /// currently held by `Interactive` (already released, or pre-empted by a
    /// later `interactive_acquire` call) — this never clobbers someone else's
    /// hold.
    pub fn interactive_release(&self, rig: &str) {
        let rejected = {
            let mut rigs = self.rigs.lock().unwrap();
            let Some(state) = rigs.get_mut(rig) else {
                return;
            };
            let is_interactive = matches!(
                state.active.as_ref().map(|a| &a.holder),
                Some(Holder::Interactive)
            );
            if !is_interactive {
                return;
            }
            state.active = None;
            tracing::info!(target: "tuxlink::routines::arbiter", rig, "released (interactive)");
            promote_next_locked(state, &self.rigs, rig, self.now)
        }; // lock guard dropped here.
           // See `reclaim_after_giveup`'s doc: rejected leases must be dropped
           // OUTSIDE the lock — `RadioLease::drop` re-locks `self.rigs`.
        drop(rejected);
    }

    /// Operator request for a run holding the radio to pause: cancels the
    /// ACTIVE run-holder's dedicated `pause` token — a `child_token()` of the
    /// run's own acquire-side `cancel` token, minted at grant time — NEVER
    /// the acquire-side `cancel` token itself.
    ///
    /// **Contract — pause vs. cancel:** these are two DISTINCT signals a
    /// holder can observe. Cancelling `cancel` (which only
    /// [`Self::interactive_acquire`]'s hard eviction does) means "your run
    /// itself is being cancelled/evicted — abandon your work and error out."
    /// Cancelling `pause` (what THIS method does) means "release the radio
    /// at your next convenient step boundary, then re-acquire later" — a
    /// graceful, resumable request, not a hard stop. The holding action is
    /// expected to observe [`RadioLease::pause_requested`] (Task 4's action
    /// glue implements the observe/release/re-acquire loop for real; this
    /// method only fires the signal — it does not itself touch `state.active`
    /// or the queue, and does not wait for the holder to act).
    ///
    /// **A UI "take the radio" flow composes two calls:** `operator_take`
    /// (this method — graceful, asks nicely, the run yields on its own
    /// schedule) followed by [`Self::interactive_acquire`] (immediate,
    /// unconditional evict-and-claim — what actually SEIZES the rig per spec
    /// §9, for when the operator does not want to wait, or the run never
    /// yields). `operator_take` alone never forces anything to give up the
    /// rig; only `interactive_acquire` does that.
    ///
    /// Deliberately does NOT touch `state.active` or the queue itself — the
    /// run still "holds" the rig, from the arbiter's bookkeeping, until its
    /// own [`RadioLease`] is dropped (normal release path, which promotes the
    /// next waiter exactly as any other release does).
    ///
    /// Returns `true` if a run holder was found and signalled, `false`
    /// otherwise (rig free, or held by `Interactive` — an interactive holder
    /// is NEVER affected by `operator_take`; the operator does not need to
    /// take the radio from themself).
    pub fn operator_take(&self, rig: &str) -> bool {
        let rigs = self.rigs.lock().unwrap();
        let Some(state) = rigs.get(rig) else {
            return false;
        };
        let Some(active) = &state.active else {
            return false;
        };
        match &active.holder {
            Holder::Interactive => false,
            Holder::Run { run_id, step } => {
                active.pause.cancel();
                tracing::info!(
                    target: "tuxlink::routines::arbiter",
                    rig,
                    run_id = %run_id,
                    step = %step,
                    "operator_take: cancelled run holder's pause token",
                );
                true
            }
        }
    }

    /// Snapshot of the current holder, if any. `held_for_s` is computed
    /// fresh against `now()` on every call (not cached at grant time).
    pub fn status(&self, rig: &str) -> Option<HolderInfo> {
        let rigs = self.rigs.lock().unwrap();
        let state = rigs.get(rig)?;
        let active = state.active.as_ref()?;
        let held_for_s = ((self.now)() - active.since).max(0) as u64;
        Some(HolderInfo {
            holder: active.holder.clone(),
            held_for_s,
            pause_requested: active.pause.is_cancelled(),
        })
    }
}

/// Pops the front FIFO waiter (if any), constructs its [`RadioLease`], and
/// attempts to hand it off through the waiter's oneshot channel, installing
/// it as the new active holder ONLY on a successful send. If the front
/// waiter's receiver was already dropped (its `acquire` future was torn down
/// some other way than through `reclaim_after_giveup` — e.g. the enclosing
/// task was aborted), the send fails, `state.active` is left untouched for
/// that waiter, and the next one in the queue is tried instead — so the rig
/// is never left "held" by bookkeeping alone with no live lease anywhere to
/// release it.
///
/// Caller MUST already hold the lock on `state`'s `RigState` (this is a free
/// function, not a method, specifically so [`RadioLease::drop`] — which
/// cannot call an async or self-borrowing method mid-`Drop` — can invoke it
/// directly under its own lock guard).
///
/// **Returns any leases that failed to be delivered** (receiver already
/// gone) so the CALLER can drop them AFTER releasing the lock — a rejected
/// lease's `seq` never matches `state.active` (since it was never installed
/// there), so dropping it is guaranteed to no-op via the seq-staleness check
/// in [`RadioLease`]'s `Drop` impl, but the drop still unconditionally
/// ACQUIRES `self.rigs`'s lock to perform that check, which would deadlock
/// (`std::sync::Mutex` is not reentrant) if attempted while this function's
/// caller is still holding the very same lock. Every call site follows the
/// same pattern: compute inside the locked block, drop the guard, THEN drop
/// the returned `Vec`.
///
/// ## Interleaving walk-through (the CRITICAL fix this function embodies)
///
/// Previously this function sent a bare `()` wakeup and installed
/// `state.active` unconditionally BEFORE knowing whether the send would even
/// succeed — the ActiveHolder bookkeeping and the actual `RadioLease` that
/// alone can release it were two independent, un-synchronized events. If the
/// receiving `acquire` future was torn down between "wakeup sent" and "next
/// poll actually reads `self.rigs` to build a `RadioLease`" (a task `abort()`,
/// not cooperative cancellation), the bookkeeping said "holder installed" and
/// no lease EVER existed to release it — a permanent wedge.
///
/// Now the value that flows through the channel — the payload the `Sender`
/// actually delivers — IS the `RadioLease`, and `state.active` is only ever
/// set alongside a successful send, under the same lock, atomically:
///
/// 1. **Send succeeds, receiver later polls normally.** `acquire`'s
///    `res = rx => Ok(lease)` arm takes ownership and returns it directly —
///    no reconstruction step exists to skip.
/// 2. **Send succeeds, but the receiving future/task is dropped/aborted
///    before ever being polled again.** tokio's `oneshot::Receiver`, when
///    dropped while still holding an unconsumed sent value, drops that
///    value as part of its own cleanup. Dropping the buffered `RadioLease`
///    runs `RadioLease::drop`, which re-locks `self.rigs`, finds its `seq`
///    still matches `state.active` (nothing else touched it), releases, and
///    promotes the next waiter — exactly the self-heal the CRITICAL finding
///    asked for. This drop happens on whatever context tears down the
///    receiver, strictly AFTER this function (and the lock it required) has
///    already returned — no deadlock.
/// 3. **Send fails — receiver already gone before we even tried.** `tx.send`
///    returns `Err(lease)` synchronously (no waiting); we never touch
///    `state.active` for this waiter, push the rejected lease onto the
///    return `Vec`, and loop to try the next waiter. The rejected lease's
///    `Drop` (run later, outside this function's lock, by the caller) is a
///    guaranteed no-op (seq never matched `state.active`).
///
/// Every reachable interleaving funnels through `RadioLease::drop` exactly
/// once for exactly the lease that got installed as `state.active` (or zero
/// times, for a lease that was never installed) — there is no path that
/// leaves an active holder with no corresponding live-or-already-dropped
/// lease.
#[must_use]
fn promote_next_locked(
    state: &mut RigState,
    rigs: &Arc<Mutex<HashMap<String, RigState>>>,
    rig: &str,
    now: fn() -> i64,
) -> Vec<RadioLease> {
    let mut rejected = Vec::new();
    while let Some(waiter) = state.waiters.pop_front() {
        let Waiter {
            holder,
            cancel,
            tx,
            seq,
        } = waiter;
        let rendered = render_holder(&holder);
        // Minted HERE, at grant/promotion time — matching the immediate-grant
        // path in `acquire` — not at enqueue time, per the pause/cancel
        // contract documented on `ActiveHolder::pause`.
        let pause = cancel.child_token();
        let lease = RadioLease {
            rigs: rigs.clone(),
            now,
            rig: rig.to_string(),
            seq,
            pause: pause.clone(),
        };
        match tx.send(lease) {
            Ok(()) => {
                state.active = Some(ActiveHolder {
                    holder,
                    cancel,
                    pause,
                    since: now(),
                    seq,
                });
                tracing::info!(target: "tuxlink::routines::arbiter", holder = %rendered, "promoted from queue");
                return rejected;
            }
            Err(lease) => {
                tracing::debug!(
                    target: "tuxlink::routines::arbiter",
                    "queued waiter's receiver already gone; trying next in queue",
                );
                rejected.push(lease);
            }
        }
    }
    rejected
}

/// RAII lease over a rig. Dropping it releases the hold and promotes the next
/// FIFO waiter, if any — UNLESS this lease was silently pre-empted by
/// [`RadioArbiter::interactive_acquire`] (its `seq` no longer matches the
/// arbiter's notion of the active holder), in which case `Drop` is a no-op:
/// the pre-emption already released (by replacing) the hold, and there is
/// nothing left for this lease to give back.
pub struct RadioLease {
    rigs: Arc<Mutex<HashMap<String, RigState>>>,
    now: fn() -> i64,
    rig: String,
    seq: u64,
    /// This lease's clone of its [`ActiveHolder`]'s dedicated pause signal —
    /// see [`Self::pause_requested`] and [`RadioArbiter::operator_take`]'s
    /// doc for the full pause-vs-cancel contract.
    pause: CancellationToken,
}

/// Manual `Debug` — `RigState` (behind `rigs`) intentionally has no `Debug`
/// derive, and formatting shouldn't take the arbiter's lock anyway. Only
/// `rig`/`seq` identify a lease usefully in test failure output
/// (`expect_err`/`unwrap_err` require `T: Debug` on the `Ok` side).
impl std::fmt::Debug for RadioLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RadioLease")
            .field("rig", &self.rig)
            .field("seq", &self.seq)
            .finish_non_exhaustive()
    }
}

impl RadioLease {
    /// The dedicated pause-request signal for THIS lease — a
    /// `child_token()` of the acquire-side `cancel` token, minted at grant
    /// time. [`RadioArbiter::operator_take`] cancels this token, NEVER the
    /// acquire-side one the caller originally passed into `acquire`. The
    /// holding action (Task 4's action glue) is expected to race its work
    /// against `pause_requested().cancelled()` alongside its own hard-cancel
    /// token, and release (drop this lease) at its next step boundary when it
    /// fires, then re-acquire later — a graceful, resumable yield, distinct
    /// from a hard cancel/evict.
    pub fn pause_requested(&self) -> &CancellationToken {
        &self.pause
    }
}

impl Drop for RadioLease {
    fn drop(&mut self) {
        let rejected = {
            let mut rigs = self.rigs.lock().unwrap();
            let Some(state) = rigs.get_mut(&self.rig) else {
                return;
            };
            if state.active.as_ref().map(|a| a.seq) != Some(self.seq) {
                // Stale — either pre-empted by `interactive_acquire`, or (the
                // CRITICAL-fix self-heal path) this lease was never actually
                // installed as `state.active` in the first place (it came
                // back via `promote_next_locked`'s `Err(lease)` branch and is
                // only being dropped now by its caller). Either way, nothing
                // for this lease to give back.
                return;
            }
            let held_by = render_holder(&state.active.as_ref().unwrap().holder);
            state.active = None;
            tracing::info!(target: "tuxlink::routines::arbiter", rig = %self.rig, holder = %held_by, "released");
            promote_next_locked(state, &self.rigs, &self.rig, self.now)
        }; // lock guard dropped here.
           // See `promote_next_locked`'s doc: rejected leases must be dropped
           // OUTSIDE the lock we just released — their own `Drop` re-locks
           // `self.rigs`, which would deadlock (std::sync::Mutex is not
           // reentrant) against the guard above, still alive if we dropped
           // `rejected` inside the block.
        drop(rejected);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deterministic, per-test-controlled clock. `RadioArbiter::new` takes a
    // non-capturing `fn() -> i64` (the plan's interface contract — mirrors
    // how Task 5 will pass a real `|| Utc::now().timestamp()` fn pointer), so
    // mocking it needs shared-but-thread-scoped state rather than a closure.
    // `thread_local!` works here because `#[tokio::test]`'s default
    // (current-thread) runtime flavor drives the whole test — including any
    // `tokio::spawn`ed sub-tasks — on the SAME OS thread that entered the
    // test function, so all reads/writes within one test observe the same
    // cell. Every test calls `reset_test_clock()` first because Rust's test
    // harness can reuse an OS thread across multiple `#[test]` functions.
    thread_local! {
        static TEST_CLOCK: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    }
    fn test_now() -> i64 {
        TEST_CLOCK.with(|c| c.get())
    }
    fn reset_test_clock() {
        TEST_CLOCK.with(|c| c.set(0));
    }
    fn advance_test_clock(delta: i64) {
        TEST_CLOCK.with(|c| c.set(c.get() + delta));
    }

    fn run_holder(id: &str, step: &str) -> Holder {
        Holder::Run {
            run_id: id.to_string(),
            step: step.to_string(),
        }
    }

    #[test]
    fn render_holder_matches_spec_wording_exactly() {
        assert_eq!(
            render_holder(&Holder::Interactive),
            "operator (interactive)"
        );
        assert_eq!(
            render_holder(&run_holder("r1", "connect")),
            "run r1 step connect"
        );
    }

    #[tokio::test]
    async fn acquire_succeeds_immediately_when_rig_is_free() {
        reset_test_clock();
        let arbiter = RadioArbiter::new(test_now);
        let cancel = CancellationToken::new();
        let lease = arbiter
            .acquire(
                "g90",
                run_holder("r1", "connect"),
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel,
            )
            .await
            .expect("free rig must acquire immediately");
        let status = arbiter.status("g90").expect("must show a holder");
        assert_eq!(status.holder, run_holder("r1", "connect"));
        assert_eq!(status.held_for_s, 0);
        drop(lease);
        assert!(
            arbiter.status("g90").is_none(),
            "no holder after the lease drops"
        );
    }

    #[tokio::test]
    async fn fail_policy_errors_immediately_and_names_the_holder() {
        reset_test_clock();
        let arbiter = RadioArbiter::new(test_now);

        let cancel_a = CancellationToken::new();
        let lease_a = arbiter
            .acquire(
                "g90",
                run_holder("r1", "connect"),
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel_a,
            )
            .await
            .expect("rig is free, must acquire immediately");

        advance_test_clock(12);

        let cancel_b = CancellationToken::new();
        let err = arbiter
            .acquire(
                "g90",
                run_holder("r2", "listen"),
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel_b,
            )
            .await
            .expect_err("rig is busy under Fail policy — must error immediately, never queue");

        match err {
            ArbiterError::Busy {
                held_by,
                held_for_s,
            } => {
                assert_eq!(held_by, "run r1 step connect");
                assert_eq!(held_for_s, 12);
            }
            other => panic!("expected Busy, got {other:?}"),
        }

        drop(lease_a);
    }

    #[tokio::test]
    async fn wait_policy_times_out_and_names_holder_plus_waited_s() {
        reset_test_clock();
        let arbiter = Arc::new(RadioArbiter::new(test_now));

        let cancel_a = CancellationToken::new();
        let lease_a = arbiter
            .acquire(
                "g90",
                run_holder("r1", "connect"),
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel_a,
            )
            .await
            .unwrap();

        let arbiter2 = arbiter.clone();
        let cancel_b = CancellationToken::new();
        let handle = tokio::spawn(async move {
            arbiter2
                .acquire(
                    "g90",
                    run_holder("r2", "listen"),
                    BusyPolicy::Wait,
                    Duration::from_millis(40),
                    &cancel_b,
                )
                .await
        });

        // Let the spawned task run to its first suspension point (the
        // `tokio::select!` inside `acquire`'s Queued branch) before advancing
        // the fake clock and letting the real 40ms timeout elapse.
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        advance_test_clock(9);

        let err = handle
            .await
            .expect("spawned acquire task must not panic")
            .expect_err("nobody released the lease — must time out");
        match err {
            ArbiterError::Timeout { held_by, waited_s } => {
                assert_eq!(held_by, "run r1 step connect");
                assert_eq!(waited_s, 9);
            }
            other => panic!("expected Timeout, got {other:?}"),
        }

        drop(lease_a);
    }

    #[tokio::test]
    async fn drop_releases_and_wakes_the_next_waiter() {
        reset_test_clock();
        let arbiter = Arc::new(RadioArbiter::new(test_now));
        let cancel_a = CancellationToken::new();
        let lease_a = arbiter
            .acquire(
                "g90",
                Holder::Interactive,
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel_a,
            )
            .await
            .unwrap();

        let arbiter2 = arbiter.clone();
        let cancel_b = CancellationToken::new();
        let handle = tokio::spawn(async move {
            arbiter2
                .acquire(
                    "g90",
                    run_holder("r1", "connect"),
                    BusyPolicy::Wait,
                    Duration::from_secs(5),
                    &cancel_b,
                )
                .await
        });
        tokio::task::yield_now().await;
        assert!(
            arbiter.status("g90").is_some(),
            "A still holds it while B is queued"
        );

        drop(lease_a);

        let lease_b = handle
            .await
            .unwrap()
            .expect("must be granted once A releases");
        let status = arbiter.status("g90").expect("B now holds it");
        assert_eq!(status.holder, run_holder("r1", "connect"));

        drop(lease_b);
        assert!(
            arbiter.status("g90").is_none(),
            "no one holds it after B releases"
        );
    }

    #[tokio::test]
    async fn fifo_order_holds_under_three_contending_waiters() {
        reset_test_clock();
        let arbiter = Arc::new(RadioArbiter::new(test_now));
        let order: Arc<std::sync::Mutex<Vec<&'static str>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        let cancel_a = CancellationToken::new();
        let lease_a = arbiter
            .acquire(
                "g90",
                run_holder("a", "s"),
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel_a,
            )
            .await
            .unwrap();

        async fn wait_and_record(
            arbiter: Arc<RadioArbiter>,
            name: &'static str,
            order: Arc<std::sync::Mutex<Vec<&'static str>>>,
        ) {
            let cancel = CancellationToken::new();
            let lease = arbiter
                .acquire(
                    "g90",
                    Holder::Run {
                        run_id: name.to_string(),
                        step: "s".to_string(),
                    },
                    BusyPolicy::Wait,
                    Duration::from_secs(5),
                    &cancel,
                )
                .await
                .expect("must eventually be granted — nothing times out in this test");
            order.lock().unwrap().push(name);
            drop(lease);
        }

        // One `yield_now` between each spawn: `acquire`'s synchronous
        // enqueue prefix (the lock-held block, before the first `.await`)
        // runs to completion on a task's FIRST poll, so this guarantees
        // enqueue order == spawn order == b, c, d.
        let hb = tokio::spawn(wait_and_record(arbiter.clone(), "b", order.clone()));
        tokio::task::yield_now().await;
        let hc = tokio::spawn(wait_and_record(arbiter.clone(), "c", order.clone()));
        tokio::task::yield_now().await;
        let hd = tokio::spawn(wait_and_record(arbiter.clone(), "d", order.clone()));
        tokio::task::yield_now().await;

        drop(lease_a);

        hb.await.unwrap();
        hc.await.unwrap();
        hd.await.unwrap();

        assert_eq!(
            *order.lock().unwrap(),
            vec!["b", "c", "d"],
            "FIFO order must match enqueue order"
        );
    }

    #[tokio::test]
    async fn interactive_holder_blocks_run_acquires_under_both_policies() {
        reset_test_clock();
        let arbiter = RadioArbiter::new(test_now);
        arbiter.interactive_acquire("g90");

        let cancel = CancellationToken::new();
        let err = arbiter
            .acquire(
                "g90",
                run_holder("r1", "connect"),
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, ArbiterError::Busy { ref held_by, .. } if held_by == "operator (interactive)"),
            "expected Busy naming the operator, got {err:?}"
        );

        let cancel2 = CancellationToken::new();
        let err2 = arbiter
            .acquire(
                "g90",
                run_holder("r2", "listen"),
                BusyPolicy::Wait,
                Duration::from_millis(20),
                &cancel2,
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err2, ArbiterError::Timeout { ref held_by, .. } if held_by == "operator (interactive)"),
            "expected Timeout naming the operator, got {err2:?}"
        );

        arbiter.interactive_release("g90");
        assert!(arbiter.status("g90").is_none());

        let cancel3 = CancellationToken::new();
        let lease = arbiter
            .acquire(
                "g90",
                run_holder("r3", "connect"),
                BusyPolicy::Wait,
                Duration::from_secs(1),
                &cancel3,
            )
            .await
            .expect("must succeed now that the operator released");
        drop(lease);
    }

    #[tokio::test]
    async fn interactive_acquire_evicts_a_run_holder_and_cancels_its_token() {
        reset_test_clock();
        let arbiter = RadioArbiter::new(test_now);
        let cancel_run = CancellationToken::new();
        let lease_run = arbiter
            .acquire(
                "g90",
                run_holder("r1", "connect"),
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel_run,
            )
            .await
            .unwrap();

        arbiter.interactive_acquire("g90");
        assert!(
            cancel_run.is_cancelled(),
            "eviction must cancel the run's acquire-side token"
        );
        let status = arbiter.status("g90").expect("operator now holds it");
        assert_eq!(status.holder, Holder::Interactive);

        // The evicted run's lease, once the run's own code notices and drops
        // it, must NOT clobber the operator's fresh hold (seq mismatch).
        drop(lease_run);
        let status_after = arbiter
            .status("g90")
            .expect("operator hold must survive the stale lease's drop");
        assert_eq!(status_after.holder, Holder::Interactive);

        arbiter.interactive_release("g90");
    }

    #[tokio::test]
    async fn operator_take_cancels_pause_not_acquire_token_then_interactive_acquire_claims() {
        reset_test_clock();
        let arbiter = RadioArbiter::new(test_now);

        // Case 1: Run holder. `operator_take` must cancel the lease's
        // dedicated `pause` token (the SEAM fix's discriminant) and must
        // NEVER touch the run's own acquire-side `cancel` token — cancelling
        // `cancel` would be indistinguishable from the run itself being
        // cancelled, which is a different, harder signal than "please pause."
        let cancel_run = CancellationToken::new();
        let lease_run = arbiter
            .acquire(
                "g90",
                run_holder("r1", "connect"),
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel_run,
            )
            .await
            .unwrap();
        assert!(!cancel_run.is_cancelled());
        assert!(!lease_run.pause_requested().is_cancelled());

        assert!(
            arbiter.operator_take("g90"),
            "must find and signal the run holder"
        );

        assert!(
            lease_run.pause_requested().is_cancelled(),
            "operator_take must cancel the lease's dedicated pause token"
        );
        assert!(
            !cancel_run.is_cancelled(),
            "operator_take must NEVER cancel the run's own acquire-side token \
             — that would be indistinguishable from the run being cancelled"
        );

        let status = arbiter
            .status("g90")
            .expect("run still bookkept as holder until its lease drops");
        assert_eq!(status.holder, run_holder("r1", "connect"));
        assert!(
            status.pause_requested,
            "status() must surface the pending pause request for the UI"
        );

        // The holder observes `pause_requested()` and releases at its own
        // step boundary — Task 4's action glue does this for real; here we
        // model "release" by dropping the lease directly.
        drop(lease_run);
        assert!(arbiter.status("g90").is_none());

        // `interactive_acquire` is the OTHER half of a UI "take the radio"
        // flow: immediate, unconditional claim (spec §9), independent of
        // whether the run ever observed the pause request.
        arbiter.interactive_acquire("g90");
        let status_operator = arbiter.status("g90").expect("operator now holds it");
        assert_eq!(status_operator.holder, Holder::Interactive);
        assert!(
            !status_operator.pause_requested,
            "a fresh Interactive hold has no pending pause request"
        );

        // Case 2: Interactive holder — operator_take must be a no-op.
        assert!(
            !arbiter.operator_take("g90"),
            "operator_take must never touch an interactive holder"
        );
        let status2 = arbiter.status("g90").expect("still held");
        assert_eq!(status2.holder, Holder::Interactive);
        arbiter.interactive_release("g90");
    }

    #[tokio::test]
    async fn operator_take_and_status_are_none_when_rig_is_unknown_or_free() {
        reset_test_clock();
        let arbiter = RadioArbiter::new(test_now);
        assert!(arbiter.status("never-touched").is_none());
        assert!(!arbiter.operator_take("never-touched"));
    }

    /// CRITICAL-finding regression test: the promote-then-drop leak. A
    /// waiter's grant is delivered (the promoter's `tx.send` succeeds and
    /// `state.active` is installed) but the receiving task is torn down
    /// (`abort()`, matching "task aborted rather than token-cancelled" from
    /// the review finding) before it is ever polled again to take ownership
    /// of the `RadioLease`. Before the fix (bare `()` over the channel, lease
    /// reconstructed post-poll) this permanently wedged the rig: bookkeeping
    /// said "B holds it," no live `RadioLease` existed anywhere to release
    /// it, and nobody queued behind B would ever be granted. After the fix
    /// (the `RadioLease` itself is the channel payload), tokio's
    /// `oneshot::Receiver::drop` drops the buffered-but-unreceived lease when
    /// B's future is torn down, which runs `RadioLease::drop` and self-heals
    /// — releasing and promoting the next waiter, C.
    #[tokio::test]
    async fn grant_survives_receiver_task_abort_does_not_wedge_the_rig() {
        reset_test_clock();
        let arbiter = Arc::new(RadioArbiter::new(test_now));

        // A holds the rig.
        let cancel_a = CancellationToken::new();
        let lease_a = arbiter
            .acquire(
                "g90",
                run_holder("a", "s"),
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel_a,
            )
            .await
            .unwrap();

        // B and C both queue behind A (Wait policy), FIFO: B first, C second.
        let arbiter_b = arbiter.clone();
        let cancel_b = CancellationToken::new();
        let handle_b = tokio::spawn(async move {
            arbiter_b
                .acquire(
                    "g90",
                    run_holder("b", "s"),
                    BusyPolicy::Wait,
                    Duration::from_secs(30),
                    &cancel_b,
                )
                .await
        });
        tokio::task::yield_now().await;

        let arbiter_c = arbiter.clone();
        let cancel_c = CancellationToken::new();
        let handle_c = tokio::spawn(async move {
            arbiter_c
                .acquire(
                    "g90",
                    run_holder("c", "s"),
                    BusyPolicy::Wait,
                    Duration::from_secs(30),
                    &cancel_c,
                )
                .await
        });
        tokio::task::yield_now().await;

        // A releases — the FIFO front (B) is granted: `promote_next_locked`
        // sends B's freshly-constructed `RadioLease` down its oneshot
        // channel and installs it as `state.active`, all synchronously,
        // BEFORE B's task is ever polled again.
        drop(lease_a);

        // Abort B's task before it is ever polled to actually take ownership
        // of the received `RadioLease` — the exact CRITICAL interleaving:
        // the handoff already succeeded (the value is sitting in B's oneshot
        // channel) but the receiving future is torn down by task abort, not
        // cooperative cancellation, before it runs `Ok(lease) => ... Ok(lease)`.
        handle_b.abort();
        let b_result = handle_b.await;
        assert!(
            b_result.is_err(),
            "B's task must have been aborted, not completed normally"
        );

        // Give the runtime a couple of beats to run B's dropped
        // `oneshot::Receiver` (which drops the buffered, never-received
        // `RadioLease` it was holding — that drop is what self-heals: it
        // runs `RadioLease::drop`, releases, and promotes C) and then to
        // schedule and poll C's task so it observes the promotion.
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;

        let lease_c = handle_c.await.expect("C's task must not panic").expect(
            "C must be promoted once B's abandoned grant self-heals — \
                 the rig must NOT wedge",
        );
        let status = arbiter.status("g90").expect("C now holds it");
        assert_eq!(status.holder, run_holder("c", "s"));
        drop(lease_c);

        // Prove the rig is fully free (not merely "C happened to get it," in
        // case some other bug left stale bookkeeping around): a fresh THIRD
        // acquire must succeed immediately.
        let cancel_d = CancellationToken::new();
        let lease_d = arbiter
            .acquire(
                "g90",
                run_holder("d", "s"),
                BusyPolicy::Fail,
                Duration::from_secs(1),
                &cancel_d,
            )
            .await
            .expect("rig must be free — the leaked grant must not wedge it forever");
        drop(lease_d);
    }

    // ---------------------------------------------------------------------
    // Proptest invariant: at most one holder per rig, across an arbitrary
    // sequence of acquire/release/interactive-acquire/operator_take ops.
    // Mirrors position/arbiter.rs's state-matrix shape — here the "state
    // space" is the arbiter's own reachable transitions rather than a fixed
    // table, so the model tracks what the driving (single) actor itself
    // should currently hold and cross-checks it against `status()` after
    // every op. Deliberately uses `BusyPolicy::Fail` exclusively (never
    // `Wait`) so every `acquire` call resolves on its FIRST poll without
    // suspending — this keeps the whole sequence single-threaded and
    // deterministic; the genuinely concurrent FIFO/timeout/drop behavior is
    // covered by the dedicated tests above instead.
    // ---------------------------------------------------------------------
    mod proptest_invariant {
        use super::*;
        use proptest::collection::vec;
        use proptest::prelude::*;

        #[derive(Debug, Clone)]
        enum Op {
            AcquireRun(String),
            AcquireInteractive,
            Release,
            OperatorTake,
        }

        fn op_strategy() -> impl Strategy<Value = Op> {
            prop_oneof![
                "[a-c]".prop_map(Op::AcquireRun),
                Just(Op::AcquireInteractive),
                Just(Op::Release),
                Just(Op::OperatorTake),
            ]
        }

        enum Held {
            None,
            Run(RadioLease),
            Interactive,
        }

        proptest! {
            #[test]
            fn at_most_one_holder_per_rig_across_arbitrary_op_sequences(ops in vec(op_strategy(), 0..12)) {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()
                    .unwrap();
                rt.block_on(async {
                    reset_test_clock();
                    let arbiter = RadioArbiter::new(test_now);
                    let rig = "g90";
                    let mut held = Held::None;

                    for op in ops {
                        match op {
                            Op::AcquireRun(id) => {
                                let cancel = CancellationToken::new();
                                let res = arbiter
                                    .acquire(
                                        rig,
                                        Holder::Run { run_id: id, step: "s".to_string() },
                                        BusyPolicy::Fail,
                                        Duration::from_millis(1),
                                        &cancel,
                                    )
                                    .await;
                                match res {
                                    Ok(lease) => {
                                        prop_assert!(matches!(&held, Held::None), "must not double-grant");
                                        held = Held::Run(lease);
                                    }
                                    Err(ArbiterError::Busy { .. }) => {
                                        prop_assert!(!matches!(&held, Held::None), "Busy must mean WE already hold it (sole actor)");
                                    }
                                    Err(other) => prop_assert!(false, "Fail policy never times out or gets cancelled, got {other:?}"),
                                }
                            }
                            Op::AcquireInteractive => {
                                arbiter.interactive_acquire(rig);
                                held = Held::Interactive;
                            }
                            Op::Release => {
                                match held {
                                    Held::Run(lease) => drop(lease),
                                    Held::Interactive => arbiter.interactive_release(rig),
                                    Held::None => {}
                                }
                                held = Held::None;
                            }
                            Op::OperatorTake => {
                                // Signal-only (see `operator_take`'s doc) — does not
                                // change what the arbiter's bookkeeping shows as
                                // held, so the local model is unaffected too.
                                arbiter.operator_take(rig);
                            }
                        }
                        let has_holder = arbiter.status(rig).is_some();
                        prop_assert_eq!(has_holder, !matches!(&held, Held::None));
                    }
                    Ok(())
                })?;
            }
        }
    }
}
