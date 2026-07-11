// dialToFavorite — map a FavoriteDial (e.g. a Find-a-Station channel) into a NEW
// Favorite for favorite_upsert (tuxlink-5016).
//
// The backend's favorite_upsert MINTS the id, FORCES starred:false, and stamps
// created_at/updated_at/last_attempt_at for a new record (empty id), so the
// placeholders here are overwritten on the wire — they exist only to satisfy the
// Favorite shape. The caller stars the returned record separately via
// favorite_star (the only writer of `starred`).

import type { Favorite, FavoriteDial } from './types';

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

/** Stable lookup key for a dial/favorite — mode + case-folded gateway. The same
 *  callsign in two modes is two distinct units (favorites are per-mode). */
export function favoriteKey(modeAndGateway: { mode: string; gateway: string }): string {
  return `${modeAndGateway.mode}|${modeAndGateway.gateway.trim().toUpperCase()}`;
}
