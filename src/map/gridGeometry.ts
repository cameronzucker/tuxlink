/**
 * Pure Maidenhead grid-overlay geometry for the offline map.
 *
 * Given the visible map bounds and a grid level, returns the lat/lon lines to
 * draw and the per-cell locator labels. This is pure math (no Leaflet, no DOM)
 * so it is unit-tested in jsdom; the overlay component (MaidenheadOverlay)
 * renders the result as SVG polylines + labels and is verified via grim.
 *
 * Labels reuse the ONE Maidenhead converter (`../forms/position/maidenhead`):
 * a cell's label is the locator of its CENTER, truncated to the field (2 chars)
 * or square (4 chars) prefix.
 */
import { latLonToGrid } from '../forms/position/maidenhead';

export enum GridLevel {
  /** 20°×10° fields, labelled by the 2-char field prefix (e.g. `JJ`). */
  Field = 'field',
  /** 2°×1° squares, labelled by the 4-char field+square prefix (e.g. `JJ00`). */
  Square = 'square',
  /**
   * 5′×2.5′ subsquares, labelled by the full 6-char locator (e.g. `JJ00aa`).
   * Used in the high-zoom band (z9-13) the LAN tile layer unlocks — without it
   * a single coarse square would span the whole z14 view.
   */
  Subsquare = 'subsquare',
}

export interface GridBounds {
  south: number;
  west: number;
  north: number;
  east: number;
}

export interface GridLabel {
  /** Latitude of the cell center. */
  lat: number;
  /** Longitude of the cell center. */
  lon: number;
  /** Locator prefix (2 chars at Field level, 4 at Square level). */
  text: string;
}

export interface GridLinesResult {
  lonLines: number[];
  latLines: number[];
  labels: GridLabel[];
}

const STEPS: Record<GridLevel, { lon: number; lat: number }> = {
  [GridLevel.Field]: { lon: 20, lat: 10 },
  [GridLevel.Square]: { lon: 2, lat: 1 },
  // Subsquare cells are 5′ lon × 2.5′ lat.
  [GridLevel.Subsquare]: { lon: 5 / 60, lat: 2.5 / 60 },
};

/** Locator-prefix length (chars) per grid level. */
const LABEL_PREFIX: Record<GridLevel, number> = {
  [GridLevel.Field]: 2,
  [GridLevel.Square]: 4,
  [GridLevel.Subsquare]: 6,
};

/**
 * Grid granularity for a map zoom across the FULL offline+LAN zoom range.
 *
 * z0-2 stay at Field and z3 stays at Square — UNCHANGED legacy behavior so the
 * offline raster substrate (maxZoom 2) and the low LAN band look exactly as
 * before. The validated LAN tile layer raises maxZoom up to 16, so this fans
 * the lattice finer as zoom climbs and fades it out entirely (null) at very
 * high zoom where even subsquare lines would clutter the view:
 *
 *   z ≤ 2  → Field      (20°×10°, 2-char)
 *   z 3-8  → Square     (2°×1°,  4-char)
 *   z 9-13 → Subsquare  (5′×2.5′, 6-char)
 *   z ≥ 14 → null       (lattice hidden)
 */
export function levelFromZoom(zoom: number): GridLevel | null {
  if (zoom >= 14) return null;
  if (zoom >= 9) return GridLevel.Subsquare;
  if (zoom >= 3) return GridLevel.Square;
  return GridLevel.Field;
}

/** Normalize `-0` to `0` so line/label values compare cleanly. */
function noNegZero(v: number): number {
  return v + 0;
}

/**
 * Grid lines (multiples of `step`) lying within `[min, max]` inclusive.
 * Index-based generation (`start + i*step`) avoids floating-point drift that
 * repeated addition would accumulate over many lines.
 */
function linesInRange(min: number, max: number, step: number): number[] {
  if (max < min) return [];
  const start = Math.ceil(min / step) * step;
  const end = Math.floor(max / step) * step;
  const count = Math.round((end - start) / step);
  const out: number[] = [];
  for (let i = 0; i <= count; i++) out.push(noNegZero(start + i * step));
  return out;
}

/**
 * SW-corner positions of cells (width `step`) that overlap `[min, max)`.
 * The cell at `v` spans `[v, v+step]`; it is included when `v < max` and the
 * cell containing `min` is the first. A cell whose SW corner equals `max`
 * (e.g. lon 180 on a world view) is excluded — it lies outside the window.
 */
function cellStarts(min: number, max: number, step: number): number[] {
  if (max <= min) return [];
  const first = Math.floor(min / step) * step;
  const out: number[] = [];
  for (let v = first; v < max; v += step) out.push(noNegZero(v));
  return out;
}

export function gridLines(bounds: GridBounds, level: GridLevel): GridLinesResult {
  const { south, west, north, east } = bounds;
  const step = STEPS[level];
  const prefix = LABEL_PREFIX[level];

  const lonLines = linesInRange(west, east, step.lon);
  const latLines = linesInRange(south, north, step.lat);

  const labels: GridLabel[] = [];
  const lonCells = cellStarts(west, east, step.lon);
  const latCells = cellStarts(south, north, step.lat);
  for (const latSW of latCells) {
    const lat = noNegZero(latSW + step.lat / 2);
    for (const lonSW of lonCells) {
      const lon = noNegZero(lonSW + step.lon / 2);
      labels.push({ lat, lon, text: latLonToGrid(lat, lon).slice(0, prefix) });
    }
  }

  return { lonLines, latLines, labels };
}
