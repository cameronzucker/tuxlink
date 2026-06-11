import { describe, it, expect } from 'vitest';
import { bandForKhz, HF_BANDS, bandLabel, type Band } from './bandPlan';

describe('bandForKhz', () => {
  it('maps amateur HF dials to their band', () => {
    expect(bandForKhz(3590)).toBe('80m');
    expect(bandForKhz(7103)).toBe('40m');
    expect(bandForKhz(10147)).toBe('30m');
    expect(bandForKhz(14103)).toBe('20m');
  });
  it('maps VHF/UHF packet dials to vhf-uhf', () => {
    expect(bandForKhz(145710)).toBe('vhf-uhf');
    expect(bandForKhz(441300)).toBe('vhf-uhf');
  });
  it('returns null for dials outside the modelled bands', () => {
    expect(bandForKhz(1850)).toBeNull(); // 160m — not in the U3 band selector
    expect(bandForKhz(28120)).toBeNull(); // 10m — not modelled in v1
  });
});

describe('HF_BANDS', () => {
  it('lists the four selectable HF bands in ascending frequency', () => {
    expect(HF_BANDS).toEqual<Band[]>(['80m', '40m', '30m', '20m']);
  });
});

describe('bandLabel', () => {
  it('renders human labels', () => {
    expect(bandLabel('40m')).toBe('40 m');
    expect(bandLabel('vhf-uhf')).toBe('VHF/UHF');
  });
});
