/**
 * MaidenheadGridLayer — the MapLibre re-expression of MaidenheadOverlay
 * (tuxlink-ndi4, plan phase 2). Draws the Maidenhead lattice + cell labels as a
 * GeoJSON source with a line layer + a symbol (text) layer, via the owned hook.
 *
 * Self-driving: reads the map's bounds + zoom and recomputes the lattice on
 * `moveend`, choosing field/square/subsquare granularity by zoom. `bounds`/`level`
 * props override the map-derived values (controlled / testing). The line/label
 * GEOMETRY is the pure gridGeometry (jsdom-tested); render correctness is
 * grim-only (C1) — do NOT assert coordinates through the mock.
 *
 * Visibility is data-driven: when hidden (or above the lattice's zoom range) the
 * source is fed an empty collection rather than toggling layer presence, so the
 * layers stay stable and the source just empties.
 */
import { useEffect, useRef, useState } from 'react';
import { useMapContext } from './MapContext';
import { useMapOverlay } from './mapHooks';
import {
  gridLines,
  levelFromZoom,
  GridLevel,
  type GridBounds,
  type GridLinesResult,
} from './gridGeometry';

export interface MaidenheadGridLayerProps {
  visible?: boolean;
  /** Override the visible bounds (else derived from the map). */
  bounds?: GridBounds;
  /** Override the grid level (else derived from the map zoom). */
  level?: GridLevel;
}

export const GRID_SOURCE_ID = 'maidenhead-grid';

const LINE_LAYER = {
  id: 'maidenhead-lines',
  type: 'line',
  source: GRID_SOURCE_ID,
  filter: ['==', ['get', 'kind'], 'line'],
  paint: { 'line-color': '#64748b', 'line-width': 1, 'line-opacity': 0.5 },
} as const;

const LABEL_LAYER = {
  id: 'maidenhead-labels',
  type: 'symbol',
  source: GRID_SOURCE_ID,
  filter: ['==', ['get', 'kind'], 'label'],
  layout: {
    'text-field': ['get', 'text'],
    'text-font': ['Noto Sans Regular'],
    'text-size': 12,
    // Collision culling left ON (B6, tuxlink-vnk7): forcing
    // text-allow-overlap/text-ignore-placement made the software rasterizer draw
    // EVERY overlapping cell label. MapLibre's default placement drops occluded
    // labels — a large fill-rate win at wide zoom where cell count peaks.
  },
  paint: { 'text-color': '#475569', 'text-halo-color': '#ffffff', 'text-halo-width': 1 },
} as const;

const GRID_LAYERS = [
  LINE_LAYER as unknown as Record<string, unknown> & { id: string },
  LABEL_LAYER as unknown as Record<string, unknown> & { id: string },
];

type FeatureCollection = { type: 'FeatureCollection'; features: unknown[] };
const EMPTY_FC: FeatureCollection = { type: 'FeatureCollection', features: [] };

function gridToGeoJSON(bounds: GridBounds, { lonLines, latLines, labels }: GridLinesResult): FeatureCollection {
  const features: unknown[] = [];
  for (const lon of lonLines) {
    features.push({
      type: 'Feature',
      properties: { kind: 'line' },
      geometry: { type: 'LineString', coordinates: [[lon, bounds.south], [lon, bounds.north]] },
    });
  }
  for (const lat of latLines) {
    features.push({
      type: 'Feature',
      properties: { kind: 'line' },
      geometry: { type: 'LineString', coordinates: [[bounds.west, lat], [bounds.east, lat]] },
    });
  }
  for (const label of labels) {
    features.push({
      type: 'Feature',
      properties: { kind: 'label', text: label.text },
      geometry: { type: 'Point', coordinates: [label.lon, label.lat] },
    });
  }
  return { type: 'FeatureCollection', features };
}

/** True when `inner` lies entirely within `outer`. */
function contains(outer: GridBounds, inner: GridBounds): boolean {
  return (
    inner.south >= outer.south &&
    inner.north <= outer.north &&
    inner.west >= outer.west &&
    inner.east <= outer.east
  );
}

/** Expand bounds by half its span on each side (clamped to ±90 lat), so the
 * lattice is generated for ~2× the visible area and a small pan stays inside it
 * (B6 — avoids regenerating on every moveend). */
function padBounds(b: GridBounds): GridBounds {
  const dLat = (b.north - b.south) / 2;
  const dLon = (b.east - b.west) / 2;
  return {
    south: Math.max(-90, b.south - dLat),
    north: Math.min(90, b.north + dLat),
    west: b.west - dLon,
    east: b.east + dLon,
  };
}

export function MaidenheadGridLayer({ visible = true, bounds, level }: MaidenheadGridLayerProps) {
  const map = useMapContext();
  const [geojson, setGeojson] = useState<FeatureCollection>(EMPTY_FC);
  const geojsonRef = useRef(geojson);
  geojsonRef.current = geojson;

  // The (level, padded-extent) the current lattice was generated for. We only
  // regenerate when the level changes or the view leaves the padded extent (B6,
  // tuxlink-vnk7) — NOT on every moveend, which re-tessellated the whole lattice
  // on every pan even when no cell boundary was crossed.
  const genRef = useRef<{ level: GridLevel; padded: GridBounds } | null>(null);

  // Source + layers are always present (cheap when empty); the owned hook keeps
  // them across style swaps with correct teardown order.
  useMapOverlay(map, GRID_SOURCE_ID, { type: 'geojson', data: EMPTY_FC }, GRID_LAYERS);

  // Recompute the lattice on pan/zoom, but only when it would actually change.
  useEffect(() => {
    if (!map) return;
    const recompute = () => {
      const effBounds: GridBounds | null =
        bounds ?? {
          south: map.getBounds().getSouth(),
          west: map.getBounds().getWest(),
          north: map.getBounds().getNorth(),
          east: map.getBounds().getEast(),
        };
      const effLevel: GridLevel | null = level ?? levelFromZoom(map.getZoom());
      if (!visible || !effBounds || effLevel === null) {
        genRef.current = null;
        setGeojson(EMPTY_FC);
        return;
      }
      const g = genRef.current;
      if (g && g.level === effLevel && contains(g.padded, effBounds)) return; // no change
      const padded = padBounds(effBounds);
      genRef.current = { level: effLevel, padded };
      setGeojson(gridToGeoJSON(padded, gridLines(padded, effLevel)));
    };
    recompute();
    map.on('moveend', recompute);
    return () => {
      map.off('moveend', recompute);
    };
  }, [map, visible, bounds, level]);

  // Re-push on `styledata` (a style swap drops the source; the hook re-adds it
  // empty on the same tick). Subscribe ONCE and read the latest lattice from a
  // ref (B6) — the old effect re-subscribed on every geojson change.
  useEffect(() => {
    if (!map) return;
    const push = () => {
      const src = map.getSource(GRID_SOURCE_ID) as { setData?: (d: unknown) => void } | undefined;
      src?.setData?.(geojsonRef.current);
    };
    map.on('styledata', push);
    return () => {
      map.off('styledata', push);
    };
  }, [map]);

  // Push imperatively whenever the lattice actually changes.
  useEffect(() => {
    if (!map) return;
    const src = map.getSource(GRID_SOURCE_ID) as { setData?: (d: unknown) => void } | undefined;
    src?.setData?.(geojson);
  }, [map, geojson]);

  return null;
}
