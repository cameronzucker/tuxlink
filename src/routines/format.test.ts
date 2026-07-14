/**
 * Tests for format.ts — the routines display formatters (plan-5 Task 6).
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { formatMissedCount, formatUtc, formatTrigger, formatRunState } from './format';
import type { RunState, Trigger } from './routinesApi';

describe('formatMissedCount', () => {
  it('clamps the 100k lower-bound missed count', () => {
    expect(formatMissedCount(3)).toBe('3');
    expect(formatMissedCount(99_999)).toBe('99999');
    expect(formatMissedCount(100_000)).toBe('100k+'); // the clamp value itself
    expect(formatMissedCount(2_000_000)).toBe('100k+'); // never the raw number
  });

  it('handles zero', () => {
    expect(formatMissedCount(0)).toBe('0');
  });
});

describe('formatUtc', () => {
  beforeEach(() => {
    // Pin "now" to 2026-07-14T12:00:00Z so "today" is deterministic.
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-07-14T12:00:00.000Z'));
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('renders HH:MMZ for an instant on today (UTC)', () => {
    const unix = Date.UTC(2026, 6, 14, 9, 5) / 1000; // 2026-07-14T09:05Z
    expect(formatUtc(unix)).toBe('09:05Z');
  });

  it('renders HH:MMZ at the very start of today', () => {
    const unix = Date.UTC(2026, 6, 14, 0, 0) / 1000;
    expect(formatUtc(unix)).toBe('00:00Z');
  });

  it('renders MM-DD HH:MMZ for an instant on a different UTC day', () => {
    const unix = Date.UTC(2026, 6, 13, 23, 59) / 1000; // yesterday
    expect(formatUtc(unix)).toBe('07-13 23:59Z');
  });

  it('renders MM-DD HH:MMZ for a future date', () => {
    const unix = Date.UTC(2026, 7, 1, 3, 4) / 1000; // 2026-08-01
    expect(formatUtc(unix)).toBe('08-01 03:04Z');
  });
});

describe('formatTrigger', () => {
  it('renders manual triggers as "manual"', () => {
    const t: Trigger = { type: 'manual' };
    expect(formatTrigger(t)).toBe('manual');
  });

  it('renders a bare schedule with just the interval', () => {
    const t: Trigger = { type: 'schedule', every: '30m' };
    expect(formatTrigger(t)).toBe('every 30m');
  });

  it('renders a schedule with align', () => {
    const t: Trigger = { type: 'schedule', every: '30m', align: 'hour' };
    expect(formatTrigger(t)).toBe('every 30m · align hour');
  });

  it('renders a schedule with align and window', () => {
    const t: Trigger = {
      type: 'schedule',
      every: '2h',
      align: 'day',
      window: '06:00-22:00',
    };
    expect(formatTrigger(t)).toBe('every 2h · align day · window 06:00-22:00');
  });

  it('renders a schedule with a window but no align', () => {
    const t: Trigger = { type: 'schedule', every: '45s', window: '06:00-22:00' };
    expect(formatTrigger(t)).toBe('every 45s · window 06:00-22:00');
  });
});

describe('formatRunState', () => {
  it('renders a human label for every RunState value', () => {
    const expected: Record<RunState, string> = {
      pending: 'Pending',
      running: 'Running',
      waiting: 'Waiting',
      awaiting_consent: 'Awaiting consent',
      awaiting_radio: 'Awaiting radio',
      completed: 'Completed',
      failed: 'Failed',
      cancelled: 'Cancelled',
      interrupted: 'Interrupted',
    };
    for (const [state, label] of Object.entries(expected) as [RunState, string][]) {
      expect(formatRunState(state)).toBe(label);
    }
  });
});
