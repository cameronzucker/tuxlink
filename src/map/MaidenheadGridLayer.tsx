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
import { useEffect, useMemo, useState } from 'react';
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
    'text-allow-overlap': true,
    'text-ignore-placement': true,
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

export function MaidenheadGridLayer({ visible = true, bounds, level }: MaidenheadGridLayerProps) {
  const map = useMapContext();
  // Recompute on pan/zoom by bumping a tick; the map is the source of truth.
  const [, setTick] = useState(0);
  useEffect(() => {
    if (!map) return;
    const bump = () => setTick((t) => t + 1);
    map.on('moveend', bump);
    return () => {
      map.off('moveend', bump);
    };
  }, [map]);

  const effBounds: GridBounds | null =
    bounds ??
    (map
      ? (() => {
          const b = map.getBounds();
          return { south: b.getSouth(), west: b.getWest(), north: b.getNorth(), east: b.getEast() };
        })()
      : null);
  const effLevel: GridLevel | null = level ?? (map ? levelFromZoom(map.getZoom()) : null);

  const geojson = useMemo<FeatureCollection>(() => {
    if (!visible || !effBounds || effLevel === null) return EMPTY_FC;
    return gridToGeoJSON(effBounds, gridLines(effBounds, effLevel));
  }, [visible, effBounds?.south, effBounds?.west, effBounds?.north, effBounds?.east, effLevel]);

  // Source + layers are always present (cheap when empty); the owned hook keeps
  // them across style swaps with correct teardown order.
  useMapOverlay(map, GRID_SOURCE_ID, { type: 'geojson', data: EMPTY_FC }, GRID_LAYERS);

  // Push the current lattice. Re-push on `styledata` too: a style swap drops the
  // source, and the hook re-adds it (empty) on the same event — this restores
  // the data on that tick. setData is idempotent.
  useEffect(() => {
    if (!map) return;
    const push = () => {
      const src = map.getSource(GRID_SOURCE_ID) as { setData?: (d: unknown) => void } | undefined;
      src?.setData?.(geojson);
    };
    push();
    map.on('styledata', push);
    return () => {
      map.off('styledata', push);
    };
  }, [map, geojson]);

  return null;
}
