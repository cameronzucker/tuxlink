/**
 * Pins the realization (tuxlink-h17b, Option A): feeding the OSM-Carto-class
 * LIGHT source through darkStyle's bake (`invert → hue-rotate(180°) →
 * brightness(1.33)`, the exact MeshMap filter) yields the MeshMap-class dark
 * palette. This is why restraining tuxlinkFlavor — not a hand-built dark style —
 * reaches the MeshMap bar. Values are the actual xformHex outputs.
 */
import { describe, it, expect } from 'vitest';
import { xformHex } from './darkStyle';
import { TUXLINK_FLAVOR_OVERRIDES } from './tuxlinkFlavor';

// OSM-Carto LIGHT source slot -> expected MeshMap-class DARK after the bake.
const CASES: Array<[string, string, string]> = [
  ['earth', '#f2efe9', '#19150d'], // neutral dark canvas
  ['water', '#aad3df', '#194f5f'], // muted teal
  ['highway', '#e990a0', '#d55e73'], // freeway salmon
  ['major', '#fcd6a4', '#5d2b00'], // arterial recedes (dark brown, not orange)
  ['minor_a', '#f7fabf', '#101400'], // minor street recedes
];

describe('OSM-Carto light source bakes to the MeshMap dark palette', () => {
  it.each(CASES)('%s: xformHex(%s) === %s', (_slot, src, dark) => {
    expect(xformHex(src)).toBe(dark);
  });

  it('the live flavor slots match the source colors the bake expects', () => {
    expect(TUXLINK_FLAVOR_OVERRIDES.earth).toBe('#f2efe9');
    expect(TUXLINK_FLAVOR_OVERRIDES.water).toBe('#aad3df');
    expect(TUXLINK_FLAVOR_OVERRIDES.highway).toBe('#e990a0');
    expect(TUXLINK_FLAVOR_OVERRIDES.major).toBe('#fcd6a4');
    expect(TUXLINK_FLAVOR_OVERRIDES.minor_a).toBe('#f7fabf');
  });
});
