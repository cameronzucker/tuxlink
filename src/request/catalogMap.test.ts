import { describe, expect, it } from 'vitest';
import type { CatalogEntry } from '../catalog/types';
import {
  NATIONAL,
  bestStateForecast,
  gatewayListFilenames,
} from './catalogMap';
import zonesGeo from './nws-zones.geo.json';
import zoneMap from './nws-zone-to-catalog.json';
import unmappedJson from './nws-zone-unmapped.json';

/** Build a CatalogEntry with sensible defaults for fixtures. */
function entry(partial: Partial<CatalogEntry> & Pick<CatalogEntry, 'category' | 'filename'>): CatalogEntry {
  return {
    description: '',
    size_bytes: 0,
    ...partial,
  };
}

describe('bestStateForecast', () => {
  it('prefers a non-tabular state-forecast entry over a tabular one', () => {
    const entries: CatalogEntry[] = [
      entry({ category: 'WX_US_WA', filename: 'WA_FOR_WA', description: 'State Forecast for Washington' }),
      entry({ category: 'WX_US_WA', filename: 'WA_TAB_NW', description: 'Tabular State Forecast for Northwest Washington' }),
    ];

    const result = bestStateForecast(entries, 'WA');

    expect(result?.filename).toBe('WA_FOR_WA');
  });

  it('falls back to a tabular state-forecast entry when no non-tabular exists', () => {
    const entries: CatalogEntry[] = [
      entry({ category: 'WX_US_AZ', filename: 'AZ_TAB_PHOE', description: 'Tabular State Forecast for Arizona Phoenix NWS' }),
      entry({ category: 'WX_US_AZ', filename: 'AZ_ZON_FOO', description: 'Zone Forecast for somewhere in Arizona' }),
    ];

    const result = bestStateForecast(entries, 'AZ');

    expect(result?.filename).toBe('AZ_TAB_PHOE');
  });

  it('returns null for a state with no state-forecast entry', () => {
    const entries: CatalogEntry[] = [
      entry({ category: 'WX_US_AK', filename: 'AK_TAB_JUNE', description: 'Tabular Forecast Alaska Juneau NWS' }),
      entry({ category: 'WX_US_AK', filename: 'AK_ZON_ANC1', description: 'Zone Forecast Alaska Anchorage NWS' }),
    ];

    const result = bestStateForecast(entries, 'AK');

    expect(result).toBeNull();
  });

  it('only matches the requested state category', () => {
    const entries: CatalogEntry[] = [
      entry({ category: 'WX_US_OR', filename: 'OR_FOR_OR', description: 'State Forecast for Oregon' }),
    ];

    const result = bestStateForecast(entries, 'WA');

    expect(result).toBeNull();
  });
});

describe('NATIONAL constants — real-catalog guard', () => {
  // Read the REAL bundled catalog so this test fails loudly if the catalog ever
  // drops a NATIONAL filename. Per docs/pitfalls/implementation-pitfalls.md
  // TEST-1, filesystem-scan tests in this Vite frontend use import.meta.glob
  // with ?raw (NOT node:fs) so both vitest and `tsc --noEmit` stay green.
  const catalogModules = import.meta.glob(
    '/src-tauri/resources/catalog/winlink-queries.txt',
    { eager: true, query: '?raw', import: 'default' },
  ) as Record<string, string>;
  const raw = Object.values(catalogModules)[0];
  const realEntries: CatalogEntry[] = raw
    .split(/\r?\n/)
    .map((line) => line.replace(/^﻿/, ''))
    .filter((line) => line.includes('|'))
    .map((line) => {
      const [category, filename, description, size] = line.split('|');
      return {
        category,
        filename,
        description: description ?? '',
        size_bytes: Number(size ?? 0),
      };
    });
  const filenames = new Set(realEntries.map((e) => e.filename));

  it('every NATIONAL filename is present in the bundled catalog', () => {
    for (const filename of Object.values(NATIONAL)) {
      expect(filenames.has(filename), `missing NATIONAL filename: ${filename}`).toBe(true);
    }
  });

  it('at least one WL2K_RMS PUB_* gateway entry exists', () => {
    const pubGateways = realEntries.filter(
      (e) => e.category === 'WL2K_RMS' && e.filename.startsWith('PUB_'),
    );
    expect(pubGateways.length).toBeGreaterThan(0);
  });

  it('some WX_US_<ST> state-forecast entries exist', () => {
    const stateForecasts = realEntries.filter(
      (e) => /^WX_US_[A-Z]{2}$/.test(e.category) && /state forecast/i.test(e.description),
    );
    expect(stateForecasts.length).toBeGreaterThan(0);
  });
});

describe('gatewayListFilenames', () => {
  it('returns only WL2K_RMS PUB_ entries, sorted by filename', () => {
    const entries: CatalogEntry[] = [
      entry({ category: 'WL2K_RMS', filename: 'PUB_PACKET', description: 'Packet Public Gateways Frequency List' }),
      entry({ category: 'WL2K_RMS', filename: 'PUB_ARDOP', description: 'ARDOP Public Gateways Frequency List' }),
      entry({ category: 'WL2K_RMS', filename: 'OTHER_RMS', description: 'Some non-public RMS list' }),
      entry({ category: 'WX_US_WA', filename: 'PUB_DECOY', description: 'Not WL2K_RMS so excluded' }),
    ];

    const result = gatewayListFilenames(entries);

    expect(result.map((e) => e.filename)).toEqual(['PUB_ARDOP', 'PUB_PACKET']);
  });

  it('returns an empty array when no PUB_ gateway entries exist', () => {
    const entries: CatalogEntry[] = [
      entry({ category: 'WL2K_RMS', filename: 'OTHER_RMS', description: 'non-public' }),
    ];

    expect(gatewayListFilenames(entries)).toEqual([]);
  });
});

describe('NWS zone mapping referential integrity', () => {
  it('every mapped NWS zone id exists in the bundled geometry', () => {
    const geoIds = new Set(
      (zonesGeo as { features: { properties: { id: string } }[] }).features.map(
        (f) => f.properties.id,
      ),
    );
    const orphan = Object.keys(
      (zoneMap as { map: Record<string, string> }).map,
    ).filter((id) => !geoIds.has(id));
    expect(orphan, `Mapped zone ids absent from geometry:\n${orphan.join('\n')}`).toEqual([]);
  });
});

describe('NWS zone mapping completeness (DoD #5)', () => {
  // Per TEST-1 (docs/pitfalls/implementation-pitfalls.md): filesystem-scan tests
  // in this Vite frontend use import.meta.glob with ?raw, NOT node:fs, so both
  // vitest and `tsc --noEmit` stay green.
  const catalogModules = import.meta.glob(
    '/src-tauri/resources/catalog/winlink-queries.txt',
    { eager: true, query: '?raw', import: 'default' },
  ) as Record<string, string>;
  const catalogRaw = Object.values(catalogModules)[0];

  // Parse zone-forecast entries the same way as the Rust catalog parser:
  // pipe-delimited, BOM-stripped, filter to WX_US_<ST> + description matching
  // /zone forecast/i (DoD #5 completeness-test target rule).
  const catalogZoneForecasts: string[] = (catalogRaw ?? '')
    .split(/\r?\n/)
    .map((l) => l.replace(/^﻿/, '').trim())
    .filter(Boolean)
    .flatMap((l) => {
      const [category, filename, description] = l.split('|');
      if (
        /^WX_US_[A-Z]{2}$/.test(category ?? '') &&
        /zone forecast/i.test(description ?? '')
      ) {
        return [filename];
      }
      return [];
    });

  it('every catalog zone-forecast filename is mapped or explicitly unmapped-by-design', () => {
    const mappedFilenames = new Set(
      Object.values((zoneMap as { map: Record<string, string> }).map),
    );
    const unmappedFilenames = new Set(
      Object.keys((unmappedJson as { unmapped: Record<string, string> }).unmapped),
    );
    const missing = catalogZoneForecasts.filter(
      (f) => !mappedFilenames.has(f) && !unmappedFilenames.has(f),
    );
    expect(missing, `Unresolved catalog zone forecasts:\n${missing.join('\n')}`).toEqual([]);
  });
});
