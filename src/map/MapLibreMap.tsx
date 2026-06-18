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
import { clampLatLon, clampMapCenter, type LatLon } from './projection';
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

// Last-known installed packs, cached at module scope across mounts (B2,
// tuxlink-vnk7). The map remounts whenever the operator navigates to a
// map-bearing surface; without this, EVERY mount constructed overview-only and
// then fired a full `setStyle` once `basemap_list_packs` resolved. Caching the
// last result lets a remount construct WITH packs already known, so the async
// resolution is a no-op (key unchanged) instead of a teardown/rebuild. The
// first-ever mount (cache empty) is unchanged. A genuine pack install/delete
// still flips the key and rebuilds — that is correct and rare.
let lastKnownPacks: PackSource[] = [];

/** Min interactive zoom (whole world fits at ~z1 on the z0–14 scale). */
const MAP_MIN_ZOOM = 0;
/** Max interactive zoom — region packs carry z0–14; the overview overzooms past z6. */
const MAP_MAX_ZOOM = 14;
/** Default world view on the z0–14 fractional scale (was raster-native z1; finding 2). */
const DEFAULT_ZOOM = 2;
// NOTE (tuxlink-rwo6): `maxBounds` is deliberately NOT set. maplibre-gl 5.24.0
// crashes during construction when any camera-bounds constraint is applied
// (constructor `maxBounds` OR a later `setMaxBounds`) on this build's
// WebKitGTK/ANGLE WebGL context: `_calcMatrices` dereferences a null
// ("null is not an object (evaluating 'n[0]')") via constrainInternal→setZoom.
// That throw is what ErrorBoundary surfaced as "map cannot be displayed on this
// system" (the map was BRICKED on the Pi). Reproduced in real WebKit2GTK 4.1 with
// the real style; WebGL1+WebGL2 both work, so it is NOT a WebGL/CSP/HW-accel
// issue — it is a maplibre 5.24.0 regression in the bounds-constraint path. The
// map constructs + loads cleanly without bounds; the only loss is pan-into-void
// past the world edges (cosmetic). Restoring a constraint (maplibre upgrade/pin
// or a manual `moveend` center-clamp) is tracked in tuxlink-rwo6's follow-up.

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
  /** Called on every camera settle (moveend) with the clamped center + zoom, so
   *  a consumer can persist the viewport and restore it next mount (tuxlink-dwzu).
   *  The center is soft-clamped to the world rectangle; a non-finite transient
   *  (teardown) is skipped. */
  onViewportChange?: (center: LatLon, zoom: number) => void;
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
  onViewportChange,
  flavor,
}: MapLibreMapProps) {
  // Follow the app color scheme unless an explicit flavor is passed.
  const themeFlavor = useBasemapFlavor();
  const effectiveFlavor = flavor ?? themeFlavor;
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [map, setMap] = useState<maplibregl.Map | null>(null);
  // tuxlink-52h6: `new maplibregl.Map()` throws synchronously when WebGL is
  // unavailable (the WebKitGTK case behind the 0.60.0 blank location screen) or
  // the style is invalid. Catch it (below) and degrade to a local "unavailable"
  // panel so the failure stays contained — the consumer's surrounding UI keeps
  // working instead of the whole screen unmounting.
  const [mapError, setMapError] = useState(false);

  // Latest callbacks held in refs so the construct-once effect never re-runs on
  // a changed callback identity.
  const onClickRef = useRef(onMapClick);
  onClickRef.current = onMapClick;
  const onZoomRef = useRef(onZoomChange);
  onZoomRef.current = onZoomChange;
  const onViewportRef = useRef(onViewportChange);
  onViewportRef.current = onViewportChange;
  // Tracks the flavor currently applied to the map (seeded at construction).
  const flavorRef = useRef(effectiveFlavor);

  // Installed region packs composited over the world overview (R7). Fetched after
  // mount (the construct-time style uses the overview only) and re-fetched when the
  // pack manager signals a change; a change drives setStyle in the rebuild effect.
  const [packs, setPacks] = useState<PackSource[]>(() => lastKnownPacks);
  // The packs known at THIS mount's construction (the module cache snapshot).
  // The construct effect builds the style with these and seeds the style-key, so
  // a later identical fetch is a no-op rather than a redundant setStyle (B2).
  const constructPacksRef = useRef(packs);
  const fetchPacks = useCallback(async () => {
    try {
      const list = await invoke<PacksList>('basemap_list_packs');
      const next = list.packs.map((p) => ({ id: p.id }));
      lastKnownPacks = next; // cache for the next mount's construction (B2)
      setPacks(next);
    } catch {
      // No backend (e.g. unit test / dev without the command) → overview only.
      // Invalidate the cache too (Codex P3): a stale pack in `lastKnownPacks`
      // would otherwise make the NEXT remount construct with a pack that may have
      // been deleted — a stale-pack first paint is worse than overview-only.
      lastKnownPacks = [];
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
    try {
      const instance = new maplibregl.Map({
        container: containerRef.current,
        // Construct WITH the packs known at mount (B2) — the module cache makes a
        // remount carry packs so the async fetchPacks resolution is a no-op.
        style: buildBasemapStyle(flavorRef.current, constructPacksRef.current),
        // Clamp the initial center to the displayable world so a bad GPS / catalog
        // coordinate can't start the camera off-map (tuxlink-rwo6).
        center: clampMapCenter(initialCenter?.lon ?? 0, initialCenter?.lat ?? 0),
        zoom: initialZoom ?? DEFAULT_ZOOM,
        minZoom: MAP_MIN_ZOOM,
        maxZoom: MAP_MAX_ZOOM,
        // maxBounds intentionally omitted — see the MAP_MAX_BOUNDS note above
        // (maplibre 5.24.0 bounds-constraint crash, tuxlink-rwo6).
        renderWorldCopies: false,
        // Software-GL (llvmpipe) render profile (B7, tuxlink-vnk7). pixelRatio:1
        // avoids the quadratic fill cost of a HiDPI canvas the CPU rasterizer
        // can't afford; fadeDuration:0 drops per-tile/symbol cross-fade passes
        // during loads. Standard software-GL mitigations, safe on this target.
        pixelRatio: 1,
        fadeDuration: 0,
        // We add the AttributionControl explicitly so "© OpenStreetMap
        // contributors" (ODbL) renders from the source attribution.
        attributionControl: false,
      });
      instance.addControl(new maplibregl.AttributionControl({ compact: false }));
      instance.addControl(new maplibregl.NavigationControl({ showCompass: false }), 'top-right');
      // Distance scale (tuxlink-hzwc bug #7). The ham audience is mixed-unit, so
      // show both an imperial and a metric bar, stacked bottom-left. `maxWidth`
      // keeps the bar compact in the narrow APRS map pane.
      instance.addControl(new maplibregl.ScaleControl({ maxWidth: 110, unit: 'imperial' }), 'bottom-left');
      instance.addControl(new maplibregl.ScaleControl({ maxWidth: 110, unit: 'metric' }), 'bottom-left');

      // Set in the cleanup BEFORE instance.remove() so a moveend fired
      // synchronously during teardown can't touch the dying map (tuxlink-dvfh).
      let mapRemoved = false;

      instance.on('click', (e: maplibregl.MapMouseEvent) => {
        onClickRef.current?.(clampLatLon(e.lngLat.lat, e.lngLat.lng));
      });
      // Emit only on a REAL zoom change (B8, tuxlink-vnk7). `moveend` fires after
      // pans too, so emitting unconditionally re-rendered the consumer subtree
      // (e.g. the position picker modal) at the end of every drag. Dedupe against
      // the last emitted zoom; `load` seeds it (NaN !== initial → fires once).
      let lastZoom = Number.NaN;
      const emitZoom = () => {
        const z = instance.getZoom();
        if (z === lastZoom) return;
        lastZoom = z;
        onZoomRef.current?.(z);
      };
      instance.on('load', () => {
        setMap(instance);
        emitZoom();
        // tuxlink-4pdu: the map reached a loaded WebGL context on this launch —
        // clear the hardware-GL safe-mode marker so a hardware attempt that DID
        // render is not second-guessed next launch. Best-effort; harmless no-op in
        // software mode / off-Linux.
        void invoke('gl_render_confirmed').catch(() => {});
      });
      instance.on('moveend', emitZoom);
      // Restore the pan-constraint dropped with maxBounds (which crashes maplibre
      // 5.24.0 on this WebKitGTK build — tuxlink-rwo6): with renderWorldCopies off,
      // the center can pan past the antimeridian into gray void. Soft-clamp it back
      // on moveend. Uses setCenter (NOT setMaxBounds, the crash path); the snap-back
      // re-fires moveend, but the now-in-world center clamps to itself → no loop.
      instance.on('moveend', () => {
        if (mapRemoved) return;
        const c = instance.getCenter();
        // A degenerate transform (during teardown on modal close, or any transient
        // bad camera state) can yield a non-finite center. clampMapCenter would
        // propagate NaN and the `!==` check treats NaN as "changed", so
        // setCenter([NaN,NaN]) would throw maplibre's "Invalid LngLat" and crash
        // the React tree via the app ErrorBoundary (tuxlink-dvfh). Bail on it.
        if (!Number.isFinite(c.lng) || !Number.isFinite(c.lat)) return;
        const [lng, lat] = clampMapCenter(c.lng, c.lat);
        if (lng !== c.lng || lat !== c.lat) {
          instance.setCenter([lng, lat]);
        }
      });
      // Persist-the-viewport hook (tuxlink-dwzu): emit the clamped center + zoom
      // on every settle so a consumer can remember where the operator left the
      // map. Registered AFTER the soft-clamp above so the emitted center is the
      // in-world one (the clamp's snap-back re-fires moveend → final emit is
      // clamped). A non-finite transient (teardown) is skipped, like the clamp.
      instance.on('moveend', () => {
        if (mapRemoved) return;
        if (!onViewportRef.current) return;
        const c = instance.getCenter();
        if (!Number.isFinite(c.lng) || !Number.isFinite(c.lat)) return;
        const [lng, lat] = clampMapCenter(c.lng, c.lat);
        onViewportRef.current({ lat, lon: lng }, instance.getZoom());
      });

      return () => {
        mapRemoved = true;
        instance.remove();
      };
    } catch (e) {
      // WebGL unavailable / invalid style → keep the failure local (tuxlink-52h6).
      // eslint-disable-next-line no-console
      console.error('MapLibre map construction failed; rendering fallback:', e);
      setMapError(true);
      return;
    }
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
    map.flyTo({ center: clampMapCenter(initialCenter.lon, initialCenter.lat) });
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
    const packKey = (ps: PackSource[]) => ps.map((p) => p.id).slice().sort().join(',');
    const key = `${effectiveFlavor}|${packKey(packs)}`;
    // Seed from what the CONSTRUCTOR actually used (flavor + construct-time packs,
    // B2). If the async fetch resolves to the same packs the cache gave the
    // constructor, the key matches and no setStyle fires.
    if (styleKeyRef.current === null) {
      styleKeyRef.current = `${effectiveFlavor}|${packKey(constructPacksRef.current)}`;
    }
    if (styleKeyRef.current === key) return;
    styleKeyRef.current = key;
    flavorRef.current = effectiveFlavor;
    map.setStyle(buildBasemapStyle(effectiveFlavor, packs));
  }, [map, effectiveFlavor, packs]);

  // tuxlink-52h6: a construction failure degrades to a contained panel rather
  // than propagating (which, with no error boundary above, blanked the whole
  // app in 0.60.0). The consumer's surrounding chrome — grid input, controls —
  // keeps rendering because the throw never escaped this component.
  if (mapError) {
    return (
      <div
        className="maplibre-unavailable"
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
      <MapProvider value={map}>{children}</MapProvider>
    </div>
  );
}
