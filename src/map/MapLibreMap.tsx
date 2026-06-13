/**
 * MapLibreMap — the offline vector map substrate (tuxlink-ndi4, plan phase 2 / L1).
 *
 * Replaces the react-leaflet `BaseMap`. Renders the bundled z0–6 vector overview
 * (light flavor) via the PMTiles 206 seam, driving `maplibre-gl` directly through
 * a thin owned layer. Overlays are NOT JSX children of a Leaflet map; they read
 * the live map from `MapContext` and wire themselves via the owned hooks.
 *
 * C11 re-expression (the frozen BaseMapProps, restated for MapLibre):
 *  - `onMapClick`  → `map.on('click')`, clamped to the mercator rectangle.
 *  - `onZoomChange`→ seeded on `load` AND fired on `moveend` with the real
 *    fractional zoom (plan A17 — not a stale literal, and covers pan+zoom).
 *  - `initialCenter`/`initialZoom` → constructor; a later `initialCenter` change
 *    drives `flyTo` (the async-arrival recenter the old RecenterOnOperator did).
 *  - `children` → consume the map via `MapContext` + owned hooks.
 *  - `tileSource` → REMOVED (the LAN raster basemap is retired; A5).
 *
 * Real projection / render / pan correctness is grim-only on WebKitGTK; jsdom
 * has no WebGL. The bundled world archive is absent until the out-of-band build
 * runs, in which case `tile://pmtiles/world` 404s and the map renders empty — by
 * design (the render is verified at the smoke, not as a merge gate).
 */
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import maplibregl from 'maplibre-gl';
import { Protocol } from 'pmtiles';
import { invoke } from '@tauri-apps/api/core';
import { clampLatLon, type LatLon } from './projection';
import { buildBasemapStyle, type BasemapFlavor, type PackSource } from './basemapStyle';
import { BASEMAP_PACKS_CHANGED_EVENT, type PacksList } from './offlineMaps';
import { useBasemapFlavor } from './useBasemapFlavor';
import { MapProvider } from './MapContext';

// Register the PMTiles protocol once, at module load. `addProtocol` throws on a
// duplicate scheme, so this must NOT run per-mount. The Protocol auto-creates a
// PMTiles instance (FetchSource) for `pmtiles://tile://pmtiles/world` on first
// request, Range-fetching the Rust 206 seam.
const pmtilesProtocol = new Protocol();
maplibregl.addProtocol('pmtiles', pmtilesProtocol.tile);

/** Min interactive zoom (whole world fits at ~z1 on the z0–14 scale). */
const MAP_MIN_ZOOM = 0;
/** Max interactive zoom — region packs carry z0–14; the overview overzooms past z6. */
const MAP_MAX_ZOOM = 14;
/** Default world view on the z0–14 fractional scale (was raster-native z1; finding 2). */
const DEFAULT_ZOOM = 2;
/** Mercator pan rectangle in MapLibre [lng, lat] order ([west,south],[east,north]). */
const MAP_MAX_BOUNDS: [[number, number], [number, number]] = [
  [-180, -85.0511],
  [180, 85.0511],
];

export interface MapLibreMapProps {
  /** Overlays that consume the map via MapContext + owned hooks. */
  children?: ReactNode;
  /** Called with the clamped lat/lon when the operator clicks the map. */
  onMapClick?: (latlon: LatLon) => void;
  /** Initial view center (defaults to 0,0). A later change drives `flyTo`. */
  initialCenter?: LatLon;
  /** Initial zoom (defaults to the world view). */
  initialZoom?: number;
  /** Called with the live zoom after load and after every view change (A17). */
  onZoomChange?: (zoom: number) => void;
  /** Basemap flavor override (L2). Omit to FOLLOW the app color scheme (dark
   * scheme → dark map, the default behavior); pass `light`/`dark` to force one.
   * A change after mount drives `setStyle`; overlays re-add on the `styledata`
   * that follows (the owned hooks already re-subscribe). */
  flavor?: BasemapFlavor;
}

export function MapLibreMap({
  children,
  onMapClick,
  initialCenter,
  initialZoom,
  onZoomChange,
  flavor,
}: MapLibreMapProps) {
  // Follow the app color scheme unless an explicit flavor is passed.
  const themeFlavor = useBasemapFlavor();
  const effectiveFlavor = flavor ?? themeFlavor;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [map, setMap] = useState<maplibregl.Map | null>(null);

  // Latest callbacks held in refs so the construct-once effect never re-runs on
  // a changed callback identity.
  const onClickRef = useRef(onMapClick);
  onClickRef.current = onMapClick;
  const onZoomRef = useRef(onZoomChange);
  onZoomRef.current = onZoomChange;
  // Tracks the flavor currently applied to the map (seeded at construction).
  const flavorRef = useRef(effectiveFlavor);

  // Installed region packs composited over the world overview (R7). Fetched after
  // mount (the construct-time style uses the overview only) and re-fetched when the
  // pack manager signals a change; a change drives setStyle in the rebuild effect.
  const [packs, setPacks] = useState<PackSource[]>([]);
  const fetchPacks = useCallback(async () => {
    try {
      const list = await invoke<PacksList>('basemap_list_packs');
      setPacks(list.packs.map((p) => ({ id: p.id })));
    } catch {
      // No backend (e.g. unit test / dev without the command) → overview only.
      setPacks([]);
    }
  }, []);
  useEffect(() => {
    void fetchPacks();
    const onChange = () => void fetchPacks();
    window.addEventListener(BASEMAP_PACKS_CHANGED_EVENT, onChange);
    return () => window.removeEventListener(BASEMAP_PACKS_CHANGED_EVENT, onChange);
  }, [fetchPacks]);

  // Construct the map exactly once.
  useEffect(() => {
    if (!containerRef.current) return;
    const instance = new maplibregl.Map({
      container: containerRef.current,
      style: buildBasemapStyle(flavorRef.current),
      center: initialCenter ? [initialCenter.lon, initialCenter.lat] : [0, 0],
      zoom: initialZoom ?? DEFAULT_ZOOM,
      minZoom: MAP_MIN_ZOOM,
      maxZoom: MAP_MAX_ZOOM,
      maxBounds: MAP_MAX_BOUNDS,
      renderWorldCopies: false,
      // We add the AttributionControl explicitly so "© OpenStreetMap
      // contributors" (ODbL) renders from the source attribution.
      attributionControl: false,
    });
    instance.addControl(new maplibregl.AttributionControl({ compact: false }));
    instance.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'top-right');

    instance.on('click', (e: maplibregl.MapMouseEvent) => {
      onClickRef.current?.(clampLatLon(e.lngLat.lat, e.lngLat.lng));
    });
    const emitZoom = () => onZoomRef.current?.(instance.getZoom());
    instance.on('load', () => {
      setMap(instance);
      emitZoom();
    });
    instance.on('moveend', emitZoom);

    return () => {
      instance.remove();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- construct once; props read at mount, later changes handled by dedicated effects
  }, []);

  // Async-arrival recenter (the old RecenterOnOperator): a center that changes
  // AFTER construction drives flyTo. Skip ONLY the construct-time center: if
  // initialCenter was present at mount, the constructor already used it (skip the
  // first reactive run); if it was ABSENT at mount (operator grid arrives later,
  // e.g. StationFinderMap), the first non-null center MUST flyTo — do not skip it.
  const skipConstructCenter = useRef(Boolean(initialCenter));
  useEffect(() => {
    if (!map || !initialCenter) return;
    if (skipConstructCenter.current) {
      skipConstructCenter.current = false;
      return;
    }
    map.flyTo({ center: [initialCenter.lon, initialCenter.lat] });
    // Depend on the primitive lat/lon (not the object ref) so a re-render passing
    // a fresh object with the SAME coordinates does not re-trigger a flyTo.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [map, initialCenter?.lat, initialCenter?.lon]);

  // Style rebuild on flavor change (L2 light↔dark) OR installed-pack change (R7).
  // The constructor already applied {flavor, no packs}; this effect seeds from that
  // and calls setStyle only when the effective {flavor, pack-ids} actually changes,
  // so a redundant render never reloads the style. Overlays re-add on the
  // `styledata` that setStyle fires (the owned hooks re-subscribe).
  const styleKeyRef = useRef<string | null>(null);
  useEffect(() => {
    if (!map) return;
    const key = `${effectiveFlavor}|${packs.map((p) => p.id).slice().sort().join(',')}`;
    if (styleKeyRef.current === null) styleKeyRef.current = `${effectiveFlavor}|`; // construct-time
    if (styleKeyRef.current === key) return;
    styleKeyRef.current = key;
    flavorRef.current = effectiveFlavor;
    map.setStyle(buildBasemapStyle(effectiveFlavor, packs));
  }, [map, effectiveFlavor, packs]);

  return (
    <div ref={containerRef} style={{ height: '100%', width: '100%' }}>
      <MapProvider value={map}>{children}</MapProvider>
    </div>
  );
}
