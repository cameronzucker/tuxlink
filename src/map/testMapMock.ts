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

const fakeMap = {
  on(type: string, handler: EventHandler): void {
    handlerRegistry.set(type, handler);
  },
  off(type: string): void {
    handlerRegistry.delete(type);
  },
  dragging: { disable: vi.fn(), enable: vi.fn() },
  mouseEventToLatLng(pt: { clientX: number; clientY: number }): LatLngLiteral {
    // identity-ish — proves wiring, NOT projection (C1).
    return { lat: pt.clientY, lng: pt.clientX };
  },
};

/** The fake Leaflet map instance returned by `useMap()`/`useMapEvents()`. */
export function getMockMap(): typeof fakeMap {
  return fakeMap;
}

/** Clear registered handlers and reset the dragging spies. Call in `beforeEach`. */
export function resetMapMock(): void {
  handlerRegistry.clear();
  fakeMap.dragging.disable.mockClear();
  fakeMap.dragging.enable.mockClear();
}

/** Fire a map event (`click`/`mousedown`/`mousemove`/`mouseup`) to the registered handler. */
export function fireMapEvent(type: string, latlng: LatLngLiteral): void {
  const handler = handlerRegistry.get(type);
  if (handler) handler({ latlng });
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
  return { MapContainer, ImageOverlay, Marker, Rectangle, Polyline, Tooltip, useMap, useMapEvents };
}

/** leaflet (`L`) module replacement — provides only what the map subsystem touches. */
export function createLeafletMock(): { default: Record<string, unknown> } {
  return {
    default: {
      CRS: { EPSG4326: { code: 'EPSG:4326' } },
      Icon: {
        Default: {
          prototype: {} as Record<string, unknown>,
          mergeOptions: vi.fn(),
        },
      },
    },
  };
}
