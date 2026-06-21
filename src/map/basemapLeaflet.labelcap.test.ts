/**
 * Regression test for the "placenames flash then erase on zoom-OUT" bug
 * (tuxlink-c973).
 *
 * Root cause: protomaps-leaflet's `LeafletLayer` hardcodes the label index's
 * `maxLabeledTiles` to 16. That cap is keyed on the DATA tile. With the world
 * overview capped at `maxDataZoom: 6`:
 *  - zoomed IN (display z7-14) every display tile overzooms a handful of z6 data
 *    tiles → ≤4 keys → no pruning → labels stable;
 *  - zoomed OUT (display z≤6, overview data native) a desktop viewport spans
 *    ~20-24 distinct data tiles → EXCEEDS 16 → `Index.pruneOrNoop` evicts
 *    on-screen tiles' labels, which then repaint without them. Labels-only erase
 *    (geometry is painted unconditionally), seen only on zoom-out.
 *
 * Fix: the vendored bundle's two `new Labelers(..., 16, ...)` sites are raised to
 * cover any realistic native-z6 viewport (see PROVENANCE.md "Local patches").
 *
 * Test A proves the eviction mechanism against the real exported `Index`.
 * Test B asserts the SHIPPED layer carries the raised cap (red at 16, green at the
 * patched value) — the actual regression guard for the fix.
 */
import { describe, it, expect } from 'vitest';
import Point from '@mapbox/point-geometry';
import L from 'leaflet';
import { Index } from '../vendor/protomaps-leaflet';
import { buildBaseLayers } from './basemapLeaflet';

// The vendored protomaps-leaflet bundle references a GLOBAL `L` (Leaflet's UMD
// side-effect sets `window.L` in the browser once any module imports it). jsdom
// does not populate that global from the bare import, so set it explicitly before
// constructing a real layer.
(globalThis as unknown as { L: typeof L }).L = L;

/** The cap the fix ships. Kept in sync with the vendored bundle patch. */
const SHIPPED_LABEL_TILE_CAP = 256;

/** A synthetic place label anchored at (x,y) with a 1px bbox — enough for the
 * Index's rbush insert + pruneOrNoop bookkeeping (no canvas needed). */
function placeLabel(x: number, y: number) {
  return {
    anchor: new Point(x, y),
    bboxes: [{ minX: x, minY: y, maxX: x + 1, maxY: y + 1 }],
    draw: () => {},
  };
}

describe('label-tile cap (tuxlink-c973 flash-then-erase)', () => {
  it('A: cap=16 evicts a still-on-screen data tile; a higher cap retains it', () => {
    // 20 native-z6 data tiles in one row (one data source "") — mimics a desktop
    // viewport when zoomed out, which exceeds the old cap of 16.
    const keys = Array.from({ length: 20 }, (_, x) => `${x}:0:6:`);

    const capped = new Index(256 << 6, 16);
    for (const k of keys) {
      const [x] = k.split(':');
      capped.insert(placeLabel(Number(x) * 8, 0), 0, k);
      capped.pruneOrNoop(k);
    }
    // The earliest (left-edge) tile is the farthest from the last-added one, so it
    // is evicted while still in the viewport → its placenames vanish on repaint.
    expect(capped.has('0:0:6:')).toBe(false);
    expect(capped.size()).toBeLessThanOrEqual(17);

    const roomy = new Index(256 << 6, SHIPPED_LABEL_TILE_CAP);
    for (const k of keys) {
      const [x] = k.split(':');
      roomy.insert(placeLabel(Number(x) * 8, 0), 0, k);
      roomy.pruneOrNoop(k);
    }
    expect(roomy.has('0:0:6:')).toBe(true);
    expect(roomy.size()).toBe(20);
  });

  it('B: the shipped overview layer carries the raised label-tile cap', () => {
    // Real (unmocked) seam: buildBaseLayers → leafletLayer → Labelers. The layer's
    // label index cap must clear a realistic native-z6 viewport so zoom-out no
    // longer prunes on-screen placenames.
    const [overview] = buildBaseLayers('dark', []) as unknown as Array<{
      labelers: { maxLabeledTiles: number };
    }>;
    expect(overview.labelers.maxLabeledTiles).toBe(SHIPPED_LABEL_TILE_CAP);
  });
});
