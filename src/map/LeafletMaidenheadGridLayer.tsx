/**
 * LeafletMaidenheadGridLayer — the Leaflet re-expression of MaidenheadGridLayer
 * (tuxlink-4hol; strangler-fig twin). Draws the Maidenhead lattice (lat/lon lines)
 * + per-cell locator labels as Leaflet vectors + label markers on an explicit
 * `L.svg()` renderer, via the owned LayerGroup.
 *
 * Self-driving: reads the map's bounds + zoom and recomputes the lattice on
 * `moveend`, choosing field/square/subsquare granularity by zoom. `bounds`/`level`
 * props override the map-derived values (controlled / testing). The line/label
 * GEOMETRY is the pure `gridGeometry` (jsdom-tested); render correctness is
 * grim-only.
 *
 * Unlike the MapLibre original this needs NO `styledata` re-push: a Leaflet
 * basemap flavor/pack swap replaces the BASE tile layer(s) and leaves overlay
 * layers untouched (see leafletHooks.ts). The B6 recompute gating (regenerate
 * only when the level changes or the view leaves the padded extent) is preserved.
 */
import { useEffect, useRef } from 'react';
import L from 'leaflet';
import { useLeafletMap } from './LeafletMapContext';
import { useLeafletLayerGroup } from './leafletHooks';
import { reportFrontendError } from '../frontendErrorLog';
import {
  gridLines,
  levelFromZoom,
  GridLevel,
  type GridBounds,
  type GridLinesResult,
} from './gridGeometry';
import './LeafletMaidenheadGridLayer.css';

export interface LeafletMaidenheadGridLayerProps {
  visible?: boolean;
  /** Override the visible bounds (else derived from the map). */
  bounds?: GridBounds;
  /** Override the grid level (else derived from the map zoom). */
  level?: GridLevel;
}

const LINE_STYLE: L.PolylineOptions = {
  color: '#64748b',
  weight: 1,
  opacity: 0.5,
  interactive: false,
};

const esc = (s: string): string =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** A cell-label divIcon: the locator prefix, halo'd for basemap contrast and
 * centred on the cell centre. Styled via the `.maidenhead-grid-label` CSS class,
 * NOT an inline `style` attribute — the production Tauri CSP nonces `style-src`,
 * which makes `'unsafe-inline'` inert and blocks parsed inline styles in divIcon
 * html (set via innerHTML), so an inline style here is dropped (tuxlink-ivfr). */
function labelIcon(text: string): L.DivIcon {
  const html = `<span class="maidenhead-grid-label" data-grid-label="${esc(text)}">${esc(text)}</span>`;
  return L.divIcon({ className: 'maidenhead-grid-label-icon', html, iconSize: [0, 0], iconAnchor: [0, 0] });
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
 * lattice covers ~2× the visible area and a small pan stays inside it (B6). */
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

/** Render the lattice + labels for `bounds`/`level` into `group`. */
function draw(group: L.LayerGroup, renderer: L.Renderer, bounds: GridBounds, result: GridLinesResult): void {
  const { lonLines, latLines, labels } = result;
  for (const lon of lonLines) {
    L.polyline(
      [
        [bounds.south, lon],
        [bounds.north, lon],
      ],
      { ...LINE_STYLE, renderer },
    ).addTo(group);
  }
  for (const lat of latLines) {
    L.polyline(
      [
        [lat, bounds.west],
        [lat, bounds.east],
      ],
      { ...LINE_STYLE, renderer },
    ).addTo(group);
  }
  for (const label of labels) {
    L.marker([label.lat, label.lon], { icon: labelIcon(label.text), interactive: false, keyboard: false }).addTo(group);
  }
}

export function LeafletMaidenheadGridLayer({ visible = true, bounds, level }: LeafletMaidenheadGridLayerProps) {
  const map = useLeafletMap();
  const group = useLeafletLayerGroup(map);
  const rendererRef = useRef<L.Renderer | null>(null);
  if (!rendererRef.current) rendererRef.current = L.svg({ padding: 2 });

  // The (level, padded-extent) the current lattice was generated for. We only
  // regenerate when the level changes or the view leaves the padded extent (B6) —
  // NOT on every moveend, which would re-tessellate the whole lattice on each pan.
  const genRef = useRef<{ level: GridLevel; padded: GridBounds } | null>(null);

  useEffect(() => {
    if (!map || !group) return;
    const renderer = rendererRef.current!;
    const recompute = () => {
      try {
        const b = map.getBounds();
        const effBounds: GridBounds | null =
          bounds ?? { south: b.getSouth(), west: b.getWest(), north: b.getNorth(), east: b.getEast() };
        const effLevel: GridLevel | null = level ?? levelFromZoom(map.getZoom());
        if (!visible || !effBounds || effLevel === null) {
          genRef.current = null;
          group.clearLayers();
          return;
        }
        const g = genRef.current;
        if (g && g.level === effLevel && contains(g.padded, effBounds)) return; // no change
        const padded = padBounds(effBounds);
        genRef.current = { level: effLevel, padded };
        group.clearLayers();
        draw(group, renderer, padded, gridLines(padded, effLevel));
      } catch (e) {
        reportFrontendError(
          'maidenhead-grid-layer',
          `recompute: ${e instanceof Error ? e.message : String(e)}`,
          e instanceof Error ? e.stack : undefined,
        );
      }
    };
    recompute();
    map.on('moveend', recompute);
    return () => {
      map.off('moveend', recompute);
    };
  }, [map, group, visible, bounds, level]);

  return null;
}
