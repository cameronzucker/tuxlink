import { describe, expect, it } from 'vitest';
import { formatGridClock, timeZoneForGrid } from './gridClock';

describe('timeZoneForGrid', () => {
  it('maps a configured Maidenhead grid to an IANA timezone', () => {
    expect(timeZoneForGrid('DM33')).toBe('America/Phoenix');
  });

  it('uses the more precise 6-character grid when available', () => {
    expect(timeZoneForGrid('JN58td')).toBe('Europe/Berlin');
  });

  it('returns null for absent or malformed grids', () => {
    expect(timeZoneForGrid(null)).toBeNull();
    expect(timeZoneForGrid('')).toBeNull();
    expect(timeZoneForGrid('ZZ99')).toBeNull();
  });
});

describe('formatGridClock', () => {
  const now = new Date('2026-06-07T13:30:00Z');

  it('formats the local side from the grid timezone', () => {
    const result = formatGridClock(now, 'DM33');
    expect(result.utc).toBe('13:30z');
    expect(result.source).toBe('grid');
    expect(result.timeZone).toBe('America/Phoenix');
    expect(result.localTitle).toContain('DM33');
    expect(result.localTitle).toContain('America/Phoenix');
    expect(result.localTitle).toContain('Approximate');
  });

  it('does not mark 6-character grids as approximate', () => {
    const result = formatGridClock(now, 'JN58td');
    expect(result.source).toBe('grid');
    expect(result.timeZone).toBe('Europe/Berlin');
    expect(result.localTitle).not.toContain('Approximate');
  });

  it('falls back to the device timezone when no valid grid exists', () => {
    const result = formatGridClock(now, null);
    expect(result.utc).toBe('13:30z');
    expect(result.source).toBe('device');
    expect(result.timeZone).toBeNull();
    expect(result.localTitle).toContain('No grid');
    expect(result.local).not.toHaveLength(0);
  });
});
