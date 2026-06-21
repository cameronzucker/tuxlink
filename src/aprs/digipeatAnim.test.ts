import { describe, it, expect } from 'vitest';
import { traceProgress, trimPath, DEFAULT_TIMING } from './digipeatAnim';
import type { PathSegment } from './digipeatPath';

describe('traceProgress', () => {
  it('uses the cn84 aprs.fi-classic default timing the boundaries below assume', () => {
    // The elapsed-time boundaries in the next test (1000=½ draw, 2000=draw end,
    // 4000=linger end, 4600=fade end) are derived from these values; assert them
    // so a retune of DEFAULT_TIMING surfaces here instead of as silent drift.
    expect(DEFAULT_TIMING).toEqual({ drawMs: 2000, lingerMs: 2000, fadeMs: 600 });
  });

  it('draws 0→1 over drawMs, then lingers full, then fades, then done', () => {
    expect(traceProgress(0)).toMatchObject({ phase: 'draw', drawProgress: 0, opacity: 1 });
    expect(traceProgress(1000)).toMatchObject({ phase: 'draw', opacity: 1 });
    expect(traceProgress(1000).drawProgress).toBeCloseTo(0.5, 2);
    expect(traceProgress(2000)).toMatchObject({ phase: 'linger', drawProgress: 1, opacity: 1 });
    expect(traceProgress(3000)).toMatchObject({ phase: 'linger', drawProgress: 1 });
    // fade window is [draw+linger, draw+linger+fade] = [4000, 4600]
    expect(traceProgress(4300).phase).toBe('fade');
    expect(traceProgress(4300).opacity).toBeGreaterThan(0);
    expect(traceProgress(4300).opacity).toBeLessThan(1);
    expect(traceProgress(5000)).toMatchObject({ phase: 'done', opacity: 0 });
  });
});

describe('trimPath', () => {
  // 2 equal-count segments: A→B (solid), B→C (solid). Progress is by SEGMENT
  // COUNT (hop-by-hop), not geographic distance — faithful to cn84.
  const segs: PathSegment[] = [
    { kind: 'solid', from: { lat: 0, lon: 0 }, to: { lat: 0, lon: 2 } },
    { kind: 'solid', from: { lat: 0, lon: 2 }, to: { lat: 0, lon: 4 } },
  ];
  it('progress 0 → nothing drawn, no dot', () => {
    expect(trimPath(segs, 0)).toEqual({ drawn: [], dot: null });
  });
  it('progress 0.25 → first segment half-drawn, dot at the leading edge', () => {
    const r = trimPath(segs, 0.25); // 0.25 of 2 segs = halfway through seg 0
    expect(r.drawn).toHaveLength(1);
    expect(r.drawn[0].to).toEqual({ lat: 0, lon: 1 });
    expect(r.dot).toEqual({ lat: 0, lon: 1 });
  });
  it('progress 1 → both segments full, dot at the end', () => {
    const r = trimPath(segs, 1);
    expect(r.drawn).toHaveLength(2);
    expect(r.drawn[1].to).toEqual({ lat: 0, lon: 4 });
    expect(r.dot).toEqual({ lat: 0, lon: 4 });
  });
  it('empty path → empty', () => {
    expect(trimPath([], 0.5)).toEqual({ drawn: [], dot: null });
  });
});
