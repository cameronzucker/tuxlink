// FT-8 heat layer on the finder map (Task 5, spec L5 traffic map): a
// grid-square choropleth of decode density, one rectangle per 4-char
// Maidenhead square that had at least one heard station in the same live
// aggregation window Ft8HeardLayer draws from. Default OFF (an operator
// opt-in overview layer, not evidence like the heard-station dots). Consumes
// the SAME `rows` Ft8HeardLayer's caller already computes via
// `aggregateLiveDecodes`; this component never re-aggregates the raw ring.
//
// Mirrors Ft8HeardLayer's structural template: a full clear + rebuild each
// time `rows`/`enabled` changes (cheap at the ring's aggregation cap), and
// its own lazily-created `L.svg()` renderer rather than a shared rendererRef
// (there is no shared renderer across this panel's layers; Task 4's
// reviewer confirmed this per-layer-owned-renderer idiom is the established
// pattern). Rectangles ride the SVG path renderer explicitly because the
// map is constructed with `preferCanvas: true` (LeafletMap.tsx): without an
// explicit SVG renderer a plain `L.rectangle` would default to canvas, which
// has no 2D context under the Pi's software-GL WebKitGTK.
import { useEffect, useRef } from 'react';
import L from 'leaflet';
import { useLeafletMap } from './LeafletMapContext';
import { useLeafletLayerGroup } from './leafletHooks';
import { gridToLatLon } from '../forms/position/maidenhead';
import type { LiveDecodeRow } from '../catalog/LiveDecodesTab';

/** Fill colour for every heat cell: the same HOT ramp colour Ft8HeardLayer
 *  uses at its brightest, so the two FT-8 layers read as one evidence family. */
const HEAT_FILL_COLOR = '#ff5470';
/** Density-scaled fillOpacity floor: even a single-station square stays visible. */
const HEAT_OPACITY_FLOOR = 0.15;
/** Density-scaled fillOpacity span added on top of the floor at max density. */
const HEAT_OPACITY_SPAN = 0.55;

/**
 * The SW/NE corners of the 4-char Maidenhead square `grid4` covers (2 deg
 * lon x 1 deg lat), as `[[swLat, swLon], [neLat, neLon]]` ready for
 * `L.rectangle`. Derived from `gridToLatLon`'s CENTER-of-square result
 * (never re-derives the Maidenhead math from scratch) by offsetting half a
 * square width/height in each direction. Returns `null` for anything that
 * is not exactly 4 characters or that `gridToLatLon` itself rejects
 * (out-of-range field/square characters).
 */
export function gridSquareBounds(grid4: string): [[number, number], [number, number]] | null {
  const g = grid4.toUpperCase();
  if (g.length !== 4) return null;
  const center = gridToLatLon(g);
  if (!center) return null;
  return [
    [center.lat - 0.5, center.lon - 1],
    [center.lat + 0.5, center.lon + 1],
  ];
}

export interface Ft8HeatLayerProps {
  /** Pre-aggregated by the caller (`aggregateLiveDecodes`), the SAME rows
   *  object Ft8HeardLayer receives; this component never reads the raw
   *  decode ring. A row with no grid contributes to no square. */
  rows: LiveDecodeRow[];
  /** Layer-control visibility: false renders no rectangles at all, not a
   *  dimmed layer (mirrors Ft8HeardLayer/StationLayers). */
  enabled: boolean;
}

export function Ft8HeatLayer({ rows, enabled }: Ft8HeatLayerProps): null {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  const rendererRef = useRef<L.Renderer | null>(null);
  if (!rendererRef.current) rendererRef.current = L.svg({ padding: 2 });

  useEffect(() => {
    if (!group) return;
    group.clearLayers();
    if (enabled) {
      const countBySquare = new Map<string, number>();
      for (const row of rows) {
        if (!row.grid) continue; // no grid heard yet, contributes to no square
        const sq = row.grid.slice(0, 4).toUpperCase();
        countBySquare.set(sq, (countBySquare.get(sq) ?? 0) + 1);
      }
      let maxCount = 0;
      for (const count of countBySquare.values()) maxCount = Math.max(maxCount, count);

      if (maxCount > 0) {
        for (const [sq, count] of countBySquare) {
          const bounds = gridSquareBounds(sq);
          if (!bounds) continue; // malformed/garbage grid, never throws, never plots
          const rect = L.rectangle(bounds, {
            renderer: rendererRef.current ?? undefined,
            stroke: false,
            fillColor: HEAT_FILL_COLOR,
            fillOpacity: HEAT_OPACITY_FLOOR + HEAT_OPACITY_SPAN * (count / maxCount),
          });
          group.addLayer(rect);
        }
      }
    }
    return () => {
      group.clearLayers();
    };
  }, [group, rows, enabled]);

  return null;
}
