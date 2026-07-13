import { describe, it, expect } from 'vitest';
import { isTipSeen, markTipSeen } from './tipLogic';

describe('tip sentinel logic', () => {
  it('empty list: nothing seen', () => {
    expect(isTipSeen([], 'find-a-station')).toBe(false);
  });
  it('listed tip is seen', () => {
    expect(isTipSeen(['find-a-station'], 'find-a-station')).toBe(true);
  });
  it('["*"] sentinel: everything seen (upgrade cohort)', () => {
    expect(isTipSeen(['*'], 'anything')).toBe(true);
  });
  it('markTipSeen is idempotent and preserves the sentinel', () => {
    expect(markTipSeen(['*'], 'x')).toEqual(['*']);
    expect(markTipSeen(['a'], 'a')).toEqual(['a']);
    expect(markTipSeen(['a'], 'b')).toEqual(['a', 'b']);
  });
});
