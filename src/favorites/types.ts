// Frontend DTOs for the Favorites feature — Task B3.
//
// These MUST mirror the Rust serde shapes EXACTLY (snake_case; the codebase has
// no `rename_all`). Sources of truth:
//   - `src-tauri/src/favorites/store.rs` — Favorite / ConnectionAttempt /
//     FavoriteDial / TodHint / StationsFile
// When a Rust shape changes, this file MUST be updated in the same PR.

/// The five radio modes a favorite may belong to. Mirrors the VALID_MODES
/// constant in `src-tauri/src/favorites/commands.rs`.
export type RadioMode = 'vara-hf' | 'vara-fm' | 'ardop-hf' | 'packet' | 'telnet';

/// A single per-mode favorite/recent station. `id` is server-assigned and is
/// the join key for `ConnectionAttempt.unit_id`. `last_attempt_at` is bumped on
/// every recorded attempt and is the LRU-dialed eviction key (M3). `freq` is
/// record-only metadata (never read back into a form, H8). `transport` is the
/// telnet-only `"CmsSsl" | "Telnet"` discriminator (H7). `contact_id` [R5-7] links
/// this favorite to a P2P roster entry when the recent originated from (or was
/// matched to) a peer.
export interface Favorite {
  id: string;
  mode: RadioMode;
  gateway: string;
  freq?: string;
  transport?: 'CmsSsl' | 'Telnet';
  band?: string;
  grid?: string;
  note?: string;
  contact_id?: string;
  starred: boolean;
  last_attempt_at?: string;
  created_at: string;
  updated_at: string;
}

/// One empirical connection attempt against a unit. `unit_id` is stamped
/// SERVER-SIDE (H3) — the client never supplies it. `ts_local` is an
/// offset-bearing ISO8601 string stored VERBATIM — NEVER converted to UTC (H1).
export interface ConnectionAttempt {
  unit_id: string;
  ts_local: string;
  freq?: string;
  outcome: 'reached' | 'failed';
}

/// The record-path DTO (H3/Codex#8). Carries everything needed to upsert/find
/// the unit; the client passes this (NOT a `unit_id`) to `favorite_record_attempt`.
/// `contact_id` [R5-7] carries the P2P roster link through to a brand-new
/// recent's `Favorite.contact_id`.
export interface FavoriteDial {
  mode: RadioMode;
  gateway: string;
  freq?: string;
  transport?: 'CmsSsl' | 'Telnet';
  band?: string;
  grid?: string;
  contact_id?: string;
}

/// The whole on-disk stations file (mirrors Rust StationsFile), returned by
/// `favorites_read`.
export interface StationsFile {
  schema_version: number;
  favorites: Favorite[];
  log: ConnectionAttempt[];
}

/// The gated time-of-day record surfaced by the (B5) `favorite_tod_hint`
/// command (mirrors Rust TodHint). Observed counts only — never a prediction,
/// never a zero-success bucket (H2).
export interface TodHint {
  bucket: 'dawn' | 'day' | 'dusk' | 'night';
  attempts: number;
  successes: number;
}
