import { describe, it, expect } from 'vitest';
import { bandForKhz, HF_BANDS, bandLabel, type Band } from './bandPlan';

describe('bandForKhz', () => {
  it('maps every amateur HF dial to its band', () => {
    expect(bandForKhz(1850)).toBe('160m');
    expect(bandForKhz(3590)).toBe('80m');
    expect(bandForKhz(5358.5)).toBe('60m'); // a US 60m channel
    expect(bandForKhz(7103)).toBe('40m');
    expect(bandForKhz(10147)).toBe('30m');
    expect(bandForKhz(14103)).toBe('20m');
    expect(bandForKhz(18106)).toBe('17m');
    expect(bandForKhz(21100)).toBe('15m');
    expect(bandForKhz(24920)).toBe('12m');
    expect(bandForKhz(28120)).toBe('10m');
  });
  it('maps VHF/UHF packet dials to vhf-uhf', () => {
    expect(bandForKhz(145710)).toBe('vhf-uhf');
    expect(bandForKhz(441300)).toBe('vhf-uhf');
  });
  it('returns null for dials outside the amateur bands', () => {
    expect(bandForKhz(9000)).toBeNull(); // between 40m and 30m
    expect(bandForKhz(2500)).toBeNull(); // between 160m and 80m
    expect(bandForKhz(30100)).toBeNull(); // above 10m, below VHF
  });
});

describe('HF_BANDS', () => {
  it('lists the full amateur HF allocation in ascending frequency', () => {
    expect(HF_BANDS).toEqual<Band[]>([
      '160m',
      '80m',
      '60m',
      '40m',
      '30m',
      '20m',
      '17m',
      '15m',
      '12m',
      '10m',
    ]);
  });
});

describe('bandLabel', () => {
  it('renders human labels', () => {
    expect(bandLabel('160m')).toBe('160 m');
    expect(bandLabel('40m')).toBe('40 m');
    expect(bandLabel('10m')).toBe('10 m');
    expect(bandLabel('vhf-uhf')).toBe('VHF/UHF');
  });
});
