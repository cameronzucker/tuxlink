/**
 * PositionMapWidget — Leaflet map for PositionFormV2's grid-override UX.
 *
 * Controlled component: caller owns `grid` state; map click fires `onGridChange`
 * with the new 6-char Maidenhead derived from the click lat/lon.
 *
 * Offline strategy:
 *   - Online: OSM tile layer (https://tile.openstreetmap.org).
 *   - Offline: tile layer is omitted; marker + grid-square rectangle remain
 *     interactive so the operator can still see and click their grid square.
 *   - Detection: navigator.onLine on mount + window online/offline events.
 *     Tile-load failures (Leaflet tileerror) set offline mode on first hit.
 *
 * Leaflet CSS import note: leaflet/dist/leaflet.css is imported here so the
 * widget is self-contained and the CSS travels with the component.
 */

import { useEffect, useState } from 'react';
import {
  MapContainer,
  TileLayer,
  Marker,
  Rectangle,
  useMapEvents,
  useMap,
} from 'react-leaflet';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';
import 'leaflet/dist/leaflet.css';
// Fix Leaflet's default icon URL resolution which breaks under Vite bundling.
// Without this, markers render as broken-image boxes.
import L from 'leaflet';
import iconUrl from 'leaflet/dist/images/marker-icon.png';
import iconRetinaUrl from 'leaflet/dist/images/marker-icon-2x.png';
import shadowUrl from 'leaflet/dist/images/marker-shadow.png';

// Apply icon fix once at module load — safe to call multiple times.
delete (L.Icon.Default.prototype as unknown as Record<string, unknown>)._getIconUrl;
L.Icon.Default.mergeOptions({ iconUrl, iconRetinaUrl, shadowUrl });

export interface PositionMapWidgetProps {
  /** Current 6-char (or 4-char) Maidenhead grid — controlled by the parent. */
  grid: string;
  /** Called when the operator clicks on the map with the new 6-char grid. */
  onGridChange: (newGrid: string) => void;
}

/** Half-widths for the grid-square rectangle overlay (in degrees).
 *  6-char: 5′ lon / 2.5′ lat per subsquare step; center offset is half that.
 *  4-char: 2° lon / 1° lat per square step; center offset is half that.
 */
const HALF_LON_6 = 2.5 / 60;  // ~0.04167°
const HALF_LAT_6 = 1.25 / 60; // ~0.02083°
const HALF_LON_4 = 1.0;
const HALF_LAT_4 = 0.5;

/** Child component — must be inside a MapContainer to call hooks. */
interface MapInteractorProps {
  onClickLatLon: (lat: number, lng: number) => void;
  onTileError: () => void;
  isOnline: boolean;
}

function MapInteractor({ onClickLatLon, onTileError, isOnline }: MapInteractorProps) {
  const map = useMap();

  // Attach Leaflet's tileerror event via the imperative API — react-leaflet's
  // TileLayer doesn't expose eventHandlers.tileerror cleanly in v5.
  useEffect(() => {
    if (!isOnline) return; // no tile layer → no tile errors to listen for
    const handler = () => { onTileError(); };
    map.on('tileerror', handler);
    return () => { map.off('tileerror', handler); };
  }, [map, onTileError, isOnline]);

  useMapEvents({
    click(e) {
      onClickLatLon(e.latlng.lat, e.latlng.lng);
    },
  });

  return null;
}

export function PositionMapWidget({ grid, onGridChange }: PositionMapWidgetProps) {
  const [isOnline, setIsOnline] = useState(navigator.onLine);

  useEffect(() => {
    const goOnline = () => { setIsOnline(true); };
    const goOffline = () => { setIsOnline(false); };
    window.addEventListener('online', goOnline);
    window.addEventListener('offline', goOffline);
    return () => {
      window.removeEventListener('online', goOnline);
      window.removeEventListener('offline', goOffline);
    };
  }, []);

  const handleTileError = () => {
    // One-time: first tile load failure switches to offline mode.
    // Re-enabling happens only when the browser fires the 'online' event.
    setIsOnline(false);
  };

  const handleClick = (lat: number, lng: number) => {
    onGridChange(latLonToGrid(lat, lng));
  };

  const ll = gridToLatLon(grid);
  // If the grid is invalid, centre on 0,0 at world zoom.
  const center: [number, number] = ll ? [ll.lat, ll.lon] : [0, 0];
  const zoom = ll ? 12 : 2;

  // Grid-square rectangle bounds.
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
    <MapContainer
      center={center}
      zoom={zoom}
      // Height is set via .position-form-v2__map in PositionFormV2.css
      style={{ height: '100%', width: '100%' }}
      data-testid="leaflet-map-container"
    >
      <MapInteractor
        onClickLatLon={handleClick}
        onTileError={handleTileError}
        isOnline={isOnline}
      />

      {isOnline && (
        <TileLayer
          url="https://tile.openstreetmap.org/{z}/{x}/{y}.png"
          attribution="&copy; <a href='https://www.openstreetmap.org/copyright'>OpenStreetMap</a> contributors"
          data-testid="osm-tile-layer"
        />
      )}

      {ll && (
        <Marker
          position={[ll.lat, ll.lon]}
        />
      )}

      {bounds && (
        <Rectangle
          bounds={bounds}
          pathOptions={{ color: '#2563eb', weight: 2, fillOpacity: 0.08 }}
        />
      )}
    </MapContainer>
  );
}
