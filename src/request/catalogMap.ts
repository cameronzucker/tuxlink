// Pure catalog-mapping layer for the Request Center.
//
// Given the loaded catalog entries, resolve which catalog FILENAMES back the
// curated request-first cards. No React/Tauri imports — pure TypeScript so the
// mapping can be unit-tested in isolation and the UI layer only ever dispatches
// real filenames (CatalogEntry.filename), never a category.

import type { CatalogEntry } from '../catalog/types';
import zoneMap from './nws-zone-to-catalog.json';

const ZONE_MAP = (zoneMap as { map: Record<string, string> }).map;

/// Real catalog FILENAMES (not categories) backing the national request cards.
/// Guarded against catalog drift by a real-file test in catalogMap.test.ts.
export const NATIONAL = {
  propagation: 'PROP_3DAY',
  solar: 'PROP_WWV',
  aurora: 'AUR_TONIGHT',
  winlinkInfo: 'INQUIRIES',
} as const;

/** Resolve an NWS zone id → the backing catalog entry (via the vetted map),
 *  or null if the zone is unmapped or its filename isn't in the loaded catalog. */
export function zoneForecastEntry(entries: CatalogEntry[], zoneId: string): CatalogEntry | null {
  const filename = ZONE_MAP[zoneId];
  if (!filename) return null;
  return entries.find((e) => e.filename === filename) ?? null;
}

/** Resolve a radar region filename → its catalog entry, or null if absent. */
export function radarEntry(entries: CatalogEntry[], filename: string): CatalogEntry | null {
  return entries.find((e) => e.category === 'WX_US_RAD' && e.filename === filename) ?? null;
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
