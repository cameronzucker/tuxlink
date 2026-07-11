/**
 * deriveBandActivity.test.ts — Task B3 (plan tuxlink-b026z.4 §Openness).
 *
 * Covers the brief's (a)-(f):
 *   (a) only decoded/band-dead outcomes count as evidence — a discarded
 *       (qsy-transition) or dropped-* slot on a band never yields a 'quiet'
 *       dot; it stays 'no-data'.
 *   (b) bandSource: 'default-unconfirmed' slots are excluded from attribution.
 *   (c) rate = decodes / (evidence-slot-count * 15s) per SAMPLED minute — the
 *       30-decodes-over-8-evidence-slots worked example (2 min sampled) = 15/min
 *       = hot, NOT diluted by a longer wall-clock window.
 *   (d) tiers: >=8 hot / >=1 warm / sampled-but-below-1 quiet.
 *   (e) fade opacity floors at 0.4.
 *   (f) gridsHeard = distinct 4+char grids in the window.
 */
import { describe, expect, it } from 'vitest';
import { deriveBandActivity, stripStats } from './deriveBandActivity';
import type { BandSource, DecodeDto, RingOutcome, SlotRecord } from './ft8Types';

const SLOT_MS = 15_000;

/** Minimal-but-complete SlotRecord factory — every field the wire contract needs. */
function mkSlot(overrides: {
  slotUtcMs: number;
  band: string;
  outcome: RingOutcome;
  bandSource?: BandSource;
  decodes?: DecodeDto[];
}): SlotRecord {
  return {
    slotUtcMs: overrides.slotUtcMs,
    band: overrides.band,
    dialHz: 14_074_000,
    bandSource: overrides.bandSource ?? 'cat-confirmed',
    bandLabelConfirmedUtcMs: overrides.slotUtcMs,
    outcome: overrides.outcome,
    decodes: overrides.decodes ?? [],
    partialSalvage: false,
    lostFrames: 0,
    boundarySkewFrames: 0,
    clipFraction: 0,
    rmsDbfs: -30,
    dwellSlotIndex: null,
  };
}

function mkDecode(overrides: Partial<DecodeDto> = {}): DecodeDto {
  return {
    slotUtcMs: overrides.slotUtcMs ?? 0,
    snrDb: overrides.snrDb ?? -10,
    dtS: overrides.dtS ?? 0.1,
    freqHz: overrides.freqHz ?? 1500,
    message: overrides.message ?? 'CQ N0CALL EM12',
    fromCall: overrides.fromCall ?? 'N0CALL',
    toCall: overrides.toCall ?? null,
    grid: overrides.grid ?? null,
    partial: overrides.partial ?? false,
  };
}

/** N decoded slots on `band`, each with `decodesPerSlot` decodes, 15s apart starting at t0. */
function decodedSlots(band: string, t0: number, count: number, decodesPerSlot: number): SlotRecord[] {
  const out: SlotRecord[] = [];
  for (let i = 0; i < count; i++) {
    const slotUtcMs = t0 + i * SLOT_MS;
    const decodes = Array.from({ length: decodesPerSlot }, (_, j) =>
      mkDecode({ slotUtcMs, grid: `EM${10 + j}`, message: `msg ${i}-${j}` }),
    );
    out.push(mkSlot({ slotUtcMs, band, outcome: { kind: 'decoded' }, decodes }));
  }
  return out;
}

describe('deriveBandActivity — evidence-only', () => {
  it('(a) a discarded slot on a band never yields quiet — stays no-data', () => {
    const ring: SlotRecord[] = [
      mkSlot({ slotUtcMs: 0, band: '40m', outcome: { kind: 'discarded', class: 'qsy-transition' } }),
    ];
    const dots = deriveBandActivity(ring, 1000);
    expect(dots.get('40m')?.tier).toBe('no-data');
  });

  it('(a) a dropped-backpressure slot on a band never yields quiet — stays no-data', () => {
    const ring: SlotRecord[] = [
      mkSlot({ slotUtcMs: 0, band: '40m', outcome: { kind: 'dropped-backpressure' } }),
      mkSlot({ slotUtcMs: SLOT_MS, band: '40m', outcome: { kind: 'dropped-lost-frames' } }),
    ];
    const dots = deriveBandActivity(ring, 2 * SLOT_MS);
    expect(dots.get('40m')?.tier).toBe('no-data');
  });

  it('(a) decoded outcome with zero decodes still counts as evidence (quiet, not no-data)', () => {
    const ring: SlotRecord[] = [mkSlot({ slotUtcMs: 0, band: '20m', outcome: { kind: 'band-dead' } })];
    const dots = deriveBandActivity(ring, 1000);
    expect(dots.get('20m')?.tier).toBe('quiet');
    expect(dots.get('20m')?.sampledAgoMs).toBe(1000);
  });

  it('(b) default-unconfirmed slots are excluded from attribution entirely', () => {
    const ring: SlotRecord[] = decodedSlots('20m', 0, 4, 5).map((s) => ({
      ...s,
      bandSource: 'default-unconfirmed' as BandSource,
    }));
    const dots = deriveBandActivity(ring, 4 * SLOT_MS);
    expect(dots.has('20m')).toBe(false);
  });

  it('(b) default-unconfirmed slots do not contaminate a confirmed band with the same label', () => {
    const confirmed = decodedSlots('20m', 0, 1, 1); // 1 decode, confirmed
    const unconfirmed = decodedSlots('20m', SLOT_MS, 10, 5).map((s) => ({
      ...s,
      bandSource: 'default-unconfirmed' as BandSource,
    }));
    const ring = [...confirmed, ...unconfirmed];
    const dots = deriveBandActivity(ring, 11 * SLOT_MS);
    // Only the 1 confirmed slot / 1 decode should count: rate = 1*4/1 = 4/min -> warm.
    expect(dots.get('20m')?.tier).toBe('warm');
  });
});

describe('deriveBandActivity — per-sampled-minute rate (not diluted by window)', () => {
  it('(c) 30 decodes over 8 evidence slots (2 min sampled) = 15/min = hot, despite a longer wall-clock window', () => {
    const evidence = decodedSlots('20m', 0, 8, 0).map((s, i) => ({
      ...s,
      // Distribute 30 decodes across 8 slots unevenly to avoid a suspicious round split.
      decodes: Array.from({ length: i < 6 ? 4 : 3 }, (_, j) => mkDecode({ slotUtcMs: s.slotUtcMs, grid: `EM${10 + j}` })),
    }));
    expect(evidence.reduce((sum, s) => sum + s.decodes.length, 0)).toBe(30);

    // Interleave 32 non-evidence slots (discarded) spanning a much longer wall-clock
    // window, so the 10-minute-plus window would dilute a naive whole-window rate.
    const filler: SlotRecord[] = [];
    for (let i = 0; i < 32; i++) {
      filler.push(
        mkSlot({
          slotUtcMs: 8 * SLOT_MS + i * SLOT_MS,
          band: '20m',
          outcome: { kind: 'discarded', class: 'qsy-transition' },
        }),
      );
    }
    const ring = [...evidence, ...filler];
    const nowMs = ring[ring.length - 1]!.slotUtcMs;

    const dots = deriveBandActivity(ring, nowMs);
    const dot = dots.get('20m');
    expect(dot?.tier).toBe('hot');

    const stats = stripStats(ring, '20m', nowMs);
    expect(stats.decodesPerMin).toBeCloseTo(15, 5);
  });
});

describe('deriveBandActivity — tier thresholds', () => {
  it('(d) >=8 decodes/min -> hot', () => {
    // 2 evidence slots, 4 decodes total: rate = 4*4/2 = 8/min exactly.
    const ring = decodedSlots('20m', 0, 2, 2);
    const dots = deriveBandActivity(ring, SLOT_MS);
    expect(dots.get('20m')?.tier).toBe('hot');
  });

  it('(d) >=1 and <8 decodes/min -> warm', () => {
    // 4 evidence slots, 1 decode total: rate = 1*4/4 = 1/min exactly.
    const ring = decodedSlots('20m', 0, 4, 0).map((s, i) => ({
      ...s,
      decodes: i === 0 ? [mkDecode({ slotUtcMs: s.slotUtcMs, grid: 'EM10' })] : [],
    }));
    const dots = deriveBandActivity(ring, 3 * SLOT_MS);
    expect(dots.get('20m')?.tier).toBe('warm');
  });

  it('(d) sampled but below 1 decode/min -> quiet', () => {
    // 10 evidence (band-dead) slots, 0 decodes: rate = 0 -> quiet, not no-data.
    const ring: SlotRecord[] = Array.from({ length: 10 }, (_, i) =>
      mkSlot({ slotUtcMs: i * SLOT_MS, band: '20m', outcome: { kind: 'band-dead' } }),
    );
    const dots = deriveBandActivity(ring, 9 * SLOT_MS);
    expect(dots.get('20m')?.tier).toBe('quiet');
  });

  it('(d) no evidence at all -> no-data', () => {
    const dots = deriveBandActivity([], 0);
    expect(dots.get('20m')).toBeUndefined();
  });
});

describe('deriveBandActivity — fade', () => {
  it('(e) opacity is at its maximum when the evidence is fresh (sampledAgoMs = 0)', () => {
    const ring = decodedSlots('20m', 0, 1, 1);
    const dots = deriveBandActivity(ring, 0);
    expect(dots.get('20m')?.opacity).toBe(1);
  });

  it('(e) opacity decreases as sampledAgoMs grows', () => {
    const ring = decodedSlots('20m', 0, 1, 1);
    const soon = deriveBandActivity(ring, 60_000).get('20m')!.opacity;
    const later = deriveBandActivity(ring, 5 * 60_000).get('20m')!.opacity;
    expect(later).toBeLessThan(soon);
  });

  it('(e) opacity floors at 0.4 no matter how stale the evidence', () => {
    const ring = decodedSlots('20m', 0, 1, 1);
    const dots = deriveBandActivity(ring, 24 * 60 * 60 * 1000); // 1 day later
    expect(dots.get('20m')?.opacity).toBe(0.4);
  });

  it('(e) a no-data dot never claims a nonzero opacity', () => {
    const ring: SlotRecord[] = [
      mkSlot({ slotUtcMs: 0, band: '40m', outcome: { kind: 'discarded', class: 'qsy-transition' } }),
    ];
    const dots = deriveBandActivity(ring, 1000);
    expect(dots.get('40m')?.opacity).toBe(0);
    expect(dots.get('40m')?.sampledAgoMs).toBeNull();
  });
});

describe('stripStats — gridsHeard', () => {
  it('(f) gridsHeard counts only distinct 4+ char grids in the window', () => {
    const ring: SlotRecord[] = [
      mkSlot({
        slotUtcMs: 0,
        band: '20m',
        outcome: { kind: 'decoded' },
        decodes: [
          mkDecode({ slotUtcMs: 0, grid: 'EM12', fromCall: 'N0CALL' }),
          mkDecode({ slotUtcMs: 0, grid: 'EM12', fromCall: 'N1CALL' }), // duplicate grid
          mkDecode({ slotUtcMs: 0, grid: 'FN2', fromCall: 'N2CALL' }), // too short — excluded
          mkDecode({ slotUtcMs: 0, grid: null, fromCall: 'N3CALL' }), // null — excluded
        ],
      }),
      mkSlot({
        slotUtcMs: SLOT_MS,
        band: '20m',
        outcome: { kind: 'decoded' },
        decodes: [mkDecode({ slotUtcMs: SLOT_MS, grid: 'FN20', fromCall: 'N4CALL' })],
      }),
    ];
    const stats = stripStats(ring, '20m', 2 * SLOT_MS);
    expect(stats.gridsHeard).toBe(2); // EM12, FN20
  });

  it('(f) gridsHeard is provenance-gated and evidence-only like the dot', () => {
    const ring: SlotRecord[] = [
      mkSlot({
        slotUtcMs: 0,
        band: '20m',
        bandSource: 'default-unconfirmed',
        outcome: { kind: 'decoded' },
        decodes: [mkDecode({ slotUtcMs: 0, grid: 'EM12' })],
      }),
    ];
    const stats = stripStats(ring, '20m', SLOT_MS);
    expect(stats.gridsHeard).toBe(0);
    expect(stats.decodesPerMin).toBe(0);
  });
});
