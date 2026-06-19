/**
 * Regression guard (tuxlink-h17b): the basemap must render admin boundaries and
 * buildings in both flavors. The operator reported "no borders"; the layers were
 * present but the prior boundary color baked too muted to notice. This pins their
 * presence so a future refactor can't silently drop them again.
 */
import { describe, it, expect } from 'vitest';
import { buildBasemapStyle } from './basemapStyle';

describe('basemap layer presence (both flavors)', () => {
  for (const flavor of ['light', 'dark'] as const) {
    it(`includes admin boundary layers (${flavor})`, () => {
      const ids = buildBasemapStyle(flavor).layers.map((l) => l.id);
      expect(ids.some((id) => /boundar/i.test(id))).toBe(true);
    });

    it(`includes building layers (${flavor})`, () => {
      const ids = buildBasemapStyle(flavor).layers.map((l) => l.id);
      expect(ids.some((id) => /building/i.test(id))).toBe(true);
    });
  }
});
