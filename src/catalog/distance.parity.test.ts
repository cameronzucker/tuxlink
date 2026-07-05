import { describe, it, expect } from 'vitest';
import { distanceFromGrids } from './distance';

// Parity anchor shared with the Rust geo.rs test (haversine_matches_shipping_fixture)
// and src-tauri/tests/propagation_live.rs:87. If the Rust and TS haversines diverge,
// one of these two assertions moves off 215.28 and the mismatch is caught in CI.
describe('haversine cross-language parity', () => {
  it('DM43->DM34 matches the shared 215.28 km fixture', () => {
    const km = distanceFromGrids('DM43', 'DM34');
    expect(km).not.toBeNull();
    expect(Math.abs((km as number) - 215.28)).toBeLessThan(0.5);
  });
});
