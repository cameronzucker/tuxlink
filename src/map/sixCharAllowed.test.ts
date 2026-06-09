/**
 * sixCharAllowed gate tests — the 6-char (subsquare) Maidenhead precision is
 * permitted ONLY when the view under the pin is backed by validated LAN tiles
 * (lan-live / lan-cached) at sufficient zoom (>= SIX_CHAR_MIN_ZOOM). Every
 * other status — and insufficient zoom — falls back to 4-char (false).
 *
 * No Tauri/Leaflet here: this is a pure-function gate consumed by sdbd.
 */
import { describe, it, expect } from 'vitest';
import {
  sixCharAllowed,
  SIX_CHAR_MIN_ZOOM,
  type TileSourceStatus,
  type StatusKind,
} from './tileSource';

function status(kind: StatusKind, zoom: number): TileSourceStatus {
  return { kind, zoom, label: null, cachedAt: null };
}

describe('sixCharAllowed', () => {
  it('is true for lan-live at/above the threshold with the view zoomed in', () => {
    expect(
      sixCharAllowed(status('lan-live', SIX_CHAR_MIN_ZOOM), { zoom: SIX_CHAR_MIN_ZOOM }),
    ).toBe(true);
    expect(sixCharAllowed(status('lan-live', 16), { zoom: 16 })).toBe(true);
  });

  it('is true for lan-cached at/above the threshold', () => {
    expect(
      sixCharAllowed(status('lan-cached', 14), { zoom: 14 }),
    ).toBe(true);
  });

  it('is false when the view zoom is below the threshold even if the source is validated deep', () => {
    expect(sixCharAllowed(status('lan-live', 16), { zoom: SIX_CHAR_MIN_ZOOM - 1 })).toBe(false);
  });

  it('is false when the source validated max is below the threshold', () => {
    expect(sixCharAllowed(status('lan-live', SIX_CHAR_MIN_ZOOM - 1), { zoom: 16 })).toBe(false);
  });

  it('is false for every non-validated status, regardless of zoom', () => {
    for (const kind of ['bundled', 'partial', 'unreachable', 'incompatible'] as StatusKind[]) {
      expect(sixCharAllowed(status(kind, 16), { zoom: 16 })).toBe(false);
    }
  });
});
