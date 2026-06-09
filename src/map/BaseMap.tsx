/**
 * BaseMap — the offline EPSG4326 map substrate shared by every map consumer.
 *
 * Renders a bundled equirectangular world raster as an `<ImageOverlay>` under
 * `L.CRS.EPSG4326` (plate carrée → linear pixel↔lat/lon). No tile layer, no
 * network: the map works fully offline, served entirely from `'self'`.
 *
 * `maxZoom={4}` caps zoom so the view cannot magnify past the 2048px native
 * raster resolution into illusory precision (C6). Panning is bounded to the
 * world rectangle (`maxBounds` + full viscosity) so there is no grey void.
 *
 * Real projection / render / pan correctness is verified via grim on
 * WebKitGTK, NOT through the react-leaflet test mock (C1).
 */
import type { ReactNode } from 'react';
import { MapContainer, ImageOverlay, useMapEvents } from 'react-leaflet';
import L from 'leaflet';
import { WORLD_BOUNDS, clampLatLon, type LatLon } from './projection';
import './leafletIconFix';
import worldEquirectPng from './assets/world-equirect-2048.png';
import 'leaflet/dist/leaflet.css';

/**
 * FROZEN CONTRACT (C11). Tasks consuming BaseMap (MaidenheadOverlay,
 * GridMapPicker, PositionMapWidget) MUST NOT change this interface. If a
 * consumer needs a new prop, stop and coordinate rather than widen it ad hoc.
 */
export interface BaseMapProps {
  /** Map layers/overlays rendered inside the MapContainer. */
  children?: ReactNode;
  /** Called with the clamped lat/lon when the operator clicks the map. */
  onMapClick?: (latlon: LatLon) => void;
  /** Initial view center (defaults to 0,0). */
  initialCenter?: LatLon;
  /** Initial zoom (defaults to 1). */
  initialZoom?: number;
}

/** Bridges Leaflet's click event to `onMapClick`, clamped to the world rectangle. */
function MapClickHandler({ onMapClick }: { onMapClick?: (latlon: LatLon) => void }) {
  useMapEvents({
    click(e) {
      if (onMapClick) onMapClick(clampLatLon(e.latlng.lat, e.latlng.lng));
    },
  });
  return null;
}

export function BaseMap({ children, onMapClick, initialCenter, initialZoom }: BaseMapProps) {
  const center: [number, number] = initialCenter
    ? [initialCenter.lat, initialCenter.lon]
    : [0, 0];

  return (
    <MapContainer
      crs={L.CRS.EPSG4326}
      center={center}
      zoom={initialZoom ?? 1}
      maxBounds={WORLD_BOUNDS}
      maxBoundsViscosity={1.0}
      minZoom={0}
      maxZoom={4}
      zoomSnap={0.5}
      worldCopyJump={false}
      // Native shift-drag box-zoom is disabled: it conflicts with the
      // GridMapPicker drag-to-select gesture, and the zoom-4 cap makes it
      // pointless on the offline substrate.
      boxZoom={false}
      attributionControl={false}
      style={{ height: '100%', width: '100%' }}
    >
      <ImageOverlay url={worldEquirectPng} bounds={WORLD_BOUNDS} />
      <MapClickHandler onMapClick={onMapClick} />
      {children}
    </MapContainer>
  );
}
