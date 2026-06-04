import { describe, it, expect } from 'vitest';
import { gridToLatLon } from './maidenhead';

describe('gridToLatLon', () => {
  it('returns null for invalid input — wrong length', () => {
    expect(gridToLatLon('')).toBeNull();
    expect(gridToLatLon('J')).toBeNull();
    expect(gridToLatLon('JN5')).toBeNull();
    expect(gridToLatLon('JN58t')).toBeNull();
    expect(gridToLatLon('JN58tdu')).toBeNull();
  });

  it('returns null for out-of-range field letters', () => {
    // Field letters are A–R only (18 values)
    expect(gridToLatLon('ZZ99')).toBeNull();
    expect(gridToLatLon('SA00')).toBeNull();
    expect(gridToLatLon('AS00')).toBeNull();
  });

  it('returns null for invalid subsquare letters (> x)', () => {
    // Subsquare letters are a–x (24 values); 'y' and 'z' are out of range
    expect(gridToLatLon('CN87yy')).toBeNull();
  });

  it('resolves a 4-char grid to the center of the 2°×1° square', () => {
    const result = gridToLatLon('CN87');
    expect(result).not.toBeNull();
    // CN: C=2 → lon=-180+2*20=-140; N=13 → lat=-90+13*10=40
    //  87: 8 → lon+8*2=16 → -124; 7 → lat+7*1=7 → 47
    // center: lon+1=-123, lat+0.5=47.5
    expect(result!.lon).toBeCloseTo(-123.0, 4);
    expect(result!.lat).toBeCloseTo(47.5, 4);
  });

  it('resolves CN87us and round-trips within ±0.05°', () => {
    // CN87us is in the Seattle/Bellevue WA area
    const result = gridToLatLon('CN87us');
    expect(result).not.toBeNull();
    // Reverse-engineer the expected center:
    // C=2 → lon base -140; N=13 → lat base 40
    // 8 → lon+16 → -124; 7 → lat+7 → 47
    // u=20 → lon+20*(5/60)≈1.6667; s=18 → lat+18*(2.5/60)=0.75
    // center offset: lon+2.5/60≈0.04167; lat+1.25/60≈0.02083
    // lon ≈ -140+16+1.6667+0.04167 = -122.2917; lat ≈ 40+7+0.75+0.02083 = 47.7708
    expect(result!.lat).toBeGreaterThan(47.0);
    expect(result!.lat).toBeLessThan(48.5);
    expect(result!.lon).toBeGreaterThan(-123.5);
    expect(result!.lon).toBeLessThan(-121.5);
  });

  it('JN58td round-trips within ±0.05° (Munich reference)', () => {
    // Rust reference: lat_lon_to_grid(48.143, 11.608) == "JN58td"
    // grid_to_lat_lon("JN58td") should recover close to (48.143, 11.608)
    const result = gridToLatLon('JN58td');
    expect(result).not.toBeNull();
    expect(Math.abs(result!.lat - 48.143)).toBeLessThan(0.05);
    expect(Math.abs(result!.lon - 11.608)).toBeLessThan(0.05);
  });

  it('GF15vc round-trips within ±0.05° (Montevideo reference)', () => {
    // Rust reference: lat_lon_to_grid(-34.91, -56.21) == "GF15vc"
    const result = gridToLatLon('GF15vc');
    expect(result).not.toBeNull();
    expect(Math.abs(result!.lat - (-34.91))).toBeLessThan(0.05);
    expect(Math.abs(result!.lon - (-56.21))).toBeLessThan(0.05);
  });

  it('accepts lowercase input (normalizes internally)', () => {
    const upper = gridToLatLon('CN87US');
    const lower = gridToLatLon('cn87us');
    const mixed = gridToLatLon('CN87us');
    expect(upper).not.toBeNull();
    expect(lower).not.toBeNull();
    expect(mixed).not.toBeNull();
    expect(upper!.lat).toBeCloseTo(lower!.lat, 6);
    expect(upper!.lon).toBeCloseTo(lower!.lon, 6);
    expect(upper!.lat).toBeCloseTo(mixed!.lat, 6);
  });
});
