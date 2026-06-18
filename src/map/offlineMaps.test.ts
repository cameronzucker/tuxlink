/**
 * Unit tests for the pure helpers in offlineMaps.ts. The component test
 * (OfflineMapsSettings.test.tsx) mocks this module, so the REAL implementations are
 * exercised here. `continentEstimateBytes` must stay in lockstep with the Rust
 * `commands::continent_estimate` (tuxlink-8g28) — the detail-picker shows the user a
 * size the backend's free-space gate must agree with.
 */
import { describe, it, expect } from 'vitest';
import { continentEstimateBytes } from './offlineMaps';

describe('continentEstimateBytes', () => {
  const baseline = 30_000_000_000; // ~30 GB z14 continent

  it('returns the baseline at z14 and above (no shrink)', () => {
    expect(continentEstimateBytes(baseline, 14)).toBe(baseline);
    expect(continentEstimateBytes(baseline, 20)).toBe(baseline);
  });

  it('halves per zoom below z14 (ceil, biases high)', () => {
    expect(continentEstimateBytes(baseline, 13)).toBe(Math.ceil(baseline / 2));
    expect(continentEstimateBytes(baseline, 11)).toBe(Math.ceil(baseline / 2 ** 3));
  });

  it('is strictly decreasing as detail drops and never zero', () => {
    expect(continentEstimateBytes(baseline, 8)).toBeLessThan(continentEstimateBytes(baseline, 11));
    expect(continentEstimateBytes(baseline, 1)).toBeGreaterThanOrEqual(1);
    expect(continentEstimateBytes(0, 8)).toBeGreaterThanOrEqual(1);
  });

  it('mirrors the Rust shrink model exactly for the bundled tiers', () => {
    // local=z8, regional=z11, wide=z13 → matches commands::continent_estimate's
    // ceil(baseline / 2^(14-maxzoom)).
    expect(continentEstimateBytes(baseline, 8)).toBe(Math.ceil(baseline / 2 ** 6));
    expect(continentEstimateBytes(baseline, 13)).toBe(Math.ceil(baseline / 2 ** 1));
  });
});
