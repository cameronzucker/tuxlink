// dialToFavorite — map a FavoriteDial (e.g. a Find-a-Station channel) into a NEW
// Favorite for favorite_upsert (tuxlink-5016).
//
// The backend's favorite_upsert MINTS the id, FORCES starred:false, and stamps
// created_at/updated_at/last_attempt_at for a new record (empty id), so the
// placeholders here are overwritten on the wire — they exist only to satisfy the
// Favorite shape. The caller stars the returned record separately via
// favorite_star (the only writer of `starred`).

import type { Favorite, FavoriteDial } from './types';
import { freqStringToCanonicalMhz } from '../radio/modes/freq';

export function dialToNewFavorite(dial: FavoriteDial): Favorite {
  return {
    id: '',
    mode: dial.mode,
    gateway: dial.gateway,
    freq: dial.freq,
    transport: dial.transport,
    band: dial.band,
    grid: dial.grid,
    // [R5-7] carry the P2P roster link through when the dial has one; absent
    // for ordinary CMS/gateway dials.
    contact_id: dial.contact_id,
    starred: false,
    created_at: '',
    updated_at: '',
  };
}

/** Stable lookup key for a dial/favorite — mode + case-folded gateway +
 *  channel (freq, falling back to transport for freq-less telnet dials).
 *
 *  tuxlink-ixasg (operator decision 2026-07-20): a favorite is a CHANNEL, not
 *  a station+mode. The earlier per-mode key made starring one KO0OOO VARA row
 *  paint the star on every KO0OOO VARA row — a misleading affordance sitting
 *  on per-channel data (the record stores ONE freq; the backend keys by id
 *  and the store's natural recents identity is (mode, gateway,
 *  freq|transport), which this key now mirrors). This REVERSES the
 *  sbf03-era rejection of a per-freq key: the claimed "backend unit" was the
 *  per-mode reading, not anything the store enforces.
 *
 *  The freq segment is canonicalized through freqStringToCanonicalMhz — the
 *  SAME magnitude heuristic the radio panels use (adrev 2026-07-20, both
 *  rounds P1): persisted favorites may carry the older kHz shape
 *  ("14105.0") while Finder/Contacts dials carry MHz ("14.105"); they are
 *  one physical channel and must produce one key. Unparseable strings pass
 *  through raw rather than keying on null.
 *
 *  Legacy freq-less RF records (older ribbon/observation writers) key as
 *  `mode|GATEWAY|` — DELIBERATE: they name no channel, so they light no
 *  channel row (the old per-mode key lit EVERY row, the reported bug). They
 *  remain visible and dialable in the Favorites panel; starring a real row
 *  creates a proper per-channel record alongside. No data is lost. */
export function favoriteKey(unit: {
  mode: string;
  gateway: string;
  freq?: string;
  transport?: string;
}): string {
  const raw = unit.freq?.trim();
  let channel: string;
  if (raw) {
    channel = freqStringToCanonicalMhz(raw) ?? raw;
  } else {
    channel = unit.transport ?? '';
  }
  return `${unit.mode}|${unit.gateway.trim().toUpperCase()}|${channel}`;
}
