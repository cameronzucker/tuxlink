// ts-local.test — M4 / H1: the connection-record timestamp MUST carry an
// offset and MUST NOT be UTC-coerced (`Z`). These tests are TZ-independent:
// expectations are derived from the runtime `Date` getters, not hardcoded.

import { describe, it, expect } from 'vitest';
import { tsLocal } from './ts-local';

describe('tsLocal', () => {
  it('matches the offset-bearing ISO8601 shape and does NOT end with Z (H1)', () => {
    const s = tsLocal();
    expect(s).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[+-]\d{2}:\d{2}$/);
    expect(s.endsWith('Z')).toBe(false);
    expect(s).not.toContain('Z');
  });

  it('reflects the LOCAL wall-clock components of a fixed input Date', () => {
    // Construct from local-component args so the getters are deterministic
    // regardless of the host TZ (no UTC parsing involved).
    const now = new Date(2026, 5, 8, 21, 42, 5); // 2026-06-08 21:42:05 local
    const s = tsLocal(now);
    const datePart = s.slice(0, 19); // YYYY-MM-DDTHH:MM:SS
    const pad = (n: number) => String(n).padStart(2, '0');
    const expected =
      `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}` +
      `T${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`;
    expect(datePart).toBe(expected);
  });

  it('carries the correct offset SIGN for the test env (derived from getTimezoneOffset)', () => {
    const now = new Date(2026, 5, 8, 12, 0, 0);
    const s = tsLocal(now);
    const offMin = -now.getTimezoneOffset(); // ISO offset minutes (east-positive)
    const expectedSign = offMin >= 0 ? '+' : '-';
    // The sign immediately precedes the trailing HH:MM offset.
    const offsetField = s.slice(19); // e.g. "-07:00"
    expect(offsetField[0]).toBe(expectedSign);
    const pad = (n: number) => String(Math.trunc(Math.abs(n))).padStart(2, '0');
    expect(offsetField).toBe(`${expectedSign}${pad(offMin / 60)}:${pad(offMin % 60)}`);
  });
});
