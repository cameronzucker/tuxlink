/**
 * GridPicker — the MapLibre re-expression of GridMapPicker (tuxlink-ndi4, phase
 * 2). Offline location picker with two modes:
 *   - 'pin': click drops a point; reports the 4-char Maidenhead grid (broadcast
 *     default). Renders a center dot + grid-square highlight.
 *   - 'box': drag a rectangle; reports the two signed lat/lon corners.
 *
 * Composes MapLibreMap + MaidenheadGridLayer. The drag-select (finding 8 — the
 * historically bug-prone half) is re-expressed on raw map events: `dragPan` is
 * disabled while drawing, a window-level mouseup aborts a drag whose pointer was
 * released off-canvas, and the click MapLibre fires after a drag is suppressed.
 * The pin marker + selection rectangles are GeoJSON layers (NOT maplibregl.Marker
 * — circle/fill/line layers are CSP-safe and avoid the A13 packaged-marker risk).
 *
 * Real interaction smoothness / rubber-band render is grim-verified (C1); the
 * tests prove wiring only.
 */
import { useEffect, useMemo, useRef, useState } from 'react';
import type { MapMouseEvent } from 'maplibre-gl';
import { MapLibreMap } from './MapLibreMap';
import { MaidenheadGridLayer } from './MaidenheadGridLayer';
import { useMapContext } from './MapContext';
import { useMapOverlay } from './mapHooks';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';
import type { LatLon } from './projection';

export interface GridPickerProps {
  mode: 'pin' | 'box';
  /** Current grid (pin-mode dot + grid-square highlight). */
  grid?: string;
  /** Pin mode: called with the 4-char grid for the clicked point. */
  onGridChange?: (grid: string) => void;
  /** Box mode: called with the two signed lat/lon drag corners. */
  onBoxChange?: (a: LatLon, b: LatLon) => void;
  /** Show the Maidenhead lattice overlay (default on). */
  gridOverlay?: boolean;
}

type Corners = [LatLon, LatLon];
/** [[south,west],[north,east]] in lat/lon, as the pure helpers produce. */
type LatLonBBox = [[number, number], [number, number]];

const RECT_HALF = { lat6: 1.25 / 60, lon6: 2.5 / 60, lat4: 0.5, lon4: 1.0 };

const SELECTION_SOURCE_ID = 'grid-selection';

function rectFromCorners(a: LatLon, b: LatLon): LatLonBBox {
  return [
    [Math.min(a.lat, b.lat), Math.min(a.lon, b.lon)],
    [Math.max(a.lat, b.lat), Math.max(a.lon, b.lon)],
  ];
}

function gridSquareBounds(grid: string, ll: LatLon): LatLonBBox {
  const is6 = grid.toUpperCase().length === 6;
  const halfLat = is6 ? RECT_HALF.lat6 : RECT_HALF.lat4;
  const halfLon = is6 ? RECT_HALF.lon6 : RECT_HALF.lon4;
  return [
    [ll.lat - halfLat, ll.lon - halfLon],
    [ll.lat + halfLat, ll.lon + halfLon],
  ];
}

type FeatureCollection = { type: 'FeatureCollection'; features: unknown[] };
const EMPTY_FC: FeatureCollection = { type: 'FeatureCollection', features: [] };

/** A closed polygon ring (GeoJSON [lng,lat]) for a lat/lon bbox. */
function polygonFeature(bbox: LatLonBBox, kind: string): unknown {
  const [[s, w], [n, e]] = bbox;
  return {
    type: 'Feature',
    properties: { kind },
    geometry: {
      type: 'Polygon',
      coordinates: [[[w, s], [e, s], [e, n], [w, n], [w, s]]],
    },
  };
}

function buildSelectionFC(mode: 'pin' | 'box', grid: string | undefined, ll: LatLon | null, temp: Corners | null): FeatureCollection {
  const features: unknown[] = [];
  if (mode === 'pin' && grid && ll) {
    features.push(polygonFeature(gridSquareBounds(grid, ll), 'pin-square'));
    features.push({
      type: 'Feature',
      properties: { kind: 'pin' },
      geometry: { type: 'Point', coordinates: [ll.lon, ll.lat] },
    });
  }
  if (temp) {
    features.push(polygonFeature(rectFromCorners(temp[0], temp[1]), 'temp'));
  }
  return { type: 'FeatureCollection', features };
}

const SELECTION_LAYERS = (
  [
    {
      id: 'sel-pin-fill',
      type: 'fill',
      source: SELECTION_SOURCE_ID,
      filter: ['==', ['get', 'kind'], 'pin-square'],
      paint: { 'fill-color': '#2563eb', 'fill-opacity': 0.08 },
    },
    {
      id: 'sel-pin-line',
      type: 'line',
      source: SELECTION_SOURCE_ID,
      filter: ['==', ['get', 'kind'], 'pin-square'],
      paint: { 'line-color': '#2563eb', 'line-width': 2 },
    },
    {
      id: 'sel-temp-fill',
      type: 'fill',
      source: SELECTION_SOURCE_ID,
      filter: ['==', ['get', 'kind'], 'temp'],
      paint: { 'fill-color': '#dc2626', 'fill-opacity': 0.1 },
    },
    {
      id: 'sel-temp-line',
      type: 'line',
      source: SELECTION_SOURCE_ID,
      filter: ['==', ['get', 'kind'], 'temp'],
      paint: { 'line-color': '#dc2626', 'line-width': 2, 'line-dasharray': [2, 1] },
    },
    {
      id: 'sel-pin-dot',
      type: 'circle',
      source: SELECTION_SOURCE_ID,
      filter: ['==', ['get', 'kind'], 'pin'],
      paint: { 'circle-radius': 6, 'circle-color': '#2563eb', 'circle-stroke-color': '#ffffff', 'circle-stroke-width': 2 },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

interface PickerHandlers {
  mode: 'pin' | 'box';
  onGridChange?: (grid: string) => void;
  onBoxChange?: (a: LatLon, b: LatLon) => void;
  onTemp: (corners: Corners | null) => void;
}

/** Wire the drag-select / pin-click gesture onto the live map (finding 8). */
function usePickerInteractions(map: ReturnType<typeof useMapContext>, handlers: PickerHandlers) {
  const ref = useRef(handlers);
  ref.current = handlers;
  const startRef = useRef<LatLon | null>(null);
  const draggedRef = useRef(false);

  useEffect(() => {
    if (!map) return;
    // Pointer released OFF-canvas → the map 'mouseup' never fires; a window-level
    // mouseup aborts the drag (re-enable pan, clear preview), no onBoxChange. An
    // on-map release clears startRef first, so this then no-ops.
    const onWindowUp = () => {
      if (startRef.current) {
        startRef.current = null;
        map.dragPan.enable();
        ref.current.onTemp(null);
      }
    };
    window.addEventListener('mouseup', onWindowUp);

    const onDown = (e: MapMouseEvent) => {
      if (ref.current.mode !== 'box') return;
      map.dragPan.disable(); // don't pan while drawing the box
      startRef.current = { lat: e.lngLat.lat, lon: e.lngLat.lng };
    };
    const onMove = (e: MapMouseEvent) => {
      if (ref.current.mode !== 'box' || !startRef.current) return;
      ref.current.onTemp([startRef.current, { lat: e.lngLat.lat, lon: e.lngLat.lng }]);
    };
    const onUp = (e: MapMouseEvent) => {
      if (ref.current.mode !== 'box' || !startRef.current) return;
      const start = startRef.current;
      const end: LatLon = { lat: e.lngLat.lat, lon: e.lngLat.lng };
      startRef.current = null;
      draggedRef.current = true; // suppress the click that follows a drag
      map.dragPan.enable();
      ref.current.onTemp(null);
      ref.current.onBoxChange?.(start, end);
    };
    const onClick = (e: MapMouseEvent) => {
      if (draggedRef.current) {
        draggedRef.current = false;
        return;
      }
      if (ref.current.mode !== 'pin') return;
      ref.current.onGridChange?.(latLonToGrid(e.lngLat.lat, e.lngLat.lng).slice(0, 4));
    };

    map.on('mousedown', onDown);
    map.on('mousemove', onMove);
    map.on('mouseup', onUp);
    map.on('click', onClick);
    // Native shift-drag box-zoom conflicts with the drag-select gesture.
    map.boxZoom.disable();

    return () => {
      window.removeEventListener('mouseup', onWindowUp);
      map.off('mousedown', onDown);
      map.off('mousemove', onMove);
      map.off('mouseup', onUp);
      map.off('click', onClick);
    };
  }, [map]);
}

function PickerBody({ mode, grid, onGridChange, onBoxChange }: Omit<GridPickerProps, 'gridOverlay'>) {
  const map = useMapContext();
  const [temp, setTemp] = useState<Corners | null>(null);

  usePickerInteractions(map, { mode, onGridChange, onBoxChange, onTemp: setTemp });

  const ll = grid ? gridToLatLon(grid) : null;
  const fc = useMemo(
    () => buildSelectionFC(mode, grid, ll, temp),
    [mode, grid, ll?.lat, ll?.lon, temp],
  );

  useMapOverlay(map, SELECTION_SOURCE_ID, { type: 'geojson', data: EMPTY_FC }, SELECTION_LAYERS);
  useEffect(() => {
    if (!map) return;
    const push = () => {
      const src = map.getSource(SELECTION_SOURCE_ID) as { setData?: (d: unknown) => void } | undefined;
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

export function GridPicker({ mode, grid, onGridChange, onBoxChange, gridOverlay = true }: GridPickerProps) {
  const ll = grid ? gridToLatLon(grid) : null;
  return (
    <MapLibreMap initialCenter={ll ?? undefined} initialZoom={ll ? 6 : 2}>
      {gridOverlay && <MaidenheadGridLayer visible />}
      <PickerBody mode={mode} grid={grid} onGridChange={onGridChange} onBoxChange={onBoxChange} />
    </MapLibreMap>
  );
}
