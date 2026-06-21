/**
 * GridPicker — the Leaflet re-expression of GridMapPicker (tuxlink-rqvk;
 * strangler-fig twin). Offline location picker with two modes:
 *   - 'pin': click drops a point; reports the 4-char Maidenhead grid (broadcast
 *     default). Renders a center dot + grid-square highlight.
 *   - 'box': drag a rectangle; reports the two signed lat/lon corners.
 *
 * Composes LeafletMap + LeafletMaidenheadGridLayer. The drag-select (the
 * historically bug-prone half) is re-expressed on the live map's raw events:
 * `map.dragging` is disabled while drawing, a window-level mouseup aborts a drag
 * whose pointer was released off-canvas, and the click Leaflet fires after a drag
 * is suppressed. The pin marker + selection rectangles are vector overlays
 * (L.circleMarker / L.rectangle) on an explicit L.svg() renderer.
 *
 * Real interaction smoothness / rubber-band render is grim-verified; the tests
 * prove wiring only.
 */
import { useEffect, useRef, useState } from 'react';
import L from 'leaflet';
import { LeafletMap } from './LeafletMap';
import { LeafletMaidenheadGridLayer } from './LeafletMaidenheadGridLayer';
import { useLeafletMap } from './LeafletMapContext';
import { useLeafletLayerGroup } from './leafletHooks';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';
import { reportFrontendError } from '../frontendErrorLog';
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

const RECT_HALF = { lat6: 1.25 / 60, lon6: 2.5 / 60, lat4: 0.5, lon4: 1.0 };

function gridSquareBounds(grid: string, ll: LatLon): L.LatLngBoundsExpression {
  const is6 = grid.toUpperCase().length === 6;
  const halfLat = is6 ? RECT_HALF.lat6 : RECT_HALF.lat4;
  const halfLon = is6 ? RECT_HALF.lon6 : RECT_HALF.lon4;
  return [
    [ll.lat - halfLat, ll.lon - halfLon],
    [ll.lat + halfLat, ll.lon + halfLon],
  ];
}

function cornersBounds(a: LatLon, b: LatLon): L.LatLngBoundsExpression {
  return [
    [Math.min(a.lat, b.lat), Math.min(a.lon, b.lon)],
    [Math.max(a.lat, b.lat), Math.max(a.lon, b.lon)],
  ];
}

interface PickerHandlers {
  mode: 'pin' | 'box';
  onGridChange?: (grid: string) => void;
  onBoxChange?: (a: LatLon, b: LatLon) => void;
  onTemp: (corners: Corners | null) => void;
}

/** Wire the drag-select / pin-click gesture onto the live map (finding 8). */
function usePickerInteractions(map: L.Map | null, handlers: PickerHandlers) {
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
        map.dragging.enable();
        ref.current.onTemp(null);
      }
    };
    window.addEventListener('mouseup', onWindowUp);

    const onDown = (e: L.LeafletMouseEvent) => {
      if (ref.current.mode !== 'box') return;
      map.dragging.disable(); // don't pan while drawing the box
      startRef.current = { lat: e.latlng.lat, lon: e.latlng.lng };
    };
    const onMove = (e: L.LeafletMouseEvent) => {
      if (ref.current.mode !== 'box' || !startRef.current) return;
      ref.current.onTemp([startRef.current, { lat: e.latlng.lat, lon: e.latlng.lng }]);
    };
    const onUp = (e: L.LeafletMouseEvent) => {
      if (ref.current.mode !== 'box' || !startRef.current) return;
      const start = startRef.current;
      const end: LatLon = { lat: e.latlng.lat, lon: e.latlng.lng };
      startRef.current = null;
      draggedRef.current = true; // suppress the click that follows a drag
      map.dragging.enable();
      ref.current.onTemp(null);
      ref.current.onBoxChange?.(start, end);
    };
    const onClick = (e: L.LeafletMouseEvent) => {
      if (draggedRef.current) {
        draggedRef.current = false;
        return;
      }
      if (ref.current.mode !== 'pin') return;
      ref.current.onGridChange?.(latLonToGrid(e.latlng.lat, e.latlng.lng).slice(0, 4));
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
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  const [temp, setTemp] = useState<Corners | null>(null);
  const rendererRef = useRef<L.Renderer | null>(null);
  if (!rendererRef.current) rendererRef.current = L.svg({ padding: 2 });

  usePickerInteractions(map, { mode, onGridChange, onBoxChange, onTemp: setTemp });

  const ll = grid ? gridToLatLon(grid) : null;
  useEffect(() => {
    if (!map || !group) return;
    try {
      group.clearLayers();
      const renderer = rendererRef.current ?? undefined;
      if (mode === 'pin' && grid && ll) {
        L.rectangle(gridSquareBounds(grid, ll), {
          renderer,
          color: '#2563eb',
          weight: 2,
          fillColor: '#2563eb',
          fillOpacity: 0.08,
          interactive: false,
        }).addTo(group);
        L.circleMarker([ll.lat, ll.lon], {
          renderer,
          radius: 6,
          color: '#ffffff',
          weight: 2,
          fillColor: '#2563eb',
          fillOpacity: 1,
          interactive: false,
        }).addTo(group);
      }
      if (temp) {
        L.rectangle(cornersBounds(temp[0], temp[1]), {
          renderer,
          color: '#dc2626',
          weight: 2,
          dashArray: '2 1',
          fillColor: '#dc2626',
          fillOpacity: 0.1,
          interactive: false,
        }).addTo(group);
      }
    } catch (e) {
      reportFrontendError(
        'grid-picker',
        `selection reconcile: ${e instanceof Error ? e.message : String(e)}`,
        e instanceof Error ? e.stack : undefined,
      );
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- primitive ll coords; rebuild on mode/grid/temp change
  }, [map, group, mode, grid, ll?.lat, ll?.lon, temp]);

  return null;
}

export function GridPicker({ mode, grid, onGridChange, onBoxChange, gridOverlay = true }: GridPickerProps) {
  const ll = grid ? gridToLatLon(grid) : null;
  return (
    <LeafletMap initialCenter={ll ?? undefined} initialZoom={ll ? 6 : 2}>
      {gridOverlay && <LeafletMaidenheadGridLayer visible />}
      <PickerBody mode={mode} grid={grid} onGridChange={onGridChange} onBoxChange={onBoxChange} />
    </LeafletMap>
  );
}
