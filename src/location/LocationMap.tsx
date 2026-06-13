/**
 * LocationMap — the offline location-setup map (tuxlink-yy1m). Composes BaseMap
 * (the bundled offline substrate) the same way PositionMapWidget does — no
 * network. Two markers, never both meaningful at once:
 *   - GPS fix present → a precise marker at the raw fix lat/lon (the "you are
 *     here" confirmation; local display only — these coords are never broadcast).
 *   - no fix          → a DRAGGABLE marker at the grid-square center; click the
 *     map OR drag the marker to set the grid by hand.
 * The grid-square rectangle always frames the current grid. onGridChange fires
 * the parent's Manual-pinning path (config_set_grid).
 *
 * Real drag/render/projection behaviour is grim-verified per the map subsystem's
 * C1 convention; the shape test proves wiring only.
 */
import { Marker, Rectangle } from 'react-leaflet';
import type { LeafletEventHandlerFnMap, Marker as LMarker } from 'leaflet';
import { BaseMap } from '../map/BaseMap';
import { useTileSource } from '../map/useTileSource';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';

export interface LocationMapProps {
  /** Current grid — square highlight + manual-marker center. */
  grid: string;
  /** Raw live GPS fix coords for the precise marker, or null when no fresh fix. */
  fixLatLon: { lat: number; lon: number } | null;
  /** Picker selection id ('manual' | 'gpsd' | 'serial:...'). When 'manual', the
   *  marker follows the manual grid (not the live fix) so a hand-set location
   *  isn't visually overridden by an arriving fix. */
  selectedSource: string;
  /** Fired with the new grid when the operator clicks the map or drags the pin. */
  onGridChange: (grid: string) => void;
}

/** Grid-square half-widths (degrees): 4-char = 2°×1° square; 6-char = 5′×2.5′. */
const HALF_LON_4 = 1.0;
const HALF_LAT_4 = 0.5;
const HALF_LON_6 = 2.5 / 60;
const HALF_LAT_6 = 1.25 / 60;

export function LocationMap({ grid, fixLatLon, selectedSource, onGridChange }: LocationMapProps) {
  const tileSource = useTileSource();
  const ll = grid ? gridToLatLon(grid) : null;
  // Show the live fix marker only while a GPS source is selected; once the
  // operator picks/sets Manual, the marker follows the manual grid so an
  // arriving fix doesn't yank their hand-set pin (flow 3).
  const showFix = selectedSource !== 'manual' && fixLatLon != null;
  const is6 = grid.toUpperCase().length === 6;
  const halfLat = is6 ? HALF_LAT_6 : HALF_LAT_4;
  const halfLon = is6 ? HALF_LON_6 : HALF_LON_4;
  const bounds: [[number, number], [number, number]] | null = ll
    ? [
        [ll.lat - halfLat, ll.lon - halfLon],
        [ll.lat + halfLat, ll.lon + halfLon],
      ]
    : null;

  const dragHandlers: LeafletEventHandlerFnMap = {
    dragend(e) {
      const m = e.target as LMarker;
      const { lat, lng } = m.getLatLng();
      onGridChange(latLonToGrid(lat, lng));
    },
  };

  // Marker position: the live fix when a GPS source is active, else the grid
  // center. The marker is ALWAYS draggable — dragging it sets the location by
  // hand (→ Manual), which is how the operator overrides a GPS fix they don't
  // want (flow 3). Clicking the map does the same.
  const markerPos: [number, number] | null = showFix
    ? [fixLatLon!.lat, fixLatLon!.lon]
    : ll
      ? [ll.lat, ll.lon]
      : null;
  const center = (showFix ? fixLatLon : ll) ?? undefined;

  // The wrapper div (.location-map) is the CSS sizing target for both chromes
  // (large left pane in the wizard; min-height block in Settings) — Task C4.
  return (
    <div className="location-map" data-testid="location-map">
      <BaseMap
        onMapClick={({ lat, lon }) => onGridChange(latLonToGrid(lat, lon))}
        initialCenter={center}
        initialZoom={center ? 3 : 1}
        tileSource={tileSource ?? undefined}
      >
        {bounds && (
          <Rectangle bounds={bounds} pathOptions={{ color: '#5fd39a', weight: 2, fillOpacity: 0.1 }} />
        )}
        {markerPos && <Marker position={markerPos} draggable eventHandlers={dragHandlers} />}
      </BaseMap>
    </div>
  );
}
