/**
 * Tests for tuxlink's high-contrast basemap flavor (tuxlink-ndi4 phase 3).
 * Locks the contrast-bearing overrides (the design the operator approved as
 * "looks just like meshmap") and that stock Protomaps slots are inherited.
 */
import { describe, it, expect } from 'vitest';
import { namedFlavor } from '@protomaps/basemaps';
import { tuxlinkFlavor, TUXLINK_FLAVOR_OVERRIDES } from './tuxlinkFlavor';

describe('tuxlinkFlavor', () => {
  it('applies the bold contrast overrides', () => {
    const f = tuxlinkFlavor() as unknown as Record<string, unknown>;
    // Roads carry legibility — bold warm ramp, not stock light-gray/white.
    expect(f.major).toBe('#f2933a');
    expect(f.highway).toBe('#e85d3a');
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
