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
 *   - Window = 10 min = 40 slots (§Openness "Window", matching the snapshot's
 *     `ring_tail` cap). Only confirmed slots whose `slotUtcMs` lies inside
 *     `[nowMs - WINDOW_MS, nowMs]` are in scope. Evidence OUTSIDE the window
 *     does NOT count toward tier / rate / opacity: a band swept away from for
 *     more than 10 minutes reverts to `no-data` (hollow dot) rather than
 *     reporting a dimmed-but-visible stale tier forever. The `<= nowMs` upper
 *     bound also drops clock-skewed future-dated slots (which would otherwise
 *     yield a negative `sampledAgoMs` and an opacity > 1).
 *   - Fade: WITHIN the window, opacity fades linearly with `sampledAgoMs` from
 *     1.0 (fresh) to the 0.4 floor at the window edge — evidence is never
 *     rendered fully invisible while it is still in scope. A `no-data` dot has
 *     no in-window evidence to fade, so it renders at 0 (distinct from the 0.4
 *     floor a real-but-old in-window dot gets).
 *   - Never-sampleable bands (60m, VHF) simply never appear in the returned
 *     Map (nothing in the ring ever names them) — the consumer's job is to not
 *     query/render a dot for those bands at all. A band that DOES appear in the
 *     ring's in-window scope, but only via non-evidence or provenance-excluded
 *     slots, is present in the Map as an explicit `'no-data'` entry (see (a)
 *     below), not omitted.
 */
import type { BandDot, RingOutcome, SlotRecord } from './ft8Types';

/** Nominal FT8 slot period (records.rs — 15s cadence). */
const SLOT_SECONDS = 15;
/** Decodes/min tier thresholds. */
const HOT_THRESHOLD_PER_MIN = 8;
const WARM_THRESHOLD_PER_MIN = 1;
/**
 * Openness window (§Openness "Window = 10 min = 40 slots"). The SINGLE window
 * used by both `deriveBandActivity` and `stripStats` for scope, tiering, and
 * fade — evidence older than this reverts a band to `no-data`.
 */
const WINDOW_MS = 600_000;
/** Fade floor (§Openness "Staleness"). Fade spans the full `WINDOW_MS`. */
const OPACITY_FLOOR = 0.4;
const OPACITY_MAX = 1;
/** A grid square must be at least 4 chars (the Maidenhead field+square) to count. */
const MIN_GRID_LEN = 4;

function isEvidence(outcome: RingOutcome): boolean {
  return outcome.kind === 'decoded' || outcome.kind === 'band-dead';
}

/**
 * The SHARED gating path: provenance-confirmed slots (`default-unconfirmed`
 * never attributes to a band) whose `slotUtcMs` lies inside the 10-minute
 * window `[nowMs - WINDOW_MS, nowMs]`. Both exported functions build on this,
 * so their evidence/provenance/window gating is byte-for-byte identical.
 */
function slotsInScope(ring: SlotRecord[], nowMs: number): SlotRecord[] {
  const lowerBound = nowMs - WINDOW_MS;
  return ring.filter(
    (rec) =>
      rec.bandSource !== 'default-unconfirmed' && rec.slotUtcMs >= lowerBound && rec.slotUtcMs <= nowMs,
  );
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

function aggregateBand(inScope: SlotRecord[], band: string): BandAggregate {
  const bandSlots = inScope.filter((rec) => rec.band === band);
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

/**
 * Linear fade from 1.0 at age=0 to the 0.4 floor at the window edge (age ==
 * WINDOW_MS). `sampledAgoMs` is always within `[0, WINDOW_MS]` for an in-scope
 * evidence slot (the caller only reaches here with in-window evidence), so the
 * floor is hit exactly at the edge; the `clamp` is belt-and-suspenders. A
 * `null` age (no in-window evidence → no-data) renders at 0.
 */
function opacityFor(sampledAgoMs: number | null): number {
  if (sampledAgoMs === null) return 0; // no-data: nothing to render.
  const decayFraction = Math.min(1, Math.max(0, sampledAgoMs / WINDOW_MS));
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
 * attribution) inside the 10-minute window. Bands with NO in-window confirmed
 * slot at all — never seen, or last seen more than 10 min ago — are simply
 * absent from the returned Map. A band present in-window only via non-evidence
 * slots (`discarded` / `dropped-*` / `failed`) is an explicit `no-data` entry.
 *
 * CONSUMER CONTRACT (spec §Openness): an absent key means "no in-window
 * evidence for this band" — NOT "never sampleable". Per the spec, `no-data` is
 * the DEFAULT for every sampleable HF band, so a consumer rendering per-band
 * chips/rows MUST treat an absent HF-band key as a hollow `no-data` dot (that
 * is C4's behavior). Only the genuinely never-sampleable bands (60m, VHF/UHF)
 * render NO dot at all, and that distinction is the CONSUMER's (by band
 * identity), not encoded by Map absence here. Do not collapse "absent key" to
 * "omit the dot" for HF bands — that would blank the openness affordance for
 * nearly every chip most of the time (a single RX samples one band at a time).
 */
export function deriveBandActivity(ring: SlotRecord[], nowMs: number): Map<string, BandDot> {
  const inScope = slotsInScope(ring, nowMs);
  const bands = new Set(inScope.map((rec) => rec.band));
  const dots = new Map<string, BandDot>();
  for (const band of bands) {
    dots.set(band, dotFor(aggregateBand(inScope, band), nowMs));
  }
  return dots;
}

/**
 * Strip stats for one band: the same evidence-only, provenance-gated,
 * window-bounded decodes/min rate `deriveBandActivity` uses for tiering, plus
 * the count of distinct 4+ char grids heard IN the same 10-minute window. Both
 * exported functions route through the identical `slotsInScope` gating, so a
 * band that reads `no-data` on the dot also reports `0` decodes/min and `0`
 * grids here — they can never disagree about what was heard.
 */
export function stripStats(
  ring: SlotRecord[],
  band: string,
  nowMs: number,
): { decodesPerMin: number; gridsHeard: number } {
  const inScope = slotsInScope(ring, nowMs);
  const agg = aggregateBand(inScope, band);
  const grids = new Set(
    agg.evidenceDecodeGrids.filter((grid): grid is string => grid !== null && grid.length >= MIN_GRID_LEN),
  );
  return {
    decodesPerMin: decodesPerSampledMinute(agg),
    gridsHeard: grids.size,
  };
}
