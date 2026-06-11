// Station/channel aggregation for Find-a-Station (design §8).
// Collapses the per-mode StationListing[] (one row per callsign+SSID per mode)
// into Station pins keyed by (base callsign, grid), each carrying the expanded
// set of Channels = (mode, frequencyKhz, ssid?, band). One pin per location;
// one channel per (mode, dial). SSID is the packet connect target; HF channels
// share the base call.

import { bandForKhz, type Band } from './bandPlan';
import type { Gateway, ListingMode, StationListing } from './stationTypes';

export interface Channel {
  mode: ListingMode;
  frequencyKhz: number;
  /** Packet connect target (e.g. N0DAJ-10); undefined for HF (base call dials). */
  ssid?: string;
  band: Band | null;
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
        };
        byKey.set(key, station);
      }
      // Fill in identity metadata from whichever listing first carries it.
      if (!station.sysopName && g.sysopName) station.sysopName = g.sysopName;
      if (!station.location && g.location) station.location = g.location;
      if (listing.fetchedAtMs && (!station.fetchedAtMs || listing.fetchedAtMs > station.fetchedAtMs)) {
        station.fetchedAtMs = listing.fetchedAtMs;
      }
      if (!station.modes.includes(listing.mode)) station.modes.push(listing.mode);

      const ssid = hasSsid(g.callsign) ? g.callsign.trim().toUpperCase() : undefined;
      for (const khz of g.frequenciesKhz) {
        station.channels.push({ mode: listing.mode, frequencyKhz: khz, ssid, band: bandForKhz(khz) });
      }
    }
  }

  return [...byKey.values()];
}
