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

    it('the Weather section includes both State forecast and Marine forecast cards', () => {
      const weather = sections.find((s) => s.id === 'weather');
      expect(weather?.cards.map((c) => c.id)).toContain('wx-state-forecast');
      expect(weather?.cards.map((c) => c.id)).toContain('wx-marine-forecast');
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
