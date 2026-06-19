import { describe, it, expect } from 'vitest';
import {
  pointAtProgress,
  computeTraceFrame,
  DEFAULT_TIMINGS,
  type ActiveTrace,
} from './pathTrace';
import type { PathSegment } from './digipeatPath';

const SEGS: PathSegment[] = [
  { kind: 'solid', from: { lat: 0, lon: 0 }, to: { lat: 0, lon: 10 } },
  { kind: 'solid', from: { lat: 0, lon: 10 }, to: { lat: 0, lon: 20 } },
];

describe('pointAtProgress', () => {
  it('p=0 is the start, p=1 is the end', () => {
    expect(pointAtProgress(SEGS, 0)).toEqual({ lat: 0, lon: 0 });
    expect(pointAtProgress(SEGS, 1)).toEqual({ lat: 0, lon: 20 });
  });
  it('p=0.5 is the midpoint of a two-equal-segment path', () => {
    expect(pointAtProgress(SEGS, 0.5)).toEqual({ lat: 0, lon: 10 });
  });
  it('clamps out-of-range progress', () => {
    expect(pointAtProgress(SEGS, -1)).toEqual({ lat: 0, lon: 0 });
    expect(pointAtProgress(SEGS, 2)).toEqual({ lat: 0, lon: 20 });
  });
});

describe('computeTraceFrame (live one-shot)', () => {
  const active: ActiveTrace = { segments: SEGS, startMs: 1000, mode: 'live', timings: DEFAULT_TIMINGS };

  it('mid-draw: progress in (0,1), packet present, full opacity', () => {
    const f = computeTraceFrame(active, 1000 + DEFAULT_TIMINGS.drawMs / 2);
    expect(f.phase).toBe('drawing');
    expect(f.progress).toBeCloseTo(0.5, 2);
    expect(f.packet).not.toBeNull();
    expect(f.opacity).toBe(1);
  });

  it('linger: progress 1, packet gone, full opacity', () => {
    const f = computeTraceFrame(active, 1000 + DEFAULT_TIMINGS.drawMs + 10);
    expect(f.phase).toBe('linger');
    expect(f.progress).toBe(1);
    expect(f.packet).toBeNull();
    expect(f.opacity).toBe(1);
  });

  it('fading: opacity decreases toward 0', () => {
    const t = 1000 + DEFAULT_TIMINGS.drawMs + DEFAULT_TIMINGS.lingerMs + DEFAULT_TIMINGS.fadeMs / 2;
    const f = computeTraceFrame(active, t);
    expect(f.phase).toBe('fading');
    expect(f.opacity).toBeGreaterThan(0);
    expect(f.opacity).toBeLessThan(1);
  });

  it('after fade: idle, opacity 0', () => {
    const t = 1000 + DEFAULT_TIMINGS.drawMs + DEFAULT_TIMINGS.lingerMs + DEFAULT_TIMINGS.fadeMs + 1;
    const f = computeTraceFrame(active, t);
    expect(f.phase).toBe('idle');
    expect(f.opacity).toBe(0);
  });
});

describe('computeTraceFrame (hover hold)', () => {
  const active: ActiveTrace = { segments: SEGS, startMs: 1000, mode: 'hover', timings: DEFAULT_TIMINGS };
  it('holds at full opacity after draw (no fade while hovered)', () => {
    const f = computeTraceFrame(active, 1000 + DEFAULT_TIMINGS.drawMs + 100000);
    expect(f.phase).toBe('linger');
    expect(f.progress).toBe(1);
    expect(f.opacity).toBe(1);
  });
});
