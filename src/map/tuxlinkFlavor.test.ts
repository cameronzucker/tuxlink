/**
 * Tests for tuxlink's high-contrast basemap flavor (tuxlink-ndi4 phase 3).
 * Locks the contrast-bearing overrides (the design the operator approved as
 * "looks just like meshmap") and that stock Protomaps slots are inherited.
 */
import { describe, it, expect } from 'vitest';
import { namedFlavor } from '@protomaps/basemaps';
import { tuxlinkFlavor, TUXLINK_FLAVOR_OVERRIDES } from './tuxlinkFlavor';

describe('tuxlinkFlavor', () => {
  it('applies the muted warm road ramp (tuxlink-hzwc bug #8)', () => {
    const f = tuxlinkFlavor() as unknown as Record<string, unknown>;
    // Roads carry legibility — a MUTED warm ramp (saturation cut, lightness
    // preserved for dark-mode inversion), not the prior garish orange/yellow
    // nor stock light-gray/white.
    expect(f.major).toBe('#cb9c6f');
    expect(f.highway).toBe('#b5705e');
    expect(f.minor_a).toBe('#dfc888');
    expect(f.water).toBe('#2f7fc4');
    // Differs from stock light (which has white/near-white roads).
    expect(f.major).not.toBe((namedFlavor('light') as unknown as Record<string, unknown>).major);
  });

  it('inherits stock slots not overridden', () => {
    const stock = namedFlavor('light') as unknown as Record<string, unknown>;
    const f = tuxlinkFlavor() as unknown as Record<string, unknown>;
    for (const key of Object.keys(stock)) {
      if (!(key in TUXLINK_FLAVOR_OVERRIDES)) {
        expect(f[key]).toBe(stock[key]);
      }
    }
  });
});
