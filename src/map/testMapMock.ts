/**
 * Canonical react-leaflet + leaflet test doubles for the map subsystem.
 *
 * SHAPE-ONLY. jsdom cannot render Leaflet, so these doubles prove *wiring and
 * structure*, never projection/render correctness (C1). Real
 * projection / click / box-drag / layout is verified ONLY via grim on
 * WebKitGTK. Do NOT assert projection arithmetic through this mock — that is
 * what `projection.test.ts` / `gribRegion.test.ts` (pure math) are for.
 *
 * Every react-leaflet component renders as a `<div>` that mirrors its props
 * onto `data-*` attributes (so `crs` / `maxBounds` / `boxZoom` are assertable
 * and no unknown-prop React warning leaks — testing-pitfalls §1). Map events
 * are routed through a module-level registry driven by `fireMapEvent`.
 *
 * Usage in a test file:
 *
 *   import { createReactLeafletMock, createLeafletMock, fireMapEvent, resetMapMock, getMockMap } from '../map/testMapMock';
 *   vi.mock('react-leaflet', async () => (await import('../map/testMapMock')).createReactLeafletMock());
 *   vi.mock('leaflet',       async () => (await import('../map/testMapMock')).createLeafletMock());
 *   beforeEach(() => resetMapMock());
 */
import * as React from 'react';
import { vi } from 'vitest';

interface LatLngLiteral {
  lat: number;
  lng: number;
}
type EventHandler = (e: { latlng: LatLngLiteral }) => void;

// ── module-level shared state (same instance for the vi.mock factory and the
//    test file, since both import this module) ────────────────────────────────
const handlerRegistry = new Map<string, EventHandler>();

// Overridable current zoom. Defaults to 1 (the historical canonical value that
// keeps MaidenheadOverlay's self-drive test at Field level). Zoom-sensitive
// tests (7.3/7.4) set this via `setMockZoom(n)` and `resetMapMock()` restores 1.
let mockZoom = 1;

/** Override the fake map's `getZoom()`; reset to 1 by `resetMapMock()`. */
export function setMockZoom(zoom: number): void {
  mockZoom = zoom;
}

const fakeMap = {
  on(type: string, handler: EventHandler): void {
    handlerRegistry.set(type, handler);
  },
  off(type: string): void {
    handlerRegistry.delete(type);
  },
  dragging: { disable: vi.fn(), enable: vi.fn() },
  // Imperative recenter used by consumers that center on async-loaded state
  // (e.g. StationFinderMap recentering on the operator grid once config_read
  // resolves). Shape-only spy: asserts the call + args, NOT real projection (C1).
  setView: vi.fn(),
  // Imperative max-zoom setter. react-leaflet's <MapContainer maxZoom> is read
  // ONCE at mount (init-only, like center/zoom), so a tile source that arrives
  // ASYNC after mount must raise the cap through this imperative call — not the
  // prop. Shape-only spy: asserts the wiring fires with the right value (C1).
  setMaxZoom: vi.fn(),
  mouseEventToLatLng(pt: { clientX: number; clientY: number }): LatLngLiteral {
    // identity-ish — proves wiring, NOT projection (C1).
    return { lat: pt.clientY, lng: pt.clientX };
  },
  // Fixed world view so map-driven components (MaidenheadOverlay self-drive)
  // render deterministically in jsdom. Real bounds/zoom are grim-verified.
  // Zoom is overridable via setMockZoom() for zoom-gating tests.
  getZoom(): number {
    return mockZoom;
  },
  getBounds() {
    return {
      getSouth: () => -90,
      getWest: () => -180,
      getNorth: () => 90,
      getEast: () => 180,
    };
  },
};

/** The fake Leaflet map instance returned by `useMap()`/`useMapEvents()`. */
export function getMockMap(): typeof fakeMap {
  return fakeMap;
}

/** Clear registered handlers, reset the dragging spies, and reset zoom to 1. */
export function resetMapMock(): void {
  handlerRegistry.clear();
  fakeMap.dragging.disable.mockClear();
  fakeMap.dragging.enable.mockClear();
  fakeMap.setView.mockClear();
  fakeMap.setMaxZoom.mockClear();
  mockZoom = 1;
}

/** Fire a map event (`click`/`mousedown`/`mousemove`/`mouseup`) to the registered handler. */
export function fireMapEvent(type: string, latlng: LatLngLiteral): void {
  const handler = handlerRegistry.get(type);
  if (handler) handler({ latlng });
}

/**
 * Fire a synthetic `zoomend` event to the registered handler.
 *
 * The `zoomend` handler shape differs from click — `e.target.getZoom()` is the
 * value rather than `e.latlng`. This seam fires with `{ target: fakeMap }` so
 * `MapZoomHandler`'s `e.target.getZoom()` call returns the current `mockZoom`
 * (overridable via `setMockZoom(n)` before calling this function).
 */
export function fireZoomEvent(): void {
  type ZoomHandler = (e: { target: typeof fakeMap }) => void;
  const handler = handlerRegistry.get('zoomend') as ZoomHandler | undefined;
  if (handler) handler({ target: fakeMap });
}

/** Mirror props onto `data-*` (skips children + function props to avoid React warnings). */
function dataProps(props: Record<string, unknown>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(props)) {
    if (key === 'children' || typeof value === 'function') continue;
    out['data-' + key.toLowerCase()] =
      value !== null && typeof value === 'object' ? JSON.stringify(value) : String(value);
  }
  return out;
}

function leafDiv(testid: string, props: Record<string, unknown>): React.ReactElement {
  return React.createElement(
    'div',
    { 'data-testid': testid, ...dataProps(props) },
    props.children as React.ReactNode,
  );
}

/** react-leaflet module replacement. */
export function createReactLeafletMock(): Record<string, unknown> {
  function MapContainer(props: Record<string, unknown>): React.ReactElement {
    return React.createElement(
      'div',
      { 'data-testid': 'leaflet-map', ...dataProps(props) },
      props.children as React.ReactNode,
    );
  }
  function ImageOverlay(props: Record<string, unknown>): React.ReactElement {
    return leafDiv('image-overlay', props);
  }
  // A Leaflet pane wrapper — in real react-leaflet it creates a z-index stacking
  // context for its children. SHAPE-ONLY here: renders a div carrying the pane
  // name/style and its children, so a test can assert children are wrapped in the
  // right pane; jsdom cannot represent the real z-index stacking (grim-verified,
  // C1) — which is exactly why the raster-over-tiles occlusion went uncaught.
  function Pane(props: Record<string, unknown>): React.ReactElement {
    return leafDiv('leaflet-pane', props);
  }
  function TileLayer(props: Record<string, unknown>): React.ReactElement {
    return leafDiv('leaflet-tilelayer', props);
  }
  function Marker(props: Record<string, unknown>): React.ReactElement {
    return leafDiv('leaflet-marker', props);
  }
  function Rectangle(props: Record<string, unknown>): React.ReactElement {
    return leafDiv('leaflet-rectangle', props);
  }
  function Polyline(props: Record<string, unknown>): React.ReactElement {
    return leafDiv('leaflet-polyline', props);
  }
  function Tooltip(props: Record<string, unknown>): React.ReactElement {
    return leafDiv('leaflet-tooltip', props);
  }
  function useMap(): typeof fakeMap {
    return fakeMap;
  }
  function useMapEvents(handlers: Record<string, EventHandler>): typeof fakeMap {
    for (const [type, handler] of Object.entries(handlers)) handlerRegistry.set(type, handler);
    return fakeMap;
  }
  return {
    MapContainer,
    ImageOverlay,
    Pane,
    TileLayer,
    Marker,
    Rectangle,
    Polyline,
    Tooltip,
    useMap,
    useMapEvents,
  };
}

/** leaflet (`L`) module replacement — provides only what the map subsystem touches. */
export function createLeafletMock(): { default: Record<string, unknown> } {
  return {
    default: {
      CRS: { EPSG4326: { code: 'EPSG:4326' }, EPSG3857: { code: 'EPSG:3857' } },
      divIcon: (opts: unknown) => opts,
      Icon: {
        Default: {
          prototype: {} as Record<string, unknown>,
          mergeOptions: vi.fn(),
        },
      },
    },
  };
}
