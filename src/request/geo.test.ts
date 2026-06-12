import { describe, it, expect } from 'vitest';
import { gridToLatLon, latLonToUsState, latLonToSeaArea, gridToNwsZone, gridToRadarRegion } from './geo';
import zoneMap from './nws-zone-to-catalog.json';

describe('gridToLatLon', () => {
  it('decodes a 4-char square to its center (CN87 ≈ Seattle area)', () => {
    // CN87 center is lat 47.5, lon -123.0 — within ~1° of the Seattle area.
    const p = gridToLatLon('CN87');
    expect(p).not.toBeNull();
    expect(Math.abs(p!.lat - 47.5)).toBeLessThanOrEqual(1);
    expect(Math.abs(p!.lon - -122.0)).toBeLessThanOrEqual(1);
  });

  it('decodes EM26 to the central US (lat ~36, lon ~-96)', () => {
    // EM26 center is lat 36.5, lon -95.0 — central US, within ~1° of -96.
    const p = gridToLatLon('EM26');
    expect(p).not.toBeNull();
    expect(Math.abs(p!.lat - 36.0)).toBeLessThanOrEqual(1);
    expect(Math.abs(p!.lon - -96.0)).toBeLessThanOrEqual(1);
  });

  it('is case-insensitive', () => {
    const upper = gridToLatLon('CN87');
    const lower = gridToLatLon('cn87');
    expect(lower).toEqual(upper);
  });

  it('decodes a 2-char field to its center', () => {
    // CN field: lon = 2*20-180 = -140, +10 center = -130; lat = 13*10-90 = 40, +5 center = 45
    const p = gridToLatLon('CN');
    expect(p).not.toBeNull();
    expect(p!.lat).toBeCloseTo(45, 0);
    expect(p!.lon).toBeCloseTo(-130, 0);
  });

  it('decodes a 6-char subsquare to its center (CN87uo ≈ Seattle), not null (tuxlink-lfz4)', () => {
    // The app stores the grid at full 6-char precision; a strict 2/4-only
    // decode returned null here and collapsed the whole location section.
    const p = gridToLatLon('CN87uo');
    expect(p).not.toBeNull();
    expect(p!.lat).toBeCloseTo(47.6, 1);
    expect(p!.lon).toBeCloseTo(-122.3, 1);
    // The subsquare lands inside the CN87 square, so downstream resolution holds.
    expect(latLonToUsState(p!.lat, p!.lon)).toBe('WA');
    expect(latLonToSeaArea(p!.lat, p!.lon)).toBe('WX_EASTPAC');
  });

  it('returns null for empty string', () => {
    expect(gridToLatLon('')).toBeNull();
  });

  it('returns null for malformed input', () => {
    expect(gridToLatLon('ZZ99zz!')).toBeNull();
  });

  it('returns null for out-of-range field characters', () => {
    expect(gridToLatLon('ZZ99')).toBeNull();
  });
});

describe('latLonToUsState', () => {
  it('resolves Seattle to WA', () => {
    expect(latLonToUsState(47.6, -122.3)).toBe('WA');
  });

  it('resolves Portland to OR', () => {
    expect(latLonToUsState(45.5, -122.7)).toBe('OR');
  });

  it('resolves a Kansas-side Kansas-City-metro point to KS', () => {
    // Overland Park, KS — well inside KS, west of the state line (~-94.6)
    expect(latLonToUsState(38.98, -94.85)).toBe('KS');
  });

  it('resolves a Missouri-side Kansas-City-metro point to MO', () => {
    // Independence, MO — east of the state line
    expect(latLonToUsState(39.09, -94.41)).toBe('MO');
  });

  it('resolves an Oahu (HI) point via MultiPolygon iteration', () => {
    // Central Oahu (~Wahiawa). Honolulu proper is on the simplified polygon's
    // coarse southern edge; this point sits robustly inside the simplified
    // Oahu ring and still exercises the MultiPolygon path (HI is 5 polygons).
    expect(latLonToUsState(21.45, -158.0)).toBe('HI');
  });

  it('returns null for a mid-Pacific ocean point', () => {
    expect(latLonToUsState(40, -150)).toBeNull();
  });

  it('returns null for a point in central Canada', () => {
    // Near Winnipeg, MB
    expect(latLonToUsState(49.9, -97.1)).toBeNull();
  });
});

describe('gridToRadarRegion', () => {
  it('resolves Seattle to the tightest region (Puget Sound, not PNW)', () => {
    const { lat, lon } = gridToLatLon('CN87uo')!;
    expect(gridToRadarRegion(lat, lon)?.filename).toBe('US.RAD.PSND');
  });
  it('returns null for a point in no curated region (mid-ocean)', () => {
    expect(gridToRadarRegion(0, -150)).toBeNull();
  });
});

describe('gridToNwsZone', () => {
  it('resolves Seattle (CN87uo) to its NWS public zone', () => {
    const { lat, lon } = gridToLatLon('CN87uo')!;
    const zone = gridToNwsZone(lat, lon);
    expect(zone?.id).toBe('WAZ315');
    expect(zone?.name).toBe('City of Seattle');
  });
  it('returns null for an ocean/non-US point', () => {
    expect(gridToNwsZone(0, -150)).toBeNull();
  });
});

describe('latLonToSeaArea', () => {
  it('maps the Pacific NW (Seattle) to WX_EASTPAC', () => {
    expect(latLonToSeaArea(47.6, -122.3)).toBe('WX_EASTPAC');
  });

  it('maps Miami to WX_ATLANTIC (Atlantic takes precedence on the FL east coast)', () => {
    expect(latLonToSeaArea(25.8, -80.2)).toBe('WX_ATLANTIC');
  });

  it('maps New Orleans to WX_CAR_GULF', () => {
    expect(latLonToSeaArea(29.95, -90.07)).toBe('WX_CAR_GULF');
  });

  it('maps Boston to WX_ATLANTIC', () => {
    expect(latLonToSeaArea(42.4, -71.1)).toBe('WX_ATLANTIC');
  });

  it('returns null for inland Phoenix', () => {
    expect(latLonToSeaArea(33.4, -112.1)).toBeNull();
  });

  it('returns null for inland Denver', () => {
    expect(latLonToSeaArea(39.7, -104.99)).toBeNull();
  });

  it('returns null for inland Chicago', () => {
    expect(latLonToSeaArea(41.9, -87.6)).toBeNull();
  });
});

// Anti-blind-spot guard (tuxlink-z1b7 DoD #9): the prior design shipped wrong 3×
// because it was validated only on Seattle/WA. This table exercises multiple state
// SHAPES — coarse multi-office (AZ), fine per-zone (WA), coarse (MI), cross-state
// (NV) — through the real resolution path (grid → gridToNwsZone → product map).
// If any row regresses, the WA-only blind spot is recurring.
describe('weather resolution by grid (structural-edge guard)', () => {
  const MAP = (zoneMap as { map: Record<string, string> }).map;
  const primary = (grid: string): string | null => {
    const ll = gridToLatLon(grid);
    if (!ll) return null;
    const z = gridToNwsZone(ll.lat, ll.lon);
    return z ? (MAP[z.id] ?? null) : null;
  };
  it.each([
    ['DM33', 'AZ_TAB_PHOE', 'Phoenix 4-char (operator grid) — the reported bug'],
    ['DM33xk', 'AZ_TAB_PHOE', 'Phoenix 6-char'],
    ['DM45', 'AZ_ZON_NOFLA', 'Flagstaff / Northern AZ'],
    ['DM42', 'AZ_ZON_SE', 'Tucson / Southeast AZ'],
    ['CN87uo', 'WA_ZON_SEA', 'Seattle — 8-state per-zone precision preserved'],
    ['EN82', 'MI_ZON_SE', 'Detroit MI — coarse state'],
    ['DM09', 'NV_ZON_WESNE', 'Reno NV — cross-state shared region'],
  ])('%s → %s (%s)', (grid, want) => {
    expect(primary(grid)).toBe(want);
  });
});
