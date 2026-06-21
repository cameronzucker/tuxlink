/**
 * PositionMapWidget — offline location picker for PositionFormV2's grid override.
 *
 * Controlled component: the caller owns `grid` state; a map click fires
 * `onGridChange` with the new 6-char Maidenhead locator. Leaflet re-expression
 * (tuxlink-kkd3; strangler-fig twin of the MapLibre edition): renders on the
 * shared LeafletMap (bundled offline vector world, no network). The current-grid
 * marker (an `L.circleMarker`) + grid-square highlight (an `L.rectangle`) are
 * vector overlays on an explicit `L.svg()` renderer — DOM-rendered, robust under
 * the Pi's software-GL WebKitGTK, and jsdom-inspectable. `onZoomChange` is
 * forwarded so PositionPickerOverlay can gate 6-char precision on the live zoom.
 */
import { useEffect, useRef } from 'react';
import L from 'leaflet';
import { LeafletMap } from '../map/LeafletMap';
import { useLeafletMap } from '../map/LeafletMapContext';
import { useLeafletLayerGroup } from '../map/leafletHooks';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';
import { reportFrontendError } from '../frontendErrorLog';
import type { LatLon } from '../map/projection';

export interface PositionMapWidgetProps {
  /** Current 6-char (or 4-char) Maidenhead grid — controlled by the parent. */
  grid: string;
  /** Called when the operator clicks the map, with the new 6-char grid. */
  onGridChange: (newGrid: string) => void;
  /** Optional zoom-change bridge (forwarded from LeafletMap; fires on ready + view change). */
  onZoomChange?: (zoom: number) => void;
}

// Grid-square half-widths (degrees): 6-char subsquare vs 4-char square.
const HALF_LON_6 = 2.5 / 60;
const HALF_LAT_6 = 1.25 / 60;
const HALF_LON_4 = 1.0;
const HALF_LAT_4 = 0.5;

/** The grid-square bounds [[south,west],[north,east]] for a grid + its centre. */
function squareBounds(grid: string, ll: LatLon): L.LatLngBoundsExpression {
  const is6 = grid.toUpperCase().length === 6;
  const halfLat = is6 ? HALF_LAT_6 : HALF_LAT_4;
  const halfLon = is6 ? HALF_LON_6 : HALF_LON_4;
  return [
    [ll.lat - halfLat, ll.lon - halfLon],
    [ll.lat + halfLat, ll.lon + halfLon],
  ];
}

function PositionMarker({ grid }: { grid: string }) {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  const rendererRef = useRef<L.Renderer | null>(null);
  if (!rendererRef.current) rendererRef.current = L.svg({ padding: 2 });

  useEffect(() => {
    if (!map || !group) return;
    const ll = gridToLatLon(grid);
    // Guard Leaflet mutations: a transient throw (mid zoom/pan) is logged + skipped,
    // never crashed to the app ErrorBoundary.
    try {
      group.clearLayers();
      if (!ll) return; // invalid grid → no pin or square
      // Square drawn first so the dot sits above it.
      const square = L.rectangle(squareBounds(grid, ll), {
        renderer: rendererRef.current ?? undefined,
        color: '#2563eb',
        weight: 2,
        fillColor: '#2563eb',
        fillOpacity: 0.08,
      });
      const dot = L.circleMarker([ll.lat, ll.lon], {
        renderer: rendererRef.current ?? undefined,
        radius: 6,
        color: '#ffffff',
        weight: 2,
        fillColor: '#2563eb',
        fillOpacity: 1,
        interactive: false,
      });
      group.addLayer(square);
      group.addLayer(dot);
    } catch (e) {
      reportFrontendError(
        'position-map-widget',
        `marker reconcile: ${e instanceof Error ? e.message : String(e)}`,
        e instanceof Error ? e.stack : undefined,
      );
    }
  }, [map, group, grid]);

  return null;
}

export function PositionMapWidget({ grid, onGridChange, onZoomChange }: PositionMapWidgetProps) {
  const ll = gridToLatLon(grid);
  return (
    <LeafletMap
      onMapClick={({ lat, lon }) => {
        // Full 6-char locator — this widget's per-message position-report contract.
        onGridChange(latLonToGrid(lat, lon));
      }}
      initialCenter={ll ?? undefined}
      initialZoom={ll ? 6 : 2}
      onZoomChange={onZoomChange}
    >
      <PositionMarker grid={grid} />
    </LeafletMap>
  );
}
