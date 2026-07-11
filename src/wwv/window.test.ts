// src/wwv/window.test.ts
//
// Pure unit tests for nextCapture() (Task 16, wwv offair spec). No
// Date.now() in assertions — a fixed hour-boundary base plus explicit
// second offsets keeps every case deterministic.

import { describe, it, expect } from 'vitest';
import { nextCapture } from './window';

// Exact hour boundary (verified: 1_783_512_000 % 3600 === 0).
const HOUR_BOUNDARY_MS = 1_783_512_000_000;
const HOUR_S = 3600;
const WWV_START_S = 18 * 60 - 5; // 1075
const WWVH_START_S = 45 * 60 - 5; // 2695

describe('nextCapture', () => {
  it('at an exact hour boundary, schedules to the WWV :18 window this hour', () => {
    const got = nextCapture(HOUR_BOUNDARY_MS);
    expect(got.label).toBe('WWV :18');
    expect(got.delayMs).toBe(WWV_START_S * 1000);
    expect(got.atUnixMs).toBe(HOUR_BOUNDARY_MS + WWV_START_S * 1000);
  });

  it('fires immediately (delayMs 0) when already inside the WWV capture span', () => {
    // intoHour 1080 s — 5 s into the WWV_START_S=1075 window, still < 1075+70=1145.
    const nowMs = HOUR_BOUNDARY_MS + 1080 * 1000;
    const got = nextCapture(nowMs);
    expect(got.delayMs).toBe(0);
    expect(got.label).toBe('WWV :18');
    expect(got.atUnixMs).toBe(nowMs);
  });

  it('schedules to WWVH :45 when past the WWV span but before the WWVH window', () => {
    // intoHour 1200 s (:20:00) — past WWV's 1075..1145 span.
    const nowMs = HOUR_BOUNDARY_MS + 1200 * 1000;
    const got = nextCapture(nowMs);
    expect(got.label).toBe('WWVH :45');
    expect(got.delayMs).toBe((WWVH_START_S - 1200) * 1000);
  });

  it('rolls into next hour for WWV :18 when past both windows this hour', () => {
    // intoHour 3000 s (:50:00) — past both WWV (1075..1145) and WWVH (2695..2765) spans.
    const nowMs = HOUR_BOUNDARY_MS + 3000 * 1000;
    const got = nextCapture(nowMs);
    expect(got.label).toBe('WWV :18');
    expect(got.delayMs).toBe((HOUR_S + WWV_START_S - 3000) * 1000);
  });

  it('fires immediately when inside the WWVH capture span', () => {
    // intoHour 2700 s — 5 s into WWVH_START_S=2695, still < 2695+70=2765.
    const nowMs = HOUR_BOUNDARY_MS + 2700 * 1000;
    const got = nextCapture(nowMs);
    expect(got.delayMs).toBe(0);
    expect(got.label).toBe('WWVH :45');
  });
});
