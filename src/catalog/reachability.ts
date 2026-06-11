// Pure reachability bucketing for the Find-a-Station map (design §7, §12).
// REL (VOACAP circuit reliability, 0..1) → one of four tiers driving pin colour
// + size. Thresholds are locked in the plan (§12): good ≥ .70, fair ≥ .40,
// marginal ≥ .15, else skip. The engine is the source of truth for the numbers;
// this only buckets them for display.

import { bandForKhz, type Band } from './bandPlan';
import type { PathPrediction } from './propagationApi';

export type ReachTier = 'good' | 'fair' | 'marginal' | 'skip';

export function relToTier(rel: number): ReachTier {
  if (rel >= 0.70) return 'good';
  if (rel >= 0.40) return 'fair';
  if (rel >= 0.15) return 'marginal';
  return 'skip';
}

const TIER_VAR: Record<ReachTier, string> = {
  good: 'var(--reach-good)',
  fair: 'var(--reach-fair)',
  marginal: 'var(--reach-marginal)',
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
