// record-format helpers — the L2 station-wall-clock + relative-ago formatters.
//
// L2 is THE subtle correctness point: the station wall-clock MUST come from the
// literal HH:MM in the offset-bearing ISO8601 string (the station's own clock),
// NOT from the viewer's timezone (`new Date(...).getHours()`) NOR from UTC
// (`.getUTCHours()`). The relative "ago" delta legitimately uses the absolute
// instant (offset-aware parse).

import { describe, it, expect } from 'vitest';
import { stationWallClock, relativeAgo } from './record-format';

describe('stationWallClock (L2)', () => {
  it('extracts the literal HH:MM from the offset-bearing string (station clock, not viewer TZ)', () => {
    // 21:42 at a -07:00 offset is the STATION wall-clock. Regardless of the
    // machine TZ the test runs under, this must be '21:42' — it is the literal
    // wall-clock the station observed, not a re-projection into viewer-local.
    expect(stationWallClock('2026-06-07T21:42:00-07:00')).toBe('21:42');
  });

  it('returns the literal HH:MM for a positive offset too', () => {
    expect(stationWallClock('2026-06-07T06:05:00+10:00')).toBe('06:05');
  });

  it('returns the literal HH:MM for a Z (UTC) timestamp', () => {
    expect(stationWallClock('2026-06-07T13:00:00Z')).toBe('13:00');
  });

  it('returns null for an unparseable string', () => {
    expect(stationWallClock('not-a-timestamp')).toBeNull();
  });
});

describe('relativeAgo (absolute-instant delta)', () => {
  const now = new Date('2026-06-07T22:00:00-07:00'); // fixed for determinism

  it('"just now" for <1 minute', () => {
    expect(relativeAgo('2026-06-07T21:59:30-07:00', now)).toBe('just now');
  });

  it('"N min ago" for minutes', () => {
    expect(relativeAgo('2026-06-07T21:42:00-07:00', now)).toBe('18 min ago');
  });

  it('"N h ago" for hours', () => {
    expect(relativeAgo('2026-06-07T20:00:00-07:00', now)).toBe('2 h ago');
  });

  it('"N d ago" for days', () => {
    expect(relativeAgo('2026-06-04T22:00:00-07:00', now)).toBe('3 d ago');
  });

  it('uses the ABSOLUTE instant (offset-aware), not the wall-clock literal', () => {
    // Same wall-clock literal '22:00' but a different offset (+00:00) is a
    // 7-hours-earlier absolute instant vs `now` (which is 22:00-07:00).
    expect(relativeAgo('2026-06-07T22:00:00+00:00', now)).toBe('7 h ago');
  });

  it('returns empty string for an unparseable timestamp', () => {
    expect(relativeAgo('not-a-timestamp')).toBe('');
  });
});
