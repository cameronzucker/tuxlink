/**
 * Tests for tuxlink's restrained OSM-Carto-class basemap flavor (tuxlink-ndi4
 * phase 3; MeshMap-fidelity rework tuxlink-h17b). Locks the OSM-Carto LIGHT
 * values (which bake-invert to the MeshMap dark — see tuxlinkFlavor.meshmap.test)
 * and that stock Protomaps slots are inherited.
 */
import { describe, it, expect } from 'vitest';
import { namedFlavor } from '@protomaps/basemaps';
import { tuxlinkFlavor, TUXLINK_FLAVOR_OVERRIDES } from './tuxlinkFlavor';

describe('tuxlinkFlavor', () => {
  it('applies restrained OSM-Carto-class road values, not the punched-up ramp', () => {
    const f = tuxlinkFlavor() as unknown as Record<string, unknown>;
    // The old loud values are gone (they inverted to a garish dark).
    expect(f.highway).not.toBe('#e85d3a');
    expect(f.major).not.toBe('#f2933a');
    // OSM-Carto warm-but-muted ramp + beige earth + muted water.
    expect(f.highway).toBe('#e990a0');
    expect(f.major).toBe('#fcd6a4');
    expect(f.water).toBe('#aad3df');
    expect(f.earth).toBe('#f2efe9');
    // Still overrides stock light (which has white/near-white roads).
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
