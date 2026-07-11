//! Opt-in CAT band sweep (spec §Sweep): round-robin over the configured
//! band list with a fixed dwell, driven by the supervisor tick, executed
//! through the arbiter's rig-session serialization. RX-only; the QSY moves
//! the dial and nothing keys.

use std::sync::Arc;

use super::records::BandSource;
use super::service::Ft8ListenerState;
use tuxlink_capture::state::{ServiceAxis, Sweep};

/// One supervisor tick's sweep bookkeeping. Cheap no-op unless: listening,
/// sweep Active, dwell complete.
pub(crate) fn tick(state: &Arc<Ft8ListenerState>) {
    // Never outside listening (covers yielded + blocked). "While a pause is
    // in progress" needs more than the axis: pause latches the hold FIRST,
    // before it joins capture and writes yielded — so a latched hold is the
    // authoritative "do not move the dial" signal for the in-between window.
    if state.axis() != ServiceAxis::Listening || state.hold().is_latched() {
        return;
    }
    let (dwell_done, band_idx, bands, dwell_slots) = {
        let g = state.lock_inner_for_sweep();
        let (active_idx, cfg) = match g.machine_sweep() {
            Sweep::Active { band_idx, .. } => (band_idx, g.sweep_config()),
            _ => return, // Inactive or FallbackHold: nothing to schedule
        };
        (
            g.machine_dwell_complete(cfg.dwell_slots),
            active_idx,
            cfg.bands.clone(),
            cfg.dwell_slots,
        )
    };
    let _ = dwell_slots;
    if !dwell_done || bands.is_empty() {
        return;
    }
    let next_idx = (band_idx + 1) % bands.len();
    let next_band = bands[next_idx].clone();
    // Through the arbiter when installed (mirrors T17's ft8_set_band): the
    // ARBITER lock is what excludes a concurrent pause_for_modem — the rig
    // lock alone never could. qsy_to_band owns the rig lock; rig_session
    // takes ONLY the arbiter lock (lock order arbiter > rig > state, each
    // acquired at most once per thread — T14's non-reentrancy contract).
    // The outer guard above raced pause_for_modem on the arbiter lock (T16
    // review): a pause that wins the lock after the guard leaves the service
    // yielded + latched, and tuning then would move the dial mid-handover.
    // Re-validate INSIDE the closure — rig_session holds the arbiter lock,
    // so this check is race-free against pause. A skip is neither success
    // (no band advance) nor failure (no FallbackHold count); the next dwell
    // boundary retries.
    enum QsyOutcome {
        Done,
        Skipped,
        Failed(String),
    }
    let do_qsy = || {
        if !matches!(state.axis(), ServiceAxis::Listening) || state.hold().is_latched() {
            return QsyOutcome::Skipped;
        }
        match state.qsy_to_band(&next_band, BandSource::CatConfirmed) {
            Ok(()) => QsyOutcome::Done,
            Err(e) => QsyOutcome::Failed(e),
        }
    };
    let result = match crate::ft8::arbiter::FT8_ARBITER.get() {
        Some(arb) => arb.rig_session(do_qsy),
        None => do_qsy(),
    };
    match result {
        QsyOutcome::Done => {
            state.on_sweep_qsy_success(next_idx);
        }
        QsyOutcome::Skipped => {
            tracing::debug!(target: "tuxlink::ft8", "sweep QSY skipped: yield won the arbiter race");
        }
        QsyOutcome::Failed(e) => {
            tracing::warn!(target: "tuxlink::ft8", "sweep QSY to {next_band} failed: {e} — retry next dwell boundary");
            state.on_sweep_qsy_failure(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Ft8Config;
    use crate::ft8::records::{BandSource, RingOutcome};
    use crate::ft8::service::Ft8Deps;
    use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink};
    use crate::winlink::ax25::devices::{StableAudioId, StableIdKind};
    use tuxlink_capture::state::{ServiceAxis, Sweep};

    fn sweep_cfg() -> Ft8Config {
        Ft8Config {
            device: Some(StableAudioId { kind: StableIdKind::ByIdSymlink, value: "usb-X-00".into() }),
            band: "80m".into(),
            sweep: crate::config::Ft8SweepConfig {
                enabled: true,
                bands: vec!["80m".into(), "40m".into(), "20m".into()],
                dwell_slots: 4,
            },
            ..Default::default()
        }
    }

    fn listening_state_with_sweep() -> (Arc<Ft8ListenerState>, Arc<FakePlatform>) {
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        *p.rig_dial.lock().unwrap() = Ok(3_573_000); // radio already on 80m
        let state = Ft8ListenerState::new(
            Ft8Deps {
                platform: p.clone(),
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: Arc::new(RecordingSink::default()),
            },
            sweep_cfg(),
        );
        state.test_run_sequence();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert!(matches!(state.snapshot_sweep(), Sweep::Active { band_idx: 0, .. }));
        (state, p)
    }

    /// Dwell counting via the machine: decoded/band-dead slots advance the
    /// dwell; Failed/Dropped freeze it (intended — jt9-degraded is the
    /// operator's signal); the QSY fires only at dwell_slots good slots.
    #[test]
    fn dwell_counts_good_slots_and_freezes_on_failures() {
        let (state, p) = listening_state_with_sweep();
        // 3 good slots: dwell not complete → tick does not QSY.
        for i in 0..3u64 {
            state.record_slot(state.test_base_record(i, RingOutcome::BandDead));
        }
        tick(&state);
        assert!(p.tuned_to.lock().unwrap().len() <= 1, "no dwell QSY yet (≤ the start tune)");
        // A failure + a drop: dwell frozen — still no QSY.
        state.record_slot(state.test_base_record(3, RingOutcome::Failed { failure: "Timeout".into() }));
        state.record_slot(state.test_base_record(4, RingOutcome::DroppedBackpressure));
        tick(&state);
        let tunes_before = p.tuned_to.lock().unwrap().len();
        // 4th good slot completes the dwell → tick QSYs to 40m.
        state.record_slot(state.test_base_record(5, RingOutcome::Decoded));
        tick(&state);
        let tunes = p.tuned_to.lock().unwrap().clone();
        assert_eq!(tunes.len(), tunes_before + 1, "exactly one dwell QSY");
        assert_eq!(*tunes.last().unwrap(), 7_074_000, "next configured band (40m)");
        assert!(matches!(state.snapshot_sweep(), Sweep::Active { band_idx: 1, .. }));
        state.test_teardown();
    }

    /// The transition slot is a scheduled discard: the next completed slot
    /// after a QSY records Discarded(qsy-transition) and is counter-neutral.
    #[test]
    fn transition_slot_is_discarded_and_counter_neutral() {
        let (state, _p) = listening_state_with_sweep();
        state.qsy_to_band("40m", BandSource::CatConfirmed).unwrap();
        let n_before = state.snapshot().n_consecutive;
        state.test_complete_one_slot(9_000); // T12's handle_completed_slot via helper
        let snap = state.snapshot();
        let last = snap.ring_tail.last().unwrap();
        assert_eq!(
            last.outcome,
            RingOutcome::Discarded { class: crate::ft8::records::DiscardClassDto::QsyTransition }
        );
        assert_eq!(snap.n_consecutive, n_before, "scheduled discard: neither counter");
        state.test_teardown();
    }

    /// Two consecutive QSY failures → FallbackHold; a start/resume re-arms
    /// (sweep_activate at step 8).
    #[test]
    fn double_qsy_failure_enters_fallback_hold_and_rearms_on_resume() {
        let (state, p) = listening_state_with_sweep();
        p.rig_tune_results.lock().unwrap().push_back(Err("serial dropped".into()));
        p.rig_tune_results.lock().unwrap().push_back(Err("serial dropped".into()));
        for i in 0..4u64 {
            state.record_slot(state.test_base_record(i, RingOutcome::BandDead));
        }
        tick(&state); // failure 1
        for i in 4..8u64 {
            state.record_slot(state.test_base_record(i, RingOutcome::BandDead));
        }
        tick(&state); // failure 2 → FallbackHold
        assert!(matches!(state.snapshot_sweep(), Sweep::FallbackHold { .. }));
        // Config untouched.
        assert!(state.snapshot_ft8_cfg().sweep.enabled);
        // FallbackHold: further ticks never QSY.
        let tunes = p.tuned_to.lock().unwrap().len();
        tick(&state);
        assert_eq!(p.tuned_to.lock().unwrap().len(), tunes);
        // Re-arm via yield → resume (steps 1–7 + 8′ re-run sweep_activate).
        state.hold().latch_now();
        state.pause_capture_for_yield().unwrap();
        state.hold().clear();
        state.tick_yielded();
        assert!(matches!(state.snapshot_sweep(), Sweep::Active { .. }), "re-armed on resume");
        state.test_teardown();
    }

    /// Partial-QSY provenance downgrade (spec's named test): tune fails →
    /// band_source = default-unconfirmed, confirmed = None; subsequent slots
    /// are NOT attributed to the stale band with confirmed provenance.
    #[test]
    fn partial_qsy_failure_downgrades_the_band_label() {
        let (state, p) = listening_state_with_sweep();
        p.rig_tune_results.lock().unwrap().push_back(Err("serial dropped mid-tune".into()));
        assert!(state.qsy_to_band("20m", BandSource::CatConfirmed).is_err());
        let snap = state.snapshot();
        assert_eq!(snap.band_source, BandSource::DefaultUnconfirmed);
        assert_eq!(snap.band_label_confirmed_utc_ms, None);
        // A slot recorded now carries the downgraded provenance.
        state.record_slot(state.test_base_record(1, RingOutcome::BandDead));
        assert_eq!(
            state.snapshot().ring_tail.last().unwrap().band_source,
            BandSource::DefaultUnconfirmed
        );
        state.test_teardown();
    }

    /// Sweep never fires while yielded (spec: nor during a pause, nor
    /// outside listening — one guard covers all three).
    #[test]
    fn sweep_never_fires_while_yielded() {
        let (state, p) = listening_state_with_sweep();
        for i in 0..4u64 {
            state.record_slot(state.test_base_record(i, RingOutcome::BandDead));
        }
        state.hold().latch_now();
        state.pause_capture_for_yield().unwrap();
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        let tunes = p.tuned_to.lock().unwrap().len();
        tick(&state);
        assert_eq!(p.tuned_to.lock().unwrap().len(), tunes, "no QSY while yielded");
        state.test_teardown();
    }

    /// cat-absent hold-band: sweep stays Inactive and the snapshot carries
    /// the instructed dial + unconfirmed provenance (spec §Hold-band,
    /// cat-absent arm — the T11 arrow-5 test pins the flag; this pins the
    /// sweep element).
    #[test]
    fn cat_absent_keeps_sweep_inactive_with_instructed_dial() {
        let p = FakePlatform::happy(); // rig_configured = false
        let mut cfg = sweep_cfg();
        cfg.sweep.enabled = true; // enabled in config, but no CAT
        let state = Ft8ListenerState::new(
            Ft8Deps {
                platform: p,
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: Arc::new(RecordingSink::default()),
            },
            cfg,
        );
        state.test_run_sequence();
        assert!(matches!(state.snapshot_sweep(), Sweep::Inactive), "cat-fixed-band ⇒ Inactive");
        let snap = state.snapshot();
        assert!(snap.flags.cat_fixed_band);
        assert_eq!(snap.dial_hz, 3_573_000, "instructed dial for the 80m chip");
        assert_eq!(snap.band_source, BandSource::DefaultUnconfirmed);
        state.test_teardown();
    }
}
