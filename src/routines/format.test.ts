/**
 * Tests for format.ts — the routines display formatters (plan-5 Task 6).
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  formatMissedCount,
  formatUtc,
  formatTrigger,
  formatRunState,
  formatIfMissed,
  formatStepErrorCause,
  formatUiError,
} from './format';
import type { RunState, StepError, Trigger } from './routinesApi';

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

describe('formatIfMissed', () => {
  it('renders the two IfMissed policies', () => {
    expect(formatIfMissed('skip')).toBe('missed: skip');
    expect(formatIfMissed('run_once_on_launch')).toBe('missed: run once on launch');
  });
});

describe('formatStepErrorCause', () => {
  it('returns an action error cause VERBATIM, never re-worded', () => {
    const e: StepError = { kind: 'action', detail: { action: 'radio.connect', cause: 'VARA HF: DISCONNECTED — link timeout 90 s' } };
    expect(formatStepErrorCause(e)).toBe('VARA HF: DISCONNECTED — link timeout 90 s');
  });

  it('returns an unset_variable message VERBATIM', () => {
    const e: StepError = { kind: 'unset_variable', detail: 'variable "gateway" was never set' };
    expect(formatStepErrorCause(e)).toBe('variable "gateway" was never set');
  });

  it('synthesizes a label for timeout', () => {
    const e: StepError = { kind: 'timeout', detail: { seconds: 90 } };
    expect(formatStepErrorCause(e)).toBe('timeout after 90s');
  });

  it('synthesizes a label for cancelled', () => {
    const e: StepError = { kind: 'cancelled' };
    expect(formatStepErrorCause(e)).toBe('cancelled');
  });
});

describe('formatUiError', () => {
  it('returns Rejected detail VERBATIM', () => {
    expect(
      formatUiError({ kind: 'Rejected', detail: 'Refused: consent required before automatic transmit' }),
    ).toBe('Refused: consent required before automatic transmit');
  });

  it('returns NotFound / NotConfigured detail VERBATIM', () => {
    expect(formatUiError({ kind: 'NotFound', detail: 'routine "x" not found' })).toBe(
      'routine "x" not found',
    );
    expect(formatUiError({ kind: 'NotConfigured', detail: 'not connected' })).toBe('not connected');
  });

  it('unwraps the reason field for Transport/AuthFailed/Unavailable', () => {
    expect(formatUiError({ kind: 'Transport', detail: { reason: 'link down' } })).toBe('link down');
    expect(formatUiError({ kind: 'AuthFailed', detail: { reason: 'bad key' } })).toBe('bad key');
    expect(formatUiError({ kind: 'Unavailable', detail: { reason: 'busy' } })).toBe('busy');
  });

  it('unwraps Internal detail and labels Cancelled', () => {
    expect(formatUiError({ kind: 'Internal', detail: { detail: 'disk full' } })).toBe('disk full');
    expect(formatUiError({ kind: 'Cancelled' })).toBe('cancelled');
  });

  it('falls back to the raw error message for a non-UiError throw', () => {
    expect(formatUiError(new Error('boom'))).toBe('boom');
    expect(formatUiError('plain string error')).toBe('plain string error');
  });
});
