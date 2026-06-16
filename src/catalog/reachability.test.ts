import { describe, it, expect } from 'vitest';
import { relToTier, bestBandNow, tierColorVar, type ReachTier } from './reachability';
import type { PathPrediction } from './propagationApi';

function hours(v: number): number[] {
  return Array.from({ length: 24 }, () => v);
}

describe('relToTier', () => {
  it('buckets reliability into the six-step green→red→grey ramp', () => {
    expect(relToTier(0.86)).toBe<ReachTier>('good');
    expect(relToTier(0.75)).toBe<ReachTier>('good');
    expect(relToTier(0.70)).toBe<ReachTier>('fair');
    expect(relToTier(0.58)).toBe<ReachTier>('fair');
    expect(relToTier(0.50)).toBe<ReachTier>('marginal');
    expect(relToTier(0.42)).toBe<ReachTier>('marginal');
    expect(relToTier(0.35)).toBe<ReachTier>('poor'); // "maybe not" — orange
    expect(relToTier(0.28)).toBe<ReachTier>('poor');
    // Operator anchor (2026-06-16): ~1-day-in-5 reliability is "almost certainly
    // not" and must read RED (unlikely), not orange.
    expect(relToTier(0.20)).toBe<ReachTier>('unlikely');
    expect(relToTier(0.12)).toBe<ReachTier>('unlikely');
    expect(relToTier(0.08)).toBe<ReachTier>('unlikely');
    // Absolute bottom: not reachable at all (inside radius) → grey, not red.
    expect(relToTier(0.04)).toBe<ReachTier>('skip');
    expect(relToTier(0)).toBe<ReachTier>('skip');
  });
});

describe('tierColorVar', () => {
  it('maps each tier to its CSS custom property', () => {
    expect(tierColorVar('good')).toBe('var(--reach-good)');
    expect(tierColorVar('skip')).toBe('var(--reach-skip)');
  });
});

describe('bestBandNow', () => {
  const prediction: PathPrediction = {
    bearingDeg: 318,
    distanceKm: 77,
    ssn: 118,
    year: 2026,
    month: 6,
    channels: [
      { frequencyKhz: 3590, voacapMhz: 4, relByHour: hours(0.74), snrByHour: hours(10), mufdayByHour: hours(0.9) },
      { frequencyKhz: 7103, voacapMhz: 7, relByHour: hours(0.86), snrByHour: hours(15), mufdayByHour: hours(1) },
      { frequencyKhz: 14103, voacapMhz: 14, relByHour: hours(0.19), snrByHour: hours(2), mufdayByHour: hours(0.3) },
    ],
  };
  it('returns the band with the highest reliability at the given UTC hour', () => {
    expect(bestBandNow(prediction, 21)).toEqual({ band: '40m', rel: 0.86 });
  });
  it('returns null when no channel maps to a modelled HF band', () => {
    const vhfOnly: PathPrediction = { ...prediction, channels: [
      { frequencyKhz: 145710, voacapMhz: 146, relByHour: hours(0.5), snrByHour: hours(5), mufdayByHour: hours(0.5) },
    ]};
    expect(bestBandNow(vhfOnly, 21)).toBeNull();
  });
});
