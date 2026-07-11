//! FT8 waterfall L3 (tuxlink-b026z.4 Task A6): the FFT consumer thread,
//! token-counted subscription lifecycle, and the `ft8-waterfall:columns`
//! event.
//!
//! Layering. L2 (`service::WaterfallTap`) owns "the decimated 12 kHz i16
//! stream is subscribable, bounded, lossy (drop-oldest), and NEVER
//! backpressures capture." This module (L3) owns everything above that: the
//! token registry that decides when the single tap consumer exists, the FFT
//! that turns i16 blocks into spectral columns, the 4 Hz column cadence, the
//! 4-column batching, and the emitted event. It is the ONLY caller of
//! [`WaterfallTap::take_blocks`].
//!
//! Lifecycle. A per-process registry of subscription ids drives one consumer
//! thread. On the registry 0→1 edge [`subscribe`] wakes the tap
//! (`tap().subscribe()`) and spawns THE single consumer; on the 1→0 edge
//! [`unsubscribe`] signals the consumer to stop, unsubscribes the tap, and
//! joins. Both commands are idempotent: a stale/duplicate unsubscribe is a
//! no-op that does not decrement a live id, two subscribes yield two live
//! ids, and only the LAST unsubscribe tears the thread down. A re-subscribe
//! after a full teardown mints a fresh stop flag and spawns a fresh thread.
//!
//! Zero-subscriber ⇒ zero-FFT. The consumer thread is the sole advancer of
//! [`WaterfallHub::fft_count`]; once the last token releases and the thread is
//! joined, the counter is frozen. That is the load-bearing backend half of
//! Exit gate 2 (`zero_subscriber_freezes_fft_counter`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use serde::Serialize;

use crate::ft8::events::EventSink;
use crate::ft8::service::Ft8ListenerState;

/// FFT window length (samples @ 12 kHz): a 2048-pt real FFT yields 1025 bins
/// spanning 0–6000 Hz (Nyquist at 12 kHz).
pub(crate) const WINDOW: usize = 2048;
/// Hop between successive column starts (samples @ 12 kHz). 3000 samples ≈
/// 250 ms → a 4 Hz column cadence. Note hop > window: the time axis is
/// intentionally undersampled (successive windows do not overlap; the 952
/// samples between them are not spectrally analyzed — inherent to a 4 Hz
/// cadence with a ~170 ms window).
pub(crate) const HOP: usize = 3000;
/// 0–3000 Hz crop of the 1025-bin spectrum. Bin width = 12000/2048 ≈ 5.859 Hz,
/// so 3000 Hz falls at bin 512; the crop keeps bins `[0, 512)` → 512 u8.
pub(crate) const CROP_BINS: usize = 512;
/// Columns per emitted [`WaterfallBatch`].
pub(crate) const BATCH_COLS: usize = 4;
/// Consumer-thread poll cadence: drain the tap, form whatever columns the
/// accumulated samples allow, then park. Also bounds `unsubscribe`'s join
/// latency to ~this.
const POLL: Duration = Duration::from_millis(50);

/// The `ft8-waterfall:columns` payload (spec §Waterfall). One batch carries
/// `BATCH_COLS` spectral columns, each `CROP_BINS` u8 (0–3000 Hz). `seq` is a
/// per-thread monotonic batch counter; `first_col_utc_ms` timestamps the first
/// column of the batch. Wire keys are camelCase (`seq` / `firstColUtcMs` /
/// `cols`). Consumed by Task C8 (`Waterfall.tsx`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaterfallBatch {
    pub seq: u64,
    pub first_col_utc_ms: u64,
    pub cols: Vec<Vec<u8>>,
}

/// Per-process token registry + the single consumer thread's handle and stop
/// flag. A field of [`Ft8ListenerState`]; its methods are driven by the
/// module-level [`subscribe`] / [`unsubscribe`] free functions (they need the
/// owning `Arc<Ft8ListenerState>` to hand the tap + sink to the thread).
#[derive(Default)]
pub struct WaterfallHub {
    inner: Mutex<HubInner>,
    /// FFT-invocation counter, shared with the consumer thread. Only the
    /// thread advances it — see the module docs' zero-FFT invariant.
    fft_count: Arc<AtomicU64>,
}

#[derive(Default)]
struct HubInner {
    /// Live subscription ids. The value is `()` — presence is the signal.
    tokens: HashMap<String, ()>,
    /// The single consumer thread handle (present iff `tokens` is non-empty).
    thread: Option<JoinHandle<()>>,
    /// The current consumer's stop flag (a fresh one per spawn).
    stop: Option<Arc<AtomicBool>>,
}

impl WaterfallHub {
    /// FFT invocations since process start — the zero-subscriber ⇒ zero-FFT
    /// witness (Exit gate 2). Frozen once the last token releases (thread
    /// joined).
    pub fn fft_count(&self) -> u64 {
        self.fft_count.load(Ordering::SeqCst)
    }

    /// Live subscription-id count (introspection / tests).
    pub fn live_ids(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .tokens
            .len()
    }

    /// Whether the single consumer thread is currently spawned.
    pub fn is_running(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .thread
            .is_some()
    }
}

/// Register a subscription and return a fresh id. On the registry 0→1 edge:
/// `tap().subscribe()` AND spawn THE single consumer thread. Idempotent-safe:
/// two calls yield two distinct live ids and only one thread.
pub fn subscribe(state: &Arc<Ft8ListenerState>) -> String {
    let hub = state.waterfall();
    let id = uuid::Uuid::new_v4().to_string();
    let mut g = hub.inner.lock().unwrap_or_else(|p| p.into_inner());
    let was_empty = g.tokens.is_empty();
    g.tokens.insert(id.clone(), ());
    if was_empty {
        // 0→1: wake the tap and spawn the consumer with a fresh stop flag.
        state.tap().subscribe();
        let stop = Arc::new(AtomicBool::new(false));
        g.stop = Some(stop.clone());
        let st = state.clone();
        let counter = hub.fft_count.clone();
        let handle = std::thread::Builder::new()
            .name("ft8-waterfall".into())
            .spawn(move || consumer_loop(st, stop, counter))
            .expect("spawn ft8-waterfall consumer thread");
        g.thread = Some(handle);
    }
    id
}

/// Remove a subscription id. Idempotent: an unknown/duplicate id is a no-op
/// that does NOT decrement a live id. On the registry 1→0 edge: signal the
/// consumer to stop, `tap().unsubscribe()`, and join (join runs outside the
/// registry lock so subscribe/unsubscribe of other ids are not blocked on it).
/// This is also the window-close reap hook (see [`reap`]).
pub fn unsubscribe(state: &Arc<Ft8ListenerState>, id: &str) {
    let hub = state.waterfall();
    let mut g = hub.inner.lock().unwrap_or_else(|p| p.into_inner());
    if g.tokens.remove(id).is_none() {
        return; // unknown id — idempotent no-op, no live id touched
    }
    // The tap subscribe/unsubscribe transition and the stop-flag set happen
    // UNDER the registry lock so they stay serialized with token bookkeeping
    // (a concurrent re-subscribe cannot interleave a tap.subscribe() between
    // our stop-signal and our tap.unsubscribe()). Only the join is deferred
    // past the lock.
    let joined = if g.tokens.is_empty() {
        if let Some(stop) = g.stop.take() {
            stop.store(true, Ordering::SeqCst);
        }
        state.tap().unsubscribe();
        g.thread.take()
    } else {
        None
    };
    drop(g);
    if let Some(h) = joined {
        let _ = h.join();
    }
}

/// Window-close reap hook (spec: "ids reaped on window close"). Alias for
/// [`unsubscribe`]; releasing the token the closed window held tears the
/// thread down if it was the last one.
pub fn reap(state: &Arc<Ft8ListenerState>, id: &str) {
    unsubscribe(state, id)
}

/// One spectral column: an i16 window → magnitude spectrum → 0–3000 Hz crop →
/// `CROP_BINS` u8. Pure (no counter side-effect) so the length contract is
/// testable in isolation; the consumer thread increments the FFT counter
/// around each call.
pub(crate) fn compute_column(
    fft: &dyn RealToComplex<f32>,
    window: &[i16],
    scratch_in: &mut [f32],
    scratch_out: &mut [Complex<f32>],
) -> Vec<u8> {
    debug_assert_eq!(window.len(), WINDOW);
    for (dst, &s) in scratch_in.iter_mut().zip(window) {
        *dst = f32::from(s);
    }
    // Fixed 2048/1025 lengths: `process` only errors on a length mismatch,
    // which cannot happen with the planner-sized scratch buffers.
    let _ = fft.process(scratch_in, scratch_out);
    scratch_out
        .iter()
        .take(CROP_BINS)
        .map(|c| {
            // dB relative to unity magnitude, mapped [0, 96] dB → [0, 255].
            let db = 20.0 * c.norm().max(1.0).log10();
            (db / 96.0 * 255.0).clamp(0.0, 255.0) as u8
        })
        .collect()
}

fn now_utc_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// THE single consumer thread. Drains the tap (the only `take_blocks` caller),
/// concatenates the variable-length 12 kHz i16 blocks into a carry buffer,
/// forms 2048-sample columns spaced `HOP` apart, FFTs each, crops to 512 u8,
/// batches `BATCH_COLS`, and emits `ft8-waterfall:columns`. Exits promptly
/// when `stop` is set (checked once per `POLL`), so `unsubscribe`'s join never
/// hangs and the thread cannot leak.
fn consumer_loop(state: Arc<Ft8ListenerState>, stop: Arc<AtomicBool>, fft_count: Arc<AtomicU64>) {
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(WINDOW);
    let mut scratch_in = fft.make_input_vec(); // len WINDOW
    let mut scratch_out = fft.make_output_vec(); // len WINDOW/2 + 1 = 1025
    let mut carry: Vec<i16> = Vec::with_capacity(HOP * 2);
    let mut batch: Vec<Vec<u8>> = Vec::with_capacity(BATCH_COLS);
    let mut batch_first_utc_ms: u64 = 0;
    let mut seq: u64 = 0;

    while !stop.load(Ordering::SeqCst) {
        for block in state.tap().take_blocks() {
            carry.extend_from_slice(&block);
        }
        // Requiring HOP samples before a column guarantees BOTH the WINDOW
        // slice (HOP > WINDOW) and a full hop advance, keeping successive
        // column starts exactly HOP apart regardless of how the drained
        // blocks chunked the stream.
        while carry.len() >= HOP {
            if batch.is_empty() {
                batch_first_utc_ms = now_utc_ms();
            }
            let col = compute_column(
                fft.as_ref(),
                &carry[..WINDOW],
                &mut scratch_in,
                &mut scratch_out,
            );
            fft_count.fetch_add(1, Ordering::SeqCst);
            batch.push(col);
            carry.drain(..HOP);
            if batch.len() == BATCH_COLS {
                let cols = std::mem::replace(&mut batch, Vec::with_capacity(BATCH_COLS));
                state.sink.emit_waterfall(&WaterfallBatch {
                    seq,
                    first_col_utc_ms: batch_first_utc_ms,
                    cols,
                });
                seq += 1;
            }
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Ft8Config;
    use crate::ft8::commands::SubDto;
    use crate::ft8::service::{Ft8Deps, Ft8ListenerState};
    use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink};

    /// A minimal listener state with a recording sink returned for
    /// inspection. No service threads are started — the waterfall path only
    /// needs the tap and the hub.
    fn state_and_sink() -> (Arc<Ft8ListenerState>, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink::default());
        let deps = Ft8Deps {
            platform: FakePlatform::happy(),
            clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
            sink: sink.clone(),
        };
        let state = Ft8ListenerState::new(deps, Ft8Config::default());
        (state, sink)
    }

    /// Push `n` full tap blocks of a modest tone so the consumer accumulates
    /// enough carry to form columns (the tap only surfaces full 1200-frame
    /// blocks; partial pending never drains).
    fn push_blocks(state: &Arc<Ft8ListenerState>, blocks: usize) {
        let frames = crate::ft8::service::TAP_BLOCK_FRAMES;
        let mut samples = Vec::with_capacity(blocks * frames);
        for i in 0..blocks * frames {
            // A cheap deterministic non-DC signal; exact values are immaterial
            // to the lifecycle/length/counter assertions.
            samples.push(((i as i32 % 200) - 100) as i16 * 50);
        }
        state.tap().push_samples(&samples);
    }

    #[test]
    fn subscribe_returns_camelcase_id_and_two_subs_two_ids() {
        let (state, _sink) = state_and_sink();
        let id = subscribe(&state);
        assert!(!id.is_empty());
        // Serialized DTO uses the camelCase wire key.
        let v = serde_json::to_value(SubDto { subscription_id: id.clone() }).unwrap();
        assert!(v["subscriptionId"].is_string(), "wire key subscriptionId");
        assert!(v.get("subscription_id").is_none(), "no snake_case key");

        let id2 = subscribe(&state);
        assert_ne!(id, id2, "each subscribe mints a fresh id");
        assert_eq!(state.waterfall().live_ids(), 2, "two live ids");
        assert!(state.waterfall().is_running(), "one consumer thread for both");

        // Unsubscribe one → thread stays alive.
        unsubscribe(&state, &id);
        assert_eq!(state.waterfall().live_ids(), 1);
        assert!(state.waterfall().is_running(), "thread alive while an id remains");

        // Unsubscribe the last → thread stops.
        unsubscribe(&state, &id2);
        assert_eq!(state.waterfall().live_ids(), 0);
        assert!(!state.waterfall().is_running(), "last unsubscribe joins the thread");
    }

    #[test]
    fn stale_unsubscribe_is_noop_and_does_not_decrement() {
        let (state, _sink) = state_and_sink();
        let id = subscribe(&state);
        // Unknown id: no-op, live id untouched, thread alive.
        unsubscribe(&state, "not-a-real-id");
        assert_eq!(state.waterfall().live_ids(), 1);
        assert!(state.waterfall().is_running());
        // Double-unsubscribe of the real id: the second call is a no-op.
        unsubscribe(&state, &id);
        assert!(!state.waterfall().is_running());
        unsubscribe(&state, &id);
        assert_eq!(state.waterfall().live_ids(), 0);
        assert!(!state.waterfall().is_running());

        // Re-subscribe after full teardown spawns a fresh thread.
        let id3 = subscribe(&state);
        assert!(state.waterfall().is_running(), "re-subscribe spawns a fresh thread");
        unsubscribe(&state, &id3);
    }

    #[test]
    fn zero_subscriber_freezes_fft_counter() {
        let (state, _sink) = state_and_sink();
        let id = subscribe(&state);
        // Feed plenty of blocks (30 * 1200 = 36000 samples → ~12 columns).
        push_blocks(&state, 30);
        // Let the consumer drain + FFT (POLL = 50 ms).
        std::thread::sleep(Duration::from_millis(300));
        let ran = state.waterfall().fft_count();
        assert!(ran > 0, "consumer ran FFTs while subscribed (got {ran})");

        // Release the last token → thread joined, tap unsubscribed.
        unsubscribe(&state, &id);
        let frozen = state.waterfall().fft_count();

        // Even with fresh pushes (rejected — tap is now unsubscribed) and time
        // elapsing, the counter must not advance.
        push_blocks(&state, 30);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(
            state.waterfall().fft_count(),
            frozen,
            "zero subscribers ⇒ zero further FFTs"
        );
    }

    #[test]
    fn column_is_512_u8() {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(WINDOW);
        let mut scratch_in = fft.make_input_vec();
        let mut scratch_out = fft.make_output_vec();
        let window: Vec<i16> = (0..WINDOW)
            .map(|i| ((i as f32 * 0.05).sin() * 8000.0) as i16)
            .collect();
        let col = compute_column(fft.as_ref(), &window, &mut scratch_in, &mut scratch_out);
        assert_eq!(col.len(), CROP_BINS, "0–3000 Hz crop of the 1025-bin FFT");
        assert_eq!(CROP_BINS, 512);
    }

    #[test]
    fn batch_serializes_camelcase_seq_and_first_col_utc_ms() {
        let batch = WaterfallBatch {
            seq: 7,
            first_col_utc_ms: 1_700_000_000_123,
            cols: vec![vec![0u8; CROP_BINS]; BATCH_COLS],
        };
        let v = serde_json::to_value(&batch).unwrap();
        assert_eq!(v["seq"], 7);
        assert_eq!(v["firstColUtcMs"], 1_700_000_000_123u64);
        assert!(v.get("first_col_utc_ms").is_none(), "no snake_case key");
        assert!(v["cols"].is_array());
        assert_eq!(v["cols"][0].as_array().unwrap().len(), CROP_BINS);
    }

    #[test]
    fn consumer_emits_batches_of_four_512_wide_columns() {
        let (state, sink) = state_and_sink();
        let id = subscribe(&state);
        push_blocks(&state, 40); // 48000 samples → ~16 columns → ~4 batches
        std::thread::sleep(Duration::from_millis(300));
        unsubscribe(&state, &id);

        let batches = sink.waterfall_batches.lock().unwrap();
        assert!(!batches.is_empty(), "consumer emitted at least one batch");
        for b in batches.iter() {
            assert_eq!(b.cols.len(), BATCH_COLS, "each batch carries 4 columns");
            for col in &b.cols {
                assert_eq!(col.len(), CROP_BINS, "each column is 512 u8");
            }
        }
        // seq is monotonic from 0.
        for (i, b) in batches.iter().enumerate() {
            assert_eq!(b.seq, i as u64, "seq is per-thread monotonic from 0");
        }
    }
}
