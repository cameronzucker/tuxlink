/**
 * BaseMap — the offline EPSG4326 map substrate shared by every map consumer.
 *
 * Renders a bundled equirectangular world raster as an `<ImageOverlay>` under
 * `L.CRS.EPSG4326` (plate carrée → linear pixel↔lat/lon). No tile layer, no
 * network: the map works fully offline, served entirely from `'self'`.
 *
 * `maxZoom={2}` caps zoom at the raster's native resolution so the view cannot
 * magnify past it into illusory precision (C6): under `L.CRS.EPSG4326` the world
 * is 512×256 CSS px at zoom 0 and doubles each level, so the 2048×1024 raster is
 * 1:1 at zoom 2. Panning is bounded to the world rectangle (`maxBounds` + full
 * viscosity) so there is no grey void.
 *
 * Real projection / render / pan correctness is verified via grim on
 * WebKitGTK, NOT through the react-leaflet test mock (C1).
 *
 * C11 WIDENING (Phase 7.3, tuxlink-dyop LAN-tiles plan). The frozen C11
 * interface gains ONE optional prop, `tileSource`, ON PURPOSE: a tile-backed LAN
 * source (status `lan-live`/`lan-cached`/`partial`) renders a `<TileLayerBridge>`
 * ABOVE the always-present bundled raster, and the zoom cap rises from 2 to the
 * source's validated max (capped at 16). Every other status — and the absent
 * prop — leaves the map exactly as before (raster-only, maxZoom 2). The raster
 * remains the always-present base so a missing/404 tile shows the raster
 * beneath at/below raster-native zoom rather than a grey void; above
 * raster-native zoom the tile layer (not a stretched raster) governs the view
 * (§8.5). The widening is additive — existing consumers that pass no
 * `tileSource` are unaffected.
 *
 * §8.5 `partial` reconcile (Phase 9.2). `partial` is a LIVE source with some
 * 404s; it is tile-backed exactly like `lan-live`/`lan-cached` — the layer
 * stays rendered and the zoom cap stays raised. The 404 tiles themselves get
 * the no-coverage treatment (raster beneath at/below raster-native zoom; NO
 * stretched raster above, which is Leaflet's default for a tile that fails to
 * load above `maxNativeZoom`).
 */
import type { ReactNode } from 'react';
import { MapContainer, ImageOverlay, useMapEvents } from 'react-leaflet';
import L from 'leaflet';
import { WORLD_BOUNDS, clampLatLon, type LatLon } from './projection';
import { TileLayerBridge } from './TileLayerBridge';
import type { TileSource, TileSourceStatus } from './tileSource';
import './leafletIconFix';
import worldEquirectPng from './assets/world-equirect-2048.png';
import 'leaflet/dist/leaflet.css';

/** Raster-native zoom cap when no validated LAN tile source backs the view. */
const RASTER_MAX_ZOOM = 2;
/** Hard upper bound on the raised zoom even when the LAN source claims higher. */
const TILE_MAX_ZOOM_CAP = 16;

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
  /**
   * Optional validated LAN tile source (C11 widening, Phase 7.3). When its
   * `status.kind` is `lan-live`/`lan-cached`, a TileLayer renders above the
   * raster and the zoom cap rises to the source's validated max (≤ 16). Any
   * other status leaves the raster-only map at maxZoom 2.
   */
  tileSource?: { source: TileSource; status: TileSourceStatus };
}

/**
 * True when a status backs a tile layer the map may serve: `lan-live`,
 * `lan-cached`, OR `partial`.
 *
 * `partial` (§8.5 "LAN live (partial)") is a LIVE source with some 404s above
 * its raster-native zoom — the TileLayer MUST stay rendered (and the zoom cap
 * raised) so the served zoom levels keep showing real tiles; a 404 tile falls
 * back to the bundled raster beneath at/below raster-native zoom and to nothing
 * (no stretched raster) above it, per Leaflet's default for a failed tile above
 * `maxNativeZoom`. Dropping the layer on `partial` would regress the whole view
 * to the coarse raster the moment a single edge tile is missing.
 */
function isTileBacked(status: TileSourceStatus | undefined): boolean {
  return (
    status?.kind === 'lan-live' ||
    status?.kind === 'lan-cached' ||
    status?.kind === 'partial'
  );
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

export function BaseMap({
  children,
  onMapClick,
  initialCenter,
  initialZoom,
  tileSource,
}: BaseMapProps) {
  const center: [number, number] = initialCenter
    ? [initialCenter.lat, initialCenter.lon]
    : [0, 0];

  const tileBacked = isTileBacked(tileSource?.status);
  // Zoom rises to the validated source max (capped at 16) when a tile-backed
  // source (lan-live/lan-cached/partial) backs the view; otherwise stay at
  // raster-native. `partial` keeps the raised cap so the served levels still
  // show real tiles (§8.5).
  const maxZoom = tileBacked
    ? Math.min(tileSource!.source.maxZoom, TILE_MAX_ZOOM_CAP)
    : RASTER_MAX_ZOOM;

  return (
    <MapContainer
      crs={L.CRS.EPSG4326}
      center={center}
      zoom={initialZoom ?? 1}
      maxBounds={WORLD_BOUNDS}
      maxBoundsViscosity={1.0}
      minZoom={0}
      maxZoom={maxZoom}
      zoomSnap={0.5}
      worldCopyJump={false}
      // Native shift-drag box-zoom is disabled: it conflicts with the
      // GridMapPicker drag-to-select gesture, and the zoom-4 cap makes it
      // pointless on the offline substrate.
      boxZoom={false}
      attributionControl={false}
      style={{ height: '100%', width: '100%' }}
    >
      {/* Bundled raster is the ALWAYS-present base. The validated LAN tile
          layer (when present) renders ABOVE it so a 404 tile reveals the
          raster beneath at/below raster-native zoom (§8.5). */}
      <ImageOverlay url={worldEquirectPng} bounds={WORLD_BOUNDS} />
      {tileBacked && (
        <TileLayerBridge source={tileSource!.source} appMaxZoom={TILE_MAX_ZOOM_CAP} />
      )}
      <MapClickHandler onMapClick={onMapClick} />
      {children}
    </MapContainer>
  );
}
