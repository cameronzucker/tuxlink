//! Listener state machine: service axis, health flags, sweep element, slot
//! phase, and the N/k counters (spec §State machine + §Counter semantics).
//!
//! PURE: no time, no I/O. Phase C's supervisor is the single writer of
//! every axis transition except pause (`pause_for_modem` writes `Yielded`;
//! the yield/stop request flags never write the axis themselves — spec
//! §Lifecycle ownership).

/// jt9-degraded threshold: N consecutive non-Decoded/non-BandDead outcomes
/// (types.rs contract, pinned N = 5).
pub const N_DEGRADED: u8 = 5;
/// band-dead threshold: k consecutive zero-decode slots (pinned k = 20,
/// 5 minutes).
pub const K_BAND_DEAD: u8 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedReason {
    /// A persisted identity that no longer resolves (supervisor-retried
    /// every 5 s — self-healing on USB replug).
    DeviceAbsent,
    /// No persisted device identity at all (command-gated: `set_device`).
    NeedsDeviceSelection,
    /// jt9 discovery failed (command-gated).
    WsjtxAbsent,
    /// hw param negotiation rejected (command-gated).
    UnsupportedSampleRate,
    /// A force-detached thread may still hold the PCM; this process can no
    /// longer arbitrate the card. Recovery: app restart. `set_device` and
    /// start are REFUSED from here.
    CaptureWedged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAxis {
    Stopped,
    Starting,
    Listening,
    /// yielded(device-busy) — a modem holds (or is about to hold) the card.
    Yielded,
    Blocked(BlockedReason),
    Stopping,
}

/// Orthogonal health flags — they coexist with `Listening`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HealthFlags {
    pub clock_unsynced: bool,
    pub cat_fixed_band: bool,
    pub jt9_degraded: bool,
}

/// The sweep element is a NAMED part of the machine, not a flag (spec).
/// Runtime state only — `config.sweep.enabled` is never mutated by the
/// machine; `FallbackHold` re-arms to `Active` at the next start or resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sweep {
    Inactive,
    Active { band_idx: usize, dwell_progress: u8 },
    FallbackHold { failures: u8 },
}

/// Slot phase within `listening` (computed from recency; never resets on
/// panel reopen — delta pin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotPhase {
    WaitingFirstSlot,
    Decoded,
    BandDead,
}

/// The per-slot-boundary outcome kind the decode/capture side folds in.
/// Every ring record maps to exactly one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingOutcomeKind {
    /// Includes salvaged/partial decodes — data flowed.
    Decoded,
    BandDead,
    /// Any `SlotFailure` from the decode engine.
    Failed,
    DroppedBackpressure,
    DroppedLostFrames,
    DroppedStorageError,
    /// Scheduled discards: first-slot, QSY transition, clock anomaly.
    Discarded,
}

pub struct ListenerMachine {
    axis: ServiceAxis,
    flags: HealthFlags,
    sweep: Sweep,
    slot_phase: SlotPhase,
    n_consecutive: u8,
    k_consecutive: u8,
    qsy_failures: u8,
}

impl Default for ListenerMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl ListenerMachine {
    pub fn new() -> Self {
        Self {
            axis: ServiceAxis::Stopped,
            flags: HealthFlags::default(),
            sweep: Sweep::Inactive,
            slot_phase: SlotPhase::WaitingFirstSlot,
            n_consecutive: 0,
            k_consecutive: 0,
            qsy_failures: 0,
        }
    }

    // ---- accessors (real from the start; the tests need them) ----
    pub fn axis(&self) -> ServiceAxis { self.axis }
    pub fn flags(&self) -> HealthFlags { self.flags }
    pub fn sweep(&self) -> Sweep { self.sweep }
    pub fn slot_phase(&self) -> SlotPhase { self.slot_phase }
    pub fn n_consecutive(&self) -> u8 { self.n_consecutive }
    pub fn k_consecutive(&self) -> u8 { self.k_consecutive }

    // ---- axis transitions ----

    /// `stopped` / non-wedged `blocked` → `starting` (true). Refused
    /// (false) from `capture-wedged` (restart-required) and from any live
    /// axis (idempotent start: the live supervisor re-runs its sequence).
    pub fn on_start_requested(&mut self) -> bool {
        match self.axis {
            ServiceAxis::Blocked(BlockedReason::CaptureWedged) => false,
            ServiceAxis::Stopped | ServiceAxis::Blocked(_) => {
                self.axis = ServiceAxis::Starting;
                self.rearm_sweep();
                true
            }
            _ => false,
        }
    }

    pub fn on_blocked(&mut self, reason: BlockedReason) {
        self.axis = ServiceAxis::Blocked(reason);
    }

    pub fn on_listening(&mut self) {
        self.axis = ServiceAxis::Listening;
    }

    /// Pause is the ONE transition not written by the supervisor (spec
    /// §Lifecycle ownership). From `stopped`: stateless no-op — no phantom
    /// listener state. From `blocked`: the hold latch is the arbiter's
    /// job; the axis and reason stay untouched.
    pub fn on_pause(&mut self) {
        if matches!(self.axis, ServiceAxis::Listening | ServiceAxis::Starting) {
            self.axis = ServiceAxis::Yielded;
        }
    }

    /// `yielded` → `starting` (the supervisor re-runs steps 1–7). k resets
    /// (spec §Counter semantics); FallbackHold re-arms and the dwell
    /// re-anchors (spec §Sweep).
    pub fn on_resume(&mut self) {
        if self.axis == ServiceAxis::Yielded {
            self.axis = ServiceAxis::Starting;
            self.k_consecutive = 0;
            self.rearm_sweep();
        }
    }

    pub fn on_stopping(&mut self) {
        self.axis = ServiceAxis::Stopping;
    }

    pub fn on_stopped(&mut self) {
        self.axis = ServiceAxis::Stopped;
        self.n_consecutive = 0;
        self.k_consecutive = 0;
        self.flags.jt9_degraded = false;
        // slot_phase intentionally KEPT: phase is ring recency, not
        // session state (delta pin: never resets on reopen).
    }

    pub fn on_capture_wedged(&mut self) {
        self.axis = ServiceAxis::Blocked(BlockedReason::CaptureWedged);
    }

    // ---- counters / phase / dwell ----

    pub fn on_slot_outcome(&mut self, outcome: RingOutcomeKind) {
        match outcome {
            RingOutcomeKind::Decoded => {
                self.n_consecutive = 0;
                self.k_consecutive = 0;
                self.flags.jt9_degraded = false;
                self.slot_phase = SlotPhase::Decoded;
                self.bump_dwell();
            }
            RingOutcomeKind::BandDead => {
                self.n_consecutive = 0;
                self.flags.jt9_degraded = false;
                self.k_consecutive = self.k_consecutive.saturating_add(1);
                if self.k_consecutive >= K_BAND_DEAD {
                    self.slot_phase = SlotPhase::BandDead;
                }
                self.bump_dwell();
            }
            RingOutcomeKind::Failed
            | RingOutcomeKind::DroppedBackpressure
            | RingOutcomeKind::DroppedLostFrames
            | RingOutcomeKind::DroppedStorageError => {
                self.n_consecutive = self.n_consecutive.saturating_add(1);
                if self.n_consecutive >= N_DEGRADED {
                    self.flags.jt9_degraded = true;
                }
                // k-neutral; phase holds; dwell frozen (a failing pipeline
                // samples nothing — rotating it is pointless).
            }
            RingOutcomeKind::Discarded => {
                // Scheduled discards: neither counter, phase holds, dwell
                // unchanged (spec §Counter semantics).
            }
        }
    }

    pub fn on_band_change(&mut self) {
        self.k_consecutive = 0;
    }

    // ---- sweep element ----

    pub fn sweep_activate(&mut self) {
        self.sweep = Sweep::Active { band_idx: 0, dwell_progress: 0 };
        self.qsy_failures = 0;
    }

    pub fn sweep_deactivate(&mut self) {
        self.sweep = Sweep::Inactive;
        self.qsy_failures = 0;
    }

    pub fn on_qsy_success(&mut self, next_band_idx: usize) {
        if matches!(self.sweep, Sweep::Inactive) {
            return;
        }
        self.sweep = Sweep::Active { band_idx: next_band_idx, dwell_progress: 0 };
        self.qsy_failures = 0;
        self.k_consecutive = 0; // k resets on band change
    }

    /// Two CONSECUTIVE failures → FallbackHold (config untouched; re-arms
    /// at the next start/resume — spec §Sweep).
    pub fn on_qsy_failure(&mut self) {
        if matches!(self.sweep, Sweep::Inactive) {
            return;
        }
        self.qsy_failures = self.qsy_failures.saturating_add(1);
        if self.qsy_failures >= 2 {
            self.sweep = Sweep::FallbackHold { failures: self.qsy_failures };
        }
    }

    pub fn dwell_complete(&self, dwell_slots: u8) -> bool {
        matches!(self.sweep, Sweep::Active { dwell_progress, .. }
                 if dwell_progress >= dwell_slots)
    }

    fn bump_dwell(&mut self) {
        if let Sweep::Active { dwell_progress, .. } = &mut self.sweep {
            *dwell_progress = dwell_progress.saturating_add(1);
        }
    }

    /// Start/resume re-arm: FallbackHold → Active (rotation restarts at
    /// band 0); an Active dwell re-anchors; Inactive stays inactive.
    fn rearm_sweep(&mut self) {
        match self.sweep {
            Sweep::FallbackHold { .. } => {
                self.sweep = Sweep::Active { band_idx: 0, dwell_progress: 0 };
                self.qsy_failures = 0;
            }
            Sweep::Active { band_idx, .. } => {
                self.sweep = Sweep::Active { band_idx, dwell_progress: 0 };
            }
            Sweep::Inactive => {}
        }
    }

    // ---- flags ----

    pub fn set_clock_unsynced(&mut self, v: bool) {
        self.flags.clock_unsynced = v;
    }

    pub fn set_cat_fixed_band(&mut self, v: bool) {
        self.flags.cat_fixed_band = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use BlockedReason::*;
    use RingOutcomeKind::*;

    /// A machine driven to `Listening` the way the supervisor does.
    fn listening() -> ListenerMachine {
        let mut m = ListenerMachine::new();
        assert!(m.on_start_requested());
        m.on_listening();
        m
    }

    // ================= counter rules (spec §Counter semantics) ==========

    #[test]
    fn n_increments_on_failed() {
        let mut m = listening();
        m.on_slot_outcome(Failed);
        assert_eq!(m.n_consecutive(), 1);
    }

    #[test]
    fn n_increments_on_every_dropped_kind() {
        let mut m = listening();
        m.on_slot_outcome(DroppedBackpressure);
        m.on_slot_outcome(DroppedLostFrames);
        m.on_slot_outcome(DroppedStorageError);
        assert_eq!(m.n_consecutive(), 3);
    }

    #[test]
    fn n_clears_on_decoded_including_salvaged() {
        let mut m = listening();
        for _ in 0..3 {
            m.on_slot_outcome(Failed);
        }
        m.on_slot_outcome(Decoded); // salvaged/partial folds as Decoded too
        assert_eq!(m.n_consecutive(), 0);
    }

    #[test]
    fn n_clears_on_band_dead_a_clean_zero_decode_exit_is_a_good_slot() {
        let mut m = listening();
        for _ in 0..4 {
            m.on_slot_outcome(Failed);
        }
        m.on_slot_outcome(BandDead);
        assert_eq!(m.n_consecutive(), 0);
    }

    #[test]
    fn jt9_degraded_sets_at_n5_and_clears_on_good_slot() {
        let mut m = listening();
        for i in 0..N_DEGRADED {
            assert!(!m.flags().jt9_degraded, "before slot {i}");
            m.on_slot_outcome(Failed);
        }
        assert!(m.flags().jt9_degraded, "N=5 must set the flag");
        m.on_slot_outcome(BandDead);
        assert!(!m.flags().jt9_degraded, "the first good slot clears");
        assert_eq!(m.n_consecutive(), 0);
    }

    #[test]
    fn k_increments_on_band_dead_and_resets_on_decoded() {
        let mut m = listening();
        m.on_slot_outcome(BandDead);
        m.on_slot_outcome(BandDead);
        assert_eq!(m.k_consecutive(), 2);
        m.on_slot_outcome(Decoded);
        assert_eq!(m.k_consecutive(), 0);
    }

    #[test]
    fn failed_and_dropped_are_k_neutral() {
        // Neither failure nor a dropped slot is evidence about band
        // quietness: k neither increments nor resets.
        let mut m = listening();
        for _ in 0..3 {
            m.on_slot_outcome(BandDead);
        }
        m.on_slot_outcome(Failed);
        m.on_slot_outcome(DroppedBackpressure);
        m.on_slot_outcome(DroppedLostFrames);
        m.on_slot_outcome(DroppedStorageError);
        assert_eq!(m.k_consecutive(), 3, "k must hold, not reset or grow");
    }

    #[test]
    fn scheduled_discards_count_toward_neither_counter() {
        let mut m = listening();
        m.on_slot_outcome(Failed);
        m.on_slot_outcome(BandDead);
        let (n, k) = (m.n_consecutive(), m.k_consecutive());
        for _ in 0..10 {
            m.on_slot_outcome(Discarded); // first-slot / QSY / clock-anomaly
        }
        assert_eq!((m.n_consecutive(), m.k_consecutive()), (n, k));
    }

    #[test]
    fn k_resets_on_band_change() {
        let mut m = listening();
        for _ in 0..7 {
            m.on_slot_outcome(BandDead);
        }
        m.on_band_change(); // manual chip QSY
        assert_eq!(m.k_consecutive(), 0);
    }

    #[test]
    fn k_resets_on_resume() {
        let mut m = listening();
        for _ in 0..7 {
            m.on_slot_outcome(BandDead);
        }
        m.on_pause();
        m.on_resume();
        assert_eq!(m.k_consecutive(), 0);
    }

    // ================= slot phase (recency; Dropped/Discarded neutral) ==

    #[test]
    fn phase_starts_waiting_and_moves_to_decoded() {
        let mut m = listening();
        assert_eq!(m.slot_phase(), SlotPhase::WaitingFirstSlot);
        m.on_slot_outcome(Decoded);
        assert_eq!(m.slot_phase(), SlotPhase::Decoded);
    }

    #[test]
    fn phase_moves_to_band_dead_only_at_k20() {
        let mut m = listening();
        for i in 0..(K_BAND_DEAD - 1) {
            m.on_slot_outcome(BandDead);
            assert_eq!(
                m.slot_phase(),
                SlotPhase::WaitingFirstSlot,
                "slot {i}: below k=20 the phase must not claim band-dead"
            );
        }
        m.on_slot_outcome(BandDead);
        assert_eq!(m.slot_phase(), SlotPhase::BandDead);
    }

    #[test]
    fn phase_holds_on_failed_dropped_and_discarded() {
        let mut m = listening();
        m.on_slot_outcome(Decoded);
        for o in [Failed, DroppedBackpressure, DroppedLostFrames,
                  DroppedStorageError, Discarded] {
            m.on_slot_outcome(o);
            assert_eq!(m.slot_phase(), SlotPhase::Decoded, "{o:?} must be phase-neutral");
        }
    }

    // ================= service axis =====================================

    #[test]
    fn start_from_stopped_enters_starting() {
        let mut m = ListenerMachine::new();
        assert_eq!(m.axis(), ServiceAxis::Stopped);
        assert!(m.on_start_requested());
        assert_eq!(m.axis(), ServiceAxis::Starting);
    }

    #[test]
    fn start_is_refused_from_capture_wedged() {
        // A detached thread may hold the PCM; starting a second capture
        // path in a process that can no longer arbitrate the card is worse
        // than refusing (spec §Device selection). Recovery: app restart.
        let mut m = listening();
        m.on_capture_wedged();
        assert!(!m.on_start_requested());
        assert_eq!(m.axis(), ServiceAxis::Blocked(CaptureWedged));
    }

    #[test]
    fn start_from_blocked_non_wedged_reenters_starting() {
        // set_device / config change / start-retry recover every
        // command-gated blocked state.
        for r in [DeviceAbsent, NeedsDeviceSelection, WsjtxAbsent, UnsupportedSampleRate] {
            let mut m = ListenerMachine::new();
            assert!(m.on_start_requested());
            m.on_blocked(r);
            assert!(m.on_start_requested(), "{r:?}");
            assert_eq!(m.axis(), ServiceAxis::Starting, "{r:?}");
        }
    }

    #[test]
    fn start_is_a_refused_noop_when_already_live() {
        // Idempotent start (spec §Lifecycle ownership): with a live
        // supervisor the handler signals a sequence re-run instead — the
        // machine refuses the transition and holds its axis.
        for (mk, axis) in [
            (ServiceAxis::Starting, ServiceAxis::Starting),
            (ServiceAxis::Listening, ServiceAxis::Listening),
            (ServiceAxis::Yielded, ServiceAxis::Yielded),
        ] {
            let mut m = ListenerMachine::new();
            assert!(m.on_start_requested());
            if mk != ServiceAxis::Starting {
                m.on_listening();
            }
            if mk == ServiceAxis::Yielded {
                m.on_pause();
            }
            assert!(!m.on_start_requested(), "{mk:?}");
            assert_eq!(m.axis(), axis, "{mk:?}");
        }
    }

    #[test]
    fn every_blocked_reason_is_reachable_from_starting() {
        for r in [DeviceAbsent, NeedsDeviceSelection, WsjtxAbsent,
                  UnsupportedSampleRate, CaptureWedged] {
            let mut m = ListenerMachine::new();
            assert!(m.on_start_requested());
            m.on_blocked(r);
            assert_eq!(m.axis(), ServiceAxis::Blocked(r));
        }
    }

    #[test]
    fn listening_from_starting() {
        let m = listening();
        assert_eq!(m.axis(), ServiceAxis::Listening);
    }

    #[test]
    fn pause_from_listening_yields() {
        let mut m = listening();
        m.on_pause();
        assert_eq!(m.axis(), ServiceAxis::Yielded);
    }

    #[test]
    fn pause_from_starting_yields() {
        // Pause during `starting` converts the sequence to yielded; the
        // supervisor's between-step flag check abandons the sequence
        // without re-writing the axis (spec §Arbitration).
        let mut m = ListenerMachine::new();
        assert!(m.on_start_requested());
        m.on_pause();
        assert_eq!(m.axis(), ServiceAxis::Yielded);
    }

    #[test]
    fn pause_from_stopped_is_a_stateless_noop() {
        // Pause fires on EVERY modem spawn, including systems that never
        // enabled FT8 — those must acquire no phantom listener state.
        let mut m = ListenerMachine::new();
        m.on_pause();
        assert_eq!(m.axis(), ServiceAxis::Stopped);
    }

    #[test]
    fn pause_from_blocked_leaves_the_axis_untouched() {
        // The arbiter latches the hold; the blocked axis and reason stay
        // (spec §Arbitration, blocked arm).
        let mut m = ListenerMachine::new();
        assert!(m.on_start_requested());
        m.on_blocked(WsjtxAbsent);
        m.on_pause();
        assert_eq!(m.axis(), ServiceAxis::Blocked(WsjtxAbsent));
    }

    #[test]
    fn resume_from_yielded_reenters_starting() {
        // Resume re-runs start steps 1–7 (spec §Lifecycle ownership) — the
        // machine re-enters Starting; the supervisor walks it forward.
        let mut m = listening();
        m.on_pause();
        m.on_resume();
        assert_eq!(m.axis(), ServiceAxis::Starting);
    }

    #[test]
    fn resume_is_a_noop_outside_yielded() {
        let mut m = listening();
        m.on_resume();
        assert_eq!(m.axis(), ServiceAxis::Listening);
    }

    #[test]
    fn capture_wedged_is_reachable_from_any_live_state() {
        // Stop-path join-bound overrun and pause-path join timeout both
        // force-detach into capture-wedged, whatever the axis was.
        let mut a = listening();
        a.on_capture_wedged();
        assert_eq!(a.axis(), ServiceAxis::Blocked(CaptureWedged));
        let mut b = listening();
        b.on_pause();
        b.on_capture_wedged();
        assert_eq!(b.axis(), ServiceAxis::Blocked(CaptureWedged));
        let mut c = listening();
        c.on_stopping();
        c.on_capture_wedged();
        assert_eq!(c.axis(), ServiceAxis::Blocked(CaptureWedged));
    }

    #[test]
    fn stop_sequence_stopping_then_stopped_resets_counters() {
        let mut m = listening();
        for _ in 0..3 {
            m.on_slot_outcome(Failed);
        }
        m.on_stopping();
        assert_eq!(m.axis(), ServiceAxis::Stopping);
        m.on_stopped();
        assert_eq!(m.axis(), ServiceAxis::Stopped);
        assert_eq!(m.n_consecutive(), 0);
        assert_eq!(m.k_consecutive(), 0);
        assert!(!m.flags().jt9_degraded);
    }

    // ================= health flags =====================================

    #[test]
    fn clock_and_cat_flags_are_orthogonal_setters() {
        let mut m = listening();
        m.set_clock_unsynced(true);
        m.set_cat_fixed_band(true);
        assert!(m.flags().clock_unsynced);
        assert!(m.flags().cat_fixed_band);
        assert_eq!(m.axis(), ServiceAxis::Listening, "flags coexist with listening");
        m.set_clock_unsynced(false);
        assert!(!m.flags().clock_unsynced);
        assert!(m.flags().cat_fixed_band);
    }

    // ================= sweep element ====================================

    #[test]
    fn sweep_activates_at_band_zero_and_dwell_counts_good_slots_only() {
        let mut m = listening();
        m.sweep_activate();
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 0 });
        m.on_slot_outcome(Decoded);
        m.on_slot_outcome(BandDead);
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 2 });
        assert!(!m.dwell_complete(8));
        for _ in 0..6 {
            m.on_slot_outcome(BandDead);
        }
        assert!(m.dwell_complete(8), "8 decoded-or-band-dead slots = dwell done");
    }

    #[test]
    fn dwell_freezes_under_a_failure_streak() {
        // Rotating a broken decode pipeline samples nothing — intended
        // freeze; jt9-degraded is the operator's signal (spec §Sweep).
        let mut m = listening();
        m.sweep_activate();
        m.on_slot_outcome(Decoded);
        for _ in 0..10 {
            m.on_slot_outcome(Failed);
            m.on_slot_outcome(DroppedBackpressure);
            m.on_slot_outcome(Discarded);
        }
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 1 });
    }

    #[test]
    fn qsy_success_advances_band_resets_dwell_and_k() {
        let mut m = listening();
        m.sweep_activate();
        for _ in 0..8 {
            m.on_slot_outcome(BandDead);
        }
        assert_eq!(m.k_consecutive(), 8);
        m.on_qsy_success(1);
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 1, dwell_progress: 0 });
        assert_eq!(m.k_consecutive(), 0, "k resets on band change");
    }

    #[test]
    fn two_consecutive_qsy_failures_enter_fallback_hold() {
        let mut m = listening();
        m.sweep_activate();
        m.on_qsy_failure();
        assert!(matches!(m.sweep(), Sweep::Active { .. }), "one failure retries");
        m.on_qsy_failure();
        assert_eq!(m.sweep(), Sweep::FallbackHold { failures: 2 });
    }

    #[test]
    fn a_qsy_success_between_failures_clears_the_streak() {
        let mut m = listening();
        m.sweep_activate();
        m.on_qsy_failure();
        m.on_qsy_success(1);
        m.on_qsy_failure();
        assert!(
            matches!(m.sweep(), Sweep::Active { .. }),
            "non-consecutive failures must not enter FallbackHold"
        );
    }

    #[test]
    fn fallback_hold_rearms_on_resume() {
        let mut m = listening();
        m.sweep_activate();
        m.on_qsy_failure();
        m.on_qsy_failure();
        assert_eq!(m.sweep(), Sweep::FallbackHold { failures: 2 });
        m.on_pause();
        m.on_resume();
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 0 });
    }

    #[test]
    fn fallback_hold_rearms_on_start() {
        let mut m = listening();
        m.sweep_activate();
        m.on_qsy_failure();
        m.on_qsy_failure();
        m.on_stopping();
        m.on_stopped();
        assert!(m.on_start_requested());
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 0 });
    }

    #[test]
    fn dwell_reanchors_on_resume() {
        let mut m = listening();
        m.sweep_activate();
        for _ in 0..5 {
            m.on_slot_outcome(BandDead);
        }
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 5 });
        m.on_pause();
        m.on_resume();
        assert_eq!(
            m.sweep(),
            Sweep::Active { band_idx: 0, dwell_progress: 0 },
            "dwell re-anchors on resume; band position is kept"
        );
    }

    #[test]
    fn sweep_deactivate_returns_to_inactive() {
        let mut m = listening();
        m.sweep_activate();
        m.on_qsy_failure();
        m.sweep_deactivate();
        assert_eq!(m.sweep(), Sweep::Inactive);
        m.on_qsy_failure();
        assert_eq!(m.sweep(), Sweep::Inactive, "failures while inactive are inert");
    }
}
