// src/aprs/digipeatAnim.ts
//
// Pure schedule + geometry for the cn84 directional path animation. No DOM, no
// canvas, no Leaflet — the layer (DigipeatPathLayer) calls these per frame and
// does the drawing. Kept pure so the animation logic is unit-tested; only the
// raw Canvas2D draw + projection is smoke-gated.
import type { LatLon, PathSegment } from './digipeatPath';

export interface TraceTiming {
  drawMs: number;
  lingerMs: number;
  fadeMs: number;
}

/** cn84 aprs.fi-classic feel. Tunable. */
export const DEFAULT_TIMING: TraceTiming = { drawMs: 2000, lingerMs: 2000, fadeMs: 600 };

export interface TraceState {
  phase: 'draw' | 'linger' | 'fade' | 'done';
  drawProgress: number; // 0..1 fraction of the path drawn
  opacity: number; // 0..1
}

export function traceProgress(elapsedMs: number, timing: TraceTiming = DEFAULT_TIMING): TraceState {
  const { drawMs, lingerMs, fadeMs } = timing;
  if (elapsedMs <= 0) return { phase: 'draw', drawProgress: 0, opacity: 1 };
  if (elapsedMs < drawMs) {
    return { phase: 'draw', drawProgress: elapsedMs / drawMs, opacity: 1 };
  }
  const lingerEnd = drawMs + lingerMs;
  if (elapsedMs < lingerEnd) {
    return { phase: 'linger', drawProgress: 1, opacity: 1 };
  }
  const fadeEnd = lingerEnd + fadeMs;
  if (elapsedMs < fadeEnd) {
    return { phase: 'fade', drawProgress: 1, opacity: 1 - (elapsedMs - lingerEnd) / fadeMs };
  }
  return { phase: 'done', drawProgress: 1, opacity: 0 };
}

function lerp(a: LatLon, b: LatLon, t: number): LatLon {
  return { lat: a.lat + (b.lat - a.lat) * t, lon: a.lon + (b.lon - a.lon) * t };
}

/** Trim the path to `drawProgress` (0..1). Progress is by SEGMENT COUNT
 * (hop-by-hop, faithful to cn84), not geographic distance: each of the N
 * segments occupies an equal 1/N slice. Returns the drawn segments (the final
 * one possibly partial) and the dot position at the leading edge. */
export function trimPath(
  segments: PathSegment[],
  drawProgress: number,
): { drawn: PathSegment[]; dot: LatLon | null } {
  const total = segments.length;
  if (total === 0 || drawProgress <= 0) return { drawn: [], dot: null };
  const drawn: PathSegment[] = [];
  let dot: LatLon | null = null;
  for (let i = 0; i < total; i++) {
    const segStart = i / total;
    const segEnd = (i + 1) / total;
    if (drawProgress <= segStart) break; // not reached yet
    const s = segments[i];
    if (drawProgress >= segEnd) {
      drawn.push(s);
      dot = s.to;
    } else {
      const frac = (drawProgress - segStart) / (segEnd - segStart);
      const to = lerp(s.from, s.to, frac);
      drawn.push({ ...s, to });
      dot = to;
      break;
    }
  }
  return { drawn, dot };
}
