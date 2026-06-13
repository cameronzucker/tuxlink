/**
 * Tests for the MapLibre basemap style builder (tuxlink-ndi4, plan phase 2).
 *
 * buildBasemapStyle assembles a MapLibre v8 style from @protomaps/basemaps'
 * light flavor over the bundled PMTiles vector source. Pure function — exercises
 * the REAL @protomaps/basemaps layers()/namedFlavor() (not mocked).
 */
import { describe, it, expect } from 'vitest';
import {
  buildBasemapStyle,
  BASEMAP_SOURCE_ID,
  PMTILES_SOURCE_URL,
  OSM_ATTRIBUTION,
} from './basemapStyle';

describe('buildBasemapStyle (light)', () => {
  it('produces a v8 style backed by the bundled PMTiles vector source', () => {
    const style = buildBasemapStyle('light');
    expect(style.version).toBe(8);
    const source = style.sources[BASEMAP_SOURCE_ID];
    expect(source).toMatchObject({
      type: 'vector',
      url: PMTILES_SOURCE_URL,
      attribution: OSM_ATTRIBUTION,
    });
  });

  it('resolves glyphs + sprite to bundled self-origin paths (fully offline)', () => {
    const style = buildBasemapStyle('light');
    // Glyphs are {fontstack}/{range}-keyed whole-file fetches served from 'self',
    // distinct from the pmtiles byte-range path (plan A8).
    expect(style.glyphs).toBe('/basemap/glyphs/{fontstack}/{range}.pbf');
    expect(style.sprite).toBe('/basemap/sprites/light');
  });

  it('includes the protomaps layers, all referencing the source, with a background', () => {
    const style = buildBasemapStyle('light');
    expect(style.layers.length).toBeGreaterThan(50);
    expect(style.layers.some((l) => l.type === 'background')).toBe(true);
    const featureLayers = style.layers.filter(
      (l): l is typeof l & { source: string } => 'source' in l && Boolean(l.source),
    );
    expect(featureLayers.length).toBeGreaterThan(0);
    expect(featureLayers.every((l) => l.source === BASEMAP_SOURCE_ID)).toBe(true);
  });
});

describe('buildBasemapStyle (dark, L2 baked)', () => {
  it('uses the dark sprite but the same glyphs + source', () => {
    const dark = buildBasemapStyle('dark');
    const light = buildBasemapStyle('light');
    expect(dark.sprite).toBe('/basemap/sprites/dark');
    expect(dark.glyphs).toBe(light.glyphs);
    expect(dark.sources[BASEMAP_SOURCE_ID]).toEqual(light.sources[BASEMAP_SOURCE_ID]);
  });

  it('bakes inverted colors — the background flips from light to dark', () => {
    const dark = buildBasemapStyle('dark');
    const light = buildBasemapStyle('light');
    const bg = (l: typeof dark) =>
      (l.layers.find((x) => x.type === 'background') as { paint?: Record<string, unknown> }).paint?.[
        'background-color'
      ];
    expect(bg(dark)).not.toBe(bg(light));
  });

  it('leaves no light *-color untransformed (every color differs from light)', () => {
    const dark = buildBasemapStyle('dark');
    const light = buildBasemapStyle('light');
    // Spot-check: each layer's first string color in dark differs from light.
    for (let i = 0; i < light.layers.length; i++) {
      const lp = (light.layers[i] as { paint?: Record<string, unknown> }).paint;
      const dp = (dark.layers[i] as { paint?: Record<string, unknown> }).paint;
      if (!lp) continue;
      for (const k of Object.keys(lp)) {
        if (k.endsWith('-color') && typeof lp[k] === 'string') {
          expect(dp?.[k]).not.toBe(lp[k]);
        }
      }
    }
  });
});
