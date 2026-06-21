/**
 * LeafletMap — the offline vector map substrate (tuxlink-6kdw, plan phase 1).
 *
 * The Leaflet twin of `MapLibreMap`, introduced ALONGSIDE it during the
 * strangler-fig migration: `AprsPositionsMap` (migrated) uses this; the four
 * un-migrated consumers keep `MapLibreMap`. It preserves the SAME public contract
 * (`MapLibreMapProps`) so consumers swap the import with no prop changes, and
 * publishes the live `L.Map` via `LeafletMapContext` for overlays.
 *
 * Re-expresses MapLibreMap's hard-won behaviors on Canvas2D, shedding the
 * WebGL/llvmpipe scar tissue:
 *  - pan-clamp → Leaflet NATIVE `maxBounds` (the maplibre-5.24.0 maxBounds crash,
 *    tuxlink-rwo6, that forced the manual moveend clamp does not exist here).
 *  - construct-once; flavor/pack swap rebuilds the base layer(s) via the dedup'd
 *    key (Leaflet has no `setStyle`).
 *  - dedup'd zoom emit, viewport-persist emit, async-arrival `flyTo`, contained
 *    error fallback, StrictMode-safe teardown.
 */
import { useEffect, useRef, useState, type ReactNode } from 'react';
import L from 'leaflet';
import 'leaflet/dist/leaflet.css';
import { invoke } from '@tauri-apps/api/core';
import { buildBaseLayers, type PackSource, type BasemapFlavor } from './basemapLeaflet';
import { useBasemapFlavor } from './useBasemapFlavor';
import { clampLatLon, clampMapCenter, MERCATOR_MAX_LAT, type LatLon } from './projection';
import { LeafletMapProvider } from './LeafletMapContext';
import { BASEMAP_PACKS_CHANGED_EVENT, type PacksList } from './offlineMaps';

/** Last-known installed packs, cached at module scope across mounts (mirrors
 * MapLibreMap's B2 cache): the map remounts on navigation; carrying the packs
 * lets a remount build WITH them so the async fetch resolution is a no-op. */
let lastKnownPacks: PackSource[] = [];

const MAP_MIN_ZOOM = 0;
const MAP_MAX_ZOOM = 14;
const DEFAULT_ZOOM = 2;

export interface LeafletMapProps {
  /** Overlays that consume the map via LeafletMapContext. */
  children?: ReactNode;
  /** Called with the clamped lat/lon when the operator clicks the map. */
  onMapClick?: (latlon: LatLon) => void;
  /** Initial view center. A later change drives `flyTo`. */
  initialCenter?: LatLon;
  /** Initial zoom (defaults to the world view). */
  initialZoom?: number;
  /** Called with the live zoom after ready and after every settle (deduped). */
  onZoomChange?: (zoom: number) => void;
  /** Called on every settle with the clamped center + zoom (viewport persist). */
  onViewportChange?: (center: LatLon, zoom: number) => void;
  /** Flavor override; omit to follow the app color scheme. */
  flavor?: BasemapFlavor;
}

export function LeafletMap({
  children,
  onMapClick,
  initialCenter,
  initialZoom,
  onZoomChange,
  onViewportChange,
  flavor,
}: LeafletMapProps) {
  const themeFlavor = useBasemapFlavor();
  const effectiveFlavor: BasemapFlavor = flavor ?? themeFlavor;

  const containerRef = useRef<HTMLDivElement | null>(null);
  const [map, setMap] = useState<L.Map | null>(null);
  const [mapError, setMapError] = useState(false);

  // Latest callbacks held in refs so the construct-once effect never re-runs.
  const onClickRef = useRef(onMapClick);
  onClickRef.current = onMapClick;
  const onZoomRef = useRef(onZoomChange);
  onZoomRef.current = onZoomChange;
  const onViewportRef = useRef(onViewportChange);
  onViewportRef.current = onViewportChange;

  // Installed packs composited over the overview. Seed from the module cache so a
  // remount constructs with packs already known.
  const [packs, setPacks] = useState<PackSource[]>(() => lastKnownPacks);
  useEffect(() => {
    let cancelled = false;
    const fetchPacks = async () => {
      try {
        const list = await invoke<PacksList>('basemap_list_packs');
        const next: PackSource[] = list.packs.map((p) => ({ id: p.id, maxZoom: p.maxzoom }));
        lastKnownPacks = next;
        if (!cancelled) setPacks(next);
      } catch {
        // No backend (unit test / dev) → overview only. (Latent: a transient
        // failure mid-session zeroes the cache — bd tuxlink-kepz, faithful port.)
        lastKnownPacks = [];
        if (!cancelled) setPacks([]);
      }
    };
    void fetchPacks();
    const onChange = () => void fetchPacks();
    window.addEventListener(BASEMAP_PACKS_CHANGED_EVENT, onChange);
    return () => {
      cancelled = true;
      window.removeEventListener(BASEMAP_PACKS_CHANGED_EVENT, onChange);
    };
  }, []);

  // Construct the map exactly once.
  useEffect(() => {
    if (!containerRef.current) return;
    let removed = false;
    try {
      // clampMapCenter takes (lng, lat) and returns [lng, lat]; Leaflet wants [lat, lng].
      const [clLng, clLat] = clampMapCenter(initialCenter?.lon ?? 0, initialCenter?.lat ?? 0);
      const instance = L.map(containerRef.current, {
        preferCanvas: true,
        // Disable Leaflet's per-tile fade-in (opacity 0→1). On the Pi's software
        // renderer each freshly-painted tile otherwise fades from transparent —
        // reading as "loading from white space," worst on zoom (a whole new tile
        // set fades at once). This is the Leaflet analog of the old MapLibre
        // `fadeDuration: 0` llvmpipe mitigation (operator smoke, tuxlink-6kdw):
        // painted tiles snap in instead of fading.
        fadeAnimation: false,
        // Zoom control added explicitly top-RIGHT below (matching the old MapLibre
        // nav placement) so it does not collide with the app's top-left controls
        // (recenter/filter/SITREP) — Leaflet's default zoom is top-left (impl review).
        zoomControl: false,
        attributionControl: true,
        center: [clLat, clLng],
        zoom: initialZoom ?? DEFAULT_ZOOM,
        minZoom: MAP_MIN_ZOOM,
        maxZoom: MAP_MAX_ZOOM,
        worldCopyJump: false,
        // Native pan-clamp (R4 P1) — no maplibre maxBounds crash here, so the
        // manual moveend snap-back is unnecessary and weaker.
        maxBounds: L.latLngBounds([
          [-MERCATOR_MAX_LAT, -180],
          [MERCATOR_MAX_LAT, 180],
        ]),
        maxBoundsViscosity: 1.0,
      });
      // Single attribution source of truth (R4 P2): drop the Leaflet "Leaflet"
      // prefix; the OSM/ODbL credit comes from the overview layer's `attribution`
      // (set in basemapLeaflet) — no separate addAttribution to avoid duplicates.
      instance.attributionControl.setPrefix(false);
      L.control.zoom({ position: 'topright' }).addTo(instance);
      L.control.scale({ imperial: true, metric: true }).addTo(instance);

      instance.on('click', (e: L.LeafletMouseEvent) => {
        onClickRef.current?.(clampLatLon(e.latlng.lat, e.latlng.lng));
      });

      // Dedup'd zoom emit (a settle at unchanged zoom must not re-fire).
      let lastZoom = Number.NaN;
      const emitZoom = () => {
        const z = instance.getZoom();
        if (z === lastZoom) return;
        lastZoom = z;
        onZoomRef.current?.(z);
      };
      instance.on('moveend', emitZoom);

      // Viewport-persist emit (clamped center + zoom); skip non-finite transients.
      instance.on('moveend', () => {
        if (removed || !onViewportRef.current) return;
        const c = instance.getCenter();
        if (!Number.isFinite(c.lng) || !Number.isFinite(c.lat)) return;
        const [lng, lat] = clampMapCenter(c.lng, c.lat);
        onViewportRef.current({ lat, lon: lng }, instance.getZoom());
      });

      // Publish + seed zoom once the map is ready (whenReady fires synchronously
      // when constructed with center+zoom — R4 P2).
      instance.whenReady(() => {
        if (removed) return;
        setMap(instance);
        emitZoom();
      });

      return () => {
        removed = true;
        instance.remove();
      };
    } catch (e) {
      // WebGL/canvas unavailable or invalid construction → keep the failure local
      // (mirrors MapLibreMap's contained fallback).
      // eslint-disable-next-line no-console
      console.error('Leaflet map construction failed; rendering fallback:', e);
      setMapError(true);
      return;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- construct once; props read at mount, later changes handled by dedicated effects
  }, []);

  // Async-arrival recenter: a center that changes AFTER construction drives flyTo.
  // Skip ONLY the construct-time center (if present at mount, the constructor used it).
  const skipConstructCenter = useRef(Boolean(initialCenter));
  useEffect(() => {
    if (!map || !initialCenter) return;
    if (skipConstructCenter.current) {
      skipConstructCenter.current = false;
      return;
    }
    const [clLng, clLat] = clampMapCenter(initialCenter.lon, initialCenter.lat);
    map.flyTo([clLat, clLng]);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- primitive lat/lon deps so a fresh object with same coords does not re-fire
  }, [map, initialCenter?.lat, initialCenter?.lon]);

  // Base-layer (re)build on flavor OR pack change. Gate on the `map` STATE (R4 P1)
  // so it never mutates a torn-down instance. Dedup on the flavor|packIds key so a
  // redundant render does not rebuild. Leaflet has no setStyle — swap the layers.
  const baseLayersRef = useRef<L.Layer[]>([]);
  const styleKeyRef = useRef<string | null>(null);
  useEffect(() => {
    if (!map) return;
    const packKey = packs
      .map((p) => p.id)
      .slice()
      .sort()
      .join(',');
    const key = `${effectiveFlavor}|${packKey}`;
    if (styleKeyRef.current === key) return;
    styleKeyRef.current = key;
    for (const layer of baseLayersRef.current) map.removeLayer(layer);
    const next = buildBaseLayers(effectiveFlavor, packs);
    for (const layer of next) layer.addTo(map);
    baseLayersRef.current = next;
  }, [map, effectiveFlavor, packs]);

  if (mapError) {
    return (
      <div
        className="leaflet-unavailable"
        data-testid="map-unavailable"
        role="alert"
        style={{
          height: '100%',
          width: '100%',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          padding: '1rem',
          textAlign: 'center',
        }}
      >
        <span>The map could not be displayed on this system.</span>
      </div>
    );
  }

  return (
    <div ref={containerRef} style={{ height: '100%', width: '100%' }}>
      <LeafletMapProvider value={map}>{children}</LeafletMapProvider>
    </div>
  );
}
