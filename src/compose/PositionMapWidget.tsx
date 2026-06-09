/**
 * PositionMapWidget — offline location picker for PositionFormV2's grid override.
 *
 * Controlled component: the caller owns `grid` state; a map click fires
 * `onGridChange` with the new 6-char Maidenhead locator derived from the
 * clicked lat/lon. Renders on the bundled offline world map (BaseMap) — no
 * network, no online OpenStreetMap tiles, no online/offline detection. The map
 * is an aid; the manual grid input in PositionFormV2 is the always-available
 * path.
 *
 * The default-marker-icon fix and the leaflet CSS travel with BaseMap, so this
 * widget no longer imports them directly.
 */

import { Marker, Rectangle } from 'react-leaflet';
import { BaseMap } from '../map/BaseMap';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';

export interface PositionMapWidgetProps {
  /** Current 6-char (or 4-char) Maidenhead grid — controlled by the parent. */
  grid: string;
  /** Called when the operator clicks the map, with the new 6-char grid. */
  onGridChange: (newGrid: string) => void;
}

/** Half-widths for the grid-square rectangle overlay (in degrees).
 *  6-char: 5′ lon / 2.5′ lat per subsquare step; center offset is half that.
 *  4-char: 2° lon / 1° lat per square step; center offset is half that.
 */
const HALF_LON_6 = 2.5 / 60; // ~0.04167°
const HALF_LAT_6 = 1.25 / 60; // ~0.02083°
const HALF_LON_4 = 1.0;
const HALF_LAT_4 = 0.5;

export function PositionMapWidget({ grid, onGridChange }: PositionMapWidgetProps) {
  const ll = gridToLatLon(grid);

  // Grid-square rectangle bounds (per-consumer, C15): width by locator length.
  const is6Char = grid.toUpperCase().length === 6;
  const halfLat = is6Char ? HALF_LAT_6 : HALF_LAT_4;
  const halfLon = is6Char ? HALF_LON_6 : HALF_LON_4;
  const bounds: [[number, number], [number, number]] | null = ll
    ? [
        [ll.lat - halfLat, ll.lon - halfLon],
        [ll.lat + halfLat, ll.lon + halfLon],
      ]
    : null;

  return (
    <BaseMap
      onMapClick={({ lat, lon }) => {
        // Full 6-char locator — this widget's per-message position-report
        // contract is unchanged; the 4-char default is a GridMapPicker concern.
        onGridChange(latLonToGrid(lat, lon));
      }}
      initialCenter={ll ?? undefined}
      initialZoom={ll ? 2 : 1}
    >
      {ll && <Marker position={[ll.lat, ll.lon]} />}
      {bounds && (
        <Rectangle
          bounds={bounds}
          pathOptions={{ color: '#2563eb', weight: 2, fillOpacity: 0.08 }}
        />
      )}
    </BaseMap>
  );
}
