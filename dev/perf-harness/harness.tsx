// Map perf harness (tuxlink-vnk7, T13 — diagnostic scratch, NOT shipped).
//
// Mounts a PRODUCTION-REPRESENTATIVE MapLibre scene — the real MapLibreMap with
// the bundled world overview, an installed region-pack source (real if the
// backend reports one, otherwise a shimmed pack id), ~50 station pins (the exact
// circle-layer paint StationFinderMap ships), and the Maidenhead grid — then runs
// a DETERMINISTIC pan/zoom script while sampling requestAnimationFrame deltas.
// The p50/p95 frame time + approximate fps are written to #perf-result for the
// Python driver to scrape, and to the console.
//
// This is the gate the front-end render-harness (dev/render-harness/) could never
// be: that harness uses canned Tauri data and a trivial scene (no real tile
// decode, no markers, no pack compositing), so its fps number is not an app-level
// prediction. See docs/pitfalls/testing-pitfalls.md MAP-PERF-1.
//
// Why this re-declares the pin layer instead of importing StationFinderMap:
// StationFinderMap pulls the reachability/voacapl prediction stack (a backend
// dependency that is irrelevant to render cost). The pin SOURCE + circle LAYER
// paint here is copied verbatim from StationFinderMap so the GPU/rasterizer work
// is identical; only the tier values are canned.
import React, { useEffect, useMemo } from 'react';
import { createRoot } from 'react-dom/client';
import type maplibregl from 'maplibre-gl';
import 'maplibre-gl/dist/maplibre-gl.css';
import '../../src/App.css';
import { MapLibreMap } from '../../src/map/MapLibreMap';
import { useMapContext } from '../../src/map/MapContext';
import { useMapOverlay } from '../../src/map/mapHooks';
import { MaidenheadGridLayer } from '../../src/map/MaidenheadGridLayer';

// ---------------------------------------------------------------------------
// Tauri-IPC shim. MapLibreMap calls invoke('basemap_list_packs') after mount to
// composite installed region packs over the world overview. The harness reports a
// shimmed pack id so the pack-compositing code path (a second source + its layers
// in buildBasemapStyle) is exercised even without a real installed pack. Override
// with ?pack=<id> or ?pack= (empty) for overview-only.
// ---------------------------------------------------------------------------
const params = new URLSearchParams(location.search);
const packId = params.has('pack') ? params.get('pack') : 'perf-region-pack';
const PACKS = packId ? { packs: [{ id: packId }] } : { packs: [] };
const RESPONSES: Record<string, unknown> = {
  basemap_list_packs: PACKS,
  config_read: { grid: 'CN87uo', review_inbound_before_download: false },
};
(window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
  invoke: (cmd: string) =>
    new Promise((resolve, reject) => {
      if (cmd in RESPONSES) setTimeout(() => resolve(RESPONSES[cmd]), 0);
      else reject(new Error(`perf-harness: no canned response for '${cmd}'`));
    }),
  transformCallback: (cb: unknown) => cb,
};

// ---------------------------------------------------------------------------
// ~50 canned station pins spread across a region (deterministic pseudo-grid so the
// circle layer paints a realistic marker count during pan/zoom).
// ---------------------------------------------------------------------------
const TIERS = ['good', 'fair', 'marginal', 'skip'] as const;
type Tier = (typeof TIERS)[number];
interface Pin { lon: number; lat: number; tier: Tier; selected: boolean }

function cannedPins(count: number): Pin[] {
  // Spread across the contiguous US-ish lon/lat box; mulberry-style deterministic.
  const pins: Pin[] = [];
  let seed = 1337;
  const rand = () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 0xffffffff;
  };
  for (let i = 0; i < count; i++) {
    pins.push({
      lon: -125 + rand() * 58, // -125 .. -67
      lat: 25 + rand() * 24, //  25 ..  49
      tier: TIERS[i % TIERS.length],
      selected: i === 0,
    });
  }
  return pins;
}

const STATIONS_SOURCE = 'perf-stations';
const STATION_PINS_LAYER = 'perf-station-pins';

// Circle layer paint copied verbatim from StationFinderMap.STATION_LAYERS so the
// per-pin fill cost the software rasterizer pays here matches production.
const STATION_LAYERS = (
  [
    {
      id: STATION_PINS_LAYER,
      type: 'circle',
      source: STATIONS_SOURCE,
      paint: {
        'circle-radius': [
          'case',
          ['boolean', ['feature-state', 'selected'], false],
          ['match', ['get', 'tier'], 'good', 12, 'fair', 10, 'marginal', 8.5, 'skip', 7, 9],
          ['match', ['get', 'tier'], 'good', 10, 'fair', 8, 'marginal', 6.5, 'skip', 5, 7],
        ],
        'circle-color': [
          'match',
          ['get', 'tier'],
          'good', '#46d07f',
          'fair', '#c9b23a',
          'marginal', '#d2842f',
          'skip', '#6c5a5a',
          '#9fb6cc',
        ],
        'circle-opacity': ['case', ['==', ['get', 'tier'], 'skip'], 0.75, 1],
        'circle-stroke-color': '#ffffff',
        'circle-stroke-width': ['case', ['get', 'selected'], 2.5, 0.5],
      },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

type FeatureCollection = { type: 'FeatureCollection'; features: unknown[] };
const EMPTY_FC: FeatureCollection = { type: 'FeatureCollection', features: [] };

function pinsToFC(pins: Pin[]): FeatureCollection {
  return {
    type: 'FeatureCollection',
    features: pins.map((p) => ({
      type: 'Feature',
      properties: { tier: p.tier, selected: p.selected },
      geometry: { type: 'Point', coordinates: [p.lon, p.lat] },
    })),
  };
}

function StationPins({ pins }: { pins: Pin[] }) {
  const map = useMapContext();
  const fc = useMemo(() => pinsToFC(pins), [pins]);
  useMapOverlay(map, STATIONS_SOURCE, { type: 'geojson', data: EMPTY_FC }, STATION_LAYERS);
  useEffect(() => {
    if (!map) return;
    const push = () => {
      const src = map.getSource(STATIONS_SOURCE) as { setData?: (d: unknown) => void } | undefined;
      src?.setData?.(fc);
    };
    push();
    map.on('styledata', push);
    return () => {
      map.off('styledata', push);
    };
  }, [map, fc]);
  return null;
}

// ---------------------------------------------------------------------------
// rAF frame-time sampler + deterministic pan/zoom driver.
//
// The driver runs entirely in-page: the Python driver only loads the URL, waits,
// and reads #perf-result. It does NOT need to drive the map over IPC (full
// WebKitGTK frame sampling via GObject is impractical), so the rAF deltas are
// sampled here, where rAF actually fires.
// ---------------------------------------------------------------------------
const PARAM = (k: string, d: number) => {
  const v = params.get(k);
  const n = v == null ? NaN : Number(v);
  return Number.isFinite(n) ? n : d;
};
const RUN_MS = PARAM('runMs', 12000); // total scripted-motion window
const WARMUP_MS = PARAM('warmupMs', 2500); // discard first frames (tile loads / style settle)

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return NaN;
  const idx = Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length));
  return sorted[idx];
}

function writeResult(state: string, payload: Record<string, unknown>) {
  const el = document.getElementById('perf-result');
  if (!el) return;
  el.setAttribute('data-state', state);
  el.setAttribute('data-result', JSON.stringify(payload));
  el.textContent =
    state === 'done'
      ? `p50=${payload.p50_ms}ms p95=${payload.p95_ms}ms ` +
        `~${payload.approx_fps}fps frames=${payload.frames} (${payload.note})`
      : `${state}: ${JSON.stringify(payload)}`;
  // eslint-disable-next-line no-console
  console.log('[perf-harness]', state, payload);
}

function runPerf(map: maplibregl.Map) {
  // Deterministic motion: alternating panBy nudges with periodic zoom steps.
  // panBy/zoomTo with a fixed duration drives the camera animation MapLibre uses
  // in production, so the render loop is exercised the same way a real drag/zoom
  // does. Timer-driven so the script is reproducible run to run.
  let stop = false;
  const start = performance.now();
  let last = start;
  const deltas: number[] = [];

  const sample = (now: number) => {
    if (stop) return;
    const dt = now - last;
    last = now;
    if (now - start >= WARMUP_MS) deltas.push(dt);
    if (now - start >= RUN_MS) {
      stop = true;
      finish();
      return;
    }
    requestAnimationFrame(sample);
  };

  let step = 0;
  const motion = window.setInterval(() => {
    if (stop) return;
    step++;
    const dx = step % 2 === 0 ? 220 : -220;
    const dy = step % 4 < 2 ? 120 : -120;
    map.panBy([dx, dy], { duration: 600 });
    if (step % 3 === 0) {
      const z = map.getZoom();
      map.zoomTo(z >= 6 ? 3 : z + 1.5, { duration: 600 });
    }
  }, 700);

  function finish() {
    window.clearInterval(motion);
    const sorted = deltas.slice().sort((a, b) => a - b);
    const p50 = percentile(sorted, 50);
    const p95 = percentile(sorted, 95);
    const round = (n: number) => Math.round(n * 10) / 10;
    writeResult('done', {
      p50_ms: round(p50),
      p95_ms: round(p95),
      approx_fps: p50 > 0 ? Math.round(1000 / p50) : 0,
      frames: deltas.length,
      run_ms: RUN_MS,
      warmup_ms: WARMUP_MS,
      pack: packId || '(none)',
      note: 'on-Pi software-GL scripted pan/zoom',
    });
  }

  requestAnimationFrame(sample);
}

// A small consumer that grabs the live map and kicks the perf run once it loads.
function PerfDriver() {
  const map = useMapContext();
  useEffect(() => {
    if (!map) return;
    // map is non-null only after MapLibreMap's 'load' fired (setMap on load), so
    // the style + first tiles are in flight. Start the run; WARMUP_MS discards the
    // initial settle frames.
    runPerf(map as unknown as maplibregl.Map);
  }, [map]);
  return null;
}

const PINS = cannedPins(50);

function App() {
  return (
    <MapLibreMap initialCenter={{ lat: 39, lon: -98 }} initialZoom={3}>
      <StationPins pins={PINS} />
      <MaidenheadGridLayer visible />
      <PerfDriver />
    </MapLibreMap>
  );
}

createRoot(document.getElementById('root')!).render(<App />);
