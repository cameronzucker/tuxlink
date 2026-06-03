import { describe, it, expect } from 'vitest';
import { parseForecastTimes, DEFAULT_GRIB_REQUEST, ALL_GRIB_PARAMETERS } from './types';

describe('DEFAULT_GRIB_REQUEST', () => {
  it('matches the canonical Saildocs minimal example shape', () => {
    expect(DEFAULT_GRIB_REQUEST.mode).toBe('send');
    expect(DEFAULT_GRIB_REQUEST.lat0).toEqual({ degrees: 40, dir: 'N' });
    expect(DEFAULT_GRIB_REQUEST.lat1).toEqual({ degrees: 60, dir: 'N' });
    expect(DEFAULT_GRIB_REQUEST.lon0).toEqual({ degrees: 140, dir: 'W' });
    expect(DEFAULT_GRIB_REQUEST.lon1).toEqual({ degrees: 120, dir: 'W' });
    expect(DEFAULT_GRIB_REQUEST.grid).toEqual([2, 2]);
    expect(DEFAULT_GRIB_REQUEST.times).toEqual([]);
    expect(DEFAULT_GRIB_REQUEST.params).toEqual([]);
    expect(DEFAULT_GRIB_REQUEST.subject).toBe('GRIB request');
  });
});

describe('ALL_GRIB_PARAMETERS', () => {
  it('exposes the full Saildocs parameter set', () => {
    expect(ALL_GRIB_PARAMETERS).toEqual(['PRMSL', 'WIND', 'HGT', 'SEATMP', 'AIRTMP', 'WAVES']);
  });
});

describe('parseForecastTimes', () => {
  it('empty input → empty array (Saildocs default applies)', () => {
    expect(parseForecastTimes('')).toEqual({ ok: true, value: [] });
    expect(parseForecastTimes('   ')).toEqual({ ok: true, value: [] });
  });

  it('parses comma-separated single hours', () => {
    expect(parseForecastTimes('24,48,72')).toEqual({
      ok: true,
      value: [{ Hour: 24 }, { Hour: 48 }, { Hour: 72 }],
    });
  });

  it('parses range syntax with ..', () => {
    expect(parseForecastTimes('6,12..96')).toEqual({
      ok: true,
      value: [{ Hour: 6 }, { Range: { start: 12, end: 96 } }],
    });
  });

  it('rejects negative hours', () => {
    const result = parseForecastTimes('-1');
    expect(result.ok).toBe(false);
  });

  it('rejects non-numeric segments', () => {
    const result = parseForecastTimes('24,abc');
    expect(result.ok).toBe(false);
  });

  it('rejects ranges where end <= start', () => {
    const result = parseForecastTimes('96..24');
    expect(result.ok).toBe(false);
    const equal = parseForecastTimes('24..24');
    expect(equal.ok).toBe(false);
  });

  it('rejects empty segments (trailing/leading commas)', () => {
    const result = parseForecastTimes('24,,48');
    expect(result.ok).toBe(false);
  });

  it('tolerates whitespace around numbers', () => {
    expect(parseForecastTimes(' 24 , 48 ')).toEqual({
      ok: true,
      value: [{ Hour: 24 }, { Hour: 48 }],
    });
  });
});
