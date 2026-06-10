// Station-list + reply DTOs for the Catalog Request Builder (bd-tuxlink-a2gd).
// Mirror the Rust serde shapes: ListingMode = kebab-case; Gateway/StationListing = camelCase;
// ReplyView = internally-tagged ({ kind, ... }) with a STRUCT raw variant (`text`), because
// serde cannot tag a newtype-of-String — see src-tauri/src/catalog/reply.rs.

import { asUiError } from '../mailbox/types';

export type ListingMode = 'vara-hf' | 'packet' | 'ardop-hf' | 'pactor' | 'robust-packet';

export const LISTING_MODES: { mode: ListingMode; label: string }[] = [
  { mode: 'vara-hf', label: 'VARA HF' },
  { mode: 'packet', label: 'Packet' },
  { mode: 'ardop-hf', label: 'ARDOP HF' },
  { mode: 'pactor', label: 'Pactor' },
  { mode: 'robust-packet', label: 'Robust Packet' },
];

export interface Gateway {
  channel: string;
  callsign: string;
  sysopName: string | null;
  grid: string | null;
  location: string | null;
  frequenciesKhz: number[];
  lastUpdate: string | null;
  email: string | null;
  homepage: string | null;
}

export interface StationListing {
  mode: ListingMode;
  title: string | null;
  gateways: Gateway[];
  raw: string;
  parsedOk: boolean;
  /// Unix millis the listing was fetched (for an "as of <time>" caption); null for an in-memory parse.
  fetchedAtMs: number | null;
}

// ---- catalog reply views (mirror src-tauri/src/catalog/reply.rs) ----

export interface ForecastDay {
  dow: string; // "Tue"
  date: string; // "Jun 09"
}
export interface ForecastCell {
  condition: string; // "Vryhot"
  low: string; // "77" (may be "MM"/"-"/"")
  high: string; // "106"
  popNight: string; // "00"
  popDay: string; // "00"
}
export interface ForecastLocation {
  name: string; // "Phoenix"
  cells: ForecastCell[];
}
export interface ForecastRegion {
  name: string; // "SOUTH-CENTRAL ARIZONA"
  locations: ForecastLocation[];
}
export interface ForecastPeriod {
  label: string; // "REST OF TONIGHT"
  text: string;
}
export interface ForecastZone {
  name: string; // "Western Mogollon Rim"
  cities: string; // "Flagstaff, Williams, and Munds Park"
  periods: ForecastPeriod[];
}

/// The decoded forecast body (internally tagged on `kind`).
export type Forecast =
  | { kind: 'tabular'; days: ForecastDay[]; regions: ForecastRegion[] }
  | { kind: 'zone'; zones: ForecastZone[] }
  | { kind: 'none' };

export type ReplyView =
  | {
      kind: 'area-weather';
      product: string;
      office: string;
      issued: string;
      title: string;
      forecast: Forecast;
      raw: string;
    }
  | { kind: 'raw'; text: string };

/// Extract a human-readable message from a thrown Tauri UiError (or anything).
/// Matches the `#[serde(tag="kind", content="detail")]` wire shape.
export function catalogErrorMessage(e: unknown): string {
  const ui = asUiError(e);
  if (!ui) return e instanceof Error ? e.message : String(e);
  switch (ui.kind) {
    case 'NotConfigured':
    case 'NotFound':
    case 'Rejected':
      return ui.detail;
    case 'AuthFailed':
    case 'Transport':
    case 'Unavailable':
      return ui.detail.reason;
    case 'Internal':
      return ui.detail.detail;
    case 'Cancelled':
      return 'cancelled';
  }
}
