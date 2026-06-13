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
  REGION_MINZOOM,
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

describe('buildBasemapStyle — R7 region-pack compositing', () => {
  it('adds no extra sources/layers when no packs are installed', () => {
    const none = buildBasemapStyle('light', []);
    const base = buildBasemapStyle('light');
    expect(Object.keys(none.sources)).toEqual([BASEMAP_SOURCE_ID]);
    expect(none.layers.length).toBe(base.layers.length);
  });

  it('adds a vector source per pack served via tile://pmtiles/<id>', () => {
    const style = buildBasemapStyle('light', [{ id: 'tier-wide-n34-w112' }]);
    expect(style.sources['pack-tier-wide-n34-w112']).toMatchObject({
      type: 'vector',
      url: 'pmtiles://tile://pmtiles/tier-wide-n34-w112',
      attribution: OSM_ATTRIBUTION,
    });
    // World overview source is untouched (stays present everywhere = never blank).
    expect(style.sources[BASEMAP_SOURCE_ID]).toMatchObject({ url: PMTILES_SOURCE_URL });
  });

  it('clamps every pack layer to minzoom >= REGION_MINZOOM and binds it to the pack source', () => {
    const style = buildBasemapStyle('light', [{ id: 'continent-na' }]);
    const packLayers = style.layers.filter(
      (l): l is typeof l & { source: string } => 'source' in l && l.source === 'pack-continent-na',
    );
    expect(packLayers.length).toBeGreaterThan(0);
    expect(packLayers.every((l) => (l.minzoom ?? 0) >= REGION_MINZOOM)).toBe(true);
    // Pack layer ids are namespaced so they can't collide with the overview's.
    expect(packLayers.every((l) => l.id.startsWith('pack-continent-na-'))).toBe(true);
  });

  it('draws pack layers AFTER (on top of) the overview layers', () => {
    const style = buildBasemapStyle('light', [{ id: 'tier-wide-n34-w112' }]);
    const srcOf = (l: (typeof style.layers)[number]): string | undefined =>
      'source' in l ? (l as { source?: string }).source : undefined;
    const worldIdx = style.layers
      .map((l, i) => (srcOf(l) === BASEMAP_SOURCE_ID ? i : -1))
      .filter((i) => i >= 0);
    const lastWorld = worldIdx[worldIdx.length - 1] ?? -1;
    const firstPack = style.layers.findIndex((l) => srcOf(l) === 'pack-tier-wide-n34-w112');
    expect(firstPack).toBeGreaterThan(lastWorld);
  });

  it('composites multiple packs, each with its own source', () => {
    const style = buildBasemapStyle('dark', [{ id: 'tier-wide-n34-w112' }, { id: 'continent-na' }]);
    expect(style.sources['pack-tier-wide-n34-w112']).toBeDefined();
    expect(style.sources['pack-continent-na']).toBeDefined();
  });
});
