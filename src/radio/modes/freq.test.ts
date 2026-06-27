import { describe, it, expect } from 'vitest';
import {
  parseFreqInputToHz,
  dialFreqToMhzString,
  dialsToQsyCandidates,
} from './freq';
import type { FavoriteDial } from '../../favorites/types';

describe('parseFreqInputToHz', () => {
  it('parses a MHz string to Hz', () => {
    expect(parseFreqInputToHz('7.102')).toBe(7102000);
  });

  it('trims surrounding whitespace', () => {
    expect(parseFreqInputToHz('  14.105  ')).toBe(14105000);
  });

  it('returns null for empty input', () => {
    expect(parseFreqInputToHz('')).toBeNull();
    expect(parseFreqInputToHz('   ')).toBeNull();
  });

  it('returns null for non-numeric input', () => {
    expect(parseFreqInputToHz('abc')).toBeNull();
  });

  it('returns null for non-positive input', () => {
    expect(parseFreqInputToHz('0')).toBeNull();
    expect(parseFreqInputToHz('-7.1')).toBeNull();
  });
});

describe('dialFreqToMhzString (C4 magnitude normalization)', () => {
  const dial = (freq?: string): FavoriteDial => ({ mode: 'ardop-hf', gateway: 'W7DG', freq });

  it('normalizes a kHz saved-favorite freq to MHz', () => {
    // The C4 bug: "14105.0" kHz was treated as MHz × 1e6 → 14.105 GHz.
    expect(dialFreqToMhzString(dial('14105.0'))).toBe('14.105');
  });

  it('leaves a Find-a-Station MHz string unchanged', () => {
    expect(dialFreqToMhzString(dial('7.103'))).toBe('7.103');
  });

  it('extracts the numeric token from a unit-suffixed string', () => {
    expect(dialFreqToMhzString(dial('7.103 MHz'))).toBe('7.103');
    expect(dialFreqToMhzString(dial('14105.0 kHz'))).toBe('14.105');
  });

  it('returns null for an absent or empty freq (clear-on-empty)', () => {
    expect(dialFreqToMhzString(dial(undefined))).toBeNull();
    expect(dialFreqToMhzString(dial(''))).toBeNull();
  });

  it('returns null for a non-numeric freq', () => {
    expect(dialFreqToMhzString(dial('n/a'))).toBeNull();
  });
});

describe('dialsToQsyCandidates', () => {
  it('maps dials to {target, freq_hz} with snake_case freq_hz', () => {
    const dials: FavoriteDial[] = [
      { mode: 'ardop-hf', gateway: 'W7DG', freq: '7.103' },
      { mode: 'ardop-hf', gateway: 'KE7XYZ', freq: '14105.0' },
    ];
    expect(dialsToQsyCandidates(dials)).toEqual([
      { target: 'W7DG', freq_hz: 7103000 },
      { target: 'KE7XYZ', freq_hz: 14105000 },
    ]);
  });

  it('emits freq_hz null when a dial has no parseable freq', () => {
    expect(dialsToQsyCandidates([{ mode: 'ardop-hf', gateway: 'W7DG' }])).toEqual([
      { target: 'W7DG', freq_hz: null },
    ]);
  });
});
