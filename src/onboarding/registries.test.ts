import { describe, it, expect } from 'vitest';
import { TOUR_STOPS } from './tourRegistry';
import { HINTS } from './hintRegistry';

describe('registries', () => {
  const allEntries = [...TOUR_STOPS, ...HINTS];

  it('IDs are unique across both registries', () => {
    const ids = allEntries.map(e => e.id);
    const uniqueIds = new Set(ids);
    expect(uniqueIds.size).toBe(ids.length);
  });

  it('every entry has non-empty title', () => {
    allEntries.forEach(entry => {
      expect(entry.title).toBeTruthy();
      expect(entry.title.length).toBeGreaterThan(0);
    });
  });

  it('every entry has non-empty body', () => {
    allEntries.forEach(entry => {
      expect(entry.body).toBeTruthy();
      expect(entry.body.length).toBeGreaterThan(0);
    });
  });

  it('no entry id contains "*"', () => {
    allEntries.forEach(entry => {
      expect(entry.id).not.toContain('*');
    });
  });
});
