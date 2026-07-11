/**
 * deriveBandActivity.ts — per-band openness dots + strip stats (Task B3, plan
 * tuxlink-b026z.4 §Frontend data layer §Openness).
 *
 * Pure derivations over the decode ring (`SlotRecord[]`) — no I/O, deterministic
 * given `(ring, nowMs)`. Consumed by C4 (chip dots), C3 (BandMatrix row dots),
 * C7 (strip stats).
 *
 * Openness invariants (load-bearing — see `BandDot` in `ft8Types.ts`):
 *   - Evidence-only: ONLY `decoded` and `band-dead` ring outcomes are evidence.
 *     A `discarded` (qsy-transition) or `dropped-*` slot on a band NEVER yields
 *     a `quiet` dot — it stays `no-data`. A dot never claims knowledge it lacks.
 *   - Provenance-gated: slots with `bandSource === 'default-unconfirmed'` are
 *     excluded from attribution entirely (the band label wasn't confirmed, so
 *     the slot cannot be attributed to any band).
 *   - Per-SAMPLED-minute rate: `rate = Σ decodes.length / (evidenceSlotCount *
 *     15s)`, expressed per minute — NOT diluted by the wall-clock span of the
 *     ring. 30 decodes over 8 evidence slots (2 minutes' worth of SAMPLING,
 *     regardless of how much real time elapsed around those 8 slots) = 15/min
 *     = hot.
 *   - Tiers: >=8 decodes/min -> 'hot'; >=1 -> 'warm'; sampled (evidence present)
 *     but below 1 -> 'quiet'; no evidence at all -> 'no-data'.
 *   - Fade: opacity fades linearly with `sampledAgoMs` over a 10-minute window,
 *     floored at 0.4 — evidence is never rendered fully invisible once it
 *     exists. A `no-data` dot has no evidence to fade, so it renders at 0.
 *   - Never-sampleable bands (60m, VHF) simply never appear in the returned
 *     Map (nothing in the ring ever names them) — the consumer's job is to not
 *     query/render a dot for those bands at all. A band that DOES appear in the
 *     ring, but only via non-evidence or provenance-excluded slots, is present
 *     in the Map as an explicit `'no-data'` entry (see (a) below), not omitted.
 */
import type { BandDot, RingOutcome, SlotRecord } from './ft8Types';

/** Nominal FT8 slot period (records.rs — 15s cadence). */
const SLOT_SECONDS = 15;
/** Decodes/min tier thresholds. */
const HOT_THRESHOLD_PER_MIN = 8;
const WARM_THRESHOLD_PER_MIN = 1;
/** Fade window + floor (§Openness "Fade"). */
const FADE_WINDOW_MS = 10 * 60 * 1000;
const OPACITY_FLOOR = 0.4;
const OPACITY_MAX = 1;
/** A grid square must be at least 4 chars (the Maidenhead field+square) to count. */
const MIN_GRID_LEN = 4;

function isEvidence(outcome: RingOutcome): boolean {
  return outcome.kind === 'decoded' || outcome.kind === 'band-dead';
}

/** Provenance-confirmed slots only — `default-unconfirmed` never attributes to a band. */
function confirmedSlots(ring: SlotRecord[]): SlotRecord[] {
  return ring.filter((rec) => rec.bandSource !== 'default-unconfirmed');
}

interface BandAggregate {
  /** All provenance-confirmed slots attributed to this band (any outcome). */
  dwellSlots: number;
  /** Provenance-confirmed slots whose outcome is evidence (decoded/band-dead). */
  evidenceCount: number;
  /** Σ decodes.length across evidence slots. */
  totalDecodes: number;
  /** Most recent evidence slot's slotUtcMs (`null` if no evidence). */
  lastEvidenceUtcMs: number | null;
  /** Evidence slots' decode payloads (for gridsHeard). */
  evidenceDecodeGrids: (string | null)[];
}

function aggregateBand(confirmed: SlotRecord[], band: string): BandAggregate {
  const bandSlots = confirmed.filter((rec) => rec.band === band);
  let evidenceCount = 0;
  let totalDecodes = 0;
  let lastEvidenceUtcMs: number | null = null;
  const evidenceDecodeGrids: (string | null)[] = [];

  for (const rec of bandSlots) {
    if (!isEvidence(rec.outcome)) continue;
    evidenceCount += 1;
    totalDecodes += rec.decodes.length;
    if (lastEvidenceUtcMs === null || rec.slotUtcMs > lastEvidenceUtcMs) {
      lastEvidenceUtcMs = rec.slotUtcMs;
    }
    for (const decode of rec.decodes) {
      evidenceDecodeGrids.push(decode.grid);
    }
  }

  return {
    dwellSlots: bandSlots.length,
    evidenceCount,
    totalDecodes,
    lastEvidenceUtcMs,
    evidenceDecodeGrids,
  };
}

/** Σ decodes / (evidenceSlotCount * 15s), expressed per minute. `0` when no evidence. */
function decodesPerSampledMinute(agg: Pick<BandAggregate, 'evidenceCount' | 'totalDecodes'>): number {
  if (agg.evidenceCount === 0) return 0;
  const sampledSeconds = agg.evidenceCount * SLOT_SECONDS;
  return (agg.totalDecodes / sampledSeconds) * 60;
}

function tierFor(rate: number, evidenceCount: number): BandDot['tier'] {
  if (evidenceCount === 0) return 'no-data';
  if (rate >= HOT_THRESHOLD_PER_MIN) return 'hot';
  if (rate >= WARM_THRESHOLD_PER_MIN) return 'warm';
  return 'quiet';
}

/** Linear fade from 1.0 at age=0 to the 0.4 floor at age>=10min, clamped at the floor beyond. */
function opacityFor(sampledAgoMs: number | null): number {
  if (sampledAgoMs === null) return 0; // no-data: nothing to render.
  const decayFraction = Math.min(1, sampledAgoMs / FADE_WINDOW_MS);
  const opacity = OPACITY_MAX - (OPACITY_MAX - OPACITY_FLOOR) * decayFraction;
  return Math.max(OPACITY_FLOOR, opacity);
}

function dotFor(agg: BandAggregate, nowMs: number): BandDot {
  const rate = decodesPerSampledMinute(agg);
  const tier = tierFor(rate, agg.evidenceCount);
  const sampledAgoMs = agg.lastEvidenceUtcMs === null ? null : nowMs - agg.lastEvidenceUtcMs;
  return {
    tier,
    opacity: opacityFor(sampledAgoMs),
    sampledAgoMs,
    dwellSlots: agg.dwellSlots,
  };
}

/**
 * Derive one openness dot per band that appears (with provenance-confirmed
 * attribution) anywhere in the ring. Bands never seen in the ring at all are
 * simply absent from the returned Map — the consumer treats an absent key the
 * same as "never sampleable" and renders no dot.
 */
export function deriveBandActivity(ring: SlotRecord[], nowMs: number): Map<string, BandDot> {
  const confirmed = confirmedSlots(ring);
  const bands = new Set(confirmed.map((rec) => rec.band));
  const dots = new Map<string, BandDot>();
  for (const band of bands) {
    dots.set(band, dotFor(aggregateBand(confirmed, band), nowMs));
  }
  return dots;
}

/**
 * Strip stats for one band: the same evidence-only, provenance-gated
 * decodes/min rate `deriveBandActivity` uses for tiering, plus the count of
 * distinct 4+ char grids heard. `nowMs` guards against clock-skewed slots
 * dated in the future contaminating the stat; per the same "not diluted by
 * window" invariant as the tier rate, it does not otherwise bound the window —
 * the ring itself (already capped upstream) IS the window.
 */
export function stripStats(
  ring: SlotRecord[],
  band: string,
  nowMs: number,
): { decodesPerMin: number; gridsHeard: number } {
  const confirmed = confirmedSlots(ring).filter((rec) => rec.slotUtcMs <= nowMs);
  const agg = aggregateBand(confirmed, band);
  const grids = new Set(
    agg.evidenceDecodeGrids.filter((grid): grid is string => grid !== null && grid.length >= MIN_GRID_LEN),
  );
  return {
    decodesPerMin: decodesPerSampledMinute(agg),
    gridsHeard: grids.size,
  };
}
