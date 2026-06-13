/**
 * Imperative MapLibre GL test double (tuxlink-ndi4, plan A14).
 *
 * Replaces the declarative react-leaflet double (`testMapMock.ts`): MapLibre is
 * driven imperatively (`map.addSource`/`addLayer`/`on`), not via JSX children, so
 * the double is a fake `maplibregl.Map` with spies backed by a queryable registry
 * (`__state.sources` / `__state.layers`) plus an event registry (`__emit`).
 *
 * SHAPE-ONLY, like its predecessor. jsdom has no WebGL or Web Workers, so this
 * verifies WIRING only — that hooks/components add the right sources/layers in
 * the right order and re-subscribe to `styledata`. Style-load ordering,
 * setStyle-re-add timing, and StrictMode/WebGL correctness are grim-only,
 * post-merge, fix-forward (A14): over-faking shipped 2 map bugs before (ku2b,
 * k61j). Do NOT assert projection/render correctness through this double.
 *
 * Installed GLOBALLY in `src/test-setup.ts` via `vi.mock('maplibre-gl', ...)`:
 * the real `maplibregl.Map` constructor touches WebGL on instantiate, so a
 * per-file mock is a footgun once App-level / PositionFormV2 transitively mount
 * a map. The global mock makes every `import maplibregl from 'maplibre-gl'`
 * resolve here.
 */
import { vi } from 'vitest';

export interface MapLibreMockState {
  /** Constructor options the component passed (container, center, zoom, style…). */
  options: Record<string, unknown>;
  /** Added sources by id. */
  sources: Map<string, unknown>;
  /** Added layers in insertion order. */
  layers: Array<{ id: string; spec: Record<string, unknown>; beforeId?: string }>;
  /** Event type → registered handler set. */
  handlers: Map<string, Set<(...args: unknown[]) => void>>;
  styleLoaded: boolean;
  zoom: number;
  /** Current viewport bounds (lng/lat extents) the overlays read via getBounds. */
  bounds: { west: number; south: number; east: number; north: number };
  removed: boolean;
}

/** What `getBounds()` returns — the LngLatBounds accessor subset overlays use. */
export interface MapLibreMockBounds {
  getWest(): number;
  getSouth(): number;
  getEast(): number;
  getNorth(): number;
}

/** The fake `maplibregl.Map` surface tuxlink touches, plus `__`-prefixed test controls. */
export interface MapLibreMock {
  readonly __state: MapLibreMockState;
  __emit(type: string, ...args: unknown[]): void;
  __setStyleLoaded(value: boolean): void;
  __setZoom(zoom: number): void;
  __setBounds(bounds: { west: number; south: number; east: number; north: number }): void;

  addSource: (id: string, source: Record<string, unknown>) => void;
  getSource: (id: string) => unknown | undefined;
  getBounds: () => MapLibreMockBounds;
  removeSource: (id: string) => void;
  addLayer: (spec: Record<string, unknown>, beforeId?: string) => void;
  getLayer: (id: string) => { id: string } | undefined;
  removeLayer: (id: string) => void;
  setStyle: (style: unknown) => void;
  isStyleLoaded: () => boolean;
  on: (type: string, handler: (...args: unknown[]) => void) => MapLibreMock;
  off: (type: string, handler: (...args: unknown[]) => void) => MapLibreMock;
  once: (type: string, handler: (...args: unknown[]) => void) => MapLibreMock;
  getZoom: () => number;
  setMaxZoom: (z: number) => void;
  setMinZoom: (z: number) => void;
  flyTo: (opts: unknown) => void;
  setCenter: (center: unknown) => void;
  setZoom: (z: number) => void;
  getCanvas: () => HTMLCanvasElement;
  addControl: (control: unknown, position?: string) => void;
  removeControl: (control: unknown) => void;
  remove: () => void;
}

/**
 * Build a queryable fake `maplibregl.Map`. `opts.styleLoaded` (default true) and
 * `opts.zoom` (default 1) seed the controllable flags; the rest are stored as
 * constructor options for assertion.
 */
export function createMapLibreMock(
  opts: Record<string, unknown> & { styleLoaded?: boolean; zoom?: number } = {},
): MapLibreMock {
  const { styleLoaded = true, zoom = 1, ...options } = opts;

  const state: MapLibreMockState = {
    options,
    sources: new Map(),
    layers: [],
    handlers: new Map(),
    styleLoaded,
    zoom,
    bounds: { west: -180, south: -85, east: 180, north: 85 },
    removed: false,
  };

  const mock: MapLibreMock = {
    __state: state,
    __emit(type, ...args) {
      const set = state.handlers.get(type);
      if (set) for (const h of [...set]) h(...args);
    },
    __setStyleLoaded(value) {
      state.styleLoaded = value;
    },
    __setZoom(z) {
      state.zoom = z;
    },
    __setBounds(b) {
      state.bounds = b;
    },

    addSource: vi.fn((id: string, source: Record<string, unknown>) => {
      // Store a GeoJSONSource-like handle so consumers can call setData (the
      // dynamic-data path overlays use on pan/zoom).
      const handle: Record<string, unknown> = {
        ...source,
        setData: vi.fn((data: unknown) => {
          handle.data = data;
        }),
      };
      state.sources.set(id, handle);
    }),
    getSource: vi.fn((id: string) => state.sources.get(id)),
    getBounds: vi.fn(() => ({
      getWest: () => state.bounds.west,
      getSouth: () => state.bounds.south,
      getEast: () => state.bounds.east,
      getNorth: () => state.bounds.north,
    })),
    removeSource: vi.fn((id: string) => {
      state.sources.delete(id);
    }),
    addLayer: vi.fn((spec: Record<string, unknown>, beforeId?: string) => {
      const id = String(spec.id);
      const entry = { id, spec, beforeId };
      if (beforeId) {
        const idx = state.layers.findIndex((l) => l.id === beforeId);
        if (idx >= 0) {
          state.layers.splice(idx, 0, entry);
          return;
        }
      }
      state.layers.push(entry);
    }),
    getLayer: vi.fn((id: string) => {
      const found = state.layers.find((l) => l.id === id);
      return found ? { id: found.id } : undefined;
    }),
    removeLayer: vi.fn((id: string) => {
      state.layers = state.layers.filter((l) => l.id !== id);
    }),
    setStyle: vi.fn((_style: unknown) => {
      // Real MapLibre drops all sources/layers when the style is replaced; the
      // owned hooks must re-add them on the subsequent `styledata`. Do NOT
      // auto-emit — tests drive `__emit('styledata')` for determinism.
      state.sources.clear();
      state.layers = [];
    }),
    isStyleLoaded: vi.fn(() => state.styleLoaded),
    on: vi.fn((type: string, handler: (...args: unknown[]) => void) => {
      if (!state.handlers.has(type)) state.handlers.set(type, new Set());
      state.handlers.get(type)!.add(handler);
      return mock;
    }),
    off: vi.fn((type: string, handler: (...args: unknown[]) => void) => {
      state.handlers.get(type)?.delete(handler);
      return mock;
    }),
    once: vi.fn((type: string, handler: (...args: unknown[]) => void) => {
      const wrapped = (...args: unknown[]) => {
        mock.off(type, wrapped);
        handler(...args);
      };
      return mock.on(type, wrapped);
    }),
    getZoom: vi.fn(() => state.zoom),
    setMaxZoom: vi.fn(),
    setMinZoom: vi.fn(),
    flyTo: vi.fn(),
    setCenter: vi.fn(),
    setZoom: vi.fn((z: number) => {
      state.zoom = z;
    }),
    getCanvas: vi.fn(() => document.createElement('canvas')),
    addControl: vi.fn(),
    removeControl: vi.fn(),
    remove: vi.fn(() => {
      state.removed = true;
      state.handlers.clear();
    }),
  };

  return mock;
}

// ── Global-install plumbing ────────────────────────────────────────────────
// The module mock (test-setup.ts) and the test file both import THIS module, so
// `constructedMaps` is shared state across the `vi.mock` factory and assertions.

/** Every `new maplibregl.Map(...)` instance, in construction order. */
export const constructedMaps: MapLibreMock[] = [];

/** Reset the construction registry between tests (called from test-setup.ts). */
export function resetMapLibreMock(): void {
  constructedMaps.length = 0;
}

/** The most-recently constructed map (the one the component under test created). */
export function getLastMap(): MapLibreMock | undefined {
  return constructedMaps.at(-1);
}

/** A fake `maplibregl.Marker` — chainable, with a real DOM element. */
function createMarkerMock(): Record<string, unknown> {
  const element = document.createElement('div');
  const marker: Record<string, unknown> = {
    setLngLat: vi.fn(() => marker),
    addTo: vi.fn(() => marker),
    remove: vi.fn(() => marker),
    getElement: vi.fn(() => element),
    setPopup: vi.fn(() => marker),
    getLngLat: vi.fn(() => ({ lng: 0, lat: 0 })),
  };
  return marker;
}

/**
 * Build the object `vi.mock('maplibre-gl', ...)` returns: a default export with
 * the `Map` / `Marker` / control constructors + `addProtocol`, plus matching
 * named exports (tuxlink imports the default; named are provided for safety).
 */
export function makeMapLibreModuleMock(): Record<string, unknown> {
  const MapConstructor = function (
    this: unknown,
    options: Record<string, unknown> = {},
  ): MapLibreMock {
    const instance = createMapLibreMock(options);
    constructedMaps.push(instance);
    return instance;
  } as unknown as new (options?: Record<string, unknown>) => MapLibreMock;

  const MarkerConstructor = function (this: unknown): Record<string, unknown> {
    return createMarkerMock();
  } as unknown as new () => Record<string, unknown>;

  class ControlMock {
    onAdd = vi.fn(() => document.createElement('div'));
    onRemove = vi.fn();
  }

  const api = {
    Map: MapConstructor,
    Marker: MarkerConstructor,
    NavigationControl: ControlMock,
    AttributionControl: ControlMock,
    ScaleControl: ControlMock,
    addProtocol: vi.fn(),
    removeProtocol: vi.fn(),
  };

  return { ...api, default: api };
}
