// Request-first sections + cards for the Request Center (Task C2).
//
// `buildSections` is a PURE function (no React, no Tauri): given the loaded
// catalog entries and the operator's home grid, it produces the curated
// "request-first" home view — a small set of sections, each holding cards that
// resolve directly to a single action.
//
// A card's action is a tagged union (adrev revision #1):
//   - addCms(filename)     — clicking adds ONE cms BasketItem carrying that real
//                            catalog FILENAME. Basket items only ever carry real
//                            catalog filenames, never a category.
//   - openBrowse(category) — clicking navigates the browse view to that category.
//                            It does NOT mutate the basket. Used where a single
//                            filename can't represent the choice (Marine forecast
//                            spans a sea-area's many files; gateway lists span
//                            one PUB_ file per mode).
//
// Cards whose resolver returns null are omitted (e.g. a state with no state
// forecast, or an interior grid with no sea area). A section is omitted only if
// it ends up with zero cards — the national + nearby sections always have cards.

import type { CatalogEntry } from '../catalog/types';
import { gridToLatLon, gridToNwsZone, gridToRadarRegion, latLonToSeaArea, latLonToUsState } from './geo';
import { NATIONAL, zoneForecastEntry, radarEntry } from './catalogMap';
import { usStateName } from './usStateName';

export type CardAction =
  | { kind: 'addCms'; filename: string }
  | { kind: 'openBrowse'; category: string };

export interface RequestCard {
  /// Stable id (used for React keys); distinct from the human label.
  id: string;
  /// Human-facing card title; also the basket-item label for addCms cards.
  label: string;
  description?: string;
  /** Short contextual annotation (e.g. zone id · filename). */
  meta?: string;
  /** When true, render as the primary / hero card in the location section. */
  primary?: boolean;
  action: CardAction;
}

export interface RequestSection {
  id: string;
  title: string;
  /** Whether this section is geo-derived ('location') or location-independent ('national'). */
  kind: 'location' | 'national';
  cards: RequestCard[];
}

/**
 * Build the request-first home sections from the loaded catalog and the
 * operator's home grid (null when location is unset / unreadable).
 *
 * Geo-derived cards (State forecast, Marine forecast) are included only when
 * `grid` resolves and the resolver returns a non-null target. The national +
 * nearby cards are location-independent and always present.
 */
export function buildSections(
  entries: CatalogEntry[],
  grid: string | null,
): RequestSection[] {
  const sections: RequestSection[] = [];

  // --- Location (geo-derived: zone forecast + radar + marine) ----------------
  const locationCards: RequestCard[] = [];
  const latLon = grid ? gridToLatLon(grid) : null;
  if (latLon) {
    const zone = gridToNwsZone(latLon.lat, latLon.lon);
    if (zone) {
      const entry = zoneForecastEntry(entries, zone.id);
      if (entry) {
        locationCards.push({
          id: 'loc-zone-forecast',
          label: zone.name,
          description: 'Your NWS public forecast zone — the local text forecast for your grid. Returns text.',
          meta: `${zone.id} · ${entry.filename}`,
          primary: true,
          action: { kind: 'addCms', filename: entry.filename },
        });
      }
    }
    const radar = gridToRadarRegion(latLon.lat, latLon.lon);
    if (radar) {
      const entry = radarEntry(entries, radar.filename);
      if (entry) {
        locationCards.push({
          id: 'loc-radar',
          label: 'Regional radar',
          description: 'Current precipitation radar snapshot for your area. Returns an image.',
          meta: `${radar.name} · ${radar.filename}`,
          action: { kind: 'addCms', filename: entry.filename },
        });
      }
    }
    const seaArea = latLonToSeaArea(latLon.lat, latLon.lon);
    if (seaArea) {
      locationCards.push({
        id: 'loc-marine',
        label: 'Marine forecast',
        description: 'Wind, wave and sea-state forecasts for your offshore sea area. Returns text.',
        meta: seaArea,
        action: { kind: 'openBrowse', category: seaArea },
      });
    }
    // Always-on "Browse all <ST> weather" — the alternatives safety net. The
    // catalog carries coarse regional products for most states (the auto-resolved
    // zone card above can't cover every region, and ~7% of grids resolve to no
    // mapped product); this card guarantees the operator can always reach their
    // state's full weather set. State comes from the resolved zone, else the
    // state polygon (so it works even when no zone/product resolves).
    const state = zone?.state ?? latLonToUsState(latLon.lat, latLon.lon);
    if (state) {
      const category = `WX_US_${state}`;
      const count = entries.filter((e) => e.category === category).length;
      if (count > 0) {
        locationCards.push({
          id: 'loc-browse-all',
          label: `All ${usStateName(state) ?? state} forecasts`,
          description: 'Browse every weather product the catalog carries for your state.',
          meta: `${count} product${count === 1 ? '' : 's'}`,
          action: { kind: 'openBrowse', category },
        });
      }
    }
  }
  if (locationCards.length > 0) {
    sections.push({ id: 'weather', title: 'For your location', kind: 'location', cards: locationCards });
  }

  // --- Propagation & space (national, always shown, one-click) ---------------
  sections.push({
    id: 'propagation',
    title: 'Propagation & space',
    kind: 'national',
    cards: [
      {
        id: 'prop-forecast',
        label: 'Propagation forecast',
        description: '3-day HF propagation outlook.',
        action: { kind: 'addCms', filename: NATIONAL.propagation },
      },
      {
        id: 'prop-solar',
        label: 'Solar-terrestrial',
        description: 'Daily solar-terrestrial & WWV summary.',
        action: { kind: 'addCms', filename: NATIONAL.solar },
      },
      {
        id: 'prop-aurora',
        label: 'Aurora tonight',
        description: 'Tonight’s auroral activity forecast.',
        action: { kind: 'addCms', filename: NATIONAL.aurora },
      },
    ],
  });

  // --- Nearby stations (nationwide, always shown) ----------------------------
  sections.push({
    id: 'nearby',
    title: 'Nearby stations',
    kind: 'national',
    cards: [
      {
        id: 'nearby-gateways',
        label: 'Public gateway lists',
        description: 'RMS gateway frequency lists — pick your mode in browse.',
        action: { kind: 'openBrowse', category: 'WL2K_RMS' },
      },
      {
        id: 'nearby-winlink-info',
        label: 'Winlink info & how-to',
        description: 'Catalog inquiries help & getting-started guides.',
        action: { kind: 'addCms', filename: NATIONAL.winlinkInfo },
      },
    ],
  });

  return sections;
}
