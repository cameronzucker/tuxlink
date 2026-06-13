/**
 * PositionMapWidget — offline location picker for PositionFormV2's grid override.
 *
 * Controlled component: the caller owns `grid` state; a map click fires
 * `onGridChange` with the new 6-char Maidenhead locator. MapLibre re-expression
 * (tuxlink-ndi4 phase 2): renders on MapLibreMap (bundled offline vector world,
 * no network). The current-grid marker + grid-square highlight are GeoJSON
 * circle/fill/line layers (CSP-safe, no maplibregl.Marker). `onZoomChange` is
 * forwarded so PositionPickerOverlay can gate 6-char precision on the live zoom.
 */
import { useEffect, useMemo } from 'react';
import { MapLibreMap } from '../map/MapLibreMap';
import { useMapContext } from '../map/MapContext';
import { useMapOverlay } from '../map/mapHooks';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';
import type { LatLon } from '../map/projection';

export interface PositionMapWidgetProps {
  /** Current 6-char (or 4-char) Maidenhead grid — controlled by the parent. */
  grid: string;
  /** Called when the operator clicks the map, with the new 6-char grid. */
  onGridChange: (newGrid: string) => void;
  /** Optional zoom-change bridge (forwarded from MapLibreMap; fires on load + view change). */
  onZoomChange?: (zoom: number) => void;
}

// Grid-square half-widths (degrees): 6-char subsquare vs 4-char square.
const HALF_LON_6 = 2.5 / 60;
const HALF_LAT_6 = 1.25 / 60;
const HALF_LON_4 = 1.0;
const HALF_LAT_4 = 0.5;

const POSITION_SOURCE = 'position';
type FeatureCollection = { type: 'FeatureCollection'; features: unknown[] };
const EMPTY_FC: FeatureCollection = { type: 'FeatureCollection', features: [] };

const POSITION_LAYERS = (
  [
    {
      id: 'pos-square-fill',
      type: 'fill',
      source: POSITION_SOURCE,
      filter: ['==', ['get', 'kind'], 'square'],
      paint: { 'fill-color': '#2563eb', 'fill-opacity': 0.08 },
    },
    {
      id: 'pos-square-line',
      type: 'line',
      source: POSITION_SOURCE,
      filter: ['==', ['get', 'kind'], 'square'],
      paint: { 'line-color': '#2563eb', 'line-width': 2 },
    },
    {
      id: 'pos-dot',
      type: 'circle',
      source: POSITION_SOURCE,
      filter: ['==', ['get', 'kind'], 'pin'],
      paint: { 'circle-radius': 6, 'circle-color': '#2563eb', 'circle-stroke-color': '#ffffff', 'circle-stroke-width': 2 },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

function buildPositionFC(grid: string, ll: LatLon | null): FeatureCollection {
  if (!ll) return EMPTY_FC;
  const is6 = grid.toUpperCase().length === 6;
  const halfLat = is6 ? HALF_LAT_6 : HALF_LAT_4;
  const halfLon = is6 ? HALF_LON_6 : HALF_LON_4;
  const w = ll.lon - halfLon;
  const e = ll.lon + halfLon;
  const s = ll.lat - halfLat;
  const n = ll.lat + halfLat;
  return {
    type: 'FeatureCollection',
    features: [
      {
        type: 'Feature',
        properties: { kind: 'square' },
        geometry: { type: 'Polygon', coordinates: [[[w, s], [e, s], [e, n], [w, n], [w, s]]] },
      },
      {
        type: 'Feature',
        properties: { kind: 'pin' },
        geometry: { type: 'Point', coordinates: [ll.lon, ll.lat] },
      },
    ],
  };
}

function PositionMarker({ grid }: { grid: string }) {
  const map = useMapContext();
  const ll = gridToLatLon(grid);
  const fc = useMemo(() => buildPositionFC(grid, ll), [grid, ll?.lat, ll?.lon]);

  useMapOverlay(map, POSITION_SOURCE, { type: 'geojson', data: EMPTY_FC }, POSITION_LAYERS);
  useEffect(() => {
    if (!map) return;
    const push = () => {
      const src = map.getSource(POSITION_SOURCE) as { setData?: (d: unknown) => void } | undefined;
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

export function PositionMapWidget({ grid, onGridChange, onZoomChange }: PositionMapWidgetProps) {
  const ll = gridToLatLon(grid);
  return (
    <MapLibreMap
      onMapClick={({ lat, lon }) => {
        // Full 6-char locator — this widget's per-message position-report contract.
        onGridChange(latLonToGrid(lat, lon));
      }}
      initialCenter={ll ?? undefined}
      initialZoom={ll ? 6 : 2}
      onZoomChange={onZoomChange}
    >
      <PositionMarker grid={grid} />
    </MapLibreMap>
  );
}
