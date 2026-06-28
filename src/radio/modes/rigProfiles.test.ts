import { describe, it, expect } from 'vitest';
import { getRigProfile, RIG_PROFILES } from './rigProfiles';

describe('rigProfiles', () => {
  it('returns the documented FT-710 profile (model 1049)', () => {
    const p = getRigProfile(1049);
    expect(p).toEqual({
      ptt_method: 'cat_command',
      data_mode: 'PKTUSB',
      cat_baud: 38400,
      close_serial_sequencing: true,
    });
  });

  it('returns undefined for an unprofiled model', () => {
    expect(getRigProfile(99999)).toBeUndefined();
  });

  it('returns undefined for null/unset', () => {
    expect(getRigProfile(null)).toBeUndefined();
    expect(getRigProfile(undefined)).toBeUndefined();
  });

  it('table is keyed by numeric hamlib model id', () => {
    expect(Object.prototype.hasOwnProperty.call(RIG_PROFILES, 1049)).toBe(true);
  });
});
