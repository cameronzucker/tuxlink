import { describe, expect, it } from 'vitest';
import type { CatalogEntry } from '../catalog/types';
import { buildSections } from './sections';

/** Minimal CatalogEntry fixture helper. */
function entry(
  category: string,
  filename: string,
  description = '',
  size_bytes = 0,
): CatalogEntry {
  return { category, filename, description, size_bytes };
}

// ---------------------------------------------------------------------------
// Fixture catalog for the CN87 (Washington state / NE Pacific) test cases.
//
// CN87 decodes to lat≈47.5, lon≈-123.0:
//   - latLonToUsState → 'WA' (western Washington)
//   - latLonToSeaArea → 'WX_EASTPAC' (lat 32–49.5, lon -125.5 to -116.5)
//
// The fixture includes exactly one non-tabular WA state-forecast entry (which
// bestStateForecast prefers) plus all four NATIONAL filenames (PROP_3DAY,
// PROP_WWV, AUR_TONIGHT, INQUIRIES) in their respective categories. No WL2K_RMS
// PUB_* entries are needed here because the nearby-stations card uses
// openBrowse('WL2K_RMS'), not a resolved filename.
// ---------------------------------------------------------------------------
const FIXTURE_ENTRIES: CatalogEntry[] = [
  // WA state forecast (non-tabular; preferred by bestStateForecast over _TAB_ variants).
  entry('WX_US_WA', 'WA_FOR_WA', 'State Forecast for Washington', 4096),
  // National propagation + space filenames.
  entry('PROPAGATION', 'PROP_3DAY', '3-Day Propagation Forecast', 800),
  entry('PROPAGATION', 'PROP_WWV', 'Daily WWV Solar Flux summary', 621),
  entry('PROPAGATION', 'AUR_TONIGHT', 'Aurora Forecast Tonight', 900),
  // Winlink info.
  entry('INQUIRIES', 'INQUIRIES', 'Winlink Catalog Inquiries Help', 1200),
];

describe('buildSections — section kind tagging', () => {
  describe('with grid CN87 (Washington state + NE Pacific coast)', () => {
    const sections = buildSections(FIXTURE_ENTRIES, 'CN87');

    it('returns exactly three sections', () => {
      expect(sections).toHaveLength(3);
    });

    it('the Weather section has kind === "location"', () => {
      const weather = sections.find((s) => s.id === 'weather');
      expect(weather).toBeDefined();
      expect(weather?.kind).toBe('location');
    });

    it('the Propagation section has kind === "national"', () => {
      const propagation = sections.find((s) => s.id === 'propagation');
      expect(propagation).toBeDefined();
      expect(propagation?.kind).toBe('national');
    });

    it('the Nearby stations section has kind === "national"', () => {
      const nearby = sections.find((s) => s.id === 'nearby');
      expect(nearby).toBeDefined();
      expect(nearby?.kind).toBe('national');
    });

    it('the location section includes the marine card (CN87 resolves to WX_EASTPAC; zone+radar absent from this fixture)', () => {
      // CN87 (47.5, -123): zone=WAZ321→WA_ZON_HOCS (not in FIXTURE_ENTRIES) → no zone card;
      // radar=US.RAD.PSND but no WX_US_RAD entry in FIXTURE_ENTRIES → no radar card;
      // sea-area=WX_EASTPAC → loc-marine card present.
      const weather = sections.find((s) => s.id === 'weather');
      expect(weather?.cards.map((c) => c.id)).toContain('loc-marine');
      expect(weather?.cards.map((c) => c.id)).not.toContain('wx-state-forecast');
      expect(weather?.cards.map((c) => c.id)).not.toContain('wx-marine-forecast');
    });
  });

  describe('with grid null (no location set)', () => {
    const sections = buildSections(FIXTURE_ENTRIES, null);

    it('returns exactly two sections', () => {
      expect(sections).toHaveLength(2);
    });

    it('contains NO section with kind === "location"', () => {
      const locationSections = sections.filter((s) => s.kind === 'location');
      expect(locationSections).toHaveLength(0);
    });

    it('the Propagation section is present with kind === "national"', () => {
      const propagation = sections.find((s) => s.id === 'propagation');
      expect(propagation).toBeDefined();
      expect(propagation?.kind).toBe('national');
    });

    it('the Nearby stations section is present with kind === "national"', () => {
      const nearby = sections.find((s) => s.id === 'nearby');
      expect(nearby).toBeDefined();
      expect(nearby?.kind).toBe('national');
    });

    it('no Weather section is present (geo cards omitted without a grid)', () => {
      expect(sections.find((s) => s.id === 'weather')).toBeUndefined();
    });
  });

  describe('kind is present on every returned section', () => {
    it('all sections with CN87 have a kind field set to a valid value', () => {
      const sections = buildSections(FIXTURE_ENTRIES, 'CN87');
      for (const section of sections) {
        expect(['location', 'national']).toContain(section.kind);
      }
    });

    it('all sections with null grid have a kind field set to "national"', () => {
      const sections = buildSections(FIXTURE_ENTRIES, null);
      for (const section of sections) {
        expect(section.kind).toBe('national');
      }
    });
  });
});

// ---------------------------------------------------------------------------
// Task 10 — adaptive zone/radar/marine location hero section.
//
// CN87uo → lat≈47.6, lon≈-122.3:
//   - gridToNwsZone → WAZ315 "City of Seattle" → WA_ZON_SEA
//   - gridToRadarRegion → US.RAD.PSND
//   - latLonToSeaArea → WX_EASTPAC
//
// DM79 → lat=39.5, lon=-105 (Denver area):
//   - gridToNwsZone → null (zone falls outside the bundled set)
//   - gridToRadarRegion → US.RAD.CCO (but NOT in CAT below)
//   - latLonToSeaArea → null (inland)
//
// IO91 → UK grid → gridToNwsZone returns null + no US radar/sea → no location section.
// ---------------------------------------------------------------------------
const CAT = [
  { category: 'WX_US_WA', filename: 'WA_ZON_SEA', description: 'City of Seattle Washington Zone Forecast', size_bytes: 2500 },
  { category: 'WX_US_RAD', filename: 'US.RAD.PSND', description: 'SNAPSHOT CURRENT RADAR U.S. PUGET SOUND & SJDF', size_bytes: 20799 },
  { category: 'WX_EASTPAC', filename: 'EPAC_COASTAL', description: 'NE Pacific coastal waters', size_bytes: 7300 },
  { category: 'PROPAGATION', filename: 'PROP_3DAY', description: '3-day', size_bytes: 1 },
];
describe('buildSections location hero', () => {
  it('coastal grid → zone (primary) + radar + marine, in order', () => {
    const loc = buildSections(CAT, 'CN87uo').find((s) => s.kind === 'location')!;
    expect(loc.cards.map((c) => c.id)).toEqual(['loc-zone-forecast', 'loc-radar', 'loc-marine']);
    expect(loc.cards[0].primary).toBe(true);
    expect(loc.cards[0].label).toBe('City of Seattle');
    expect(loc.cards[0].action).toEqual({ kind: 'addCms', filename: 'WA_ZON_SEA' });
    expect(loc.cards[0].meta).toContain('WAZ315');
    expect(loc.cards[0].meta).toContain('WA_ZON_SEA');
    expect(loc.cards[1].action).toEqual({ kind: 'addCms', filename: 'US.RAD.PSND' });
    expect(loc.cards[2].action).toEqual({ kind: 'openBrowse', category: 'WX_EASTPAC' });
  });
  it('inland grid → no marine card', () => {
    const loc = buildSections(CAT, 'DM79').find((s) => s.kind === 'location');
    // DM79 is inland (latLonToSeaArea null) → marine omitted. Radar (US.RAD.CCO) present
    // only if CAT contains its filename — so this asserts marine ABSENCE specifically.
    if (loc) expect(loc.cards.some((c) => c.id === 'loc-marine')).toBe(false);
  });
  it('non-US grid → no location section', () => {
    expect(buildSections(CAT, 'IO91').find((s) => s.kind === 'location')).toBeUndefined();
  });
});
