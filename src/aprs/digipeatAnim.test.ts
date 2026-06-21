import { describe, it, expect } from 'vitest';
import { traceProgress, trimPath, DEFAULT_TIMING } from './digipeatAnim';
import type { PathSegment } from './digipeatPath';

describe('traceProgress', () => {
  it('uses the brisk cn84 default timing (operator-tuned ~2× the original)', () => {
    // The single place the concrete feel is pinned. The boundary test below
    // derives all its elapsed values from DEFAULT_TIMING, so a future retune
    // only needs to change this assertion — not the boundary checks.
    expect(DEFAULT_TIMING).toEqual({ drawMs: 1000, lingerMs: 1000, fadeMs: 300 });
  });

  it('draws 0→1 over drawMs, then lingers full, then fades, then done', () => {
    const { drawMs, lingerMs, fadeMs } = DEFAULT_TIMING;
    const lingerEnd = drawMs + lingerMs;
    const fadeEnd = lingerEnd + fadeMs;

    expect(traceProgress(0)).toMatchObject({ phase: 'draw', drawProgress: 0, opacity: 1 });
    // Half-way through the draw phase.
    expect(traceProgress(drawMs / 2)).toMatchObject({ phase: 'draw', opacity: 1 });
    expect(traceProgress(drawMs / 2).drawProgress).toBeCloseTo(0.5, 2);
    // Draw complete → linger begins exactly at drawMs.
    expect(traceProgress(drawMs)).toMatchObject({ phase: 'linger', drawProgress: 1, opacity: 1 });
    expect(traceProgress(drawMs + lingerMs / 2)).toMatchObject({ phase: 'linger', drawProgress: 1 });
    // Fade window is [drawMs+lingerMs, drawMs+lingerMs+fadeMs); sample its midpoint.
    const midFade = lingerEnd + fadeMs / 2;
    expect(traceProgress(midFade).phase).toBe('fade');
    expect(traceProgress(midFade).opacity).toBeGreaterThan(0);
    expect(traceProgress(midFade).opacity).toBeLessThan(1);
    // Past the fade window → done, fully transparent.
    expect(traceProgress(fadeEnd + 100)).toMatchObject({ phase: 'done', opacity: 0 });
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
