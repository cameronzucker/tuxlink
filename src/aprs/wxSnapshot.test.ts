import { describe, it, expect } from 'vitest';
import { composeSnapshotHeader } from './wxSnapshot';

describe('composeSnapshotHeader', () => {
  it('includes grid, UTC, and station count', () => {
    const h = composeSnapshotHeader({ grid: 'DM43', utcMs: Date.UTC(2026, 5, 19, 19, 42, 0), stationCount: 7 });
    expect(h).toContain('DM43');
    expect(h).toContain('1942Z');
    expect(h).toContain('7');
  });
  it('omits the grid segment when no grid is known', () => {
    const h = composeSnapshotHeader({ utcMs: Date.UTC(2026, 5, 19, 1, 5, 0), stationCount: 0 });
    expect(h).not.toContain('grid');
    expect(h).toContain('0105Z');
  });
});
