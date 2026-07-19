// Frontend DTOs for the Contacts feature — Task A4, grown to the v2
// reachability superset by Task T-E (operator pivot 2026-07-10/11: a peer IS a
// contact; the separate peers.json entity died — see
// docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md §AMENDMENT).
//
// These MUST mirror the Rust serde shapes EXACTLY (snake_case; the codebase has
// no `rename_all` EXCEPT the enums below, which mirror `reachability.rs`'s
// `#[serde(rename_all = "kebab-case")]`). Sources of truth:
//   - `src-tauri/src/contacts/store.rs` — Contact / GroupMember / Group / ContactsFile
//   - `src-tauri/src/contacts/reachability.rs` — ContactTier / Origin / GridSource /
//     ContactGrid / ChannelTransport / Direction / Provenance / ChannelBandwidth /
//     AttemptCounts / Channel / Endpoint (the v2 reachability fields on Contact)
//   - `src-tauri/src/contacts/suggest.rs` — Suggestion
// When a Rust shape changes, this file MUST be updated in the same PR.
//
// The v2 fields (`tier`, `origin`, `grid`, `channels`, `endpoints`) all carry
// `#[serde(default)]` on the Rust side, so a `contact_upsert` write payload MAY
// omit them (the backend backfills the default) — hence `?` optional here,
// mirroring this file's own existing convention for `email`/`tactical`/`notes`
// (Option<String> fields) rather than peers/types.ts's stricter `| null`
// idiom. `contacts_read` always emits all five keys explicitly (tier/origin as
// their concrete kebab-case value, channels/endpoints as `[]` when empty, grid
// as `null` when absent) — callers on the read path should not assume absence.

/// Mirrors `reachability.rs::ContactTier`. `Confirmed` = curated (operator
/// added/confirmed) — the pre-pivot Contact semantics. `Unconfirmed` =
/// auto-created from a P2P observation or manual dial.
export type ContactTier = 'confirmed' | 'unconfirmed' | 'unknown';

/// Mirrors `reachability.rs::Origin` — plain-language provenance of the record.
export type Origin = 'incoming' | 'outgoing' | 'manual' | 'aprs' | 'unknown';

/// Mirrors `reachability.rs::GridSource`. NOTE: the pivot DROPPED the
/// peer-model's `'contact'` variant (a contact sourcing its grid "from a
/// contact" is meaningless now that the grid lives ON the contact) — this is
/// NOT the same union as the deleted `peers/types.ts::GridSource`.
export type GridSource = 'aprs' | 'manual' | 'unknown';

/// Mirrors `reachability.rs::ChannelTransport`.
export type ChannelTransport = 'packet' | 'ardop' | 'vara-hf' | 'vara-fm' | 'unknown';

/// Mirrors `reachability.rs::Direction`.
export type Direction = 'incoming' | 'outgoing' | 'unknown';

/// Mirrors `reachability.rs::ChannelSource` (tuxlink-6vn4x). `'manual'` = an
/// operator-entered radio dial (the ContactEditor "Radio dials" rows);
/// `'observed'` = a row minted by an actual on-air observation. Absent on a
/// pre-migration record ⇒ treat as observed (the historical population).
export type ChannelSource = 'observed' | 'manual';

/// Mirrors `reachability.rs::Provenance`. `'operator'` is the ONLY
/// agent-dialable provenance — never derive dialability from any other value.
export type Provenance = 'operator' | 'observed-incoming' | 'unknown';

/// Mirrors `reachability.rs::ChannelBandwidth` — internally tagged on `"kind"`
/// (`#[serde(tag = "kind", rename_all = "kebab-case")]`).
export type ChannelBandwidth =
  | { kind: 'hz'; hz: number }
  | { kind: 'wide' }
  | { kind: 'narrow' }
  | { kind: 'unknown' };

/// Mirrors `reachability.rs::ContactGrid`.
export interface ContactGrid {
  value: string;
  source: GridSource;
}

/// Mirrors `reachability.rs::AttemptCounts`.
export interface AttemptCounts {
  ok: number;
  fail: number;
}

/// Mirrors `reachability.rs::Channel` — one RF reachability observation row.
/// Dedup key (backend): `(transport, target_callsign, via, freq_hz, bandwidth)`.
export interface Channel {
  transport: ChannelTransport;
  target_callsign: string;
  /// `'manual'` (operator-entered dial) vs `'observed'` (on-air observation).
  /// `#[serde(default)]` on the Rust side → optional here per this file's
  /// convention; absent means observed. Only `'manual'` rows are editable in
  /// the ContactEditor "Radio dials" section; observed rows pass through
  /// untouched on save.
  source?: ChannelSource;
  via: string[];
  freq_hz: number | null;
  bandwidth: ChannelBandwidth | null;
  direction: Direction;
  counts: AttemptCounts;
  /// Most recent attempt (OK or FAIL) — bumps on failures too, so NEVER derive
  /// a "reached / heard" claim from it. Use `last_ok`.
  last_seen: string;
  /// Most recent SUCCESSFUL attempt; `null` until one completes (T-F Part 0).
  /// The ONLY honest source for a reachability label — a failed dial never
  /// sets it. `#[serde(default)]` on the Rust side → present as `null` when
  /// absent.
  last_ok: string | null;
  /// Direction of the `last_ok` success, set atomically WITH it (ok outcomes
  /// only). `direction` mutates on every observation — failures included — so
  /// the "heard" (incoming) vs "reached" (outgoing) verb MUST key on this
  /// field, never on `direction`. `null` on pre-T-F records / never-succeeded
  /// channels: render a direction-neutral form rather than guessing.
  last_ok_direction: Direction | null;
}

/// Mirrors `reachability.rs::Endpoint` — one network reachability row (telnet
/// P2P).
export interface Endpoint {
  id: string;
  host: string;
  port: number;
  provenance: Provenance;
  last_seen: string;
  /// Most recent SUCCESSFUL attempt; `null` until one completes (T-F Part 0).
  last_ok: string | null;
}

/// One address-book entry — since schema v2 the SUPERSET of added + observed
/// stations. `callsign` is the primary, SSID-bearing identity — never strip
/// the SSID; observation routing matches on the EXACT presented callsign only
/// (no base-normalization merging). `created_at` / `updated_at` are RFC3339
/// UTC strings. There is NO `last_connected_at` — recency derives from the
/// `last_seen` on `channels`/`endpoints`.
export interface Contact {
  id: string;
  name: string;
  callsign: string;
  email?: string;
  tactical?: string;
  notes?: string;
  /// `Confirmed` (curated) vs `Unconfirmed` (auto-created). Absent on a write
  /// payload defaults to `Confirmed` (the v1→v2 migration semantics).
  tier?: ContactTier;
  /// Plain-language provenance: incoming / outgoing / added.
  origin?: Origin;
  grid?: ContactGrid;
  /// Observed RF reachability rows.
  channels?: Channel[];
  /// Observed / operator-entered network reachability rows (telnet P2P).
  endpoints?: Endpoint[];
  created_at: string;
  updated_at: string;
}

/// A distribution-group member. Mirrors the Rust `GroupMember` enum, which is
/// serialized with `#[serde(tag = "type", rename_all = "snake_case")]` — so the
/// wire shape is `{ type: 'contact', contact_id }` or `{ type: 'raw', callsign }`.
/// Added-from-a-contact members store a `contact_id` (so edits propagate); typed
/// members store the raw `callsign` literal.
export type GroupMember =
  | { type: 'contact'; contact_id: string }
  | { type: 'raw'; callsign: string };

/// A distribution group expanded to member callsigns at send time (frontend).
export interface Group {
  id: string;
  name: string;
  members: GroupMember[];
  created_at: string;
  updated_at: string;
}

/// The whole on-disk contacts file (contacts + groups + schema version). Returned
/// verbatim by the `contacts_read` command.
export interface ContactsFile {
  schema_version: number;
  contacts: Contact[];
  groups: Group[];
}

/// A suggest-from-history candidate (Task A8 consumer). `message_count` is how
/// many mailbox messages reference the un-saved correspondent.
export interface Suggestion {
  callsign: string;
  message_count: number;
}
