//! The FT8 listener service: managed state + the supervisor thread + the
//! start sequence (spec §Service structure, §Start sequence, §Lifecycle
//! ownership), plus the capture/decode thread bodies and the waterfall tap
//! (part 2, Task 12). Stop + resume protocols land in part 3 (Task 13).
//!
//! Lock discipline (spec §Lock discipline, pinned): thread handles live
//! OUTSIDE the state mutex and are take()n before any join; the state mutex
//! is leaf-level — never held across a join, an ALSA call, a rig session, or
//! an event emit; lock order arbiter > rig > state everywhere, each acquired
//! AT MOST ONCE per thread (the arbiter's rig_session takes only the arbiter
//! lock; rig-touching helpers own the rig lock themselves — T14).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::config::Ft8Config;
use crate::ft8::clock::{ClockProbe, ClockSync};
use crate::ft8::events::EventSink;
use crate::ft8::records::{
    AudioDeviceChoice, BandSource, DiscardClassDto, Ft8ListeningChange, RingOutcome,
    ServiceAxisDto, SlotRecord,
};
use crate::ft8::traits::{DecodeEngine, Ft8Platform, SourceError};
use crate::winlink::ax25::devices::ResolvedManagedDevice;
use serde::Serialize;
use tuxlink_capture::state::{BlockedReason, ListenerMachine, ServiceAxis};

pub(crate) const SUPERVISOR_TICK: Duration = Duration::from_secs(5);
pub(crate) const RING_CAP: usize = 240;
pub(crate) const CLOCK_REPROBE_BOUNDARIES: u64 = 20;
pub(crate) const PIPE_WATERMARK_BOUNDARIES: u64 = 100;
pub(crate) const PIPE_WATERMARK_EXCESS: usize = 16;
pub(crate) const HOLD_LATCH_TTL: Duration = Duration::from_secs(30);
/// Stop/yield join bounds (spec §Lifecycle ownership): capture's PCM closes
/// on drop, so 2 s covers the ALSA teardown; decode's 16 s absorbs the 14 s
/// worst-case decode; the supervisor's 16 s covers a blocking prewarm plus
/// its park_timeout tick.
pub(crate) const CAPTURE_JOIN: Duration = Duration::from_secs(2);
pub(crate) const DECODE_JOIN: Duration = Duration::from_secs(16);
pub(crate) const SUPERVISOR_JOIN: Duration = Duration::from_secs(16);

/// Poll `is_finished()` to the bound, then join. Returns Err(handle) on
/// overrun so the caller can force-detach with provenance.
fn join_bounded(handle: JoinHandle<()>, timeout: Duration) -> Result<(), JoinHandle<()>> {
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return Err(handle);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = handle.join();
    Ok(())
}

/// Capture join overran at pause: the arbiter maps this to
/// PauseError::CaptureWedged (T14).
#[derive(Debug)]
pub(crate) struct YieldJoinTimeout;

/// The positive hold token (spec §Hold latch). A lazily-evaluated timestamp:
/// it needs no supervisor to expire. Shared between the service (start-
/// sequence step 6 consults it) and the arbiter (every pause latches it).
#[derive(Default)]
pub struct SharedHold {
    latched_at: Mutex<Option<Instant>>,
}

impl SharedHold {
    pub fn latch_now(&self) {
        *self.latched_at.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
    }
    pub fn clear(&self) {
        *self.latched_at.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
    /// TTL-aware: a latch older than 30 s reads clear (an aborted modem
    /// spawn must not wedge FT8) and is dropped on observation.
    pub fn is_latched(&self) -> bool {
        let mut g = self.latched_at.lock().unwrap_or_else(|p| p.into_inner());
        match *g {
            Some(t) if t.elapsed() < HOLD_LATCH_TTL => true,
            Some(_) => {
                *g = None;
                false
            }
            None => false,
        }
    }
    /// Test-only TTL exercise: backdate the latch by `d` without a fake
    /// clock detour (Instant is not fake-able without one the spec does not
    /// require).
    #[cfg(test)]
    pub(crate) fn test_backdate(&self, d: Duration) {
        *self.latched_at.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now() - d);
    }
}

/// Injected seams, bundled (clippy too_many_arguments discipline).
pub struct Ft8Deps {
    pub platform: Arc<dyn Ft8Platform>,
    pub clock: Arc<dyn ClockProbe>,
    pub sink: Arc<dyn EventSink>,
}

/// The rendezvous-channel payload (spec §Threads): capture hands this to
/// decode over the `sync_channel(0)` master channel once the slot's WAV is
/// on disk.
pub(crate) struct SlotJob {
    pub slot_utc_ms: u64,
    pub dir: PathBuf,
    pub wav: PathBuf,
    pub lost_frames: u64,
    pub boundary_skew_frames: u64,
    pub clip_fraction: f32,
    pub rms_dbfs: f32,
}

/// The waterfall tap (spec §Waterfall tap): a bounded lossy ring of
/// decimated 12 kHz i16 blocks, 1200 frames (100 ms) per block, capacity 32
/// (3.2 s). Drop-OLDEST under a stalled/absent consumer; pushes never block
/// and never backpressure capture. L2's whole contract is "the 12 kHz
/// stream is subscribable, bounded, and never backpressures capture" — FFT,
/// column cadence, and events are L3's.
pub struct WaterfallTap {
    inner: Mutex<TapInner>,
}

struct TapInner {
    blocks: VecDeque<Vec<i16>>,
    /// Partial block being accumulated to the 1200-frame boundary.
    pending: Vec<i16>,
    subscribed: bool,
}

pub(crate) const TAP_BLOCK_FRAMES: usize = 1_200;
pub(crate) const TAP_CAPACITY_BLOCKS: usize = 32;

impl Default for WaterfallTap {
    fn default() -> Self {
        Self {
            inner: Mutex::new(TapInner {
                blocks: VecDeque::with_capacity(TAP_CAPACITY_BLOCKS),
                pending: Vec::with_capacity(TAP_BLOCK_FRAMES),
                subscribed: false,
            }),
        }
    }
}

impl WaterfallTap {
    pub fn push_samples(&self, samples_12k: &[i16]) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if !g.subscribed {
            // No subscriber (the common state): keep the ring warm at zero
            // cost — reset pending and skip block assembly entirely.
            g.pending.clear();
            g.blocks.clear();
            return;
        }
        let mut rest = samples_12k;
        while !rest.is_empty() {
            let need = TAP_BLOCK_FRAMES - g.pending.len();
            let take = need.min(rest.len());
            g.pending.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if g.pending.len() == TAP_BLOCK_FRAMES {
                if g.blocks.len() == TAP_CAPACITY_BLOCKS {
                    g.blocks.pop_front(); // drop-oldest
                }
                let full = std::mem::replace(
                    &mut g.pending,
                    Vec::with_capacity(TAP_BLOCK_FRAMES),
                );
                g.blocks.push_back(full);
            }
        }
    }
    pub fn subscribe(&self) {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).subscribed = true;
    }
    pub fn unsubscribe(&self) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.subscribed = false;
        g.blocks.clear();
        g.pending.clear();
    }
    pub fn take_blocks(&self) -> Vec<Vec<i16>> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .blocks
            .drain(..)
            .collect()
    }
}

/// Capture-side slot measurements carried into every ring record
/// (bundled: clippy too_many_arguments, same discipline as Ft8Deps).
#[derive(Clone, Copy)]
pub(crate) struct SlotProvenance {
    pub lost_frames: u64,
    pub boundary_skew_frames: u64,
    pub clip_fraction: f32,
    pub rms_dbfs: f32,
}

/// Everything behind the leaf-level state mutex.
struct Inner {
    machine: ListenerMachine,
    ft8_cfg: Ft8Config,
    band: String,
    dial_hz: u64,
    band_source: BandSource,
    band_label_confirmed_utc_ms: Option<u64>,
    engine_version: Option<String>,
    last_slot_utc_ms: Option<u64>,
    last_failure: Option<String>,
    ring: VecDeque<SlotRecord>,
    /// Set by QSY (T16): the next completed slot is a scheduled discard.
    discard_next_slot: Option<DiscardClassDto>,
    /// The device the last successful resolution produced — live handles for
    /// probes + release-confirm. Refreshed by every sequence run.
    resolved: Option<ResolvedManagedDevice>,
}

/// Thread handles — OUTSIDE the state mutex (lock discipline). `capture` +
/// `decode` are joined by T13's stop protocol; `spawn_workers` wires them.
#[derive(Default)]
struct Handles {
    supervisor: Option<JoinHandle<()>>,
    capture: Option<JoinHandle<()>>,
    decode: Option<JoinHandle<()>>,
}

pub struct Ft8ListenerState {
    inner: Mutex<Inner>,
    handles: Mutex<Handles>,
    /// The master `SyncSender<SlotJob>` (spec §Lifecycle): survives yield
    /// and device loss; only stop() drops it (T13), which is decode's sole
    /// exit signal.
    master_tx: Mutex<Option<SyncSender<SlotJob>>>,
    engine: Mutex<Option<Arc<dyn DecodeEngine>>>,
    pub(crate) platform: Arc<dyn Ft8Platform>,
    pub(crate) clock: Arc<dyn ClockProbe>,
    pub(crate) sink: Arc<dyn EventSink>,
    hold: Arc<SharedHold>,
    /// Serializes ALL FT8 rig sessions (start-labeling, band chip, sweep).
    /// The arbiter (T14) holds a clone: "the arbiter owns all rig sessions"
    /// is true by construction — one mutex, arbiter-visible.
    rig_lock: Arc<Mutex<()>>,
    stop_request: AtomicBool,
    yield_request: AtomicBool,
    start_rerun_request: AtomicBool,
    /// Signals the capture thread to exit on its next poll (T13's stop
    /// protocol; checked every read-loop iteration).
    capture_abort: Arc<AtomicBool>,
    /// Process-monotonic per-slot-dir sequence (collision-proof under
    /// backward clock steps) — `slot-<utc_ms>-<seq>`.
    slot_seq: AtomicU64,
    tap: WaterfallTap,
    /// Capture-side slot-boundary counter (spec: cadences count BOUNDARIES,
    /// not decoded slots).
    slot_boundaries: AtomicU64,
    /// The supervisor's Thread handle for park_timeout interruption.
    supervisor_thread: Mutex<Option<std::thread::Thread>>,
    pipe_fd_baseline: Mutex<Option<usize>>,
}

impl Ft8ListenerState {
    pub fn new(deps: Ft8Deps, ft8_cfg: Ft8Config) -> Arc<Self> {
        let dial = tuxlink_capture::bands::dial_hz(&ft8_cfg.band).unwrap_or(14_074_000);
        let band = ft8_cfg.band.clone();
        Arc::new(Self {
            inner: Mutex::new(Inner {
                machine: ListenerMachine::new(),
                ft8_cfg,
                band,
                dial_hz: dial,
                band_source: BandSource::DefaultUnconfirmed,
                band_label_confirmed_utc_ms: None,
                engine_version: None,
                last_slot_utc_ms: None,
                last_failure: None,
                ring: VecDeque::with_capacity(RING_CAP),
                discard_next_slot: None,
                resolved: None,
            }),
            handles: Mutex::new(Handles::default()),
            master_tx: Mutex::new(None),
            engine: Mutex::new(None),
            platform: deps.platform,
            clock: deps.clock,
            sink: deps.sink,
            hold: Arc::new(SharedHold::default()),
            rig_lock: Arc::new(Mutex::new(())),
            stop_request: AtomicBool::new(false),
            yield_request: AtomicBool::new(false),
            start_rerun_request: AtomicBool::new(false),
            capture_abort: Arc::new(AtomicBool::new(false)),
            slot_seq: AtomicU64::new(0),
            tap: WaterfallTap::default(),
            slot_boundaries: AtomicU64::new(0),
            supervisor_thread: Mutex::new(None),
            pipe_fd_baseline: Mutex::new(None),
        })
    }

    pub fn hold(&self) -> Arc<SharedHold> {
        self.hold.clone()
    }
    pub fn rig_lock(&self) -> Arc<Mutex<()>> {
        self.rig_lock.clone()
    }
    pub fn tap(&self) -> &WaterfallTap {
        &self.tap
    }
    pub fn set_ft8_config(&self, cfg: Ft8Config) {
        let mut g = self.lock_inner();
        // A device change invalidates the constructed runner (spec: "no
        // runner reconstruction unless the device changed").
        if g.ft8_cfg.device != cfg.device {
            drop(g);
            *self.engine.lock().unwrap_or_else(|p| p.into_inner()) = None;
            g = self.lock_inner();
        }
        g.ft8_cfg = cfg;
    }

    fn lock_inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) fn axis(&self) -> ServiceAxis {
        self.lock_inner().machine.axis()
    }

    fn interrupted(&self) -> bool {
        self.stop_request.load(Ordering::SeqCst) || self.yield_request.load(Ordering::SeqCst)
    }

    /// Emit the current listening-change summary (call OUTSIDE the state
    /// lock — build the payload under the lock, emit after).
    pub(crate) fn emit_listening_change(&self) {
        let change = {
            let g = self.lock_inner();
            Ft8ListeningChange {
                service: ServiceAxisDto::from(g.machine.axis()),
                flags: g.machine.flags().into(),
                slot_phase: g.machine.slot_phase().into(),
                band: g.band.clone(),
                dial_hz: g.dial_hz,
                sweep: g.machine.sweep().into(),
            }
        };
        self.sink.emit_listening_change(&change);
    }

    fn set_blocked(&self, reason: BlockedReason, diagnostic: Option<String>) {
        {
            let mut g = self.lock_inner();
            g.machine.on_blocked(reason);
            if let Some(d) = diagnostic {
                g.last_failure = Some(d);
            }
        }
        self.emit_listening_change();
    }

    /// start / autostart entry (spec §Lifecycle table): spawns the
    /// supervisor from `stopped` ONLY; with a live supervisor it signals a
    /// sequence re-run instead. Callers run under spawn_blocking (T17).
    pub fn start(self: &Arc<Self>) -> Result<(), String> {
        if matches!(self.axis(), ServiceAxis::Blocked(BlockedReason::CaptureWedged)) {
            return Err(
                "the FT8 capture thread is wedged and may still hold the sound card; \
                 restart Tuxlink to recover"
                    .into(),
            );
        }
        let mut h = self.handles.lock().unwrap_or_else(|p| p.into_inner());
        let live = h.supervisor.as_ref().map(|s| !s.is_finished()).unwrap_or(false);
        if live {
            // Idempotent start: signal a sequence re-run.
            self.start_rerun_request.store(true, Ordering::SeqCst);
            self.unpark_supervisor();
            return Ok(());
        }
        // Reap a finished supervisor handle before respawn.
        if let Some(old) = h.supervisor.take() {
            let _ = old.join();
        }
        self.stop_request.store(false, Ordering::SeqCst);
        self.yield_request.store(false, Ordering::SeqCst);
        {
            let mut g = self.lock_inner();
            if !g.machine.on_start_requested() {
                return Err(format!("cannot start from {:?}", g.machine.axis()));
            }
        }
        let state = self.clone();
        let handle = std::thread::Builder::new()
            .name("ft8-supervisor".into())
            .spawn(move || supervisor_loop(state))
            .map_err(|e| format!("spawn ft8-supervisor: {e}"))?;
        *self.supervisor_thread.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(handle.thread().clone());
        h.supervisor = Some(handle);
        drop(h);
        self.emit_listening_change();
        Ok(())
    }

    pub(crate) fn unpark_supervisor(&self) {
        if let Some(t) = self
            .supervisor_thread
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            t.unpark();
        }
    }

    /// Full stop protocol (spec §Lifecycle ownership). Blocking-context
    /// only — the Tauri command wraps it in spawn_blocking (T17).
    ///
    /// Gate E P1 fix (tuxlink-qea6r): `capture-wedged` is a sticky, terminal
    /// axis — spec §Lifecycle ownership: "recovery from capture-wedged is
    /// app restart"; `start()` already refuses to run from it (:378-384).
    /// `stop()` has no such refusal (a defensive/second stop() is legitimate
    /// — it must still join whatever handles are live), so instead it seeds
    /// its local `wedged` decision from the PRE-CALL axis: a call that finds
    /// the axis already wedged stays wedged even if every handle it
    /// personally joins happens to land inside its bound.
    pub fn stop(&self) {
        let was_wedged = {
            let mut g = self.lock_inner();
            if matches!(g.machine.axis(), ServiceAxis::Stopped) {
                return;
            }
            let was_wedged = matches!(
                g.machine.axis(),
                ServiceAxis::Blocked(BlockedReason::CaptureWedged)
            );
            g.machine.on_stopping();
            was_wedged
        };
        self.emit_listening_change();
        self.stop_request.store(true, Ordering::SeqCst);
        self.capture_abort.store(true, Ordering::SeqCst);
        self.unpark_supervisor();

        // Seeded from the pre-call axis (see the fn doc comment) rather than
        // unconditional `false` — the prior bug: a wedge recorded by a
        // DIFFERENT path (e.g. `pause_capture_for_yield`'s force-detach) was
        // invisible to a later stop() call, which could then silently heal
        // the axis back to `Stopped`.
        let mut wedged = was_wedged;

        // 1. capture (if present): ≤ 2 s; PCM closes on drop.
        let capture = self.handles.lock().unwrap_or_else(|p| p.into_inner()).capture.take();
        if let Some(h) = capture {
            if let Err(detached) = join_bounded(h, CAPTURE_JOIN) {
                tracing::warn!(
                    target: "tuxlink::ft8",
                    "capture join overran {CAPTURE_JOIN:?} at stop — force-detaching; \
                     the detached thread may still hold the PCM (capture-wedged)"
                );
                drop(detached);
                wedged = true;
            }
        }

        // 2. drop the master Sender → decode's recv returns Disconnected.
        *self.master_tx.lock().unwrap_or_else(|p| p.into_inner()) = None;

        // 3. decode (if present): ≤ 16 s (covers the 14 s worst-case decode).
        let decode = self.handles.lock().unwrap_or_else(|p| p.into_inner()).decode.take();
        if let Some(h) = decode {
            if let Err(detached) = join_bounded(h, DECODE_JOIN) {
                tracing::warn!(target: "tuxlink::ft8", "decode join overran at stop — force-detaching");
                drop(detached);
                wedged = true;
            }
        }

        // 4. supervisor: ≤ 16 s (it may be inside an unabortable prewarm).
        let supervisor = self.handles.lock().unwrap_or_else(|p| p.into_inner()).supervisor.take();
        if let Some(h) = supervisor {
            self.unpark_supervisor();
            if let Err(detached) = join_bounded(h, SUPERVISOR_JOIN) {
                tracing::warn!(target: "tuxlink::ft8", "supervisor join overran at stop — force-detaching");
                drop(detached);
                wedged = true;
            }
        }
        *self.supervisor_thread.lock().unwrap_or_else(|p| p.into_inner()) = None;

        // 5. the runner is reconstructed on the next start.
        *self.engine.lock().unwrap_or_else(|p| p.into_inner()) = None;

        {
            let mut g = self.lock_inner();
            if wedged {
                g.machine.on_capture_wedged();
            } else {
                g.machine.on_stopped();
            }
        }
        self.emit_listening_change();
    }

    // ---- start sequence (spec §Start sequence; executed BY the supervisor;
    // pub(crate) so unit tests drive it synchronously) -----------------------

    /// Steps 1–8 (fresh start) or 1–7 + 8′ capture-only (resume /
    /// device-absent recovery). A yield/stop request is checked between
    /// every step; the flags never write the axis (pause already did).
    pub(crate) fn execute_start_sequence(self: &Arc<Self>, resume: bool) {
        if !resume {
            // Stale slot-dir sweep from crashed runs (spec §WAV writeout).
            self.sweep_stale_slot_dirs();
        }
        {
            let mut g = self.lock_inner();
            match g.machine.axis() {
                ServiceAxis::Starting => {}
                // Resume re-enters via on_resume (yielded → starting; k
                // reset; sweep re-arm).
                ServiceAxis::Yielded => g.machine.on_resume(),
                // Blocked re-entry (set_device retrigger, device-absent
                // retry) MUST go through on_start_requested — T6 permits
                // Blocked→Starting (+ sweep re-arm) there, while on_resume
                // is a no-op outside Yielded. Without this, the sequence
                // runs on a STALE blocked axis and a mid-sequence pause
                // (step 6 busy) is silently swallowed (on_pause from
                // blocked leaves the axis untouched).
                ServiceAxis::Blocked(r) if r != BlockedReason::CaptureWedged => {
                    let _ = g.machine.on_start_requested();
                }
                // Wedged / other axes: leave the machine alone (start()
                // already refuses wedged; defensive no-op here).
                _ => {}
            }
        }
        self.emit_listening_change();

        // Step 1: discover jt9 (start + resume — the delta's pinned probe
        // timing keeps discovery at exactly these moments).
        let bin = match self.platform.discover_jt9() {
            Ok(b) => b,
            Err(e) => return self.set_blocked(BlockedReason::WsjtxAbsent, Some(e)),
        };
        {
            let mut g = self.lock_inner();
            g.engine_version = Some(bin.engine_version.clone());
        }
        // No stored Jt9Binary: resume re-discovers (step 1 runs on both
        // paths) and make_engine consumes the fresh local — a cached copy
        // would be an unread field.
        if self.interrupted() {
            return;
        }

        // Step 2: resolve device.
        let stable_id = { self.lock_inner().ft8_cfg.device.clone() };
        let Some(stable_id) = stable_id else {
            return self.set_blocked(BlockedReason::NeedsDeviceSelection, None);
        };
        let Some(resolved) = self.platform.resolve_device(&stable_id) else {
            return self.set_blocked(
                BlockedReason::DeviceAbsent,
                Some(format!("configured device {:?} not found", stable_id.value)),
            );
        };
        {
            self.lock_inner().resolved = Some(resolved.clone());
        }
        if self.interrupted() {
            return;
        }

        // Step 3: clock probe → flag.
        let sync = self.clock.ntp_synchronized();
        {
            let mut g = self.lock_inner();
            g.machine.set_clock_unsynced(matches!(sync, ClockSync::Unsynced));
        }
        if matches!(sync, ClockSync::Unknown) {
            tracing::info!(target: "tuxlink::ft8", "clock sync unverifiable (timedatectl absent/unparseable)");
        }
        if self.interrupted() {
            return;
        }

        // Step 4: wisdom dir + prewarm — once per runner construction,
        // BEFORE any PCM is held. Skipped on resume (runner survives).
        let need_engine = self.engine.lock().unwrap_or_else(|p| p.into_inner()).is_none();
        if need_engine {
            let wisdom = self.platform.wisdom_dir();
            if let Err(e) = std::fs::create_dir_all(&wisdom) {
                tracing::warn!(target: "tuxlink::ft8", "wisdom dir create failed: {e} — proceeding (costs first-slot planning time)");
            }
            let engine = self.platform.make_engine(&bin, &wisdom);
            match engine.prewarm() {
                Ok(()) => {}
                Err(e) if e.contains("SpawnFailed") || e.contains("not found") => {
                    return self.set_blocked(BlockedReason::WsjtxAbsent, Some(e));
                }
                Err(e) => {
                    // A failed prewarm costs ~1.7 s planning on the first
                    // slots; it does not block listening (spec step 4).
                    tracing::warn!(target: "tuxlink::ft8", "jt9 prewarm failed (non-fatal): {e}");
                }
            }
            *self.engine.lock().unwrap_or_else(|p| p.into_inner()) = Some(engine);
        }
        if self.interrupted() {
            return;
        }

        // Step 5: CAT presence → flag / start-labeling rig session.
        if self.platform.rig_configured() {
            {
                self.lock_inner().machine.set_cat_fixed_band(false);
            }
            // Through the arbiter when installed: the ARBITER lock is what
            // excludes a concurrent pause_for_modem; start_rig_labeling
            // itself owns the rig lock (lock order arbiter > rig > state,
            // each acquired at most once per thread).
            let label = || self.start_rig_labeling();
            match crate::ft8::arbiter::FT8_ARBITER.get() {
                Some(arb) => arb.rig_session(label),
                None => label(),
            }
        } else {
            let mut g = self.lock_inner();
            g.machine.set_cat_fixed_band(true);
            // cat-absent: the snapshot instructs the dial for the chip band.
            g.dial_hz =
                tuxlink_capture::bands::dial_hz(&g.ft8_cfg.band).unwrap_or(g.dial_hz);
            g.band = g.ft8_cfg.band.clone();
        }
        if self.interrupted() {
            return;
        }

        // Step 6: busy probe — the hold latch is consulted here too (a fresh
        // start inside a pause-to-modem-open window must not steal the card).
        let busy = self.hold.is_latched()
            || self
                .platform
                .probe_busy(&resolved.alsa_plughw, resolved.card_index)
                .is_err();
        if busy {
            {
                self.lock_inner().machine.on_pause();
            }
            self.emit_listening_change();
            return;
        }
        if self.interrupted() {
            return;
        }

        // Step 7: ALSA open (hw:).
        let source = match self.platform.open_source(&resolved.alsa_hw) {
            Ok(s) => s,
            Err(SourceError::Busy) => {
                {
                    self.lock_inner().machine.on_pause();
                }
                self.emit_listening_change();
                return;
            }
            Err(SourceError::Absent) | Err(SourceError::Wedged) => {
                return self.set_blocked(BlockedReason::DeviceAbsent, None);
            }
            Err(SourceError::UnsupportedFormat(d)) => {
                return self.set_blocked(BlockedReason::UnsupportedSampleRate, Some(d));
            }
            Err(e) => {
                return self.set_blocked(BlockedReason::DeviceAbsent, Some(format!("{e:?}")));
            }
        };
        if self.interrupted() {
            // Past step 7 the supervisor holds the PCM: drop it BEFORE
            // abandoning the sequence (spec §Arbitration, starting case).
            drop(source);
            return;
        }

        // Step 8 / 8′: spawn workers → listening.
        self.spawn_workers(source, resume);
        // Lock discipline: rig_configured() reads config (file I/O) — probe
        // it BEFORE taking the state mutex (leaf-level, never held across
        // I/O; step 5 already follows this shape).
        let rig_ok = self.platform.rig_configured();
        {
            let mut g = self.lock_inner();
            g.machine.on_listening();
            // Sweep (re-)arms at start/resume when enabled + CAT (T16 wires
            // the dwell scheduler; arming is part of entering listening).
            if g.ft8_cfg.sweep.enabled && rig_ok {
                g.machine.sweep_activate();
            }
        }
        self.emit_listening_change();
    }

    /// Step-5 helper: one rig session — read dial, label band
    /// (nearest table entry within ±3 kHz, else "unknown"), tune to the
    /// configured band's dial if it differs, drop the session.
    ///
    /// Lock architecture (pinned): this helper OWNS the rig-lock
    /// acquisition; `Ft8Arbiter::rig_session` (T14) takes ONLY the arbiter
    /// lock and never the rig lock — lock order arbiter > rig > state, each
    /// acquired at most once per thread. T14 routes the step-5 call through
    /// `rig_session` (the arbiter cannot exist at this task's commit, so
    /// that one-match routing edit lands in T14); until then the rig lock
    /// alone serializes FT8 rig sessions against each other.
    fn start_rig_labeling(self: &Arc<Self>) {
        let _rig = self.rig_lock.lock().unwrap_or_else(|p| p.into_inner());
        let configured_dial = {
            let g = self.lock_inner();
            tuxlink_capture::bands::dial_hz(&g.ft8_cfg.band)
        };
        match self.platform.rig_read_dial() {
            Ok(dial) => {
                let label = nearest_band(dial);
                let now = self.platform.utc_now_ms();
                let mut tune_target = None;
                {
                    let mut g = self.lock_inner();
                    match label {
                        Some((band, table_dial)) => {
                            g.band = band.to_string();
                            g.dial_hz = table_dial;
                        }
                        None => {
                            g.band = "unknown".into();
                            g.dial_hz = dial;
                        }
                    }
                    g.band_source = BandSource::CatConfirmed;
                    g.band_label_confirmed_utc_ms = Some(now);
                    if let Some(cfg_dial) = configured_dial {
                        if g.dial_hz != cfg_dial {
                            tune_target = Some((g.ft8_cfg.band.clone(), cfg_dial));
                        }
                    }
                }
                if let Some((band, cfg_dial)) = tune_target {
                    // Starting the listener is the consenting action (RX-only).
                    match self.platform.rig_tune(cfg_dial) {
                        Ok(()) => {
                            let mut g = self.lock_inner();
                            g.band = band;
                            g.dial_hz = cfg_dial;
                            g.band_source = BandSource::CatConfirmed;
                            g.band_label_confirmed_utc_ms = Some(self.platform.utc_now_ms());
                        }
                        Err(e) => {
                            // Partial tune ⇒ dial position unknown (T16 test
                            // pins this downgrade).
                            let mut g = self.lock_inner();
                            g.band_source = BandSource::DefaultUnconfirmed;
                            g.band_label_confirmed_utc_ms = None;
                            g.last_failure = Some(format!("start QSY failed: {e}"));
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(target: "tuxlink::ft8", "start-labeling dial read failed: {e}");
                let mut g = self.lock_inner();
                g.band_source = BandSource::DefaultUnconfirmed;
                g.band_label_confirmed_utc_ms = None;
            }
        }
    }

    fn sweep_stale_slot_dirs(&self) {
        let root = self.platform.slot_dir_root();
        let Ok(entries) = std::fs::read_dir(&root) else { return };
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with("slot-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }

    /// Step 8 / 8′ (spec §Lifecycle): capture is spawned on every entry to
    /// listening; decode + the channel are spawned ONCE and survive yield
    /// and device loss (only stop() drops the master sender).
    ///
    /// Gate E P2 fix (tuxlink-qea6r): the last `interrupted()` check the
    /// start sequence makes before calling this function (immediately
    /// above, post-step-7 PCM-open) runs BEFORE this function's lock
    /// acquisition, so a stop() landing in that gap used to be lost — this
    /// function would still register live handles that stop() (already
    /// past its own capture/decode/supervisor takes) never joins, orphaning
    /// them. `handles` is the one lock both sides touch (stop() take()s
    /// capture/decode/supervisor under it, one field at a time; see
    /// `stop()`), so it is the serialization point: re-check `stop_request`
    /// immediately after taking it, before any handle is registered or
    /// `capture_abort` is reset, and hold it for the whole decision — both
    /// handles are assigned before the lock is released.
    fn spawn_workers(
        self: &Arc<Self>,
        source: Box<dyn crate::ft8::traits::SampleSource>,
        resume: bool,
    ) {
        let mut h = self.handles.lock().unwrap_or_else(|p| p.into_inner());
        if self.stop_request.load(Ordering::SeqCst) {
            // A stop() call landed between the caller's interrupted() check
            // and this lock acquisition. Close the PCM and bail without
            // spawning or touching capture_abort/master_tx — stop() already
            // owns those (it set capture_abort BEFORE taking this lock's
            // first field, and will drop master_tx itself). Not spawning
            // means stop()'s capture/decode/supervisor take()s — whichever
            // haven't already run — simply find nothing to join, same as any
            // other never-started sequence.
            drop(source);
            return;
        }
        // Reap a finished capture handle (post-yield / post-device-loss).
        if let Some(old) = h.capture.take() {
            if old.is_finished() {
                let _ = old.join();
            } else {
                // Should be unreachable: pause/stop join before respawn.
                tracing::warn!(target: "tuxlink::ft8", "capture respawn with live predecessor — detaching old");
            }
        }
        let decode_alive = h.decode.as_ref().map(|d| !d.is_finished()).unwrap_or(false);
        if !resume || !decode_alive {
            // Fresh channel + decode thread. sync_channel(0) = rendezvous:
            // try_send succeeds ONLY when decode is parked in recv (spec
            // §Backpressure — a 1-slot queue would drop N+2, not N+1).
            let (tx, rx) = std::sync::mpsc::sync_channel::<SlotJob>(0);
            *self.master_tx.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
            let engine = self
                .engine
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
                .expect("engine constructed at step 4 before step 8");
            let state = self.clone();
            if let Some(old) = h.decode.take() {
                let _ = old.join(); // finished (checked above)
            }
            h.decode = Some(
                std::thread::Builder::new()
                    .name("ft8-decode".into())
                    .spawn(move || decode_loop(state, engine, rx))
                    .expect("spawn ft8-decode"),
            );
        }
        self.capture_abort.store(false, Ordering::SeqCst);
        let tx = self
            .master_tx
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .expect("master sender lives until stop()");
        let state = self.clone();
        let abort = self.capture_abort.clone();
        h.capture = Some(
            std::thread::Builder::new()
                .name("ft8-capture".into())
                .spawn(move || capture_loop(state, source, tx, abort))
                .expect("spawn ft8-capture"),
        );
    }

    /// One completed slot: tmpfs dir + WAV + rendezvous handoff, with the
    /// three drop paths (storage / backpressure / scheduled QSY discard).
    fn handle_completed_slot(
        &self,
        slot: tuxlink_capture::slot::CompletedSlot,
        tx: &SyncSender<SlotJob>,
    ) {
        // Scheduled QSY-transition discard (T16 sets the flag).
        let discard = { self.lock_inner().discard_next_slot.take() };
        if let Some(class) = discard {
            self.record_slot(self.base_record(
                slot.slot_utc_ms,
                RingOutcome::Discarded { class },
                Vec::new(),
                SlotProvenance {
                    lost_frames: slot.lost_frames,
                    boundary_skew_frames: slot.boundary_skew_frames,
                    clip_fraction: slot.clip_fraction,
                    rms_dbfs: slot.rms_dbfs,
                },
            ));
            return;
        }
        let seq = self.slot_seq.fetch_add(1, Ordering::SeqCst);
        let dir = self
            .platform
            .slot_dir_root()
            .join(format!("slot-{}-{}", slot.slot_utc_ms, seq));
        let wav = dir.join("slot.wav");
        let write = std::fs::create_dir_all(&dir)
            .and_then(|()| self.platform.write_slot_wav(&wav, &slot.samples));
        if let Err(e) = write {
            // Storage failure is a DEFINED outcome (spec §WAV writeout):
            // counted toward N, best-effort cleanup, capture continues.
            let _ = std::fs::remove_dir_all(&dir);
            let diag = format!("slot WAV write failed: {e}");
            let mut rec = self.base_record(
                slot.slot_utc_ms,
                RingOutcome::DroppedStorageError { diagnostic: diag.clone() },
                Vec::new(),
                SlotProvenance {
                    lost_frames: slot.lost_frames,
                    boundary_skew_frames: slot.boundary_skew_frames,
                    clip_fraction: slot.clip_fraction,
                    rms_dbfs: slot.rms_dbfs,
                },
            );
            rec.partial_salvage = false;
            {
                self.lock_inner().last_failure = Some(diag);
            }
            self.record_slot(rec);
            return;
        }
        let job = SlotJob {
            slot_utc_ms: slot.slot_utc_ms,
            dir,
            wav,
            lost_frames: slot.lost_frames,
            boundary_skew_frames: slot.boundary_skew_frames,
            clip_fraction: slot.clip_fraction,
            rms_dbfs: slot.rms_dbfs,
        };
        match tx.try_send(job) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(job)) => {
                // Decode busy → THIS slot (N+1) drops; never queue.
                let _ = std::fs::remove_dir_all(&job.dir);
                tracing::info!(
                    target: "tuxlink::ft8",
                    slot_utc_ms = job.slot_utc_ms,
                    "slot dropped: decode still busy (backpressure)"
                );
                self.record_slot(self.base_record(
                    job.slot_utc_ms,
                    RingOutcome::DroppedBackpressure,
                    Vec::new(),
                    SlotProvenance {
                        lost_frames: job.lost_frames,
                        boundary_skew_frames: job.boundary_skew_frames,
                        clip_fraction: job.clip_fraction,
                        rms_dbfs: job.rms_dbfs,
                    },
                ));
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(job)) => {
                // stop() dropped the master sender mid-flight: clean up.
                let _ = std::fs::remove_dir_all(&job.dir);
            }
        }
    }

    /// Ring-record constructor stamped with the current band identity.
    fn base_record(
        &self,
        slot_utc_ms: u64,
        outcome: RingOutcome,
        decodes: Vec<crate::ft8::records::DecodeDto>,
        prov: SlotProvenance,
    ) -> SlotRecord {
        let SlotProvenance { lost_frames, boundary_skew_frames, clip_fraction, rms_dbfs } = prov;
        let g = self.lock_inner();
        let partial_salvage = decodes.iter().any(|d| d.partial);
        SlotRecord {
            slot_utc_ms,
            band: g.band.clone(),
            dial_hz: g.dial_hz,
            band_source: g.band_source,
            band_label_confirmed_utc_ms: g.band_label_confirmed_utc_ms,
            outcome,
            decodes,
            partial_salvage,
            lost_frames,
            boundary_skew_frames,
            clip_fraction,
            rms_dbfs,
            dwell_slot_index: match g.machine.sweep() {
                tuxlink_capture::state::Sweep::Active { dwell_progress, .. } => {
                    Some(dwell_progress)
                }
                _ => None,
            },
        }
    }

    /// Counter fold + ring push + emits. EVERY slot boundary lands here
    /// (spec §Ring: drops and discards included).
    pub(crate) fn record_slot(&self, rec: SlotRecord) {
        let flags_changed = {
            let mut g = self.lock_inner();
            let before = (g.machine.flags(), g.machine.slot_phase());
            g.machine.on_slot_outcome(rec.outcome.kind());
            if let RingOutcome::Failed { failure } = &rec.outcome {
                g.last_failure = Some(failure.clone());
            }
            g.last_slot_utc_ms = Some(rec.slot_utc_ms);
            if g.ring.len() == RING_CAP {
                g.ring.pop_front();
            }
            g.ring.push_back(rec.clone());
            before != (g.machine.flags(), g.machine.slot_phase())
        };
        self.sink.emit_slot(&rec);
        if flags_changed {
            self.emit_listening_change();
        }
    }

    /// Mid-run device loss (spec §Device loss): the capture thread calls
    /// this and returns; the PCM closes on drop; the supervisor retries
    /// every 5 s.
    pub(crate) fn on_device_lost(&self, diagnostic: Option<String>) {
        self.set_blocked(BlockedReason::DeviceAbsent, diagnostic);
    }

    // ---- sweep QSY (T16) ------------------------------------------------

    /// Spawn-tune-drop QSY to a table band + relabel + k-reset. This helper
    /// OWNS the rig-lock acquisition; every caller (sweep::tick, T17's
    /// ft8_set_band) ADDITIONALLY wraps the call in the arbiter's
    /// rig_session (arbiter-lock-only, T14) — the ARBITER lock is what
    /// excludes a concurrent pause_for_modem; the rig lock only serializes
    /// rig sessions against each other. Lock order arbiter > rig > state,
    /// each acquired at most once per thread. On failure the band label
    /// DOWNGRADES: a failed tune may have moved the dial anyway
    /// (freq-before-mode).
    pub(crate) fn qsy_to_band(
        self: &Arc<Self>,
        band: &str,
        source: BandSource,
    ) -> Result<(), String> {
        let dial = tuxlink_capture::bands::dial_hz(band)
            .ok_or_else(|| format!("{band:?} is not an FT8 band"))?;
        let result = {
            let rig = self.rig_lock();
            let _g = rig.lock().unwrap_or_else(|p| p.into_inner());
            self.platform.rig_tune(dial)
        };
        match result {
            Ok(()) => {
                {
                    let mut g = self.lock_inner();
                    g.band = band.to_string();
                    g.dial_hz = dial;
                    g.band_source = source;
                    g.band_label_confirmed_utc_ms = Some(self.platform.utc_now_ms());
                    g.machine.on_band_change(); // k resets on band change
                    // The slot in progress during the QSY is the transition
                    // slot: a scheduled discard.
                    g.discard_next_slot = Some(DiscardClassDto::QsyTransition);
                }
                self.emit_listening_change();
                Ok(())
            }
            Err(e) => {
                {
                    let mut g = self.lock_inner();
                    // Slots must NOT keep being attributed to the stale band
                    // with confirmed provenance — the dial position is now
                    // unknown.
                    g.band_source = BandSource::DefaultUnconfirmed;
                    g.band_label_confirmed_utc_ms = None;
                    g.last_failure = Some(format!("QSY failed: {e}"));
                }
                self.emit_listening_change();
                Err(e)
            }
        }
    }

    pub(crate) fn on_sweep_qsy_success(self: &Arc<Self>, next_idx: usize) {
        {
            self.lock_inner().machine.on_qsy_success(next_idx);
        }
        self.emit_listening_change();
    }
    pub(crate) fn on_sweep_qsy_failure(self: &Arc<Self>, _diag: String) {
        {
            self.lock_inner().machine.on_qsy_failure();
        }
        self.emit_listening_change();
    }

    // Narrow read accessors for sweep::tick (keep Inner private).
    pub(crate) fn lock_inner_for_sweep(&self) -> SweepView<'_> {
        SweepView { guard: self.lock_inner() }
    }

    // ---- snapshot -----------------------------------------------------------

    pub fn snapshot(&self) -> Ft8Snapshot {
        let g = self.lock_inner();
        let axis = g.machine.axis();
        // §Device selection is the ONE rule: devices embedded when device is
        // unset OR blocked on device-absent/needs-device-selection.
        let wants_devices = g.ft8_cfg.device.is_none()
            || matches!(
                axis,
                ServiceAxis::Blocked(BlockedReason::DeviceAbsent)
                    | ServiceAxis::Blocked(BlockedReason::NeedsDeviceSelection)
            );
        let ring_tail: Vec<SlotRecord> = g.ring.iter().rev().take(40).rev().cloned().collect();
        let snap = Ft8Snapshot {
            service: axis.into(),
            flags: g.machine.flags().into(),
            slot_phase: g.machine.slot_phase().into(),
            band: g.band.clone(),
            dial_hz: g.dial_hz,
            band_source: g.band_source,
            band_label_confirmed_utc_ms: g.band_label_confirmed_utc_ms,
            sweep: g.machine.sweep().into(),
            engine_version: g.engine_version.clone(),
            n_consecutive: g.machine.n_consecutive(),
            k_consecutive: g.machine.k_consecutive(),
            last_slot_utc_ms: g.last_slot_utc_ms,
            last_failure: g.last_failure.clone(),
            available_devices: None,
            ring_tail,
        };
        drop(g); // never hold the state lock across the enumeration I/O
        Ft8Snapshot {
            available_devices: wants_devices.then(|| self.platform.enumerate_capture()),
            ..snap
        }
    }

    #[cfg(test)]
    pub(crate) fn platform_tmp_for_test(&self) -> PathBuf {
        self.platform.slot_dir_root().parent().map(|p| p.to_path_buf()).unwrap_or_default()
    }

    /// Shared test helper (cross-module: `service.rs`'s own tests AND
    /// `arbiter.rs`'s tests both drive the start sequence synchronously):
    /// signal `on_start_requested` then run the sequence inline on the
    /// calling thread — no supervisor spawn.
    #[cfg(test)]
    pub(crate) fn test_run_sequence(self: &Arc<Self>) {
        {
            self.lock_inner().machine.on_start_requested();
        }
        self.execute_start_sequence(false);
    }

    /// Shared test helper: full stop + tmp-dir cleanup, for tests that don't
    /// need finer control over teardown ordering.
    #[cfg(test)]
    pub(crate) fn test_teardown(self: &Arc<Self>) {
        self.stop();
        let _ = std::fs::remove_dir_all(self.platform_tmp_for_test());
    }

    #[cfg(test)]
    pub(crate) fn snapshot_sweep(&self) -> tuxlink_capture::state::Sweep {
        self.lock_inner().machine.sweep()
    }
    #[cfg(test)]
    pub(crate) fn snapshot_ft8_cfg(&self) -> crate::config::Ft8Config {
        self.lock_inner().ft8_cfg.clone()
    }
    #[cfg(test)]
    pub(crate) fn test_base_record(
        &self,
        slot_utc_ms: u64,
        outcome: crate::ft8::records::RingOutcome,
    ) -> crate::ft8::records::SlotRecord {
        self.base_record(slot_utc_ms, outcome, Vec::new(), SlotProvenance {
            lost_frames: 0,
            boundary_skew_frames: 0,
            clip_fraction: 0.0,
            rms_dbfs: -60.0,
        })
    }
    #[cfg(test)]
    pub(crate) fn test_complete_one_slot(self: &Arc<Self>, slot_utc_ms: u64) {
        let tx = self.master_tx.lock().unwrap().clone().expect("listening");
        // Reuse the T12 test helper's CompletedSlot constructor.
        self.handle_completed_slot(test_completed_slot(slot_utc_ms), &tx);
    }
}

/// Read-only sweep view over the state mutex (one lock, three reads).
pub(crate) struct SweepView<'a> {
    guard: std::sync::MutexGuard<'a, Inner>,
}
impl SweepView<'_> {
    pub(crate) fn machine_sweep(&self) -> tuxlink_capture::state::Sweep {
        self.guard.machine.sweep()
    }
    pub(crate) fn machine_dwell_complete(&self, dwell_slots: u8) -> bool {
        self.guard.machine.dwell_complete(dwell_slots)
    }
    pub(crate) fn sweep_config(&self) -> crate::config::Ft8SweepConfig {
        self.guard.ft8_cfg.sweep.clone()
    }
}

/// Shared `CompletedSlot` test fixture (hoisted from T12's locally-scoped
/// `completed()` so both this module's test suite and `sweep.rs`'s test
/// suite — via `Ft8ListenerState::test_complete_one_slot` — can build one).
#[cfg(test)]
fn test_completed_slot(slot_utc_ms: u64) -> tuxlink_capture::slot::CompletedSlot {
    tuxlink_capture::slot::CompletedSlot {
        slot_utc_ms,
        samples: vec![0i16; tuxlink_capture::slot::OUT_SLOT_FRAMES],
        lost_frames: 0,
        boundary_skew_frames: 0,
        clip_fraction: 0.0,
        rms_dbfs: -60.0,
    }
}

/// The ALSA read loop → gap accounting → tap → slot assembler (spec
/// §Threads). The assembler owns the DECODE-path decimator; the tap runs a
/// second identical `Decimator` (same COEFFS, bit-identical output — see
/// the Phase C preamble's cross-cutting interface note).
fn capture_loop(
    state: Arc<Ft8ListenerState>,
    mut source: Box<dyn crate::ft8::traits::SampleSource>,
    tx: SyncSender<SlotJob>,
    abort: Arc<AtomicBool>,
) {
    use tuxlink_capture::decimator::Decimator;
    use tuxlink_capture::slot::{BoundaryConfig, SlotAssembler};

    let mut asm = SlotAssembler::new(BoundaryConfig::default());
    let mut tap_decim = Decimator::new();
    let mut tap_out: Vec<i16> = Vec::new();
    let mut buf = vec![0i16; 4_800]; // one 100 ms period

    loop {
        if abort.load(Ordering::SeqCst) {
            return; // PCM closes on source drop
        }
        let batch = match source.read(&mut buf) {
            Ok(b) => b,
            Err(SourceError::Suspended) => {
                // Clock-anomaly path: an empty push carrying the Suspended
                // gap makes the assembler abandon the slot; the source
                // already recovered its PCM.
                let events = asm.push(
                    &[],
                    state.platform.utc_now_ms(),
                    state.platform.mono_now_us(),
                    Some(tuxlink_capture::slot::GapReport {
                        kind: tuxlink_capture::slot::GapKind::Suspended,
                    }),
                );
                state.fold_slot_events(events, &tx);
                continue;
            }
            Err(SourceError::Absent) | Err(SourceError::Wedged) => {
                state.on_device_lost(None);
                return;
            }
            Err(e) => {
                state.on_device_lost(Some(format!("{e:?}")));
                return;
            }
        };
        let samples = &buf[..batch.frames];
        if !samples.is_empty() {
            tap_out.clear();
            tap_decim.process(samples, &mut tap_out);
            state.tap.push_samples(&tap_out);
        }
        let events = asm.push(samples, state.platform.utc_now_ms(), batch.mono_ts_us, batch.gap);
        state.fold_slot_events(events, &tx);
    }
}

impl Ft8ListenerState {
    pub(crate) fn fold_slot_events(
        &self,
        events: Vec<tuxlink_capture::slot::SlotEvent>,
        tx: &SyncSender<SlotJob>,
    ) {
        use tuxlink_capture::slot::{DiscardClass, DropClass, SlotEvent};
        for ev in events {
            self.slot_boundaries.fetch_add(1, Ordering::SeqCst);
            match ev {
                SlotEvent::Completed(slot) => self.handle_completed_slot(slot, tx),
                SlotEvent::Abandoned { class } => {
                    let dto = match class {
                        DiscardClass::FirstSlot => DiscardClassDto::FirstSlot,
                        DiscardClass::ClockAnomaly => DiscardClassDto::ClockAnomaly,
                    };
                    let utc = self.platform.utc_now_ms();
                    self.record_slot(self.base_record(
                        utc,
                        RingOutcome::Discarded { class: dto },
                        Vec::new(),
                        SlotProvenance {
                            lost_frames: 0,
                            boundary_skew_frames: 0,
                            clip_fraction: 0.0,
                            rms_dbfs: f32::NEG_INFINITY,
                        },
                    ));
                }
                SlotEvent::Dropped { class: DropClass::LostFrames, slot_utc_ms, lost_frames } => {
                    self.record_slot(self.base_record(
                        slot_utc_ms,
                        RingOutcome::DroppedLostFrames,
                        Vec::new(),
                        SlotProvenance {
                            lost_frames,
                            boundary_skew_frames: 0,
                            clip_fraction: 0.0,
                            rms_dbfs: f32::NEG_INFINITY,
                        },
                    ));
                }
            }
        }
    }
}

/// recv → decode → outcome fold → ring/event → slot-dir delete (spec
/// §Threads). Exits when the master sender drops (stop) — Disconnected is
/// the ONLY exit; no stop sentinel exists in this design.
fn decode_loop(
    state: Arc<Ft8ListenerState>,
    engine: Arc<dyn DecodeEngine>,
    rx: std::sync::mpsc::Receiver<SlotJob>,
) {
    use tuxlink_jt9::types::SlotOutcome;
    while let Ok(job) = rx.recv() {
        let outcome = engine.decode_slot(&job.wav, &job.dir, job.slot_utc_ms);
        let rec = match outcome {
            SlotOutcome::Decoded(decodes) => {
                let dtos: Vec<crate::ft8::records::DecodeDto> =
                    decodes.iter().map(Into::into).collect();
                state.base_record(
                    job.slot_utc_ms,
                    RingOutcome::Decoded,
                    dtos,
                    SlotProvenance {
                        lost_frames: job.lost_frames,
                        boundary_skew_frames: job.boundary_skew_frames,
                        clip_fraction: job.clip_fraction,
                        rms_dbfs: job.rms_dbfs,
                    },
                )
            }
            SlotOutcome::BandDead => state.base_record(
                job.slot_utc_ms,
                RingOutcome::BandDead,
                Vec::new(),
                SlotProvenance {
                    lost_frames: job.lost_frames,
                    boundary_skew_frames: job.boundary_skew_frames,
                    clip_fraction: job.clip_fraction,
                    rms_dbfs: job.rms_dbfs,
                },
            ),
            SlotOutcome::Failed(f) => state.base_record(
                job.slot_utc_ms,
                RingOutcome::Failed { failure: format!("{f:?}") },
                Vec::new(),
                SlotProvenance {
                    lost_frames: job.lost_frames,
                    boundary_skew_frames: job.boundary_skew_frames,
                    clip_fraction: job.clip_fraction,
                    rms_dbfs: job.rms_dbfs,
                },
            ),
        };
        state.record_slot(rec);
        let _ = std::fs::remove_dir_all(&job.dir);
    }
}

/// Nearest FT8 band within ±3 kHz of a dial reading (spec §Hold-band).
fn nearest_band(dial_hz: u64) -> Option<(&'static str, u64)> {
    tuxlink_capture::bands::BANDS
        .iter()
        .find(|(_, hz)| dial_hz.abs_diff(*hz) <= 3_000)
        .map(|&(b, hz)| (b, hz))
}

/// spec §Snapshot, field-for-field — the L3/L4 contract.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ft8Snapshot {
    pub service: ServiceAxisDto,
    pub flags: crate::ft8::records::HealthFlagsDto,
    pub slot_phase: crate::ft8::records::SlotPhaseDto,
    pub band: String,
    pub dial_hz: u64,
    pub band_source: BandSource,
    pub band_label_confirmed_utc_ms: Option<u64>,
    pub sweep: crate::ft8::records::SweepStatusDto,
    pub engine_version: Option<String>,
    pub n_consecutive: u8,
    pub k_consecutive: u8,
    pub last_slot_utc_ms: Option<u64>,
    pub last_failure: Option<String>,
    pub available_devices: Option<Vec<AudioDeviceChoice>>,
    pub ring_tail: Vec<SlotRecord>,
}

// ---- supervisor -------------------------------------------------------------

/// The service's owner and the FIRST thread spawned (spec §Threads). It
/// executes the start sequence, then ticks every 5 s: yielded-resume poll +
/// device-absent retry (T13), clock re-probe every 20 slot boundaries,
/// pipe-fd watermark every 100 (b026z.8), sweep dwell bookkeeping (T16),
/// hold-latch TTL (lazy — nothing to do here). It outlives every blocked
/// state; only stop() ends it.
fn supervisor_loop(state: Arc<Ft8ListenerState>) {
    {
        let mut base = state.pipe_fd_baseline.lock().unwrap_or_else(|p| p.into_inner());
        if base.is_none() {
            *base = state.platform.count_pipe_fds();
        }
    }
    state.execute_start_sequence(false);
    let mut last_clock_probe = 0u64;
    let mut last_watermark = 0u64;
    loop {
        std::thread::park_timeout(SUPERVISOR_TICK);
        if state.stop_request.load(Ordering::SeqCst) {
            return;
        }
        if state.start_rerun_request.swap(false, Ordering::SeqCst) {
            state.execute_start_sequence(false);
            continue;
        }
        match state.axis() {
            ServiceAxis::Yielded => state.tick_yielded(),           // T13
            ServiceAxis::Blocked(BlockedReason::DeviceAbsent) => state.tick_device_absent(), // T13
            ServiceAxis::Listening => {
                state.tick_listening(&mut last_clock_probe, &mut last_watermark)
            }
            _ => {}
        }
    }
}

impl Ft8ListenerState {
    /// The `listening`-case pause mechanics (spec §Arbitration): abort →
    /// join capture ≤ 2 s → (PCM closed by the capture thread's drop) →
    /// write yielded. Latch + release-confirm + rig cancellation live in the
    /// arbiter (T14) — pause is that transition's single writer and calls
    /// this. On join overrun: blocked(capture-wedged) + Err.
    pub(crate) fn pause_capture_for_yield(&self) -> Result<(), YieldJoinTimeout> {
        self.capture_abort.store(true, Ordering::SeqCst);
        let capture = self.handles.lock().unwrap_or_else(|p| p.into_inner()).capture.take();
        if let Some(h) = capture {
            if let Err(detached) = join_bounded(h, CAPTURE_JOIN) {
                drop(detached);
                {
                    self.lock_inner().machine.on_capture_wedged();
                }
                self.emit_listening_change();
                return Err(YieldJoinTimeout);
            }
        }
        {
            self.lock_inner().machine.on_pause();
        }
        self.emit_listening_change();
        Ok(())
    }

    /// The arbiter's `starting`-axis pause: no capture thread exists yet
    /// (spawned only at step 8), so there is nothing to join. The flag
    /// makes the supervisor abandon its sequence at the next between-step
    /// check (dropping a held PCM if past step 7); `on_pause` writes the
    /// axis — the flag itself never does (spec §Lifecycle ownership).
    pub(crate) fn request_yield_from_starting(&self) {
        self.yield_request.store(true, Ordering::SeqCst);
        {
            self.lock_inner().machine.on_pause();
        }
        self.emit_listening_change();
    }

    /// The last-resolved device's card index, for the arbiter's
    /// release-confirm probe. `None` when no sequence has resolved a device
    /// yet (e.g. yielded out of `starting` before step 2).
    pub(crate) fn resolved_card_index(&self) -> Option<u32> {
        self.lock_inner().resolved.as_ref().map(|r| r.card_index)
    }

    /// Resume conditions (spec §Resume — ALL must hold): latch clear, card
    /// probe free, modem session positively resume-eligible.
    pub(crate) fn resume_conditions_met(&self) -> bool {
        if self.hold.is_latched() {
            return false;
        }
        let resolved = { self.lock_inner().resolved.clone() };
        let probe_free = match resolved {
            Some(r) => self.platform.probe_busy(&r.alsa_plughw, r.card_index).is_ok(),
            // No resolution yet (yielded out of `starting` before step 2):
            // let the sequence re-run resolve it — treat as free.
            None => true,
        };
        probe_free && self.platform.modem_resume_eligible()
    }

    /// Supervisor tick, `yielded` axis: resume when all conditions hold.
    /// Positive latch clearing on observed card-busy also lives here (the
    /// modem actually acquired the card — the latch's job is done).
    pub(crate) fn tick_yielded(self: &Arc<Self>) {
        // Positive-evidence latch clear: card observed busy while latched.
        if self.hold.is_latched() {
            if let Some(r) = { self.lock_inner().resolved.clone() } {
                if self.platform.probe_busy(&r.alsa_plughw, r.card_index).is_err() {
                    self.hold.clear();
                }
            }
            return; // still latched or just cleared — resume next tick
        }
        if self.resume_conditions_met() {
            self.yield_request.store(false, Ordering::SeqCst);
            // Resume = steps 1–7 + 8′ capture-only (prewarm skipped: the
            // runner survives; jt9 discovery re-runs by design).
            self.execute_start_sequence(true);
        }
    }

    /// Supervisor tick, `blocked(device-absent)`: retry every tick (5 s).
    /// Identical path to resume — fresh re-resolution, capture-only respawn
    /// when the decode thread survives.
    pub(crate) fn tick_device_absent(self: &Arc<Self>) {
        self.execute_start_sequence(true);
    }

    /// The `listening`-axis supervisor tick body, extracted so the cadence
    /// test drives it directly (no 5 s parks): clock re-probe every 20 slot
    /// boundaries, pipe-fd watermark every 100 — cadences count BOUNDARIES
    /// via the capture-side atomic, never decoded slots.
    pub(crate) fn tick_listening(
        self: &Arc<Self>,
        last_clock_probe: &mut u64,
        last_watermark: &mut u64,
    ) {
        let boundaries = self.slot_boundaries.load(Ordering::SeqCst);
        if boundaries.saturating_sub(*last_clock_probe) >= CLOCK_REPROBE_BOUNDARIES {
            *last_clock_probe = boundaries;
            let sync = self.clock.ntp_synchronized();
            let changed = {
                let mut g = self.lock_inner();
                let before = g.machine.flags().clock_unsynced;
                g.machine
                    .set_clock_unsynced(matches!(sync, ClockSync::Unsynced));
                before != g.machine.flags().clock_unsynced
            };
            if changed {
                self.emit_listening_change();
            }
        }
        if boundaries.saturating_sub(*last_watermark) >= PIPE_WATERMARK_BOUNDARIES {
            *last_watermark = boundaries;
            self.check_pipe_watermark();
        }
        crate::ft8::sweep::tick(self);
    }

    /// Returns whether the watermark tripped (testable seam — the spec's
    /// named "pipe-fd watermark trip (fake /proc reader)" test drives this
    /// return value; the caller logs).
    pub(crate) fn check_pipe_watermark(&self) -> bool {
        let (Some(base), Some(now)) = (
            *self.pipe_fd_baseline.lock().unwrap_or_else(|p| p.into_inner()),
            self.platform.count_pipe_fds(),
        ) else {
            return false;
        };
        let tripped = now > base + PIPE_WATERMARK_EXCESS;
        if tripped {
            tracing::warn!(
                target: "tuxlink::ft8",
                baseline = base,
                current = now,
                "pipe-fd watermark exceeded — possible jt9 grandchild pipe-holder leak (tuxlink-b026z.8)"
            );
        }
        tripped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Ft8Config;
    use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink};
    use crate::winlink::ax25::devices::{StableAudioId, StableIdKind};
    use tuxlink_capture::state::{BlockedReason, ServiceAxis};

    fn test_state(platform: Arc<FakePlatform>, cfg: Ft8Config) -> Arc<Ft8ListenerState> {
        Ft8ListenerState::new(
            Ft8Deps {
                platform,
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: Arc::new(RecordingSink::default()),
            },
            cfg,
        )
    }

    fn cfg_with_device() -> Ft8Config {
        let mut c = Ft8Config::default();
        c.enabled = true;
        c.device = Some(StableAudioId {
            kind: StableIdKind::ByIdSymlink,
            value: "usb-DRA-100-00".into(),
        });
        c
    }

    fn run_sequence(state: &Arc<Ft8ListenerState>) {
        state.test_run_sequence();
    }

    // Arrow 1: jt9 absent → blocked(wsjtx-absent); the snapshot still
    // carries available_devices when device is ALSO unset (§Device
    // selection's one rule — both first-contact blockers in one visit).
    #[test]
    fn arrow1_jt9_absent_blocks_wsjtx_absent_and_still_offers_devices() {
        let p = FakePlatform::happy();
        *p.jt9.lock().unwrap() = Err("NotOnPath".into());
        let state = test_state(p, Ft8Config::default()); // device: None
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::WsjtxAbsent));
        let snap = state.snapshot();
        let devs = snap.available_devices.expect("picker must render while blocked on wsjtx");
        assert_eq!(devs.len(), 1);
        let _ = std::fs::remove_dir_all(&state.platform_tmp_for_test());
    }

    // Arrow 2a: device None → needs-device-selection.
    #[test]
    fn arrow2_no_device_blocks_needs_device_selection() {
        let p = FakePlatform::happy();
        let state = test_state(p, Ft8Config::default());
        run_sequence(&state);
        assert_eq!(
            state.axis(),
            ServiceAxis::Blocked(BlockedReason::NeedsDeviceSelection)
        );
    }

    // Arrow 2b: persisted-but-unresolvable → device-absent (supervisor-
    // retried; the retry itself is T13's test).
    #[test]
    fn arrow2_unresolvable_device_blocks_device_absent() {
        let p = FakePlatform::happy();
        *p.resolved.lock().unwrap() = None;
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::DeviceAbsent));
        // Stale-device snapshot ALSO offers the picker (§Device selection).
        assert!(state.snapshot().available_devices.is_some());
    }

    // Arrow 3: clock probe sets the flag; Unknown does NOT.
    #[test]
    fn arrow3_clock_probe_drives_the_flag() {
        for (sync, want) in [
            (crate::ft8::clock::ClockSync::Unsynced, true),
            (crate::ft8::clock::ClockSync::Synced, false),
            (crate::ft8::clock::ClockSync::Unknown, false),
        ] {
            let p = FakePlatform::happy();
            let state = Ft8ListenerState::new(
                Ft8Deps {
                    platform: p,
                    clock: FakeClock::new(sync),
                    sink: Arc::new(RecordingSink::default()),
                },
                cfg_with_device(),
            );
            run_sequence(&state);
            assert_eq!(state.snapshot().flags.clock_unsynced, want, "{sync:?}");
            // Reaches Listening 3x — spawn_workers is real since T12: reap
            // the worker threads + tmp dir each iteration.
            teardown(&state);
        }
    }

    // Arrow 4: prewarm spawn-class failure → wsjtx-absent; any other
    // prewarm failure proceeds to listening.
    #[test]
    fn arrow4_prewarm_failure_classes() {
        use crate::ft8::testutil::FakeEngine;
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead();
        *eng.prewarm_result.lock().unwrap() =
            Err("SpawnFailed(\"No such file\")".into());
        *p.engine.lock().unwrap() = eng;
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::WsjtxAbsent));

        let p2 = FakePlatform::happy();
        let eng2 = FakeEngine::band_dead();
        *eng2.prewarm_result.lock().unwrap() = Err("Timeout".into());
        *p2.engine.lock().unwrap() = eng2;
        let state2 = test_state(p2, cfg_with_device());
        run_sequence(&state2);
        assert_eq!(state2.axis(), ServiceAxis::Listening, "non-spawn prewarm failure proceeds");
        teardown(&state2);
    }

    // Arrow 5: CAT absent → cat-fixed-band + instructed dial; CAT present →
    // start-labeling (cat-confirmed) + tune-if-differs.
    #[test]
    fn arrow5_cat_presence_labels_or_flags() {
        // Absent.
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        let snap = state.snapshot();
        assert!(snap.flags.cat_fixed_band);
        assert_eq!(snap.band, "20m");
        assert_eq!(snap.dial_hz, 14_074_000, "instructed dial for the chip band");
        assert_eq!(snap.band_source, crate::ft8::records::BandSource::DefaultUnconfirmed);
        teardown(&state);

        // Present, radio on 40m, configured chip 20m → labeled then retuned.
        let p2 = FakePlatform::happy();
        *p2.rig_configured.lock().unwrap() = true;
        *p2.rig_dial.lock().unwrap() = Ok(7_074_000);
        let state2 = test_state(p2.clone(), cfg_with_device());
        run_sequence(&state2);
        let snap2 = state2.snapshot();
        assert!(!snap2.flags.cat_fixed_band);
        assert_eq!(snap2.band_source, crate::ft8::records::BandSource::CatConfirmed);
        assert!(snap2.band_label_confirmed_utc_ms.is_some());
        assert_eq!(*p2.tuned_to.lock().unwrap(), vec![14_074_000], "tuned to the configured band");
        assert_eq!(snap2.band, "20m");
        teardown(&state2);
    }

    // Arrow 6: busy probe busy → yielded; hold latch latched → treated as
    // busy even when the probe reads free.
    #[test]
    fn arrow6_busy_or_latched_yields() {
        let p = FakePlatform::happy();
        *p.busy.lock().unwrap() = Err("plughw:CARD=DRA,DEV=0 is in use".into());
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Yielded);

        let p2 = FakePlatform::happy();
        let state2 = test_state(p2, cfg_with_device());
        state2.hold().latch_now();
        run_sequence(&state2);
        assert_eq!(state2.axis(), ServiceAxis::Yielded, "latched hold is treated as busy");
    }

    // Arrow 7: open errors map EBUSY→yielded, absent→device-absent,
    // param→unsupported-sample-rate (with diagnostic).
    #[test]
    fn arrow7_open_error_mapping() {
        use crate::ft8::traits::SourceError;
        let cases = [
            (SourceError::Busy, None),
            (SourceError::Absent, Some(BlockedReason::DeviceAbsent)),
            (
                SourceError::UnsupportedFormat("rate 44100 only".into()),
                Some(BlockedReason::UnsupportedSampleRate),
            ),
        ];
        for (err, want_block) in cases {
            let p = FakePlatform::happy();
            p.open_results.lock().unwrap().push_back(Err(err.clone()));
            let state = test_state(p, cfg_with_device());
            run_sequence(&state);
            match want_block {
                None => assert_eq!(state.axis(), ServiceAxis::Yielded, "{err:?}"),
                Some(b) => assert_eq!(state.axis(), ServiceAxis::Blocked(b), "{err:?}"),
            }
        }
        // The diagnostic surfaces.
        let p = FakePlatform::happy();
        p.open_results
            .lock()
            .unwrap()
            .push_back(Err(SourceError::UnsupportedFormat("rate 44100 only".into())));
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.snapshot().last_failure.as_deref(), Some("rate 44100 only"));
    }

    // Arrow 8: happy path lands listening / waiting-first-slot.
    #[test]
    fn arrow8_happy_path_reaches_listening() {
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert_eq!(
            state.snapshot().slot_phase,
            crate::ft8::records::SlotPhaseDto::WaitingFirstSlot
        );
        teardown(&state);
    }

    // Autostart contract: enabled=true, device=None → the supervisor lands
    // blocked(needs-device-selection) — the state that RESUMES the
    // interrupted first-contact flow (never silently stopped).
    #[test]
    fn autostart_with_no_device_lands_needs_device_selection() {
        let p = FakePlatform::happy();
        let state = test_state(p, Ft8Config { enabled: true, ..Ft8Config::default() });
        state.start().expect("start spawns the supervisor");
        // The supervisor runs the sequence async; poll briefly.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if state.axis() == ServiceAxis::Blocked(BlockedReason::NeedsDeviceSelection) {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "axis: {:?}", state.axis());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Idempotent start: a second start() with a live supervisor is Ok
        // and does not spawn a second supervisor.
        state.start().expect("idempotent start");
        teardown(&state);
    }

    /// Pipe-fd watermark trip via the fake /proc reader (spec §Testing:
    /// named test; tuxlink-b026z.8). Baseline is captured at supervisor
    /// spawn; here we seed it directly and drive the counter.
    #[test]
    fn pipe_fd_watermark_trips_only_past_the_excess_threshold() {
        let p = FakePlatform::happy();
        let state = test_state(p.clone(), cfg_with_device());
        *state.pipe_fd_baseline.lock().unwrap() = Some(8);
        *p.pipe_fds.lock().unwrap() = Some(8 + PIPE_WATERMARK_EXCESS); // == threshold: no trip
        assert!(!state.check_pipe_watermark());
        *p.pipe_fds.lock().unwrap() = Some(8 + PIPE_WATERMARK_EXCESS + 1); // > threshold: trip
        assert!(state.check_pipe_watermark());
        *p.pipe_fds.lock().unwrap() = None; // /proc unreadable: never trips
        assert!(!state.check_pipe_watermark());
    }

    /// Supervisor cadence wiring (spec: cadences count BOUNDARIES): driving
    /// the capture-side boundary atomic through tick_listening, the clock is
    /// probed once per 20-boundary window and the pipe watermark read once
    /// per 100-boundary window — pinned via the fakes' call counters.
    #[test]
    fn supervisor_cadences_fire_per_boundary_window() {
        let p = FakePlatform::happy();
        let clock = FakeClock::new(crate::ft8::clock::ClockSync::Synced);
        let state = Ft8ListenerState::new(
            Ft8Deps {
                platform: p.clone(),
                clock: clock.clone(),
                sink: Arc::new(RecordingSink::default()),
            },
            cfg_with_device(),
        );
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Listening);
        *state.pipe_fd_baseline.lock().unwrap() = Some(8);
        // Deltas from here: run_sequence already probed the clock once
        // (step 3) and never read the pipe count.
        let probes_before = clock.probe_calls.load(Ordering::SeqCst);
        let fd_reads_before = p.pipe_fd_calls.load(Ordering::SeqCst);
        let (mut last_probe, mut last_watermark) = (0u64, 0u64);
        for boundary in 1..=200u64 {
            state.slot_boundaries.store(boundary, Ordering::SeqCst);
            state.tick_listening(&mut last_probe, &mut last_watermark);
        }
        assert_eq!(
            clock.probe_calls.load(Ordering::SeqCst) - probes_before,
            10, // boundaries 20, 40, …, 200: one probe per 20-boundary window
            "clock re-probe cadence"
        );
        assert_eq!(
            p.pipe_fd_calls.load(Ordering::SeqCst) - fd_reads_before,
            2, // boundaries 100 and 200: one read per 100-boundary window
            "pipe watermark cadence"
        );
        teardown(&state);
    }

    // Snapshot completeness: every §Snapshot field is present + serializes.
    #[test]
    fn snapshot_carries_every_contract_field() {
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        let snap = state.snapshot();
        let v = serde_json::to_value(&snap).unwrap();
        for field in [
            "service", "flags", "slotPhase", "band", "dialHz", "bandSource",
            "bandLabelConfirmedUtcMs", "sweep", "engineVersion", "nConsecutive",
            "kConsecutive", "lastSlotUtcMs", "lastFailure", "availableDevices",
            "ringTail",
        ] {
            assert!(v.get(field).is_some(), "snapshot missing {field}: {v}");
        }
        assert_eq!(v["engineVersion"], "WSJT-X test 0.0");
        assert_eq!(v["service"]["axis"], "listening");
        teardown(&state);
    }

    use crate::ft8::records::RingOutcome;
    use crate::ft8::testutil::{FakeEngine, SourceStep};

    /// Backpressure (spec §Backpressure): with decode parked busy, slot N
    /// decodes, slot N+1 SPECIFICALLY drops (dir deleted, N incremented,
    /// ring-recorded), N+2 decodes after release.
    #[test]
    fn backpressure_drops_slot_n_plus_1_specifically() {
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead();
        *p.engine.lock().unwrap() = eng.clone();
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state); // spawns decode via spawn_workers
        assert_eq!(state.axis(), ServiceAxis::Listening);
        let tx = state.master_tx.lock().unwrap().clone().unwrap();

        // Slot 1: decode accepts it, then we gate the engine busy.
        eng.hold_gate();
        state.handle_completed_slot(test_completed_slot(1_000), &tx);
        // Wait until decode has STARTED slot 1 (parked on the gate).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while eng.decodes_started.load(Ordering::SeqCst) < 1 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Slot 2 (N+1): rendezvous refuses — dropped, dir deleted.
        state.handle_completed_slot(test_completed_slot(2_000), &tx);
        {
            let g = state.lock_inner();
            let dropped: Vec<_> = g
                .ring
                .iter()
                .filter(|r| r.outcome == RingOutcome::DroppedBackpressure)
                .collect();
            assert_eq!(dropped.len(), 1);
            assert_eq!(dropped[0].slot_utc_ms, 2_000, "slot N+1 specifically");
            assert_eq!(g.machine.n_consecutive(), 1, "backpressure drop counts toward N");
        }
        // No orphan dir for the dropped slot.
        let root = state.platform.slot_dir_root();
        let leftovers: Vec<_> = std::fs::read_dir(&root)
            .map(|it| it.flatten().collect())
            .unwrap_or_default();
        assert!(
            leftovers.iter().all(|e: &std::fs::DirEntry| {
                !e.file_name().to_string_lossy().starts_with("slot-2000-")
            }),
            "dropped slot dir must be deleted immediately"
        );
        // Release: slot 1 finishes (BandDead clears nothing here — BandDead
        // clears N per types.rs), slot 3 flows.
        eng.release_gate();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while eng.decodes_finished.load(Ordering::SeqCst) < 1 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        state.handle_completed_slot(test_completed_slot(3_000), &tx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while eng.decodes_finished.load(Ordering::SeqCst) < 2 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        teardown(&state);
    }

    /// Storage failure (spec §WAV writeout): ENOSPC-class write error →
    /// DroppedStorageError recorded, N incremented, last_failure set, no
    /// panic, capture path continues (the next slot writes fine).
    #[test]
    fn storage_failure_is_a_defined_outcome_and_no_stall() {
        let p = FakePlatform::happy();
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state);
        let tx = state.master_tx.lock().unwrap().clone().unwrap();
        *p.wav_result.lock().unwrap() = Err("No space left on device (os error 28)".into());
        state.handle_completed_slot(test_completed_slot(1_000), &tx);
        {
            let g = state.lock_inner();
            assert!(matches!(
                g.ring.back().unwrap().outcome,
                RingOutcome::DroppedStorageError { .. }
            ));
            assert_eq!(g.machine.n_consecutive(), 1);
            assert!(g.last_failure.as_deref().unwrap().contains("No space left"));
        }
        // Recovery: the next slot flows to decode.
        *p.wav_result.lock().unwrap() = Ok(());
        state.handle_completed_slot(test_completed_slot(2_000), &tx);
        let snap_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let done = {
                let g = state.lock_inner();
                g.ring.iter().any(|r| r.slot_utc_ms == 2_000 && r.outcome == RingOutcome::BandDead)
            };
            if done {
                break;
            }
            assert!(std::time::Instant::now() < snap_deadline, "capture stalled after ENOSPC");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        teardown(&state);
    }

    /// Tap (spec §Waterfall tap): drop-oldest under a stalled consumer; and
    /// pushing through the tap never slips a slot boundary (the assembler
    /// sees the identical sample stream).
    #[test]
    fn tap_drops_oldest_and_never_blocks() {
        let tap = WaterfallTap::default();
        tap.subscribe();
        // 40 blocks' worth: 8 oldest must be gone, newest 32 retained.
        for i in 0..40 {
            let block = vec![i as i16; TAP_BLOCK_FRAMES];
            tap.push_samples(&block);
        }
        let blocks = tap.take_blocks();
        assert_eq!(blocks.len(), TAP_CAPACITY_BLOCKS);
        assert_eq!(blocks[0][0], 8, "oldest 8 dropped");
        assert_eq!(blocks[31][0], 39);
        // Unsubscribed: pushes are free and retain nothing.
        tap.unsubscribe();
        tap.push_samples(&vec![1i16; TAP_BLOCK_FRAMES * 2]);
        assert!(tap.take_blocks().is_empty());
    }

    /// No boundary slip: a full scripted slot through capture_loop with a
    /// stalled tap consumer still emits exactly its slots on the synthetic
    /// boundaries.
    #[test]
    fn tap_pressure_does_not_slip_slot_boundaries() {
        let p = FakePlatform::happy();
        let state = test_state(p.clone(), cfg_with_device());
        state.tap().subscribe(); // subscribed but never drained = max pressure
        // Script: 30 s of audio (2 slots' worth) + idle. happy()'s clock
        // epoch is 1_760_000_000_000 ms; mod 15_000 = 5_000, so synthetic
        // time starts 5 s PAST a boundary — 10 s BEFORE the next one. The
        // assembler anchors at the NEXT boundary (T4 pinned next-boundary
        // semantics): boundary 1 lands at +10 s and emits the scheduled
        // FirstSlot discard; boundary 2 lands at +25 s and emits one
        // Completed slot. 30 s of audio therefore produces exactly 2
        // boundary events — the `>= 2` assertion below.
        for _ in 0..(2 * 720_000 / 4_800) {
            p.source_steps
                .lock()
                .unwrap()
                .push_back(SourceStep::Frames { frames: 4_800, value: 100, gap: None });
        }
        run_sequence(&state);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.slot_boundaries.load(Ordering::SeqCst) < 2 {
            assert!(std::time::Instant::now() < deadline, "boundary slipped under tap pressure");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        teardown(&state);
    }

    /// The ring records ALL outcome kinds (spec §Ring: every boundary yields
    /// a record — drops and discards included) and the counters fold per
    /// §Counter semantics.
    #[test]
    fn ring_records_every_outcome_kind() {
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        let recs = [
            RingOutcome::Decoded,
            RingOutcome::BandDead,
            RingOutcome::Failed { failure: "Timeout".into() },
            RingOutcome::DroppedBackpressure,
            RingOutcome::DroppedLostFrames,
            RingOutcome::DroppedStorageError { diagnostic: "ENOSPC".into() },
            RingOutcome::Discarded { class: DiscardClassDto::ClockAnomaly },
        ];
        for (i, o) in recs.iter().enumerate() {
            state.record_slot(state.base_record(
                i as u64,
                o.clone(),
                Vec::new(),
                SlotProvenance {
                    lost_frames: 0,
                    boundary_skew_frames: 0,
                    clip_fraction: 0.0,
                    rms_dbfs: -60.0,
                },
            ));
        }
        let g = state.lock_inner();
        assert_eq!(g.ring.len(), recs.len());
        // Counter spot-checks: the trailing Failed+drops streak after the
        // last BandDead: Failed, DroppedBackpressure, DroppedLostFrames,
        // DroppedStorageError count toward N; the final Discarded does NOT.
        assert_eq!(g.machine.n_consecutive(), 4, "scheduled discard is counter-neutral");
    }

    /// Ring eviction: capacity 240 — the 241st record evicts the OLDEST.
    #[test]
    fn ring_evicts_oldest_at_capacity() {
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        for i in 0..241u64 {
            state.record_slot(state.base_record(
                i,
                RingOutcome::BandDead,
                Vec::new(),
                SlotProvenance {
                    lost_frames: 0,
                    boundary_skew_frames: 0,
                    clip_fraction: 0.0,
                    rms_dbfs: -60.0,
                },
            ));
        }
        let g = state.lock_inner();
        assert_eq!(g.ring.len(), RING_CAP);
        assert_eq!(g.ring.front().unwrap().slot_utc_ms, 1, "slot 0 evicted");
        assert_eq!(g.ring.back().unwrap().slot_utc_ms, 240);
    }

    fn teardown(state: &Arc<Ft8ListenerState>) {
        state.test_teardown();
    }

    /// Asserts the §Lifecycle threads-per-state table for the current axis.
    fn assert_thread_liveness(state: &Arc<Ft8ListenerState>) {
        let h = state.handles.lock().unwrap();
        let alive = |o: &Option<std::thread::JoinHandle<()>>| {
            o.as_ref().map(|j| !j.is_finished()).unwrap_or(false)
        };
        let (sup, cap, dec) = (alive(&h.supervisor), alive(&h.capture), alive(&h.decode));
        drop(h);
        match state.axis() {
            ServiceAxis::Stopped => {
                assert!(!sup && !cap && !dec, "stopped: no threads");
            }
            ServiceAxis::Blocked(BlockedReason::CaptureWedged) => {} // detached: unknowable
            ServiceAxis::Blocked(_) | ServiceAxis::Starting => {
                assert!(!cap, "blocked/starting: no capture thread");
            }
            ServiceAxis::Yielded => {
                assert!(!cap, "yielded: capture joined");
                assert!(dec, "yielded: decode survives");
            }
            ServiceAxis::Listening => {
                assert!(cap && dec, "listening: capture + decode alive");
            }
            ServiceAxis::Stopping => {}
        }
    }

    /// Stop during an in-flight decode (slow fake engine): completes WITHOUT
    /// the force-detach path — the 16 s decode bound absorbs the 14 s
    /// worst case (spec §Lock discipline names this exact test).
    #[test]
    fn stop_during_inflight_decode_completes_without_force_detach() {
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead();
        *p.engine.lock().unwrap() = eng.clone();
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        let tx = state.master_tx.lock().unwrap().clone().unwrap();
        eng.hold_gate();
        state.handle_completed_slot(test_completed_slot(1_000), &tx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while eng.decodes_started.load(Ordering::SeqCst) < 1 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Release the gate from a helper thread ~200 ms into the stop, well
        // inside the 16 s decode bound.
        let eng2 = eng.clone();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            eng2.release_gate();
        });
        state.stop();
        releaser.join().unwrap();
        assert_eq!(state.axis(), ServiceAxis::Stopped, "no capture-wedged");
        assert_thread_liveness(&state);
        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }

    /// Gate E P1 regression (tuxlink-qea6r): stop() must never heal a
    /// wedged axis back to `Stopped`. Force a REAL wedge — a Park'd capture
    /// read past `CAPTURE_JOIN`'s 2 s bound, the same mechanism as
    /// `arbiter.rs`'s `wedged_capture_join_yields_capture_wedged_error` —
    /// via stop()'s OWN capture join overrunning, then call stop() again.
    /// Pre-fix, the second call's local `wedged` flag reset to `false` and
    /// found nothing left to join (capture + decode were already reaped by
    /// the first call), so it silently wrote `Stopped` over a still-wedged
    /// axis.
    #[test]
    fn stop_preserves_capture_wedged_across_repeated_calls() {
        let p = FakePlatform::happy();
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Listening);

        // Park the capture read: the abort flag cannot reach a blocked read
        // (testutil::SourceStep::Park), so stop()'s own capture join
        // overruns CAPTURE_JOIN and force-detaches.
        let park = crate::ft8::testutil::park_flag();
        p.source_steps
            .lock()
            .unwrap()
            .push_back(crate::ft8::testutil::SourceStep::Park(park.clone()));
        std::thread::sleep(std::time::Duration::from_millis(100)); // capture enters the parked read

        // Release the park ~2.5 s in — comfortably past CAPTURE_JOIN's 2 s
        // bound (a wide-enough margin that scheduler jitter on a loaded CI
        // runner can't make the release race the join_bounded deadline
        // check), so the capture join reliably overruns first — from a
        // helper thread. Without this, the detached zombie keeps holding its
        // own `tx` clone forever and decode's SEPARATE 16 s join bound would
        // also overrun (a second, unrelated wedge this test isn't about).
        let releaser = {
            let park = park.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(2_500));
                park.store(false, Ordering::SeqCst);
            })
        };
        state.stop();
        releaser.join().unwrap();
        assert_eq!(
            state.axis(),
            ServiceAxis::Blocked(BlockedReason::CaptureWedged),
            "capture join overran its bound at stop() — axis must read wedged"
        );

        // The regression: a second stop() call must NOT heal the axis.
        state.stop();
        assert_eq!(
            state.axis(),
            ServiceAxis::Blocked(BlockedReason::CaptureWedged),
            "a second stop() call must not heal a wedged axis back to Stopped"
        );

        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }

    /// Gate E P2 regression (tuxlink-qea6r): spawn_workers must re-check
    /// `stop_request` under the SAME `handles` lock stop() takes when it
    /// take()s — the one point both sides touch. This pins the deterministic
    /// half of the fix: a `stop_request` that is already set by the time
    /// spawn_workers acquires the lock must abort cleanly (no spawn, no
    /// `capture_abort` reset, no channel registration).
    ///
    /// What this test does NOT pin: calling `spawn_workers` directly with
    /// `stop_request` pre-set is not a true concurrent race against a live
    /// `stop()` call landing inside the interrupted()-check→lock-acquisition
    /// gap — this crate's fakes have no seam to pause `spawn_workers`
    /// mid-acquisition, so the narrow interleaving window itself (the
    /// TOCTOU the finding describes) is not independently exercised here,
    /// only the recheck's effect once the flag is observed.
    #[test]
    fn spawn_workers_aborts_without_spawning_when_stop_already_requested() {
        let p = FakePlatform::happy();
        let state = test_state(p.clone(), cfg_with_device());
        let source = p.open_source("hw:1,0").expect("fake source always opens");

        // Simulates stop() having already run its pre-lock store()s (the
        // first three lines of stop()'s body) before spawn_workers reaches
        // the handles lock.
        state.stop_request.store(true, Ordering::SeqCst);
        state.capture_abort.store(true, Ordering::SeqCst);

        state.spawn_workers(source, false);

        let h = state.handles.lock().unwrap();
        assert!(h.capture.is_none(), "no capture thread once stop_request is observed");
        assert!(h.decode.is_none(), "no decode thread once stop_request is observed");
        drop(h);
        assert!(
            state.capture_abort.load(Ordering::SeqCst),
            "spawn_workers must not clear capture_abort once stop_request is observed \
             — stop() owns that flag from this point on"
        );
        assert!(
            state.master_tx.lock().unwrap().is_none(),
            "no channel/master_tx registered once stop_request is observed"
        );

        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }

    /// Stop during `starting`, mid-prewarm: the stop-request is honored at
    /// the next between-step check; the supervisor join bound covers the
    /// blocking prewarm; NO capture-wedged (no capture thread ever existed).
    /// Deterministic by construction: the gate is held BEFORE start(), so
    /// the sequence parks INSIDE prewarm at step 4.
    #[test]
    fn stop_during_starting_mid_prewarm_completes_clean() {
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead_with_prewarm_gate(); // Step 0b
        eng.hold_gate(); // BEFORE start(): the sequence parks at step 4
        *p.engine.lock().unwrap() = eng.clone();
        let state = test_state(p, cfg_with_device());
        state.start().expect("supervisor spawns");
        // Parked inside prewarm: axis holds Starting and no decode ever
        // starts while a short window elapses.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while state.axis() != ServiceAxis::Starting {
            assert!(std::time::Instant::now() < deadline, "never reached Starting");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(state.axis(), ServiceAxis::Starting, "parked in prewarm");
        assert_eq!(eng.decodes_started.load(Ordering::SeqCst), 0);
        // Release the gate ~200 ms into the stop, well inside the 16 s
        // supervisor join bound.
        let eng2 = eng.clone();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            eng2.release_gate();
        });
        state.stop();
        releaser.join().unwrap();
        assert_eq!(
            state.axis(),
            ServiceAxis::Stopped,
            "mid-prewarm stop is clean — never capture-wedged"
        );
        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }

    /// Pause from stopped = stateless no-op (spec §Arbitration first arm):
    /// no latch, no state change, no thread interaction.
    #[test]
    fn pause_from_stopped_is_a_stateless_noop() {
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        assert_eq!(state.axis(), ServiceAxis::Stopped);
        // T14's arbiter dispatches on axis(): Stopped returns Ok(()) WITHOUT
        // latching; pin the primitive here: the hold stays clear and the
        // axis stays stopped even if pause mechanics are (wrongly) invoked.
        assert!(!state.hold().is_latched());
        assert_eq!(state.axis(), ServiceAxis::Stopped);
    }

    /// Resume re-spawn: after a yield, the decode thread SURVIVES and the
    /// resume spawns capture only (8′); prewarm is not re-run.
    #[test]
    fn resume_respawns_capture_only_and_decode_survives() {
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead();
        *p.engine.lock().unwrap() = eng.clone();
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert_thread_liveness(&state);

        // Yield (the T14 arbiter's listening arm, mechanics only).
        state.hold().latch_now();
        state.pause_capture_for_yield().expect("clean join");
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        assert_thread_liveness(&state); // decode alive, capture joined

        // FakeEngine has no prewarm counter; pin via engine identity —
        // the SAME Arc must still be installed after resume. (Fat pointers
        // cannot cast to usize — E0606; compare Arcs with ptr_eq.)
        let engine_before = state.engine.lock().unwrap().clone().unwrap();

        // Clear the latch + free card + eligible modem → tick resumes.
        state.hold().clear();
        *p.modem_eligible.lock().unwrap() = true;
        state.tick_yielded();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert_thread_liveness(&state);
        let engine_after = state.engine.lock().unwrap().clone().unwrap();
        assert!(
            Arc::ptr_eq(&engine_before, &engine_after),
            "runner NOT reconstructed on resume"
        );
        teardown(&state);
    }

    /// Device-absent retry recovery: mid-run loss blocks device-absent; the
    /// tick re-resolves (fresh index — the card moved!) and recovers with a
    /// capture-only respawn.
    #[test]
    fn device_absent_retry_recovers_with_fresh_resolution() {
        let p = FakePlatform::happy();
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Listening);

        // Mid-run loss: the capture loop calls on_device_lost when the
        // source errors; simulate the loss directly + join the capture
        // thread the way the loop's return does.
        p.source_steps
            .lock()
            .unwrap()
            .push_back(SourceStep::Fail(crate::ft8::traits::SourceError::Absent));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while state.axis() != ServiceAxis::Blocked(BlockedReason::DeviceAbsent) {
            assert!(std::time::Instant::now() < deadline, "loss not detected");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Replug on a NEW index: retry must use the fresh resolution.
        *p.resolved.lock().unwrap() = Some(crate::winlink::ax25::devices::ResolvedManagedDevice {
            alsa_plughw: "plughw:CARD=DRA,DEV=0".into(),
            alsa_hw: "hw:3,0".into(),
            card_index: 3,
        });
        state.tick_device_absent();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert_eq!(
            state.lock_inner().resolved.as_ref().unwrap().card_index,
            3,
            "recovery re-resolved to the LIVE index, never a cached name"
        );
        assert_thread_liveness(&state);
        teardown(&state);
    }

    /// Blocked re-entry with a BUSY card lands `Yielded`, never a stale
    /// blocked axis (the set_device-from-blocked path): the sequence entry
    /// re-enters Starting from every non-wedged blocked reason
    /// (on_start_requested — T11 entry match), so step 6's pause writes
    /// Yielded; on_pause from Blocked would have been silently swallowed.
    /// tick_yielded then recovers once the card frees.
    #[test]
    fn set_device_from_blocked_with_busy_card_lands_yielded_then_recovers() {
        let p = FakePlatform::happy();
        *p.resolved.lock().unwrap() = None;
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::DeviceAbsent));

        // Device replugs, but a modem holds the card.
        *p.resolved.lock().unwrap() =
            Some(crate::winlink::ax25::devices::ResolvedManagedDevice {
                alsa_plughw: "plughw:CARD=DRA,DEV=0".into(),
                alsa_hw: "hw:1,0".into(),
                card_index: 1,
            });
        *p.busy.lock().unwrap() = Err("card busy".into());
        state.execute_start_sequence(false); // the set_device retrigger path
        assert_eq!(
            state.axis(),
            ServiceAxis::Yielded,
            "busy re-entry must yield — a stale blocked axis strands the operator"
        );

        // Card frees (modem already eligible in happy()): the supervisor
        // tick recovers.
        *p.busy.lock().unwrap() = Ok(());
        state.tick_yielded();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        teardown(&state);
    }
}
