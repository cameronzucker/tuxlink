// src/aprs/pathTrace.ts
//
// Pure timeline math for the digipeat-path trace animation. The maplibre layer
// is a thin shell that, each rAF tick, calls computeTraceFrame(active, now) and
// applies the result. Kept DOM-free + WebGL-free so it is fully unit-testable
// (jsdom has no WebGL). aprs.fi-classic feel: ~2s hop-by-hop draw, a packet dot
// rides src→you, ~2s linger, fade. Hover mode holds (no auto-fade) until cleared.

import type { LatLon, PathSegment } from './digipeatPath';

export interface TraceTimings {
  drawMs: number;
  lingerMs: number;
  fadeMs: number;
}
export const DEFAULT_TIMINGS: TraceTimings = { drawMs: 2000, lingerMs: 2000, fadeMs: 600 };

export type TraceMode = 'live' | 'hover';
export type TracePhase = 'idle' | 'drawing' | 'linger' | 'fading';

export interface ActiveTrace {
  segments: PathSegment[];
  startMs: number;
  mode: TraceMode;
  timings: TraceTimings;
}

export interface TraceFrame {
  phase: TracePhase;
  progress: number; // 0..1 draw-in along the whole polyline
  packet: LatLon | null; // riding dot during draw; null otherwise
  opacity: number; // whole-path opacity (linger=1, fades to 0)
  segments: PathSegment[];
}

/// Linear point at fractional progress `p` (0..1) along the concatenated
/// segments, weighting each segment equally (hop-by-hop feel, not arc-length).
export function pointAtProgress(segments: PathSegment[], p: number): LatLon {
  if (segments.length === 0) return { lat: 0, lon: 0 };
  const clamped = Math.max(0, Math.min(1, p));
  const target = clamped * segments.length;
  const idx = Math.min(segments.length - 1, Math.floor(target));
  const frac = target - idx;
  const s = segments[idx];
  return {
    lat: s.from.lat + (s.to.lat - s.from.lat) * frac,
    lon: s.from.lon + (s.to.lon - s.from.lon) * frac,
  };
}

export function computeTraceFrame(active: ActiveTrace, nowMs: number): TraceFrame {
  const { segments, startMs, mode, timings } = active;
  const t = nowMs - startMs;
  const { drawMs, lingerMs, fadeMs } = timings;

  if (t < drawMs) {
    const progress = drawMs === 0 ? 1 : t / drawMs;
    return { phase: 'drawing', progress, packet: pointAtProgress(segments, progress), opacity: 1, segments };
  }
  // Hover: hold fully drawn until the controller clears it (mode flips / segments change).
  if (mode === 'hover') {
    return { phase: 'linger', progress: 1, packet: null, opacity: 1, segments };
  }
  const sinceDraw = t - drawMs;
  if (sinceDraw < lingerMs) {
    return { phase: 'linger', progress: 1, packet: null, opacity: 1, segments };
  }
  const sinceLinger = sinceDraw - lingerMs;
  if (sinceLinger < fadeMs) {
    return { phase: 'fading', progress: 1, packet: null, opacity: 1 - sinceLinger / fadeMs, segments };
  }
  return { phase: 'idle', progress: 1, packet: null, opacity: 0, segments };
}
