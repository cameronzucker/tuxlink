/**
 * Tests for the MapLibre basemap style builder (tuxlink-ndi4, plan phase 2).
 *
 * buildBasemapStyle assembles a MapLibre v8 style from @protomaps/basemaps'
 * light flavor over the bundled PMTiles vector source. Pure function — exercises
 * the REAL @protomaps/basemaps layers()/namedFlavor() (not mocked).
 */
import { describe, it, expect, vi, afterEach } from 'vitest';
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
    // distinct from the pmtiles byte-range path (plan A8). maplibre v5 requires
    // ABSOLUTE URLs (tuxlink-56ki) — origin-prefixed at runtime.
    expect(style.glyphs).toBe(`${location.origin}/basemap/glyphs/{fontstack}/{range}.pbf`);
    expect(style.sprite).toBe(`${location.origin}/basemap/sprites/light`);
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
    expect(dark.sprite).toBe(`${location.origin}/basemap/sprites/dark`);
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

describe('buildBasemapStyle (dark) memoization (B3, tuxlink-vnk7)', () => {
  it('returns the SAME baked base layer array across no-pack calls (bake once per flavor)', () => {
    const a = buildBasemapStyle('dark');
    const b = buildBasemapStyle('dark');
    // The overview (non-pack) layers must be the identical cached array reference,
    // proving bakeDarkColors ran once for the base, not on every call.
    expect(a.layers).toBe(b.layers);
  });
  it('light path is also memoized (same base array reference across no-pack calls)', () => {
    const a = buildBasemapStyle('light');
    const b = buildBasemapStyle('light');
    expect(a.layers).toBe(b.layers);
  });
  it('dark still differs from light per-color after memoization', () => {
    const dark = buildBasemapStyle('dark');
    const light = buildBasemapStyle('light');
    const bg = (s: typeof dark) =>
      (s.layers.find((x) => x.type === 'background') as { paint?: Record<string, unknown> }).paint?.[
        'background-color'
      ];
    expect(bg(dark)).not.toBe(bg(light));
  });
});

describe('buildBasemapStyle — absolute URL hardening for opaque origins (tuxlink-1tai)', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('resolves sprite to an absolute URL when the origin is opaque (tauri://localhost)', () => {
    // A custom scheme like tauri://localhost is an OPAQUE origin in the URL spec:
    // location.origin === 'null'. The #693 origin-concat fix would emit
    // 'null/basemap/sprites/light' in a packaged build — a URL maplibre rejects,
    // silently dropping icons. Resolving against location.href instead is correct.
    vi.stubGlobal('location', { href: 'tauri://localhost/index.html', origin: 'null' });
    const style = buildBasemapStyle('light');
    // Exact value proves it is absolute AND not the 'null/...' opaque-origin bug.
    expect(style.sprite).toBe('tauri://localhost/basemap/sprites/light');
  });

  it('keeps {fontstack}/{range} tokens LITERAL in the absolute glyphs URL', () => {
    // maplibre substitutes {fontstack}/{range} at fetch time, so they MUST stay
    // literal. A naive new URL() over the whole template percent-encodes the
    // braces (%7Bfontstack%7D) and breaks every font load — guard against it.
    vi.stubGlobal('location', { href: 'tauri://localhost/index.html', origin: 'null' });
    const style = buildBasemapStyle('light');
    expect(style.glyphs).toBe('tauri://localhost/basemap/glyphs/{fontstack}/{range}.pbf');
    expect(style.glyphs).not.toContain('%7B');
    expect(style.glyphs).toContain('{fontstack}');
  });

  it('produces a valid absolute http URL in a dev (tuple) origin', () => {
    vi.stubGlobal('location', { href: 'http://localhost:1420/', origin: 'http://localhost:1420' });
    const style = buildBasemapStyle('dark');
    expect(style.sprite).toBe('http://localhost:1420/basemap/sprites/dark');
    expect(style.glyphs).toBe('http://localhost:1420/basemap/glyphs/{fontstack}/{range}.pbf');
  });
});

describe('buildBasemapStyle — R7 region-pack compositing', () => {
  it('adds no extra sources/layers when no packs are installed', () => {
    const none = buildBasemapStyle('light', []);
    const base = buildBasemapStyle('light');
    expect(Object.keys(none.sources)).toEqual([BASEMAP_SOURCE_ID]);
    expect(none.layers.length).toBe(base.layers.length);
  });

  it('does NOT advertise an overview source maxzoom above the z0-6 archive max (D3, Codex P1)', () => {
    // Advertising maxzoom>6 makes MapLibre request z7+ tiles the archive lacks →
    // blank above z6. Let the PMTiles header (z6) govern; MapLibre overzooms it.
    const style = buildBasemapStyle('light');
    const src = style.sources[BASEMAP_SOURCE_ID] as { maxzoom?: number };
    expect(src.maxzoom === undefined || src.maxzoom <= REGION_MINZOOM).toBe(true);
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

  it('a pack contributes NO duplicate symbol/label layers — labels come from the base only (B1)', () => {
    const style = buildBasemapStyle('light', [{ id: 'continent-na' }]);
    const packLayers = style.layers.filter(
      (l): l is typeof l & { source: string } => 'source' in l && l.source === 'pack-continent-na',
    );
    expect(packLayers.length).toBeGreaterThan(0); // detail layers present
    expect(packLayers.every((l) => l.type !== 'symbol')).toBe(true); // but NO label layers
  });

  it('total label (symbol) layer count does NOT grow with pack count (B1)', () => {
    const symbolCount = (s: ReturnType<typeof buildBasemapStyle>) =>
      s.layers.filter((l) => l.type === 'symbol').length;
    const zero = symbolCount(buildBasemapStyle('light', []));
    const three = symbolCount(
      buildBasemapStyle('light', [{ id: 'a' }, { id: 'b' }, { id: 'c' }]),
    );
    expect(three).toBe(zero); // labels owned by the base overview only
  });

  it('contributes NO extra background/global layer per pack (regression: opaque canvas)', () => {
    // @protomaps/basemaps layers() emits an opaque, sourceless `background` layer.
    // If a pack's background were appended it would paint the whole canvas, hiding
    // the overview + overlays. There must be exactly ONE background (the overview's),
    // and every pack layer must be source-bound (so it only draws within the pack).
    const base = buildBasemapStyle('light');
    const baseBg = base.layers.filter((l) => l.type === 'background').length;
    const withPacks = buildBasemapStyle('light', [{ id: 'tier-wide-n34-w112' }, { id: 'continent-na' }]);
    const composedBg = withPacks.layers.filter((l) => l.type === 'background').length;
    expect(composedBg).toBe(baseBg); // packs add zero backgrounds
    const packLayers = withPacks.layers.filter((l) => l.id.startsWith('pack-'));
    expect(packLayers.length).toBeGreaterThan(0);
    expect(packLayers.every((l) => 'source' in l && Boolean((l as { source?: string }).source))).toBe(true);
  });
});
