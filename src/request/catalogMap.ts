// Pure catalog-mapping layer for the Request Center.
//
// Given the loaded catalog entries, resolve which catalog FILENAMES back the
// curated request-first cards. No React/Tauri imports — pure TypeScript so the
// mapping can be unit-tested in isolation and the UI layer only ever dispatches
// real filenames (CatalogEntry.filename), never a category.

import type { CatalogEntry } from '../catalog/types';

/// Real catalog FILENAMES (not categories) backing the national request cards.
/// Guarded against catalog drift by a real-file test in catalogMap.test.ts.
export const NATIONAL = {
  propagation: 'PROP_3DAY',
  solar: 'PROP_WWV',
  aurora: 'AUR_TONIGHT',
  winlinkInfo: 'INQUIRIES',
} as const;

/**
 * Return the best "state forecast" entry for a US state, looking in category
 * `WX_US_<USPS>`.
 *
 * - Prefer an entry whose description matches /state forecast/i AND whose
 *   filename does NOT contain `_TAB_` (the non-tabular, narrative forecast).
 * - Fall back to a tabular (`_TAB_`) state-forecast entry if no non-tabular
 *   one exists.
 * - Return null if the state has no state-forecast entry at all.
 */
export function bestStateForecast(
  entries: CatalogEntry[],
  usps: string,
): CatalogEntry | null {
  const category = `WX_US_${usps}`;
  const stateForecasts = entries.filter(
    (e) => e.category === category && /state forecast/i.test(e.description),
  );

  const nonTabular = stateForecasts.find((e) => !e.filename.includes('_TAB_'));
  if (nonTabular) {
    return nonTabular;
  }

  const tabular = stateForecasts.find((e) => e.filename.includes('_TAB_'));
  return tabular ?? null;
}

/**
 * Return the `WL2K_RMS` entries whose filename starts with `PUB_` — the public
 * gateway frequency lists by mode (PUB_ARDOP, PUB_PACKET, PUB_VARA, …), sorted
 * by filename.
 *
 * Consumed by the Task D1 browse pane (per-mode PUB_* enumeration). The C2
 * "Public gateway lists" card uses openBrowse('WL2K_RMS') to navigate there, so
 * this resolver is intentionally not wired into sections.ts — do not delete it
 * as "unused"; D1 selects the mode-specific list from these entries.
 */
export function gatewayListFilenames(entries: CatalogEntry[]): CatalogEntry[] {
  return entries
    .filter((e) => e.category === 'WL2K_RMS' && e.filename.startsWith('PUB_'))
    .sort((a, b) => a.filename.localeCompare(b.filename));
}
