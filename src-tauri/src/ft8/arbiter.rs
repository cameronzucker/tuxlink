//! Modem yield/resume arbitration (spec §Arbitration). Design principle
//! (adversarial round 2): resume decisions must not rest on negative
//! evidence alone — every yield LATCHES A HOLD the resume poll honors,
//! cleared by positive evidence (card observed busy = the modem actually
//! acquired it) or a 30 s TTL (an aborted spawn must not wedge FT8).
//!
//! The arbiter also owns ALL rig sessions the FT8 service creates
//! (start-labeling QSY, band-chip QSY, sweep QSY) via
//! [`Ft8Arbiter::rig_session`]: rig_session holds the ARBITER lock (only)
//! around the closure, and pause_for_modem takes the same arbiter lock plus
//! a brief rig-lock await — so a modem connect's pre-audio tune can never
//! overlap an FT8 rig session (the FT-710 dual-CAT-user contention class).
//! The closure itself owns the rig lock; lock order arbiter > rig > state,
//! each acquired at most once per thread.

use std::sync::{Arc, Mutex, OnceLock};

use super::service::Ft8ListenerState;
use tuxlink_capture::state::ServiceAxis;

/// Why a pause could not hand the card over cleanly. The modem seams (T15)
/// surface both as the existing device-busy error class and DO NOT proceed
/// to a doomed spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseError {
    /// The capture join overran its 2 s bound — a hung USB device can park
    /// even the wait-loop. The service is now blocked(capture-wedged);
    /// recovery is app restart.
    CaptureWedged,
    /// `confirm_audio_device_released` timed out: something else still holds
    /// the device path.
    ReleaseTimeout,
}

impl PauseError {
    /// The device-busy-class message the modem seams surface (matches the
    /// tone of `direwolf_probe::device_busy_message`).
    pub fn device_busy_message(&self) -> String {
        match self {
            PauseError::CaptureWedged => {
                "the FT8 listener's audio capture is wedged and may still hold the sound card; \
                 restart Tuxlink"
                    .into()
            }
            PauseError::ReleaseTimeout => {
                "the sound card was not released in time for the modem to start; \
                 try again in a few seconds"
                    .into()
            }
        }
    }
}

pub struct Ft8Arbiter {
    /// THE arbiter lock (lock order: arbiter > rig > state, pinned — spec
    /// §Lock discipline). Serializes pause_for_modem against itself and
    /// against rig sessions.
    lock: Mutex<()>,
    service: Arc<Ft8ListenerState>,
}

impl Ft8Arbiter {
    pub fn new(service: Arc<Ft8ListenerState>) -> Arc<Self> {
        Arc::new(Self { lock: Mutex::new(()), service })
    }

    /// Yield the audio device to a modem that is about to open it.
    ///
    /// **BLOCKING-CONTEXT-ONLY CONTRACT (pinned):** this joins a thread
    /// (≤ 2 s) and polls lsof (≤ 2 s). Every call site MUST run under
    /// `spawn_blocking` or on a plain std thread — never on a tokio worker.
    /// All current call sites comply (ardopcf spawns and Dire Wolf's
    /// spawn_inner run under spawn_blocking; the VARA seam wraps its call —
    /// T15).
    pub fn pause_for_modem(&self) -> Result<(), PauseError> {
        let _arb = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let axis = self.service.axis();
        match axis {
            // stopped: no latch, no state change — a system that never
            // enabled FT8 must never acquire phantom listener state.
            ServiceAxis::Stopped => Ok(()),
            // blocked(*) / stopping: latch only; the axis stays put (pause
            // never rewrites a blocked reason, and stopping is already
            // headed to stopped on its own).
            ServiceAxis::Blocked(_) | ServiceAxis::Stopping => {
                self.service.hold().latch_now();
                Ok(())
            }
            // yielded: already handed over; refresh the latch so the
            // incoming spawn keeps its protection window.
            ServiceAxis::Yielded => {
                self.service.hold().latch_now();
                Ok(())
            }
            ServiceAxis::Listening => {
                // Cancel/await any in-flight rig session: taking the rig
                // lock waits it out; holding it briefly excludes new ones.
                {
                    let rig = self.service.rig_lock();
                    let _rig_guard = rig.lock().unwrap_or_else(|p| p.into_inner());
                }
                self.service.hold().latch_now();
                self.service
                    .pause_capture_for_yield()
                    .map_err(|_| PauseError::CaptureWedged)?;
                self.confirm_release()
            }
            ServiceAxis::Starting => {
                {
                    let rig = self.service.rig_lock();
                    let _rig_guard = rig.lock().unwrap_or_else(|p| p.into_inner());
                }
                self.service.hold().latch_now();
                // There is never a capture thread during starting (spawned
                // only at step 8, which transitions to listening). The
                // yield-request flag makes the supervisor abandon its
                // sequence at the next between-step check, dropping the PCM
                // if it holds one (post-step-7); pause writes the axis.
                self.service.request_yield_from_starting();
                self.confirm_release()
            }
        }
    }

    fn confirm_release(&self) -> Result<(), PauseError> {
        // The trailing release-confirm absorbs the milliseconds until a
        // post-step-7 supervisor's PCM drop lands.
        let card = self.service.resolved_card_index();
        match card {
            Some(idx) if !self.service.platform.confirm_released(idx) => {
                Err(PauseError::ReleaseTimeout)
            }
            _ => Ok(()),
        }
    }

    /// Serialize an FT8-owned rig session (start-labeling, band chip QSY,
    /// sweep QSY) against pause_for_modem and against other rig sessions.
    ///
    /// **Lock architecture (pinned):** this takes ONLY the arbiter lock —
    /// the closure `f` OWNS the rig-lock acquisition itself (`qsy_to_band`
    /// and `start_rig_labeling` each take the rig lock internally). Lock
    /// order: arbiter > rig > state, each acquired AT MOST ONCE per thread.
    /// `rig_session` must never take the rig lock: std's `Mutex` is
    /// non-reentrant, so taking it here and again inside `f` deadlocks —
    /// the exact composition `rig_session(|| qsy_to_band(..))` that the
    /// pre-fix design deadlocked on and that
    /// `rig_session_composed_with_qsy_does_not_deadlock` pins (T16, once
    /// `qsy_to_band` exists). The arbiter lock is what excludes a
    /// concurrent `pause_for_modem`; the rig lock alone never could.
    pub fn rig_session<R>(&self, f: impl FnOnce() -> R) -> R {
        let _arb = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        f()
    }
}

/// Global install point (lib.rs setup, T17). The modem seams call
/// [`pause_for_modem_global`]; before install (unit tests of modem_commands,
/// early startup) it is a no-op Ok — exactly the `stopped` semantics.
pub static FT8_ARBITER: OnceLock<Arc<Ft8Arbiter>> = OnceLock::new();

pub fn pause_for_modem_global() -> Result<(), PauseError> {
    match FT8_ARBITER.get() {
        Some(arb) => arb.pause_for_modem(),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Ft8Config;
    use crate::ft8::service::{Ft8Deps, SharedHold, HOLD_LATCH_TTL};
    use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink};
    use crate::winlink::ax25::devices::{StableAudioId, StableIdKind};
    use tuxlink_capture::state::{BlockedReason, ServiceAxis};

    fn setup(platform: Arc<FakePlatform>, cfg: Ft8Config) -> (Arc<Ft8ListenerState>, Arc<Ft8Arbiter>) {
        let state = Ft8ListenerState::new(
            Ft8Deps {
                platform,
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: Arc::new(RecordingSink::default()),
            },
            cfg,
        );
        let arb = Ft8Arbiter::new(state.clone());
        (state, arb)
    }

    fn cfg_with_device() -> Ft8Config {
        let mut c = Ft8Config::default();
        c.device = Some(StableAudioId { kind: StableIdKind::ByIdSymlink, value: "usb-X-00".into() });
        c
    }

    /// Axis arm 1 — stopped: Ok, NO latch, NO state change.
    #[test]
    fn pause_from_stopped_latches_nothing() {
        let (state, arb) = setup(FakePlatform::happy(), cfg_with_device());
        assert_eq!(arb.pause_for_modem(), Ok(()));
        assert!(!state.hold().is_latched(), "stopped pause must not latch");
        assert_eq!(state.axis(), ServiceAxis::Stopped);
    }

    /// Axis arm 2 — blocked(*): latch only; reason untouched.
    #[test]
    fn pause_from_blocked_latches_and_leaves_the_axis() {
        let p = FakePlatform::happy();
        *p.jt9.lock().unwrap() = Err("NotOnPath".into());
        let (state, arb) = setup(p, cfg_with_device());
        {
            state.test_run_sequence();
        }
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::WsjtxAbsent));
        assert_eq!(arb.pause_for_modem(), Ok(()));
        assert!(state.hold().is_latched());
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::WsjtxAbsent));
    }

    /// Axis arm 3 — listening: join + yielded + release-confirm + latch.
    #[test]
    fn pause_from_listening_joins_confirms_and_latches() {
        let p = FakePlatform::happy();
        let (state, arb) = setup(p.clone(), cfg_with_device());
        state.test_run_sequence();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert_eq!(arb.pause_for_modem(), Ok(()));
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        assert!(state.hold().is_latched());
        state.test_teardown();
    }

    /// Listening + release-confirm timeout → Err(ReleaseTimeout), no doomed
    /// spawn.
    #[test]
    fn release_timeout_surfaces_as_pause_error() {
        let p = FakePlatform::happy();
        *p.released.lock().unwrap() = false;
        let (state, arb) = setup(p, cfg_with_device());
        state.test_run_sequence();
        assert_eq!(arb.pause_for_modem(), Err(PauseError::ReleaseTimeout));
        state.test_teardown();
    }

    /// Axis arm 4 — starting: flag + latch + yielded; never a capture join.
    /// Park the sequence mid-prewarm (gated engine) on a supervisor-less
    /// helper thread, pause, then release.
    #[test]
    fn pause_during_starting_converts_to_yielded_without_join() {
        use crate::ft8::testutil::FakeEngine;
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead_with_prewarm_gate();
        eng.hold_gate();
        *p.engine.lock().unwrap() = eng.clone();
        let (state, arb) = setup(p, cfg_with_device());
        let s2 = state.clone();
        let seq = std::thread::spawn(move || s2.test_run_sequence());
        // Wait until the sequence is inside prewarm.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(arb.pause_for_modem(), Ok(()));
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        assert!(state.hold().is_latched());
        eng.release_gate();
        seq.join().unwrap();
        // The abandoned sequence must NOT have overwritten yielded
        // (the flag never writes the axis; pause already did).
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        state.test_teardown();
    }

    /// Hold latch TTL: a latch older than 30 s reads clear. Driven through
    /// the SharedHold primitive directly (Instant is not fake-able without
    /// a clock trait detour the spec does not require).
    #[test]
    fn hold_latch_ttl_expires_lazily() {
        let hold = SharedHold::default();
        hold.latch_now();
        assert!(hold.is_latched());
        // Backdate: reach inside via the test-only setter.
        hold.test_backdate(HOLD_LATCH_TTL + std::time::Duration::from_secs(1));
        assert!(!hold.is_latched(), "TTL-expired latch reads clear");
        assert!(!hold.is_latched(), "and stays cleared (dropped on observation)");
    }

    /// Positive latch clear: while yielded + latched, the supervisor tick
    /// observing the card BUSY clears the latch (the modem got the card —
    /// positive evidence, not TTL).
    #[test]
    fn latch_clears_on_observed_card_busy() {
        let p = FakePlatform::happy();
        let (state, arb) = setup(p.clone(), cfg_with_device());
        state.test_run_sequence();
        arb.pause_for_modem().unwrap();
        assert!(state.hold().is_latched());
        *p.busy.lock().unwrap() = Err("card busy".into());
        state.tick_yielded();
        assert!(!state.hold().is_latched(), "positive-evidence clear");
        // And with the card still busy, no resume happened.
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        state.test_teardown();
    }

    /// rig_session serializes against pause: a pause issued while a rig
    /// session runs waits for it (no dual-CAT overlap).
    #[test]
    fn rig_session_excludes_pause() {
        let p = FakePlatform::happy();
        let (state, arb) = setup(p, cfg_with_device());
        state.test_run_sequence();
        let arb2 = arb.clone();
        let in_session = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = in_session.clone();
        let t = std::thread::spawn(move || {
            arb2.rig_session(|| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(300));
                flag.store(false, std::sync::atomic::Ordering::SeqCst);
            })
        });
        while !in_session.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        arb.pause_for_modem().unwrap();
        assert!(
            !in_session.load(std::sync::atomic::Ordering::SeqCst),
            "pause returned while a rig session was mid-flight"
        );
        t.join().unwrap();
        state.test_teardown();
    }

    /// The non-reentrancy composition pin (the pre-fix design deadlocked
    /// EXACTLY here and no test drove it): rig_session takes ONLY the
    /// arbiter lock; the closure owns the rig lock via qsy_to_band. If
    /// rig_session ever re-acquires the rig lock, this composition hangs —
    /// the deadline poll turns the hang into a failure. LOCAL arbiter, not
    /// the process-global OnceLock. (Deferred from T14: qsy_to_band is a
    /// T16 product; Gate F checks this test exists and fails-on-deadlock.)
    #[test]
    fn rig_session_composed_with_qsy_does_not_deadlock() {
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        let (state, arb) = setup(p.clone(), cfg_with_device());
        state.test_run_sequence();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        let s2 = state.clone();
        let arb2 = arb.clone();
        let worker = std::thread::spawn(move || {
            arb2.rig_session(|| {
                s2.qsy_to_band("40m", crate::ft8::records::BandSource::CatConfirmed)
            })
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !worker.is_finished() {
            assert!(
                std::time::Instant::now() < deadline,
                "rig_session(qsy_to_band) deadlocked — the non-reentrancy \
                 contract is broken (rig_session must not take the rig lock)"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        worker.join().unwrap().unwrap();
        assert_eq!(*p.tuned_to.lock().unwrap().last().unwrap(), 7_074_000);
        state.test_teardown();
    }

    /// Negative resume gate (spec §Resume — ALL conditions must hold): an
    /// INELIGIBLE modem session (e.g. ConnectedIss) blocks resume even with
    /// the latch clear and the card free; eligibility flipping back is what
    /// releases it.
    #[test]
    fn resume_blocked_while_modem_ineligible() {
        let p = FakePlatform::happy();
        let (state, arb) = setup(p.clone(), cfg_with_device());
        state.test_run_sequence();
        arb.pause_for_modem().unwrap();
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        state.hold().clear(); // latch clear + card free (happy default) ...
        *p.modem_eligible.lock().unwrap() = false; // ... but modem ineligible
        state.tick_yielded();
        assert_eq!(
            state.axis(),
            ServiceAxis::Yielded,
            "an ineligible modem must block resume on its own"
        );
        *p.modem_eligible.lock().unwrap() = true;
        state.tick_yielded();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        state.test_teardown();
    }

    /// Wedged join (spec §Testing: "wedged join → blocked(capture-wedged) +
    /// Err"): a capture thread whose READ blocks past the 2 s join bound —
    /// the hung-USB class the abort flag cannot reach — force-detaches;
    /// pause returns Err(CaptureWedged); the axis says the process can no
    /// longer arbitrate the card.
    #[test]
    fn wedged_capture_join_yields_capture_wedged_error() {
        let p = FakePlatform::happy();
        let (state, arb) = setup(p.clone(), cfg_with_device());
        state.test_run_sequence();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        // Park the source: reads block, ignoring the abort flag (the read
        // itself hangs — SourceStep::Park in testutil).
        let park = crate::ft8::testutil::park_flag();
        p.source_steps
            .lock()
            .unwrap()
            .push_back(crate::ft8::testutil::SourceStep::Park(park.clone()));
        // Give the capture loop time to enter the parked read.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(arb.pause_for_modem(), Err(PauseError::CaptureWedged));
        assert_eq!(
            state.axis(),
            ServiceAxis::Blocked(BlockedReason::CaptureWedged)
        );
        // Hygiene: release the detached thread so the test binary exits.
        park.store(false, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }

    /// ProdPlatform's positive modem-eligibility set (spec §Resume): the
    /// resume-eligible ModemStates are EXACTLY Stopped, Error, SocketLost —
    /// `Idle` (listen-only, ardopcf holds the card) stays active.
    #[test]
    fn prod_platform_modem_eligibility_is_the_pinned_set() {
        use crate::ft8::traits::{Ft8Platform, ProdPlatform};
        use crate::modem_status::{ModemSession, ModemState, ModemStatus};
        let session = Arc::new(ModemSession::new());
        let plat = ProdPlatform {
            wisdom_dir: std::env::temp_dir(),
            slot_root: std::env::temp_dir(),
            modem: session.clone(),
        };
        let set_state = |st: ModemState| {
            let mut s = ModemStatus::stopped();
            s.state = st;
            session.set_status(s);
        };
        // EVERY ModemState variant appears here (modem_status.rs) — the
        // eligibility contract is pinned over the full enum, and a new
        // variant added without updating this table shows up as a missing
        // row in review (matches! has no exhaustiveness lever for this).
        for (st, want) in [
            (ModemState::Stopped, true),
            (ModemState::Error, true),
            (ModemState::SocketLost, true),
            (ModemState::Idle, false),
            (ModemState::Spawning, false),
            (ModemState::Initializing, false),
            (ModemState::Connecting, false),
            (ModemState::ConnectedIrs, false),
            (ModemState::ConnectedIss, false),
            (ModemState::Disconnecting, false),
        ] {
            let label = format!("{st:?}");
            set_state(st);
            assert_eq!(plat.modem_resume_eligible(), want, "{label}");
        }
    }

    /// The global seam: uninstalled → Ok (unit tests of modem paths never
    /// need FT8 state).
    #[test]
    fn global_pause_is_ok_when_uninstalled() {
        // NB: FT8_ARBITER is process-global; this test relies on test
        // binaries not installing it (only lib.rs setup does).
        assert_eq!(pause_for_modem_global(), Ok(()));
    }
}
