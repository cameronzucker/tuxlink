//! Test fakes for the four testability seams (spec §Testing strategy:
//! "fakes for all four traits") plus the synthetic clock they share.
//! `#[cfg(test)]`-gated via the mod declaration in mod.rs.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use super::clock::{ClockProbe, ClockSync};
use super::events::EventSink;
use super::records::{Ft8ListeningChange, SlotRecord};
use super::traits::{DecodeEngine, ReadBatch, SampleSource, SourceError};
use tuxlink_capture::slot::GapReport;
use tuxlink_jt9::types::SlotOutcome;

/// Shared synthetic time. UTC and monotonic advance in lockstep; tests (and
/// the ScriptedSource) drive it — nothing reads the ambient clock.
#[derive(Default)]
pub struct SyntheticClock {
    utc_ms: AtomicU64,
    mono_us: AtomicU64,
}

impl SyntheticClock {
    pub fn new(start_utc_ms: u64) -> Arc<Self> {
        let c = Self::default();
        c.utc_ms.store(start_utc_ms, Ordering::SeqCst);
        Arc::new(c)
    }
    pub fn advance_ms(&self, ms: u64) {
        self.utc_ms.fetch_add(ms, Ordering::SeqCst);
        self.mono_us.fetch_add(ms * 1_000, Ordering::SeqCst);
    }
    /// An NTP step: UTC moves, monotonic does not.
    pub fn step_utc_ms(&self, delta_ms: i64) {
        if delta_ms >= 0 {
            self.utc_ms.fetch_add(delta_ms as u64, Ordering::SeqCst);
        } else {
            self.utc_ms.fetch_sub(delta_ms.unsigned_abs(), Ordering::SeqCst);
        }
    }
    pub fn utc_ms(&self) -> u64 {
        self.utc_ms.load(Ordering::SeqCst)
    }
    pub fn mono_us(&self) -> u64 {
        self.mono_us.load(Ordering::SeqCst)
    }
}

/// One scripted step for the ScriptedSource.
pub enum SourceStep {
    /// Deliver `frames` frames of `value`, advancing synthetic time by
    /// frames/48 ms.
    Frames { frames: usize, value: i16, gap: Option<GapReport> },
    /// Return this error once.
    Fail(SourceError),
    /// Returns an EMPTY batch after a 1 ms sleep — models a wait timeout
    /// WITHOUT wedging: the read always RETURNS, so the capture loop keeps
    /// polling its abort flag (contrast `Park`, added in T14, whose read
    /// blocks — the hung-USB class the abort flag cannot reach).
    Idle,
}

/// Scripted [`SampleSource`]: replays a step queue against the shared
/// synthetic clock.
pub struct ScriptedSource {
    pub steps: Arc<Mutex<VecDeque<SourceStep>>>,
    pub clock: Arc<SyntheticClock>,
}

impl SampleSource for ScriptedSource {
    fn read(&mut self, buf: &mut [i16]) -> Result<ReadBatch, SourceError> {
        let step = self.steps.lock().unwrap().pop_front();
        match step {
            Some(SourceStep::Frames { frames, value, gap }) => {
                let n = frames.min(buf.len());
                for s in buf.iter_mut().take(n) {
                    *s = value;
                }
                // 48 frames per ms at 48 kHz; scripts use multiples of 48.
                self.clock.advance_ms((n as u64) / 48);
                Ok(ReadBatch { frames: n, mono_ts_us: self.clock.mono_us(), gap })
            }
            Some(SourceStep::Fail(e)) => Err(e),
            Some(SourceStep::Idle) | None => {
                // Bounded park like PCM::wait's timeout arm.
                std::thread::sleep(Duration::from_millis(1));
                Ok(ReadBatch { frames: 0, mono_ts_us: self.clock.mono_us(), gap: None })
            }
        }
    }
}

pub struct FakeClock {
    pub sync: Mutex<ClockSync>,
    /// Probe-call counter — the supervisor-cadence test (T11) asserts one
    /// probe per 20-boundary window through this.
    pub probe_calls: AtomicU64,
}

impl FakeClock {
    pub fn new(sync: ClockSync) -> Arc<Self> {
        Arc::new(Self { sync: Mutex::new(sync), probe_calls: AtomicU64::new(0) })
    }
}

impl ClockProbe for FakeClock {
    fn ntp_synchronized(&self) -> ClockSync {
        self.probe_calls.fetch_add(1, Ordering::SeqCst);
        *self.sync.lock().unwrap()
    }
}

#[derive(Default)]
pub struct RecordingSink {
    pub listening_changes: Mutex<Vec<Ft8ListeningChange>>,
    pub slots: Mutex<Vec<SlotRecord>>,
}

impl EventSink for RecordingSink {
    fn emit_listening_change(&self, change: &Ft8ListeningChange) {
        self.listening_changes.lock().unwrap().push(change.clone());
    }
    fn emit_slot(&self, record: &SlotRecord) {
        self.slots.lock().unwrap().push(record.clone());
    }
}

/// Programmable [`DecodeEngine`]: a queue of outcomes (last one repeats), an
/// optional per-decode delay gate (for backpressure tests), and a prewarm
/// gate (for stop-during-starting tests).
pub struct FakeEngine {
    pub outcomes: Mutex<VecDeque<SlotOutcome>>,
    pub default_outcome: SlotOutcome,
    pub prewarm_result: Mutex<Result<(), String>>,
    /// (blocked?, condvar): while the bool is true, decode_slot (and prewarm
    /// when `gate_prewarm`) parks — tests flip it to release.
    pub gate: Arc<(Mutex<bool>, Condvar)>,
    pub gate_prewarm: bool,
    pub decodes_started: AtomicU64,
    pub decodes_finished: AtomicU64,
}

impl FakeEngine {
    pub fn band_dead() -> Arc<Self> {
        Arc::new(Self {
            outcomes: Mutex::new(VecDeque::new()),
            default_outcome: SlotOutcome::BandDead,
            prewarm_result: Mutex::new(Ok(())),
            gate: Arc::new((Mutex::new(false), Condvar::new())),
            gate_prewarm: false,
            decodes_started: AtomicU64::new(0),
            decodes_finished: AtomicU64::new(0),
        })
    }
    pub fn hold_gate(&self) {
        *self.gate.0.lock().unwrap() = true;
    }
    pub fn release_gate(&self) {
        *self.gate.0.lock().unwrap() = false;
        self.gate.1.notify_all();
    }
    fn wait_gate(&self) {
        let (lock, cv) = (&self.gate.0, &self.gate.1);
        let mut blocked = lock.lock().unwrap();
        while *blocked {
            let (g, _t) = cv.wait_timeout(blocked, Duration::from_millis(50)).unwrap();
            blocked = g;
        }
    }
}

impl DecodeEngine for FakeEngine {
    fn prewarm(&self) -> Result<(), String> {
        if self.gate_prewarm {
            self.wait_gate();
        }
        self.prewarm_result.lock().unwrap().clone()
    }
    fn decode_slot(&self, _wav: &Path, _slot_tmp: &Path, _slot_utc_ms: u64) -> SlotOutcome {
        self.decodes_started.fetch_add(1, Ordering::SeqCst);
        self.wait_gate();
        let out = self
            .outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| self.default_outcome.clone());
        self.decodes_finished.fetch_add(1, Ordering::SeqCst);
        out
    }
}

/// Self-tests: each fake is exercised minimally HERE so the T8–T10 batch
/// carries its own consumers (dead-code discipline at the Gate D push) and
/// a broken fake fails fast instead of surfacing as a confusing service-test
/// failure in T11+.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ft8::records::{BandSource, Ft8ListeningChange, RingOutcome, SlotRecord};

    #[test]
    fn synthetic_clock_advances_domains_in_lockstep_and_steps_utc_alone() {
        let c = SyntheticClock::new(30_000);
        c.advance_ms(1_500);
        assert_eq!(c.utc_ms(), 31_500);
        assert_eq!(c.mono_us(), 1_500_000);
        c.step_utc_ms(-2_000); // NTP step: UTC moves, monotonic does not
        assert_eq!(c.utc_ms(), 29_500);
        assert_eq!(c.mono_us(), 1_500_000);
    }

    #[test]
    fn scripted_source_replays_frames_fails_then_idles() {
        let clock = SyntheticClock::new(0);
        let steps = Arc::new(Mutex::new(VecDeque::from([
            SourceStep::Frames { frames: 48, value: 7, gap: None },
            SourceStep::Idle, // explicit Idle step (constructs the variant —
                              // an exhausted queue exercises only the None arm)
            SourceStep::Fail(SourceError::Busy),
        ])));
        let mut src = ScriptedSource { steps, clock: clock.clone() };
        let mut buf = vec![0i16; 96];
        let batch = src.read(&mut buf).unwrap();
        assert_eq!(batch.frames, 48);
        assert_eq!(buf[0], 7);
        assert_eq!(batch.mono_ts_us, 1_000, "48 frames = 1 ms at 48 kHz");
        let explicit_idle = src.read(&mut buf).unwrap();
        assert_eq!(explicit_idle.frames, 0, "explicit Idle: empty batch");
        assert_eq!(src.read(&mut buf), Err(SourceError::Busy));
        // An exhausted queue behaves as Idle: empty batch, clock untouched.
        let idle = src.read(&mut buf).unwrap();
        assert_eq!(idle.frames, 0);
        assert_eq!(clock.mono_us(), 1_000);
    }

    #[test]
    fn fake_clock_reports_its_sync_and_counts_probes() {
        let c = FakeClock::new(ClockSync::Unsynced);
        assert_eq!(c.ntp_synchronized(), ClockSync::Unsynced);
        *c.sync.lock().unwrap() = ClockSync::Synced;
        assert_eq!(c.ntp_synchronized(), ClockSync::Synced);
        assert_eq!(c.probe_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn recording_sink_records_both_event_kinds() {
        let sink = RecordingSink::default();
        sink.emit_listening_change(&Ft8ListeningChange {
            service: tuxlink_capture::state::ServiceAxis::Stopped.into(),
            flags: tuxlink_capture::state::HealthFlags::default().into(),
            slot_phase: tuxlink_capture::state::SlotPhase::WaitingFirstSlot.into(),
            band: "20m".into(),
            dial_hz: 14_074_000,
            sweep: tuxlink_capture::state::Sweep::Inactive.into(),
        });
        sink.emit_slot(&SlotRecord {
            slot_utc_ms: 15_000,
            band: "20m".into(),
            dial_hz: 14_074_000,
            band_source: BandSource::DefaultUnconfirmed,
            band_label_confirmed_utc_ms: None,
            outcome: RingOutcome::BandDead,
            decodes: Vec::new(),
            partial_salvage: false,
            lost_frames: 0,
            boundary_skew_frames: 0,
            clip_fraction: 0.0,
            rms_dbfs: -60.0,
            dwell_slot_index: None,
        });
        assert_eq!(sink.listening_changes.lock().unwrap().len(), 1);
        assert_eq!(sink.slots.lock().unwrap().len(), 1);
    }

    #[test]
    fn fake_engine_pops_queued_outcomes_then_repeats_the_default() {
        let eng = FakeEngine::band_dead();
        assert_eq!(eng.prewarm(), Ok(()));
        eng.outcomes
            .lock()
            .unwrap()
            .push_back(SlotOutcome::Decoded(Vec::new()));
        let p = Path::new("unused.wav");
        assert!(matches!(eng.decode_slot(p, p, 0), SlotOutcome::Decoded(_)));
        assert!(matches!(eng.decode_slot(p, p, 1), SlotOutcome::BandDead));
        assert_eq!(eng.decodes_started.load(Ordering::SeqCst), 2);
        assert_eq!(eng.decodes_finished.load(Ordering::SeqCst), 2);
    }
}
