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
import { gridToLatLon, latLonToUsState, latLonToSeaArea } from './geo';
import { NATIONAL, bestStateForecast } from './catalogMap';

export type CardAction =
  | { kind: 'addCms'; filename: string }
  | { kind: 'openBrowse'; category: string };

export interface RequestCard {
  /// Stable id (used for React keys); distinct from the human label.
  id: string;
  /// Human-facing card title; also the basket-item label for addCms cards.
  label: string;
  description?: string;
  action: CardAction;
}

export interface RequestSection {
  id: string;
  title: string;
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

  // --- Weather (geo-derived) -------------------------------------------------
  const weatherCards: RequestCard[] = [];
  const latLon = grid ? gridToLatLon(grid) : null;
  if (latLon) {
    const state = latLonToUsState(latLon.lat, latLon.lon);
    if (state) {
      const forecast = bestStateForecast(entries, state);
      if (forecast) {
        weatherCards.push({
          id: 'wx-state-forecast',
          label: 'State forecast',
          description: forecast.description,
          action: { kind: 'addCms', filename: forecast.filename },
        });
      }
    }

    const seaArea = latLonToSeaArea(latLon.lat, latLon.lon);
    if (seaArea) {
      weatherCards.push({
        id: 'wx-marine-forecast',
        label: 'Marine forecast',
        description: 'Coastal & offshore marine forecasts for your sea area.',
        action: { kind: 'openBrowse', category: seaArea },
      });
    }
  }
  if (weatherCards.length > 0) {
    sections.push({ id: 'weather', title: 'Weather', cards: weatherCards });
  }

  // --- Propagation & space (national, always shown, one-click) ---------------
  sections.push({
    id: 'propagation',
    title: 'Propagation & space',
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
