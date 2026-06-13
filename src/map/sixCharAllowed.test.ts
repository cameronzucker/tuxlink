/**
 * sixCharAllowed gate tests (tuxlink-ndi4 A16). On the vector basemap, 6-char
 * (subsquare) Maidenhead precision is offered purely on view zoom: at/above
 * SIX_CHAR_MIN_ZOOM it unlocks; below it falls back to 4-char. The old
 * LAN-raster-status coupling is gone with the raster basemap.
 *
 * No Tauri/Leaflet here: a pure-function gate.
 */
import { describe, it, expect } from 'vitest';
import { sixCharAllowed, SIX_CHAR_MIN_ZOOM } from './sixCharAllowed';

describe('sixCharAllowed', () => {
  it('is true at or above the zoom threshold', () => {
    expect(sixCharAllowed({ zoom: SIX_CHAR_MIN_ZOOM })).toBe(true);
    expect(sixCharAllowed({ zoom: 14 })).toBe(true);
  });

  it('is false below the zoom threshold', () => {
    expect(sixCharAllowed({ zoom: SIX_CHAR_MIN_ZOOM - 0.1 })).toBe(false);
    expect(sixCharAllowed({ zoom: 2 })).toBe(false);
    expect(sixCharAllowed({ zoom: 0 })).toBe(false);
  });
});
