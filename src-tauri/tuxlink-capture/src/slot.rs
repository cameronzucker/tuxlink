//! Wall-clock-true 15 s UTC slot assembler (spec §Slot assembly).
//!
//! PURE: time is data at this seam — `(utc_now_ms, mono_now_us)` arrive
//! with every push and are never read ambiently. The two clock domains
//! have disjoint jobs (pinned):
//!   - UTC labels slot identity only: sampled at boundary detection to
//!     stamp `slot_utc_ms` (0/15/30/45 s, start within ±0.5 s) and to
//!     choose the next boundary.
//!   - Monotonic drives everything inside a slot: the per-slot anchor is
//!     captured at the boundary and the expected-frame counter is
//!     (mono_now − anchor) × 48 000 — NTP steps and slews cannot
//!     manufacture in-slot gaps.
//!
//! Input is the RAW post-extraction 48 kHz channel-0 stream. The assembler
//! owns zero-fill at 48 k, holds the `Decimator` (filter state persists
//! across slot boundaries — continuity model), and emits exactly
//! 180 000-frame `CompletedSlot`s.

use crate::decimator::Decimator;

pub const IN_RATE_HZ: u64 = 48_000;
pub const IN_SLOT_FRAMES: usize = 720_000;
pub const OUT_SLOT_FRAMES: usize = 180_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryConfig {
    /// Slot length in UTC milliseconds.
    pub slot_ms: u64,
    /// Gap deficits below this are scheduling jitter, not loss — filling
    /// them is pure signal damage (spec: 2 400 frames = 50 ms; empirical
    /// basis: 0.25 s time-shift = 0 decodes; 0.25 s zero-filled = 13/14).
    pub min_gap_fill_frames: u64,
    /// A SINGLE intra-slot gap above this is a clock anomaly (48 000 = 1 s).
    pub max_single_gap_frames: u64,
    /// Cumulative filled frames above this drop the slot as a real failure
    /// (48 000 = 1 s). Enforced in Task 5.
    pub max_lost_frames: u64,
    /// UTC-vs-monotonic divergence above this observed at a boundary is a
    /// clock anomaly (an NTP step): 1 000 ms.
    pub max_boundary_divergence_ms: u64,
}

impl Default for BoundaryConfig {
    fn default() -> Self {
        Self {
            slot_ms: 15_000,
            min_gap_fill_frames: 2_400,
            max_single_gap_frames: 48_000,
            max_lost_frames: 48_000,
            max_boundary_divergence_ms: 1_000,
        }
    }
}

/// Reported by the capture loop alongside the first batch after a
/// gap-causing event. EPIPE tells us THAT an overrun occurred, never how
/// much was lost — the deficit always comes from the monotonic
/// expected-frame counter (spec §ALSA read loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GapReport {
    pub kind: GapKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    /// Capture restarted after an ALSA overrun (`-EPIPE` recover).
    Overrun,
    /// `-ESTRPIPE`: the stream was suspended — uniformly a clock anomaly.
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardClass {
    /// The partial first slot after start/resume — a scheduled discard
    /// (policy, not failure; counts toward NEITHER counter).
    FirstSlot,
    /// Negative computed gap, a single gap longer than 1 s, UTC-vs-mono
    /// divergence over 1 s at a boundary, or suspend: the slot's timing
    /// cannot be trusted. Scheduled discard; re-anchor at the next UTC
    /// boundary. (Doc phrasing avoids a line-leading `>` — clippy's
    /// doc-quote lint.)
    ClockAnomaly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropClass {
    /// `lost_frames` exceeded the per-slot bound (48 000 = 1 s): too much
    /// of the slot is synthetic zeros to trust a decode.
    LostFrames,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlotEvent {
    Completed(CompletedSlot),
    Abandoned { class: DiscardClass },
    /// A REAL failure (counts toward N upstream, unlike scheduled
    /// discards): the slot is discarded with provenance so the ring can
    /// record it honestly.
    Dropped {
        class: DropClass,
        slot_utc_ms: u64,
        lost_frames: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletedSlot {
    /// UTC label: the slot's boundary, a multiple of `slot_ms`.
    pub slot_utc_ms: u64,
    /// Exactly `OUT_SLOT_FRAMES` decimated 12 kHz frames.
    pub samples: Vec<i16>,
    /// Zero-filled input frames inside the slot (gap fills + boundary
    /// shortfall), input rate.
    pub lost_frames: u64,
    /// Surplus input frames DROPPED at boundary close — never carried
    /// (carryover would accumulate card-vs-wall skew without bound; at
    /// ≤ 50 ppm the drop lands in FT8's inter-slot guard interval).
    pub boundary_skew_frames: u64,
    /// Fraction of DELIVERED frames at ±full scale (fills excluded).
    pub clip_fraction: f32,
    /// RMS of DELIVERED frames only, dBFS re 32768 (denominator
    /// 720 000 − lost, so degraded slots don't read as quiet).
    /// `f32::NEG_INFINITY` when nothing was delivered.
    pub rms_dbfs: f32,
}

pub struct SlotAssembler {
    cfg: BoundaryConfig,
    decimator: Decimator,
    phase: Phase,
    /// The scheduled first-slot discard record is emitted when the first
    /// boundary after construction opens (start/resume). Clock-anomaly
    /// re-anchors do NOT re-emit it — the anomaly emitted its own record.
    pending_first_slot_discard: bool,
}

enum Phase {
    /// Waiting for the next UTC boundary; the target is chosen from the
    /// first push seen while waiting.
    Waiting { next_boundary_utc_ms: Option<u64> },
    InSlot(Current),
}

struct Current {
    slot_utc_ms: u64,
    anchor_mono_us: u64,
    /// Delivered + zero-filled input frames, capped at IN_SLOT_FRAMES.
    buf: Vec<i16>,
    lost_frames: u64,
    surplus_frames: u64,
    clipped: u64,
    delivered_in_slot: u64,
    sum_sq: f64,
}

impl Current {
    fn open(slot_utc_ms: u64, anchor_mono_us: u64) -> Self {
        Self {
            slot_utc_ms,
            anchor_mono_us,
            buf: Vec::with_capacity(IN_SLOT_FRAMES),
            lost_frames: 0,
            surplus_frames: 0,
            clipped: 0,
            delivered_in_slot: 0,
            sum_sq: 0.0,
        }
    }

    fn append_delivered(&mut self, samples: &[i16]) {
        for &s in samples {
            if self.buf.len() < IN_SLOT_FRAMES {
                self.delivered_in_slot += 1;
                if s == i16::MAX || s == i16::MIN {
                    self.clipped += 1;
                }
                self.sum_sq += f64::from(s) * f64::from(s);
                self.buf.push(s);
            } else {
                // Surplus past the slot's 720 000: dropped at close, never
                // carried — recorded as boundary_skew_frames.
                self.surplus_frames += 1;
            }
        }
    }

    fn fill_zeros(&mut self, frames: u64) {
        let room = (IN_SLOT_FRAMES - self.buf.len()) as u64;
        let n = frames.min(room);
        self.buf.resize(self.buf.len() + n as usize, 0);
        self.lost_frames += n;
    }

    /// Input frames accounted to this slot so far (delivered-in-slot +
    /// fills + surplus) — the "have" side of the deficit computation.
    fn accounted_input_frames(&self) -> u64 {
        self.buf.len() as u64 + self.surplus_frames
    }
}

impl SlotAssembler {
    pub fn new(cfg: BoundaryConfig) -> Self {
        Self {
            cfg,
            decimator: Decimator::new(),
            phase: Phase::Waiting { next_boundary_utc_ms: None },
            pending_first_slot_discard: true,
        }
    }

    /// Feed one delivered batch. `utc_now_ms`/`mono_now_us` are sampled by
    /// the capture loop AFTER the batch was read (they timestamp the batch
    /// end). Returns zero or more slot events.
    pub fn push(
        &mut self,
        samples: &[i16],
        utc_now_ms: u64,
        mono_now_us: u64,
        gap: Option<GapReport>,
    ) -> Vec<SlotEvent> {
        let mut events = Vec::new();
        let slot_ms = self.cfg.slot_ms;

        if matches!(gap, Some(GapReport { kind: GapKind::Suspended })) {
            // -ESTRPIPE: uniformly a clock anomaly — abandon, re-anchor at
            // the next boundary. The suspended batch dies with the slot.
            self.abandon_clock_anomaly(&mut events);
            return events;
        }

        // Waiting: open at the first push at/after the chosen boundary.
        if let Phase::Waiting { next_boundary_utc_ms } = &mut self.phase {
            let next = *next_boundary_utc_ms.get_or_insert(
                (utc_now_ms / slot_ms).saturating_add(1).saturating_mul(slot_ms),
            );
            if utc_now_ms < next {
                return events; // pre-boundary partial: scheduled discard
            }
            if self.pending_first_slot_discard {
                self.pending_first_slot_discard = false;
                events.push(SlotEvent::Abandoned {
                    class: DiscardClass::FirstSlot,
                });
            }
            let slot_utc = utc_now_ms - utc_now_ms % slot_ms;
            // The anchor is the opening batch's START (mono_now minus the
            // batch's duration): anchoring at the batch END would bias
            // every later deficit computation one batch low and misfire
            // the negative-gap anomaly on healthy streams.
            let anchor = mono_now_us.saturating_sub(frames_to_us(samples.len()));
            let mut cur = Current::open(slot_utc, anchor);
            cur.append_delivered(samples); // the crossing batch opens the slot
            self.phase = Phase::InSlot(cur);
            return events;
        }

        // In slot: the boundary close runs BEFORE the crossing batch is
        // appended — the batch arrived after the boundary and belongs
        // wholly to the new slot (batch granularity ≤ one 100 ms period,
        // inside the ±0.5 s start tolerance).
        let (slot_utc, anchor) = match &self.phase {
            Phase::InSlot(c) => (c.slot_utc_ms, c.anchor_mono_us),
            Phase::Waiting { .. } => unreachable!("handled above"),
        };
        if utc_now_ms >= slot_utc.saturating_add(slot_ms) {
            // Clock-anomaly rule: UTC-vs-monotonic divergence observed at
            // the boundary (an NTP step) abandons the slot.
            let mono_elapsed_ms = mono_now_us.saturating_sub(anchor) / 1_000;
            let utc_elapsed_ms = utc_now_ms - slot_utc;
            if mono_elapsed_ms.abs_diff(utc_elapsed_ms)
                > self.cfg.max_boundary_divergence_ms
            {
                self.abandon_clock_anomaly(&mut events);
                return events; // the crossing batch dies with the anomaly
            }
            self.close_slot(&mut events);
            let slot_utc = utc_now_ms - utc_now_ms % slot_ms;
            // Same batch-START anchoring as the open path above.
            let anchor = mono_now_us.saturating_sub(frames_to_us(samples.len()));
            let mut cur = Current::open(slot_utc, anchor);
            cur.append_delivered(samples);
            self.phase = Phase::InSlot(cur);
            return events;
        }

        // Overrun gap: the deficit comes from the monotonic expected-frame
        // counter, never from ALSA (spec §ALSA read loop / §Slot assembly).
        if matches!(gap, Some(GapReport { kind: GapKind::Overrun })) {
            let Phase::InSlot(cur) = &mut self.phase else {
                unreachable!("handled above")
            };
            let mono_elapsed_us = mono_now_us.saturating_sub(cur.anchor_mono_us);
            let expected = (u128::from(mono_elapsed_us) * u128::from(IN_RATE_HZ)
                / 1_000_000) as u64;
            let have = cur.accounted_input_frames() + samples.len() as u64;
            if expected < have {
                // Negative computed gap: clock anomaly (spec rule).
                self.abandon_clock_anomaly(&mut events);
                return events;
            }
            let deficit = expected - have;
            if deficit > self.cfg.max_single_gap_frames {
                // A single intra-slot gap > 1 s: clock anomaly.
                self.abandon_clock_anomaly(&mut events);
                return events;
            }
            if deficit >= self.cfg.min_gap_fill_frames {
                // Zero-fill in place, immediately after the last delivered
                // frame (i.e. BEFORE this batch is appended).
                cur.fill_zeros(deficit);
            }
            // Below the threshold: scheduling jitter — never filled.
        }

        let Phase::InSlot(cur) = &mut self.phase else {
            unreachable!("handled above")
        };
        cur.append_delivered(samples);
        events
    }

    fn abandon_clock_anomaly(&mut self, events: &mut Vec<SlotEvent>) {
        if matches!(self.phase, Phase::InSlot(_)) {
            events.push(SlotEvent::Abandoned {
                class: DiscardClass::ClockAnomaly,
            });
        }
        // Re-anchor: the next boundary is chosen from the NEXT push's UTC
        // (time may have stepped arbitrarily). No FirstSlot record — the
        // anomaly is its own record.
        self.phase = Phase::Waiting { next_boundary_utc_ms: None };
    }

    fn close_slot(&mut self, events: &mut Vec<SlotEvent>) {
        let Phase::InSlot(cur) = &mut self.phase else { return };
        let shortfall = (IN_SLOT_FRAMES - cur.buf.len()) as u64;
        if shortfall > 0 {
            cur.fill_zeros(shortfall);
        }
        if cur.lost_frames > self.cfg.max_lost_frames {
            // Spec §Slot assembly: drop the slot when lost_frames > 48 000
            // (1 s). A real failure — counted toward N upstream. The
            // decimator is NOT fed: its state continuity covers emitted
            // slots only (same as abandoned slots).
            events.push(SlotEvent::Dropped {
                class: DropClass::LostFrames,
                slot_utc_ms: cur.slot_utc_ms,
                lost_frames: cur.lost_frames,
            });
            return;
        }
        let mut samples = Vec::with_capacity(OUT_SLOT_FRAMES);
        self.decimator.process(&cur.buf, &mut samples);
        debug_assert_eq!(samples.len(), OUT_SLOT_FRAMES);
        let clip_fraction = if cur.delivered_in_slot == 0 {
            0.0
        } else {
            (cur.clipped as f64 / cur.delivered_in_slot as f64) as f32
        };
        let rms_dbfs = if cur.delivered_in_slot == 0 {
            f32::NEG_INFINITY
        } else {
            let rms = (cur.sum_sq / cur.delivered_in_slot as f64).sqrt();
            (20.0 * (rms / 32_768.0).log10()) as f32
        };
        events.push(SlotEvent::Completed(CompletedSlot {
            slot_utc_ms: cur.slot_utc_ms,
            samples,
            lost_frames: cur.lost_frames,
            boundary_skew_frames: cur.surplus_frames,
            clip_fraction,
            rms_dbfs,
        }));
    }
}

/// Duration of `frames` input frames, in µs (exact for the 100 ms
/// production period; truncates sub-µs remainders for odd lengths).
fn frames_to_us(frames: usize) -> u64 {
    frames as u64 * 1_000_000 / IN_RATE_HZ
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drives the assembler with synthetic, exactly-tracked clocks.
    /// Frame counts used by tests are multiples of 48 so the µs conversion
    /// (× 125 / 6) is exact and UTC/mono never drift by rounding.
    struct Sim {
        asm: SlotAssembler,
        utc_us: u64,
        mono_us: u64,
        events: Vec<SlotEvent>,
    }

    impl Sim {
        fn new(start_utc_ms: u64) -> Self {
            Self {
                asm: SlotAssembler::new(BoundaryConfig::default()),
                utc_us: start_utc_ms * 1_000,
                mono_us: 5_000_000,
                events: Vec::new(),
            }
        }

        fn advance_frames(&mut self, frames: u64) {
            assert_eq!(frames % 48, 0, "test discipline: keep µs math exact");
            let us = frames * 125 / 6;
            self.utc_us += us;
            self.mono_us += us;
        }

        /// Real time passes for `frames` worth of audio AND the batch is
        /// delivered (the normal case).
        fn deliver(&mut self, frames: usize, value: i16) {
            self.deliver_gap(frames, value, None);
        }

        fn deliver_gap(&mut self, frames: usize, value: i16, gap: Option<GapReport>) {
            self.advance_frames(frames as u64);
            let batch = vec![value; frames];
            let ev = self.asm.push(&batch, self.utc_us / 1_000, self.mono_us, gap);
            self.events.extend(ev);
        }

        /// Time passes, nothing is delivered (a dropout).
        fn stall_frames(&mut self, frames: u64) {
            self.advance_frames(frames);
        }

        /// Deliver `frames` while only `wall_frames` of real time pass —
        /// a card catching up after jitter (wall < frames), or a fabricated
        /// negative-gap anomaly (wall = 0, Task 5).
        fn deliver_wall(
            &mut self,
            frames: usize,
            wall_frames: u64,
            value: i16,
            gap: Option<GapReport>,
        ) {
            self.advance_frames(wall_frames);
            let batch = vec![value; frames];
            let ev = self.asm.push(&batch, self.utc_us / 1_000, self.mono_us, gap);
            self.events.extend(ev);
        }

        fn completed(&self) -> Vec<&CompletedSlot> {
            self.events
                .iter()
                .filter_map(|e| match e {
                    SlotEvent::Completed(c) => Some(c),
                    _ => None,
                })
                .collect()
        }

        fn abandoned(&self) -> Vec<DiscardClass> {
            self.events
                .iter()
                .filter_map(|e| match e {
                    SlotEvent::Abandoned { class } => Some(*class),
                    _ => None,
                })
                .collect()
        }
    }

    /// 100 ms production-period batches.
    const BATCH: usize = 4_800;

    #[test]
    fn first_partial_slot_is_a_scheduled_discard_and_first_full_slot_completes() {
        // Start mid-slot at UTC 10.03 s (off-phase from the boundary, like
        // real capture): everything before the 15.0 s boundary is the
        // partial first slot (discarded, one FirstSlot record); the
        // 15.0–30.0 s slot completes.
        let mut sim = Sim::new(10_030);
        for _ in 0..250 {
            sim.deliver(BATCH, 1_000); // 25 s of audio
        }
        assert_eq!(sim.abandoned(), vec![DiscardClass::FirstSlot]);
        let done = sim.completed();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].slot_utc_ms, 15_000);
        assert_eq!(done[0].samples.len(), OUT_SLOT_FRAMES);
        assert_eq!(done[0].lost_frames, 0);
        assert_eq!(done[0].boundary_skew_frames, 0);
    }

    #[test]
    fn boundary_detection_is_within_half_a_second() {
        // Batches land at 100 ms cadence starting off-phase (start UTC
        // 10.030 s → pushes at ...14.930, 15.030...): the slot label is the
        // exact multiple of 15 000 and the opening batch arrived within
        // 0.5 s after it.
        let mut sim = Sim::new(10_030);
        let mut opened_at_utc_ms = None;
        for _ in 0..200 {
            sim.deliver(BATCH, 0);
            if opened_at_utc_ms.is_none() && !sim.abandoned().is_empty() {
                opened_at_utc_ms = Some(sim.utc_us / 1_000);
            }
        }
        let opened = opened_at_utc_ms.expect("slot must open");
        assert!((15_000..15_500).contains(&opened), "opened at {opened}");
        assert_eq!(sim.completed()[0].slot_utc_ms, 15_000);
    }

    #[test]
    fn boundary_shortfall_is_zero_filled_to_exact_length() {
        // A slow source: one 4 800-frame batch goes missing near the end of
        // the slot with NO gap report and no catch-up (frames simply never
        // existed — e.g. a slow card). The close must fill to exactly
        // 720 000 in / 180 000 out and account the fill in lost_frames.
        let mut sim = Sim::new(10_030);
        for _ in 0..50 {
            sim.deliver(BATCH, 1_000); // reach the 15 s boundary
        }
        for _ in 0..148 {
            sim.deliver(BATCH, 1_000); // with the opener: 149 of 150 batches
        }
        sim.stall_frames(BATCH as u64); // one batch of wall time, no data
        for _ in 0..30 {
            sim.deliver(BATCH, 1_000); // crosses the 30 s boundary
        }
        let done = sim.completed();
        assert!(!done.is_empty());
        assert_eq!(done[0].slot_utc_ms, 15_000);
        assert_eq!(done[0].samples.len(), OUT_SLOT_FRAMES);
        assert_eq!(done[0].lost_frames, BATCH as u64, "shortfall counted as filled");
    }

    #[test]
    fn gap_is_zero_filled_in_place_and_counted() {
        // 0.5 s dropout mid-slot with an Overrun report on the next batch:
        // 24 000 zeros land immediately after the last delivered frame.
        // Delivered content is DC 1000, so the decimated output shows ~0 in
        // the filled region and ~1000 away from it — placement is
        // observable, not just counted.
        let mut sim = Sim::new(10_030);
        for _ in 0..50 {
            sim.deliver(BATCH, 1_000);
        }
        for _ in 0..74 {
            sim.deliver(BATCH, 1_000); // with the opener: 360 000 frames in
        }
        sim.stall_frames(24_000); // 0.5 s dropout
        sim.deliver_gap(BATCH, 1_000, Some(GapReport { kind: GapKind::Overrun }));
        for _ in 0..70 {
            sim.deliver(BATCH, 1_000); // the 70th push crosses the boundary
        }
        let done = sim.completed();
        assert_eq!(done.len(), 1);
        let slot = done[0];
        assert_eq!(slot.lost_frames, 24_000);
        assert_eq!(slot.boundary_skew_frames, 0);
        assert_eq!(slot.samples.len(), OUT_SLOT_FRAMES);
        // Fill placement: input frames 360 000..384 000 are zeros → output
        // indices 90 000..96 000. Probe the middle of the fill and a point
        // far from it.
        assert!(
            slot.samples[91_800].abs() < 50,
            "fill region should be ~0, got {}",
            slot.samples[91_800]
        );
        assert!(
            (i32::from(slot.samples[50_000]) - 1_000).abs() <= 2,
            "delivered region should be ~1000, got {}",
            slot.samples[50_000]
        );
    }

    #[test]
    fn sub_threshold_deficit_is_jitter_not_loss() {
        // A 2 352-frame deficit (< 2 400) with an Overrun report: NO fill.
        // The late frames then arrive (catch-up, no time advance) and the
        // slot completes with lost_frames == 0.
        let mut sim = Sim::new(10_030);
        for _ in 0..50 {
            sim.deliver(BATCH, 1_000);
        }
        for _ in 0..100 {
            sim.deliver(BATCH, 1_000);
        }
        sim.stall_frames(2_352);
        sim.deliver_gap(BATCH, 1_000, Some(GapReport { kind: GapKind::Overrun }));
        // The card catches up: a full batch delivered in 2 448 frames of
        // wall time (49 + 51 ms = one whole 100 ms period — cadence
        // restored, the late frames were jitter, not loss).
        sim.deliver_wall(BATCH, 2_448, 1_000, None);
        for _ in 0..49 {
            sim.deliver(BATCH, 1_000); // the 48th push crosses the boundary
        }
        let done = sim.completed();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].lost_frames, 0, "sub-threshold deficits are never filled");
        assert_eq!(done[0].boundary_skew_frames, 0);
        assert_eq!(done[0].samples.len(), OUT_SLOT_FRAMES);
    }

    #[test]
    fn exact_divisibility_720000_in_180000_out() {
        // The clean case: exactly 150 batches per slot, zero fill, zero
        // skew, for three consecutive slots (proves per-slot re-anchoring
        // and decimator phase continuity: 720 000 ≡ 0 mod 4). Start 130 ms
        // before the boundary: the FIRST push (14 970) picks 15 000 as the
        // target — a start whose first push lands exactly ON a boundary
        // waits for the NEXT one (documented knife-edge; scheduled-discard
        // either way).
        let mut sim = Sim::new(15_000 - 130);
        for _ in 0..(1 + 150 * 3 + 1) {
            sim.deliver(BATCH, 200);
        }
        let done = sim.completed();
        assert!(done.len() >= 3, "got {} slots", done.len());
        for (i, slot) in done.iter().take(3).enumerate() {
            assert_eq!(slot.slot_utc_ms, 15_000 + 15_000 * i as u64);
            assert_eq!(slot.samples.len(), OUT_SLOT_FRAMES, "slot {i}");
            assert_eq!(slot.lost_frames, 0, "slot {i}");
            assert_eq!(slot.boundary_skew_frames, 0, "slot {i}");
        }
    }

    #[test]
    fn provenance_math_is_computed_on_delivered_frames_only() {
        // First 48 000 delivered frames are full-scale (clipped), the rest
        // are 16 384; a 24 000-frame filled gap sits in the middle. The
        // denominator is delivered frames (720 000 − 24 000 = 696 000);
        // fills are excluded from clip_fraction and rms_dbfs.
        let mut sim = Sim::new(10_030);
        for _ in 0..50 {
            // Pre-boundary fodder at 16 384 too: the 50th push OPENS the
            // slot and its batch is delivered slot content.
            sim.deliver(BATCH, 16_384);
        }
        for _ in 0..10 {
            sim.deliver(BATCH, i16::MAX); // 48 000 clipped frames
        }
        for _ in 0..64 {
            sim.deliver(BATCH, 16_384);
        }
        sim.stall_frames(24_000);
        sim.deliver_gap(BATCH, 16_384, Some(GapReport { kind: GapKind::Overrun }));
        for _ in 0..70 {
            sim.deliver(BATCH, 16_384); // the 70th push crosses the boundary
        }
        let done = sim.completed();
        assert_eq!(done.len(), 1);
        let slot = done[0];
        assert_eq!(slot.lost_frames, 24_000);
        let delivered = 720_000.0 - 24_000.0;
        let want_clip = 48_000.0 / delivered;
        assert!(
            (f64::from(slot.clip_fraction) - want_clip).abs() < 1e-6,
            "clip_fraction {} want {want_clip}",
            slot.clip_fraction
        );
        let sum_sq = 48_000.0 * f64::from(i16::MAX) * f64::from(i16::MAX)
            + (delivered - 48_000.0) * 16_384.0f64 * 16_384.0;
        let want_rms_dbfs = 20.0 * ((sum_sq / delivered).sqrt() / 32_768.0).log10();
        assert!(
            (f64::from(slot.rms_dbfs) - want_rms_dbfs).abs() < 0.01,
            "rms_dbfs {} want {want_rms_dbfs:.4}",
            slot.rms_dbfs
        );
    }

    #[test]
    fn surplus_is_dropped_at_close_never_carried() {
        // A 1% fast card: 4 800 frames arrive in 99 ms of wall time. Every
        // slot sheds its own bounded surplus as boundary_skew_frames and
        // the NEXT slot starts clean — no inherited offset, no later
        // shortfall (the carry-nothing invariant).
        let mut sim = Sim::new(10_030);
        for _ in 0..1_000 {
            sim.deliver_wall(BATCH, 4_752, 0, None);
        }
        let done = sim.completed();
        assert!(done.len() >= 5, "got {} slots", done.len());
        for (i, slot) in done.iter().enumerate() {
            assert_eq!(
                slot.lost_frames, 0,
                "slot {i}: dropped surplus must never resurface as fill"
            );
            assert!(
                slot.boundary_skew_frames > 0,
                "slot {i}: a fast card must shed surplus every slot"
            );
            assert!(
                slot.boundary_skew_frames <= 2 * BATCH as u64,
                "slot {i}: skew {} not bounded",
                slot.boundary_skew_frames
            );
            assert_eq!(slot.samples.len(), OUT_SLOT_FRAMES, "slot {i}");
        }
    }

    #[test]
    fn fast_clock_1000_slots_keeps_skew_bounded_never_carried() {
        // +50 ppm soundcard: 4 800 frames delivered every 99 995 µs. Spec
        // §Testing strategy pins 1 000 slots: slot-content-vs-UTC skew
        // stays bounded (carryover would accumulate ~4.3 s/day — the
        // delta's time-shift kill mechanism, self-inflicted; zero decodes
        // after ~11 h). NOTE: this test decimates 180 M output samples —
        // ~25 s in release, ~5 MINUTES in a debug build on the dev Pi. It
        // is not hung; `cargo test --release -p tuxlink-capture` is a
        // legitimate iteration shortcut (CI runs the debug profile).
        let mut asm = SlotAssembler::new(BoundaryConfig::default());
        let mut utc_us: u64 = 10_030_000;
        let mut mono_us: u64 = 5_000_000;
        let batch = vec![0i16; BATCH];
        let mut completed = 0usize;
        let mut max_skew = 0u64;
        let mut total_skew = 0u64;
        while completed < 1_000 {
            utc_us += 99_995;
            mono_us += 99_995;
            for ev in asm.push(&batch, utc_us / 1_000, mono_us, None) {
                match ev {
                    SlotEvent::Completed(c) => {
                        completed += 1;
                        assert_eq!(c.samples.len(), OUT_SLOT_FRAMES);
                        assert_eq!(c.lost_frames, 0, "slot {completed}");
                        max_skew = max_skew.max(c.boundary_skew_frames);
                        total_skew += c.boundary_skew_frames;
                    }
                    SlotEvent::Abandoned { class } => {
                        assert_eq!(
                            class,
                            DiscardClass::FirstSlot,
                            "only the scheduled first-slot discard is allowed"
                        );
                    }
                    SlotEvent::Dropped { .. } => {
                        panic!("a healthy fast clock must never drop a slot")
                    }
                }
            }
        }
        // Bounded per slot: never more than one delivery batch of surplus.
        assert!(max_skew <= BATCH as u64, "max per-slot skew {max_skew}");
        // And the surplus is real (~36 frames/slot at +50 ppm): if closes
        // silently carried instead of dropping, this would read 0 while
        // slot content drifted ~0.75 s by slot 1 000.
        assert!(total_skew >= 20_000, "total dropped surplus {total_skew}");
    }

    #[test]
    fn negative_computed_gap_is_a_clock_anomaly() {
        let mut sim = Sim::new(10_030);
        for _ in 0..60 {
            sim.deliver(BATCH, 0); // opens at 15.03 s + 10 in-slot batches
        }
        // An Overrun report whose batch arrives with ZERO wall advance:
        // delivered exceeds the monotonic expectation → negative gap.
        sim.deliver_wall(BATCH, 0, 0, Some(GapReport { kind: GapKind::Overrun }));
        assert_eq!(
            sim.abandoned(),
            vec![DiscardClass::FirstSlot, DiscardClass::ClockAnomaly]
        );
        assert!(sim.completed().is_empty());
        // Re-anchor at the NEXT boundary (30 s); the following slot
        // completes — and no second FirstSlot record appears (the anomaly
        // was its own record).
        for _ in 0..320 {
            sim.deliver(BATCH, 0);
        }
        assert_eq!(sim.completed().len(), 1);
        assert_eq!(sim.completed()[0].slot_utc_ms, 30_000);
        assert_eq!(
            sim.abandoned(),
            vec![DiscardClass::FirstSlot, DiscardClass::ClockAnomaly]
        );
    }

    #[test]
    fn single_gap_over_one_second_is_a_clock_anomaly() {
        let mut sim = Sim::new(10_030);
        for _ in 0..60 {
            sim.deliver(BATCH, 0);
        }
        sim.stall_frames(50_400); // a single 1.05 s dropout
        sim.deliver_gap(BATCH, 0, Some(GapReport { kind: GapKind::Overrun }));
        assert_eq!(
            sim.abandoned(),
            vec![DiscardClass::FirstSlot, DiscardClass::ClockAnomaly]
        );
        assert!(sim.completed().is_empty());
    }

    #[test]
    fn utc_vs_mono_divergence_at_boundary_is_a_clock_anomaly() {
        let mut sim = Sim::new(10_030);
        for _ in 0..100 {
            sim.deliver(BATCH, 0); // utc 20.03 s, mid-slot
        }
        sim.utc_us += 2_000_000; // NTP step: UTC jumps +2 s; monotonic does not
        for _ in 0..80 {
            sim.deliver(BATCH, 0); // reaches the (stepped) boundary
        }
        assert!(
            sim.abandoned().contains(&DiscardClass::ClockAnomaly),
            "the step must be observed at the boundary"
        );
        assert!(sim.completed().is_empty());
        // Recovery: re-anchored at the next boundary, a full clean slot
        // completes (300 pushes cover waiting out the partial interval plus
        // one whole slot).
        for _ in 0..310 {
            sim.deliver(BATCH, 0);
        }
        assert_eq!(sim.completed().len(), 1);
    }

    #[test]
    fn cumulative_lost_frames_over_one_second_drops_the_slot() {
        // Two 0.6 s gaps: each under the 48 000-frame single-gap anomaly
        // bound, together 57 600 filled frames — over the 48 000
        // lost-frames bound. The slot is EMITTED AS A DROP (a real failure,
        // counts toward N upstream), not completed, not a scheduled
        // discard.
        let mut sim = Sim::new(10_030);
        for _ in 0..50 {
            sim.deliver(BATCH, 1_000); // opens at 15.03 s
        }
        for _ in 0..40 {
            sim.deliver(BATCH, 1_000);
        }
        sim.stall_frames(28_800); // gap 1: 0.6 s
        sim.deliver_gap(BATCH, 1_000, Some(GapReport { kind: GapKind::Overrun }));
        for _ in 0..40 {
            sim.deliver(BATCH, 1_000);
        }
        sim.stall_frames(28_800); // gap 2: 0.6 s
        sim.deliver_gap(BATCH, 1_000, Some(GapReport { kind: GapKind::Overrun }));
        for _ in 0..60 {
            sim.deliver(BATCH, 1_000); // crosses the 30 s boundary
        }
        let drops: Vec<_> = sim
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    SlotEvent::Dropped { class: DropClass::LostFrames, .. }
                )
            })
            .collect();
        assert_eq!(drops.len(), 1);
        match drops[0] {
            SlotEvent::Dropped { slot_utc_ms, lost_frames, .. } => {
                assert_eq!(*slot_utc_ms, 15_000);
                assert!(*lost_frames > 48_000, "lost {lost_frames}");
            }
            _ => unreachable!(),
        }
        assert!(sim.completed().is_empty());
        // The next slot completes normally — the drop did not poison the
        // stream.
        for _ in 0..160 {
            sim.deliver(BATCH, 1_000);
        }
        assert_eq!(sim.completed().len(), 1);
        assert_eq!(sim.completed()[0].slot_utc_ms, 30_000);
    }

    #[test]
    fn push_with_utc_near_u64_max_does_not_panic_on_boundary_selection() {
        // Gate B P2 (slot.rs:234): `(utc_now_ms / slot_ms + 1) * slot_ms`
        // panics `attempt to multiply with overflow` when `utc_now_ms` is
        // within one `slot_ms` of `u64::MAX`. This test FAILS (panics)
        // before the `saturating_add`/`saturating_mul` fix and PASSES
        // after — boundary selection saturates to `u64::MAX` instead of
        // wrapping. A boundary this far out is never reachable via any
        // real wall clock (~584M years of continuous uptime); saturation
        // is the sane "cannot represent it, so pin to the ceiling"
        // behavior rather than a panic or silent wraparound.
        let mut asm = SlotAssembler::new(BoundaryConfig::default());
        let utc_now_ms = u64::MAX - 10;
        let events = asm.push(&[], utc_now_ms, 0, None);
        assert!(events.is_empty(), "pre-boundary partial: still a scheduled discard");
        assert!(
            matches!(
                asm.phase,
                Phase::Waiting { next_boundary_utc_ms: Some(u64::MAX) }
            ),
            "boundary must saturate to u64::MAX, not wrap"
        );
    }

    #[test]
    fn boundary_close_add_overflow_when_slot_utc_near_max_does_not_panic() {
        // Gate B P2 (slot.rs:264): `slot_utc + slot_ms` panics `attempt to
        // add with overflow` once `slot_utc` is within one `slot_ms` of
        // `u64::MAX`, reachable independently of the line-234 site by
        // constructing an in-slot assembler directly (private-field
        // access from this same-file test submodule). This test FAILS
        // (panics) before the `saturating_add` fix and PASSES after.
        let mut asm = SlotAssembler::new(BoundaryConfig::default());
        asm.phase = Phase::InSlot(Current::open(u64::MAX - 100, 0));
        // utc_now_ms == u64::MAX is >= any saturated slot_utc + slot_ms,
        // so the boundary-crossing branch fires deterministically.
        let events = asm.push(&[], u64::MAX, 0, None);
        // Sane clamped behavior, not just "didn't panic": with
        // mono_now_us == anchor == 0, mono_elapsed_ms is 0 and
        // utc_elapsed_ms is 100 (u64::MAX − (u64::MAX − 100)) — well
        // under the 1 s divergence bound, so this is NOT flagged as a
        // clock anomaly. The slot closes with zero real content (the
        // synthetic setup never delivered samples), so it fills entirely
        // and is correctly dropped as a real failure (lost_frames far
        // exceeds max_lost_frames), not silently swallowed.
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            SlotEvent::Dropped { class: DropClass::LostFrames, .. }
        ));
    }

    #[test]
    fn suspended_abandons_and_reanchors() {
        let mut sim = Sim::new(10_030);
        for _ in 0..60 {
            sim.deliver(BATCH, 0);
        }
        sim.deliver_gap(BATCH, 0, Some(GapReport { kind: GapKind::Suspended }));
        assert_eq!(
            sim.abandoned(),
            vec![DiscardClass::FirstSlot, DiscardClass::ClockAnomaly]
        );
        // Recovery into the next boundary; the following slot completes.
        for _ in 0..320 {
            sim.deliver(BATCH, 0);
        }
        assert_eq!(sim.completed().len(), 1);
        assert_eq!(sim.completed()[0].slot_utc_ms, 30_000);
    }
}
