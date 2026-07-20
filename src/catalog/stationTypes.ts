// Station-list + reply DTOs for the Catalog Request Builder (bd-tuxlink-a2gd).
// Mirror the Rust serde shapes: ListingMode = kebab-case; Gateway/StationListing = camelCase;
// ReplyView = internally-tagged ({ kind, ... }) with a STRUCT raw variant (`text`), because
// serde cannot tag a newtype-of-String — see src-tauri/src/catalog/reply.rs.

import { asUiError } from '../mailbox/types';

// tuxlink-nkzng (Task 8, Rust) added the channels JSON API and the VARA FM mode
// it is sourced from. VARA FM has no text `/listings/` endpoint (see the Rust
// `ListingMode::ALL` doc comment) so it is deliberately NOT added to
// `LISTING_MODES` below (that list mirrors the confirmed text-endpoint set);
// it is fetched by name and synthesized server-side instead.
export type ListingMode = 'vara-hf' | 'packet' | 'ardop-hf' | 'pactor' | 'robust-packet' | 'vara-fm';

/// Occupied-bandwidth classes the bandwidth filter chips offer (Task 9). Mirrors
/// the fixed HF bandwidths VARA/ARDOP channels report on the wire (VARA:
/// 500/2300/2750 Hz; ARDOP additionally reports 1000/2000 Hz, which have no chip
/// and are therefore treated the same as an unclassified/unknown bandwidth by
/// `bandwidthClass` below; see `stationMatchesFilters` in stationModel.ts for
/// the load-bearing "unknown passes every filter" rule).
export type BandwidthClass = '500' | '2300' | '2750';

export const BANDWIDTH_CLASSES: BandwidthClass[] = ['500', '2300', '2750'];

/** Classify a channel's occupied bandwidth (Hz) into one of the three filter
 *  chip classes, or `null` when the value is missing OR does not match one of
 *  the three known classes (e.g. ARDOP's 1000/2000 Hz). `null` is the signal
 *  `stationMatchesFilters` uses to let the channel pass every bandwidth filter:
 *  a chip can only SUBTRACT a channel whose bandwidth it can classify. */
export function bandwidthClass(hz: number | null | undefined): BandwidthClass | null {
  switch (hz) {
    case 500:
      return '500';
    case 2300:
      return '2300';
    case 2750:
      return '2750';
    default:
      return null;
  }
}

/// One per-channel row from the Winlink gateway channels JSON API
/// (tuxlink-nkzng, Task 8), joined onto a `Gateway` by callsign. Mirrors
/// `src-tauri/src/catalog/stations.rs::ChannelDetail` (camelCase on the wire).
export interface ChannelDetail {
  /** DIAL frequency in kHz: the API reports dial Hz directly, divided by
   *  1000; no audio-center offset math. */
  frequencyKhz: number;
  /** Occupied bandwidth in Hz when the mode implies a fixed one; null when it
   *  doesn't (VARA FM, Packet, Pactor, Robust Packet) or wasn't reported. */
  bandwidthHz: number | null;
  mode: ListingMode;
  operatingHours: string | null;
  grid: string | null;
}

/// The gateway's self-reported "Antenna being used" code, parsed from the listing
/// (legend: B = Beam, D = Dipole, V = Vertical). Mirrors the Rust `GatewayAntenna`
/// serde lowercase shape. Drives the far-end antenna model in the HF predictor.
export type GatewayAntenna = 'beam' | 'dipole' | 'vertical';

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
  /// Self-reported antenna code (B/D/V), if the listing carried one. Drives the
  /// far-end antenna model in the HF predictor; null → isotrope (never a whip).
  antenna: GatewayAntenna | null;
  /// Per-channel bandwidth/frequency rows from the channels JSON API
  /// (tuxlink-nkzng, Task 8), joined by callsign. Omitted/empty for a gateway
  /// not present in the channels feed; `aggregateStations` (Task 9) prefers
  /// these over `frequenciesKhz` when present.
  channelDetails?: ChannelDetail[];
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
