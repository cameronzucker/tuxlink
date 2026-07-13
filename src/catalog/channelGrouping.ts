// Right-rail channel presentation (design §7): group a station's channels by
// mode (each frequency once, ascending), attach per-channel reliability from the
// path prediction, and build the FavoriteDial handed to the modem on Use →.

import type { Channel, Station } from './stationModel';
import type { PathPrediction } from './propagationApi';
import { relToTier, type ReachTier } from './reachability';
import type { FavoriteDial, RadioMode } from '../favorites/types';
import type { ListingMode } from './stationTypes';

export interface ChannelGroup {
  mode: ListingMode;
  channels: Channel[];
}

const MODE_ORDER: ListingMode[] = ['vara-hf', 'ardop-hf', 'packet', 'pactor', 'robust-packet'];

export function groupChannelsByMode(station: Station): ChannelGroup[] {
  const byMode = new Map<ListingMode, Channel[]>();
  for (const ch of station.channels) {
    const list = byMode.get(ch.mode) ?? [];
    list.push(ch);
    byMode.set(ch.mode, list);
  }
  return MODE_ORDER.filter((m) => byMode.has(m)).map((mode) => ({
    mode,
    channels: byMode.get(mode)!.slice().sort((a, b) => a.frequencyKhz - b.frequencyKhz),
  }));
}

/** ListingMode → modem RadioMode; null for modes with no prefillable modem. */
function radioModeFor(mode: ListingMode): RadioMode | null {
  if (mode === 'vara-hf' || mode === 'ardop-hf' || mode === 'packet') return mode;
  return null;
}

// Preserve fractional-kHz catalog centers (e.g. 14112.5 kHz → "14.1125"):
// three decimals rounded a .5 kHz center to the wrong kHz, which put the
// derived sideband dial 500 Hz off (tuxlink-9pzaj, Codex adrev P2 #3).
// Whole-kHz channels keep the conventional 3-decimal form ("145.570").
export const mhz = (khz: number): string =>
  Number.isInteger(khz) ? (khz / 1000).toFixed(3) : (khz / 1000).toFixed(4);

export function channelToDial(station: Station, channel: Channel): FavoriteDial | null {
  const mode = radioModeFor(channel.mode);
  if (!mode) return null;
  return {
    mode,
    gateway: channel.ssid ?? station.baseCallsign,
    freq: mhz(channel.frequencyKhz),
    grid: station.grid,
  };
}

export interface ChannelReliabilityResult {
  rel: number;
  tier: ReachTier;
}

export function channelReliability(
  channel: Channel,
  prediction: PathPrediction,
  utcHour: number,
): ChannelReliabilityResult | null {
  if (channel.band === 'vhf-uhf' || channel.band == null) return null;
  const pc = prediction.channels.find((c) => c.frequencyKhz === channel.frequencyKhz);
  if (!pc) return null;
  const rel = pc.relByHour[utcHour] ?? 0;
  return { rel, tier: relToTier(rel) };
}
