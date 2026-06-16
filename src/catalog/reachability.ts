// Pure reachability bucketing for the Find-a-Station map (design §7, §12).
// REL (VOACAP circuit reliability, 0..1) → one of SIX tiers driving pin colour
// + size. The ramp reads honestly against the modelled reliability:
//   good/fair/marginal — green→lime→amber, increasingly usable
//   poor   (orange) — "maybe not" (a real coin-flip-leaning-no band)
//   unlikely (red)  — "almost certainly not": connects only a small fraction of
//                     days (e.g. REL .20 ≈ 1 day in 5) — must read red, not orange
//   skip   (grey)   — the absolute bottom: not reachable at all, but inside the
//                     search radius (REL ≈ 0). Grey, not red, so a sliver-of-a-
//                     chance path is visibly distinct from a dead one.
// Recalibrated 2026-06-16 (operator). The engine owns the numbers; this buckets them.

import { bandForKhz, type Band } from './bandPlan';
import type { PathPrediction } from './propagationApi';

export type ReachTier = 'good' | 'fair' | 'marginal' | 'poor' | 'unlikely' | 'skip';

export function relToTier(rel: number): ReachTier {
  if (rel >= 0.75) return 'good'; // reliable
  if (rel >= 0.58) return 'fair'; // likely
  if (rel >= 0.42) return 'marginal'; // even odds
  if (rel >= 0.28) return 'poor'; // maybe not (orange)
  if (rel >= 0.08) return 'unlikely'; // almost certainly not (red)
  return 'skip'; // not reachable, inside radius (grey)
}

const TIER_VAR: Record<ReachTier, string> = {
  good: 'var(--reach-good)',
  fair: 'var(--reach-fair)',
  marginal: 'var(--reach-marginal)',
  poor: 'var(--reach-poor)',
  unlikely: 'var(--reach-unlikely)',
  skip: 'var(--reach-skip)',
};

export function tierColorVar(tier: ReachTier): string {
  return TIER_VAR[tier];
}

export interface BestBand {
  band: Band;
  rel: number;
}

/** The modelled-HF band with the highest reliability at `utcHour`, or null. */
export function bestBandNow(prediction: PathPrediction, utcHour: number): BestBand | null {
  let best: BestBand | null = null;
  for (const ch of prediction.channels) {
    const band = bandForKhz(ch.frequencyKhz);
    if (!band || band === 'vhf-uhf') continue;
    const rel = ch.relByHour[utcHour] ?? 0;
    if (!best || rel > best.rel) best = { band, rel };
  }
  return best;
}
