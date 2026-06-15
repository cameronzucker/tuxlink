/**
 * LocationMap — the offline location-setup map (tuxlink-yy1m), MapLibre edition.
 *
 * Ported from the retired Leaflet BaseMap to the MapLibre stack (tuxlink-ndi4):
 * composes MapLibreMap + MaidenheadGridLayer and draws its own overlay (grid
 * square + a single marker) as GeoJSON circle/fill/line layers via useMapOverlay
 * — markers are GeoJSON, not maplibregl.Marker (CSP-safe, per the map subsystem).
 *
 * Behaviors (operator wire-walk flows):
 *  - GPS source selected + a live fix → marker sits at the PRECISE fix lat/lon
 *    ("you are here"); the live fix coords are local-display only, never broadcast.
 *  - Manual (or no fix) → marker at the grid-square center.
 *  - Click the map OR drag the marker → sets the location by hand (→ Manual),
 *    which is how the operator overrides a GPS fix (flow 3). Drag is the MapLibre
 *    draggable-point recipe (mousedown on the marker → dragPan.disable → mousemove
 *    updates the source → mouseup commits onGridChange).
 *
 * Real render/drag smoothness is grim-verified (the map subsystem's C1
 * convention); the tests prove wiring only via the global maplibre mock.
 */
import { useEffect, useMemo, useRef } from 'react';
import type { MapMouseEvent } from 'maplibre-gl';
import { MapLibreMap } from '../map/MapLibreMap';
import { MaidenheadGridLayer } from '../map/MaidenheadGridLayer';
import { useMapContext } from '../map/MapContext';
import { useMapOverlay } from '../map/mapHooks';
import { gridToLatLon, latLonToGrid } from '../forms/position/maidenhead';
import type { LatLon } from '../map/projection';

export interface LocationMapProps {
  /** Current grid — square highlight + manual-marker center. */
  grid: string;
  /** Raw live GPS fix coords for the precise marker, or null when no fresh fix. */
  fixLatLon: { lat: number; lon: number } | null;
  /** Picker selection id ('manual' | 'gpsd' | 'serial:...'). 'manual' → the
   *  marker follows the grid, so an arriving fix doesn't yank a hand-set pin. */
  selectedSource: string;
  /** Fired with the new grid when the operator clicks the map or drags the pin. */
  onGridChange: (grid: string) => void;
}

const SOURCE_ID = 'location-pin';
const MARKER_LAYER_ID = 'loc-pin-dot';

/** Grid-square half-widths (deg): 4-char = 2°×1°; 6-char = 5′×2.5′. */
const RECT_HALF = { lat6: 1.25 / 60, lon6: 2.5 / 60, lat4: 0.5, lon4: 1.0 };

type FeatureCollection = { type: 'FeatureCollection'; features: unknown[] };
const EMPTY_FC: FeatureCollection = { type: 'FeatureCollection', features: [] };

function squareFeature(grid: string, ll: LatLon): unknown {
  const is6 = grid.toUpperCase().length === 6;
  const halfLat = is6 ? RECT_HALF.lat6 : RECT_HALF.lat4;
  const halfLon = is6 ? RECT_HALF.lon6 : RECT_HALF.lon4;
  const s = ll.lat - halfLat;
  const n = ll.lat + halfLat;
  const w = ll.lon - halfLon;
  const e = ll.lon + halfLon;
  return {
    type: 'Feature',
    properties: { kind: 'square' },
    geometry: { type: 'Polygon', coordinates: [[[w, s], [e, s], [e, n], [w, n], [w, s]]] },
  };
}

/** Build the source data: the grid square (if a grid is set) + the marker point. */
function buildFC(grid: string, ll: LatLon | null, markerLngLat: [number, number] | null): FeatureCollection {
  const features: unknown[] = [];
  if (grid && ll) features.push(squareFeature(grid, ll));
  if (markerLngLat) {
    features.push({
      type: 'Feature',
      properties: { kind: 'marker' },
      geometry: { type: 'Point', coordinates: markerLngLat },
    });
  }
  return { type: 'FeatureCollection', features };
}

const LAYERS = (
  [
    {
      id: 'loc-square-fill',
      type: 'fill',
      source: SOURCE_ID,
      filter: ['==', ['get', 'kind'], 'square'],
      paint: { 'fill-color': '#5fd39a', 'fill-opacity': 0.1 },
    },
    {
      id: 'loc-square-line',
      type: 'line',
      source: SOURCE_ID,
      filter: ['==', ['get', 'kind'], 'square'],
      paint: { 'line-color': '#5fd39a', 'line-width': 2 },
    },
    {
      id: MARKER_LAYER_ID,
      type: 'circle',
      source: SOURCE_ID,
      filter: ['==', ['get', 'kind'], 'marker'],
      paint: { 'circle-radius': 7, 'circle-color': '#5fd39a', 'circle-stroke-color': '#0a1a2a', 'circle-stroke-width': 2 },
    },
  ] as unknown[]
).map((l) => l as Record<string, unknown> & { id: string });

function LocationOverlay({ grid, fixLatLon, selectedSource, onGridChange }: LocationMapProps) {
  const map = useMapContext();
  const ll = grid ? gridToLatLon(grid) : null;
  const showFix = selectedSource !== 'manual' && fixLatLon != null;
  const markerLngLat: [number, number] | null = showFix
    ? [fixLatLon!.lon, fixLatLon!.lat]
    : ll
      ? [ll.lon, ll.lat]
      : null;

  const fc = useMemo(
    () => buildFC(grid, ll, markerLngLat),
    [grid, ll?.lat, ll?.lon, markerLngLat?.[0], markerLngLat?.[1]],
  );

  useMapOverlay(map, SOURCE_ID, { type: 'geojson', data: EMPTY_FC }, LAYERS);

  // Subscribe `styledata` ONCE and push the latest data from a ref (B10,
  // tuxlink-vnk7) — the old effect re-subscribed on every `fc` change.
  const fcRef = useRef(fc);
  fcRef.current = fc;
  useEffect(() => {
    if (!map) return;
    const push = () => {
      const src = map.getSource(SOURCE_ID) as { setData?: (d: unknown) => void } | undefined;
      src?.setData?.(fcRef.current);
    };
    map.on('styledata', push);
    return () => {
      map.off('styledata', push);
    };
  }, [map]);

  // Push imperatively whenever the data actually changes.
  useEffect(() => {
    if (!map) return;
    const src = map.getSource(SOURCE_ID) as { setData?: (d: unknown) => void } | undefined;
    src?.setData?.(fc);
  }, [map, fc]);

  // Draggable marker (flow 3): drag the pin to set the location by hand. Standard
  // MapLibre draggable-point recipe — grab on the marker layer, move the point
  // live, commit on release (→ Manual via onGridChange).
  useEffect(() => {
    if (!map) return;
    let dragging = false;
    // Coalesce the live drag preview to one setData per animation frame (B10,
    // tuxlink-vnk7) — pointer mousemove can fire faster than the (software-GL)
    // render cadence, so an un-throttled setData rebuilt the GeoJSON many times
    // per frame while the map was already CPU-limited.
    let rafId: number | null = null;
    let pending: [number, number] | null = null;
    const flushPreview = () => {
      rafId = null;
      if (!pending) return;
      const src = map.getSource(SOURCE_ID) as { setData?: (d: unknown) => void } | undefined;
      src?.setData?.(buildFC(grid, ll, pending));
      pending = null;
    };
    const cancelPreview = () => {
      if (rafId != null) {
        cancelAnimationFrame(rafId);
        rafId = null;
      }
      pending = null;
    };
    const setCursor = (c: string) => {
      try {
        map.getCanvas().style.cursor = c;
      } catch {
        /* jsdom canvas has no style in some envs */
      }
    };
    const onEnter = () => setCursor('move');
    const onLeave = () => {
      if (!dragging) setCursor('');
    };
    const onDown = (e: MapMouseEvent) => {
      e.preventDefault?.();
      dragging = true;
      map.dragPan.disable();
    };
    const onMove = (e: MapMouseEvent) => {
      if (!dragging) return;
      pending = [e.lngLat.lng, e.lngLat.lat];
      if (rafId == null) rafId = requestAnimationFrame(flushPreview);
    };
    const onUp = (e: MapMouseEvent) => {
      if (!dragging) return;
      dragging = false;
      cancelPreview();
      map.dragPan.enable();
      setCursor('');
      onGridChange(latLonToGrid(e.lngLat.lat, e.lngLat.lng));
    };
    map.on('mouseenter', MARKER_LAYER_ID, onEnter);
    map.on('mouseleave', MARKER_LAYER_ID, onLeave);
    map.on('mousedown', MARKER_LAYER_ID, onDown);
    map.on('mousemove', onMove);
    map.on('mouseup', onUp);
    return () => {
      cancelPreview();
      map.off('mouseenter', MARKER_LAYER_ID, onEnter);
      map.off('mouseleave', MARKER_LAYER_ID, onLeave);
      map.off('mousedown', MARKER_LAYER_ID, onDown);
      map.off('mousemove', onMove);
      map.off('mouseup', onUp);
    };
  }, [map, grid, ll?.lat, ll?.lon, onGridChange]);

  return null;
}

export function LocationMap({ grid, fixLatLon, selectedSource, onGridChange }: LocationMapProps) {
  const ll = grid ? gridToLatLon(grid) : null;
  const showFix = selectedSource !== 'manual' && fixLatLon != null;
  const center: LatLon | undefined = (showFix ? fixLatLon : ll) ?? undefined;
  return (
    <div className="location-map" data-testid="location-map">
      <MapLibreMap
        onMapClick={({ lat, lon }) => onGridChange(latLonToGrid(lat, lon))}
        initialCenter={center}
        initialZoom={center ? 6 : 2}
      >
        <MaidenheadGridLayer visible />
        <LocationOverlay
          grid={grid}
          fixLatLon={fixLatLon}
          selectedSource={selectedSource}
          onGridChange={onGridChange}
        />
      </MapLibreMap>
    </div>
  );
}
