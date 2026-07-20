// Station/channel aggregation for Find-a-Station (design §8).
// Collapses the per-mode StationListing[] (one row per callsign+SSID per mode)
// into Station pins keyed by (base callsign, grid), each carrying the expanded
// set of Channels = (mode, frequencyKhz, ssid?, band). One pin per location;
// one channel per (mode, dial). SSID is the packet connect target; HF channels
// share the base call.

import { bandForKhz, type Band } from './bandPlan';
import {
  bandwidthClass,
  type BandwidthClass,
  type GatewayAntenna,
  type ListingMode,
  type StationListing,
} from './stationTypes';

export interface Channel {
  mode: ListingMode;
  frequencyKhz: number;
  /** Packet connect target (e.g. N0DAJ-10); undefined for HF (base call dials). */
  ssid?: string;
  band: Band | null;
  /** Occupied bandwidth in Hz, sourced from the channels JSON API's
   *  `ChannelDetail.bandwidthHz` (Task 9); `null`/`undefined` for a channel
   *  sourced from the text-listing `frequenciesKhz` fallback (no per-channel
   *  bandwidth data) or a mode with no fixed bandwidth. See
   *  `stationMatchesFilters` for how this drives the bandwidth filter chips. */
  bandwidthHz?: number | null;
}

export interface Station {
  /** Aggregation key part 1 — SSID-stripped, upper-cased call. */
  baseCallsign: string;
  /** Aggregation key part 2 — Maidenhead grid (non-null; gridless rows dropped). */
  grid: string;
  sysopName: string | null;
  location: string | null;
  /** Distinct modes this station offers, for the pin's mode badges. */
  modes: ListingMode[];
  channels: Channel[];
  /** Most-recent fetch stamp across contributing listings (freshness caption). */
  fetchedAtMs: number | null;
  /** Gateway's self-reported antenna (B/D/V), if any listing carried one. Passed
   *  to the HF predictor as the far-end antenna; null → isotrope (never a whip). */
  gatewayAntenna: GatewayAntenna | null;
}

/** Strip a trailing -NN SSID and upper-case. */
export function baseCallsign(call: string): string {
  return call.trim().toUpperCase().replace(/-\d+$/, '');
}

function hasSsid(call: string): boolean {
  return /-\d+$/.test(call.trim());
}

export function aggregateStations(listings: StationListing[]): Station[] {
  const byKey = new Map<string, Station>();
  // Defensive: a malformed/empty backend response (null/undefined) must degrade
  // to an empty map, never crash the panel (production-mount-path robustness).
  if (!Array.isArray(listings)) return [];

  for (const listing of listings) {
    for (const g of listing.gateways) {
      const grid = g.grid?.trim();
      if (!grid) continue; // no grid → unplaceable on the map (spec: map needs lat/lon)
      const base = baseCallsign(g.callsign);
      const key = `${base}|${grid.toUpperCase()}`;

      let station = byKey.get(key);
      if (!station) {
        station = {
          baseCallsign: base, grid, sysopName: g.sysopName, location: g.location,
          modes: [], channels: [], fetchedAtMs: listing.fetchedAtMs,
          gatewayAntenna: g.antenna,
        };
        byKey.set(key, station);
      }
      // Fill in identity metadata from whichever listing first carries it.
      if (!station.sysopName && g.sysopName) station.sysopName = g.sysopName;
      if (!station.location && g.location) station.location = g.location;
      if (!station.gatewayAntenna && g.antenna) station.gatewayAntenna = g.antenna;
      if (listing.fetchedAtMs && (!station.fetchedAtMs || listing.fetchedAtMs > station.fetchedAtMs)) {
        station.fetchedAtMs = listing.fetchedAtMs;
      }
      const ssid = hasSsid(g.callsign) ? g.callsign.trim().toUpperCase() : undefined;

      // Task 9: prefer the channels-JSON-API detail rows when present; they
      // carry per-channel bandwidth (and, for a synthesized VARA FM listing,
      // the only source of dial data at all, since VARA FM has no text
      // listing). Each detail becomes exactly one Channel, keyed by its OWN
      // mode (join_channels on the Rust side already filters details to the
      // listing's own mode, but reading `d.mode` rather than `listing.mode`
      // keeps this correct even if that constraint ever loosens). Falls back
      // to the text-listing `frequenciesKhz` expansion when a gateway has no
      // channel details (channels-API join was best-effort and may be empty).
      if (g.channelDetails && g.channelDetails.length > 0) {
        for (const d of g.channelDetails) {
          if (!station.modes.includes(d.mode)) station.modes.push(d.mode);
          station.channels.push({
            mode: d.mode,
            frequencyKhz: d.frequencyKhz,
            ssid,
            band: bandForKhz(d.frequencyKhz),
            bandwidthHz: d.bandwidthHz,
          });
        }
      } else {
        if (!station.modes.includes(listing.mode)) station.modes.push(listing.mode);
        for (const khz of g.frequenciesKhz) {
          station.channels.push({ mode: listing.mode, frequencyKhz: khz, ssid, band: bandForKhz(khz) });
        }
      }
    }
  }

  return [...byKey.values()];
}

/** True when a channel's bandwidth passes the bandwidth filter (Task 9,
 *  load-bearing unknown-bandwidth rule): a channel whose bandwidth is
 *  `null`/`undefined`, OR whose Hz value does not classify into one of the
 *  three chip classes (`bandwidthClass` returns `null`), passes EVERY
 *  bandwidth filter: the chips can only SUBTRACT a channel with a KNOWN,
 *  classified, non-selected bandwidth. */
function channelPassesBandwidth(
  bandwidthHz: number | null | undefined,
  bandwidths: ReadonlySet<BandwidthClass>,
): boolean {
  const cls = bandwidthClass(bandwidthHz);
  if (cls == null) return true;
  return bandwidths.has(cls);
}

/**
 * Band + mode + bandwidth FILTER predicate (tuxlink-hlas, extended Task 9),
 * evaluated at the CHANNEL level. A station is visible iff it has at least one
 * channel whose band is selected, whose mode is enabled, AND whose bandwidth
 * passes the bandwidth filter (see `channelPassesBandwidth`). This is why a
 * 145 MHz packet station (band === 'vhf-uhf') disappears when only HF bands
 * are selected: that channel matches no selected band, and the station has no
 * other matching channel, and likewise why a station whose only 20m VARA
 * channel is 500 Hz disappears when only the 2300/2750 Hz chips are on, UNLESS
 * that same station also carries a null-bandwidth (or unclassified) channel
 * that satisfies band+mode, which keeps it visible under the "keep if ANY
 * channel passes" rule.
 */
export function stationMatchesFilters(
  station: Station,
  bands: ReadonlySet<Band>,
  modes: ReadonlySet<string>,
  bandwidths: ReadonlySet<BandwidthClass>,
): boolean {
  return station.channels.some(
    (c) =>
      c.band != null &&
      bands.has(c.band) &&
      modes.has(c.mode) &&
      channelPassesBandwidth(c.bandwidthHz, bandwidths),
  );
}

/** Format a `Channel.frequencyKhz` DIAL value for display (Task 10, the
 *  frequency hero + BandMatrix row badges): thousands-comma-grouped, always
 *  one decimal place, e.g. `7103.5` -> `"7,103.5 kHz"` and `14108` ->
 *  `"14,108.0 kHz"` (a whole-kHz channel still shows the ".0" so the column
 *  never jitters between one-decimal and zero-decimal rows). */
export function formatDialKhz(khz: number): string {
  const formatted = khz.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  return `${formatted} kHz`;
}
