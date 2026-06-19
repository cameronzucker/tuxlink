// src/aprs/stationCategories.ts
//
// Generic station-category filter for the APRS map. "Weather mode" is the first
// category; tuxlink-8fjx extends this list (vehicles/digipeaters/iGates) by
// adding predicates — the filter mechanism does not change. Each category is a
// pure predicate over a small per-station context.

export interface StationCategoryCtx {
  call: string;
  isWeather: boolean;
}

export interface StationCategory {
  key: string;
  label: string;
  matches(ctx: StationCategoryCtx): boolean;
}

export const CATEGORIES: StationCategory[] = [
  { key: 'all', label: 'All stations', matches: () => true },
  { key: 'weather', label: 'Weather', matches: (ctx) => ctx.isWeather },
];

export function categoryByKey(key: string): StationCategory {
  return CATEGORIES.find((c) => c.key === key) ?? CATEGORIES[0];
}
