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
