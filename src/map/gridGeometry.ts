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
};

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
  const prefix = level === GridLevel.Field ? 2 : 4;

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
