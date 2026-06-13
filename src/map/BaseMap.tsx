/**
 * BaseMap — the offline EPSG:3857 map substrate shared by every map consumer.
 *
 * Renders a bundled Mercator world raster as an `<ImageOverlay>` under
 * `L.CRS.EPSG3857` (Web Mercator). No tile layer, no network: the map works
 * fully offline, served entirely from `'self'`.
 *
 * `maxZoom={3}` caps zoom at the raster's native resolution so the view cannot
 * magnify past it into illusory precision (C6): under `L.CRS.EPSG3857` the world
 * is 256×256 CSS px at zoom 0 and doubles each level, so the 2048×2048 raster is
 * 1:1 at zoom 3. Panning is bounded to the Mercator world rectangle
 * (`maxBounds` ±85.0511° + full viscosity) so there is no grey void.
 *
 * Real projection / render / pan correctness is verified via grim on
 * WebKitGTK, NOT through the react-leaflet test mock (C1).
 *
 * C11 WIDENING (Phase 7.3, tuxlink-dyop LAN-tiles plan). The frozen C11
 * interface gains ONE optional prop, `tileSource`, ON PURPOSE: a tile-backed LAN
 * source (status `lan-live`/`lan-cached`/`partial`) renders a `<TileLayerBridge>`
 * ABOVE the always-present bundled raster, and the zoom cap rises from 3 to the
 * source's validated max (capped at 16). Every other status — and the absent
 * prop — leaves the map exactly as before (raster-only, maxZoom 3). The raster
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
import { useEffect, type ReactNode } from 'react';
import { MapContainer, ImageOverlay, Pane, useMap, useMapEvents } from 'react-leaflet';
import L from 'leaflet';
import { MERCATOR_BOUNDS, clampLatLon, type LatLon } from './projection';
import { TileLayerBridge } from './TileLayerBridge';
import type { TileSource, TileSourceStatus } from './tileSource';
import './leafletIconFix';
import worldMercatorPng from './assets/world-mercator-2048.png';
import 'leaflet/dist/leaflet.css';

/**
 * Raster-native zoom cap when no validated LAN tile source backs the view.
 * Under `L.CRS.EPSG3857` the world tile is 256×256 px at z0; the 2048-px
 * Mercator raster is 1:1 at z3 (256·2³ = 2048).
 */
const RASTER_MAX_ZOOM = 3;
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
   * other status leaves the raster-only map at maxZoom 3.
   */
  tileSource?: { source: TileSource; status: TileSourceStatus };
  /**
   * Called with the new zoom level after every `zoomend` event (Task 5 bridge).
   * Used by consumers that need to gate UI (e.g. the 6-char Maidenhead grid)
   * on a minimum zoom level without polling `useMap()` themselves.
   */
  onZoomChange?: (zoom: number) => void;
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

/** Bridges Leaflet's `zoomend` event to `onZoomChange` with the new zoom level. */
function MapZoomHandler({ onZoomChange }: { onZoomChange?: (zoom: number) => void }) {
  useMapEvents({ zoomend(e) { onZoomChange?.(e.target.getZoom()); } });
  return null;
}

/**
 * Imperatively keeps the live Leaflet map's max-zoom in sync with `maxZoom`.
 *
 * `<MapContainer>`'s `maxZoom` is read ONCE at construction and is NOT reactive
 * (the same react-leaflet contract that makes `center`/`zoom` non-reactive —
 * see `RecenterOnOperator` in StationFinderMap). The validated LAN tile source
 * arrives ASYNCHRONOUSLY from `useTileSource` AFTER the map has mounted, so the
 * raised cap (`tileSource.source.maxZoom`) would never take effect through the
 * prop alone — the map would stay clamped at the raster-native `RASTER_MAX_ZOOM`
 * (3) forever, and the operator could not zoom into the freshly-bound source
 * (bd tuxlink-k61j). This child lives inside the MapContainer, gets the live map
 * via `useMap()`, and `setMaxZoom`s whenever the computed cap changes
 * (3→raised when a source binds, raised→3 if it is later cleared). The MapContainer
 * `maxZoom` prop stays as the construction-time initial; this is the reactive
 * follow-up for the post-mount case.
 */
function ApplyMaxZoom({ maxZoom }: { maxZoom: number }) {
  const map = useMap();
  useEffect(() => {
    map.setMaxZoom(maxZoom);
  }, [map, maxZoom]);
  return null;
}

export function BaseMap({
  children,
  onMapClick,
  initialCenter,
  initialZoom,
  tileSource,
  onZoomChange,
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
      crs={L.CRS.EPSG3857}
      center={center}
      zoom={initialZoom ?? 1}
      maxBounds={MERCATOR_BOUNDS}
      maxBoundsViscosity={1.0}
      minZoom={0}
      maxZoom={maxZoom}
      zoomSnap={0.5}
      worldCopyJump={false}
      // Native shift-drag box-zoom is disabled: it conflicts with the
      // GridMapPicker drag-to-select gesture, and the zoom-cap makes it
      // pointless on the offline substrate.
      boxZoom={false}
      attributionControl={false}
      style={{ height: '100%', width: '100%' }}
    >
      {/* Bundled Mercator raster is the ALWAYS-present base. It MUST live in a
          pane BELOW Leaflet's `tilePane` (z-index 200): an `ImageOverlay`'s
          default pane is `overlayPane` (z-index 400), which sits ABOVE the LAN
          `TileLayer` in `tilePane` — so the bundled raster would PAINT OVER the
          LAN tiles and hide them entirely. That occlusion is why bound tiles
          fetched (HTTP 200) yet never displayed; Leaflet stacks by PANE z-index,
          not DOM order, so the prior "renders above via DOM order" assumption was
          wrong (bd tuxlink-k61j). The custom pane at z-index 100 keeps the raster
          the bottom layer, with LAN tiles (200) and grid/markers (400+) above. */}
      <Pane name="tux-raster-base" style={{ zIndex: 100 }}>
        <ImageOverlay url={worldMercatorPng} bounds={MERCATOR_BOUNDS} />
      </Pane>
      {tileBacked && (
        <TileLayerBridge source={tileSource!.source} appMaxZoom={TILE_MAX_ZOOM_CAP} />
      )}
      <MapClickHandler onMapClick={onMapClick} />
      <MapZoomHandler onZoomChange={onZoomChange} />
      {/* react-leaflet reads `maxZoom` once at mount; this applies the cap
          imperatively so an async-arriving tile source actually raises it
          (bd tuxlink-k61j). */}
      <ApplyMaxZoom maxZoom={maxZoom} />
      {children}
    </MapContainer>
  );
}
