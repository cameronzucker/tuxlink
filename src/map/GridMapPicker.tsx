/**
 * GridMapPicker — offline location picker with two modes (C2):
 *   - 'pin': click drops a single point; reports the 4-char Maidenhead grid
 *     (the broadcast default — finer precision needs the opt-in server,
 *     out of scope / tuxlink-dyop).
 *   - 'box': drag a rectangle; reports the two signed lat/lon corners via
 *     onBoxChange (the caller normalizes them, e.g. signedBboxToGribRegion).
 *
 * Composes BaseMap (offline substrate) + MaidenheadOverlay. The live
 * rubber-band preview, no-pan-during-drag, and post-drag click suppression
 * are real-interaction behaviours verified via grim — the shape test proves
 * only wiring (C1).
 */
import { useRef, useState } from 'react';
import { Marker, Rectangle, useMap, useMapEvents } from 'react-leaflet';
import { BaseMap } from './BaseMap';
import { MaidenheadOverlay } from './MaidenheadOverlay';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';
import type { LatLon } from './projection';

export interface GridMapPickerProps {
  mode: 'pin' | 'box';
  /** Current grid (pin mode marker + grid-square highlight). */
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

function rectFromCorners(a: LatLon, b: LatLon): [[number, number], [number, number]] {
  return [
    [Math.min(a.lat, b.lat), Math.min(a.lon, b.lon)],
    [Math.max(a.lat, b.lat), Math.max(a.lon, b.lon)],
  ];
}

/** Pin-mode grid-square highlight (per-consumer rectangle, C15). */
function gridSquareBounds(grid: string, ll: LatLon): [[number, number], [number, number]] {
  const is6 = grid.toUpperCase().length === 6;
  const halfLat = is6 ? RECT_HALF.lat6 : RECT_HALF.lat4;
  const halfLon = is6 ? RECT_HALF.lon6 : RECT_HALF.lon4;
  return [
    [ll.lat - halfLat, ll.lon - halfLon],
    [ll.lat + halfLat, ll.lon + halfLon],
  ];
}

interface InteractionsProps {
  mode: 'pin' | 'box';
  onGridChange?: (grid: string) => void;
  onBoxChange?: (a: LatLon, b: LatLon) => void;
  onTemp: (corners: Corners | null) => void;
}

function PickerInteractions({ mode, onGridChange, onBoxChange, onTemp }: InteractionsProps) {
  const map = useMap();
  const startRef = useRef<LatLon | null>(null);
  const draggedRef = useRef(false);

  // useMapEvents auto-cleans listeners on unmount (no bare map.on leak).
  useMapEvents({
    mousedown(e) {
      if (mode !== 'box') return;
      map.dragging.disable(); // don't pan while drawing the box
      startRef.current = { lat: e.latlng.lat, lon: e.latlng.lng };
    },
    mousemove(e) {
      if (mode !== 'box' || !startRef.current) return;
      onTemp([startRef.current, { lat: e.latlng.lat, lon: e.latlng.lng }]);
    },
    mouseup(e) {
      if (mode !== 'box' || !startRef.current) return;
      const start = startRef.current;
      const end: LatLon = { lat: e.latlng.lat, lon: e.latlng.lng };
      startRef.current = null;
      draggedRef.current = true; // suppress the click Leaflet fires after a drag
      map.dragging.enable();
      onTemp(null);
      onBoxChange?.(start, end);
    },
    click(e) {
      if (draggedRef.current) {
        draggedRef.current = false;
        return;
      }
      if (mode !== 'pin') return;
      onGridChange?.(latLonToGrid(e.latlng.lat, e.latlng.lng).slice(0, 4));
    },
  });

  return null;
}

export function GridMapPicker({
  mode,
  grid,
  onGridChange,
  onBoxChange,
  gridOverlay = true,
}: GridMapPickerProps) {
  const [temp, setTemp] = useState<Corners | null>(null);

  const ll = grid ? gridToLatLon(grid) : null;
  const pinBounds = mode === 'pin' && grid && ll ? gridSquareBounds(grid, ll) : null;

  return (
    <BaseMap initialCenter={ll ?? undefined} initialZoom={ll ? 4 : 1}>
      {gridOverlay && <MaidenheadOverlay visible />}
      <PickerInteractions
        mode={mode}
        onGridChange={onGridChange}
        onBoxChange={onBoxChange}
        onTemp={setTemp}
      />
      {mode === 'pin' && ll && <Marker position={[ll.lat, ll.lon]} />}
      {pinBounds && (
        <Rectangle
          bounds={pinBounds}
          pathOptions={{ color: '#2563eb', weight: 2, fillOpacity: 0.08 }}
        />
      )}
      {temp && (
        <Rectangle
          bounds={rectFromCorners(temp[0], temp[1])}
          pathOptions={{ color: '#dc2626', weight: 2, dashArray: '6', fillOpacity: 0.1 }}
        />
      )}
    </BaseMap>
  );
}
