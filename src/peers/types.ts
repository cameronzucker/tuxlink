// Frontend DTOs for the P2P Peers feature — Task 22 (spec §1/§2:
// docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md).
//
// These MUST mirror the Rust serde shapes EXACTLY — snake_case, no
// `rename_all` (favorites/types.ts:2-7 pattern). Sources of truth:
//   - `src-tauri/src/peers/model.rs` — Peer / Channel / Endpoint / PeersFile /
//     the enum wire strings (all `#[serde(rename_all = "kebab-case")]`).
//   - `src-tauri/src/contacts/commands.rs` — `P2pCapabilities` (relocated by
//     Task T-B's contacts fold; shape reconciled by Task T-D).
// When either Rust shape changes, this file MUST be updated in the same PR.
//
// `Option<T>` fields in model.rs carry NO `skip_serializing_if` — the backend
// always emits an explicit JSON `null` for an absent value, never omits the
// key. Those fields are typed `T | null` here (NOT `T | undefined`/`T?`) so a
// `null` literal typechecks against the exact wire shape — see the
// `contact_id: null, grid: null, last_connected_at: null` literals in
// `peerModel.test.ts`'s `peer()` fixture factory.

/// Mirrors `model.rs::IdentityKind`. Tactical calls dedup on their FULL
/// presented string, never base-normalized (see the Rust doc comment).
export type IdentityKind = 'individual' | 'tactical' | 'club' | 'unknown';

/// Mirrors `model.rs::RecordSource`.
export type RecordSource = 'auto' | 'manual' | 'operator-pinned' | 'unknown';

/// Mirrors `model.rs::Origin`.
export type Origin = 'incoming' | 'outgoing' | 'manual' | 'aprs' | 'unknown';

/// Mirrors `model.rs::GridSource`.
export type GridSource = 'contact' | 'aprs' | 'manual' | 'unknown';

/// Mirrors `model.rs::ChannelTransport`.
export type ChannelTransport = 'packet' | 'ardop' | 'vara-hf' | 'vara-fm' | 'unknown';

/// Mirrors `model.rs::Direction`.
export type Direction = 'incoming' | 'outgoing' | 'unknown';

/// Mirrors `model.rs::Provenance`. `'operator'` is the ONLY agent-dialable
/// provenance (spec §4 I1) — never derive dialability from any other value.
export type Provenance = 'operator' | 'observed-incoming' | 'unknown';

/// Mirrors `model.rs::ChannelBandwidth` — internally tagged on `"kind"`
/// (`#[serde(tag = "kind", rename_all = "kebab-case")]`), so the one
/// data-carrying variant is a discriminated union member, not a sibling
/// optional field.
export type ChannelBandwidth =
  | { kind: 'hz'; hz: number }
  | { kind: 'wide' }
  | { kind: 'narrow' }
  | { kind: 'unknown' };

/// Mirrors `model.rs::PeerGrid`.
export interface PeerGrid {
  value: string;
  source: GridSource;
}

/// Mirrors `model.rs::AttemptCounts`.
export interface AttemptCounts {
  ok: number;
  fail: number;
}

/// Mirrors `model.rs::Channel` — one RF reachability observation row.
export interface Channel {
  transport: ChannelTransport;
  target_callsign: string;
  via: string[];
  freq_hz: number | null;
  bandwidth: ChannelBandwidth | null;
  direction: Direction;
  counts: AttemptCounts;
  last_seen: string;
}

/// Mirrors `model.rs::Endpoint` — one network reachability row (telnet P2P).
export interface Endpoint {
  id: string;
  host: string;
  port: number;
  provenance: Provenance;
  last_seen: string;
}

/// Mirrors `model.rs::Peer`.
export interface Peer {
  id: string;
  canonical_base: string;
  presented_callsigns: string[];
  identity_kind: IdentityKind;
  do_not_merge: boolean;
  conflict: boolean;
  source: RecordSource;
  origin: Origin;
  contact_id: string | null;
  grid: PeerGrid | null;
  note: string;
  created_at: string;
  last_connected_at: string | null;
  channels: Channel[];
  endpoints: Endpoint[];
}

/// Mirrors `model.rs::PeersFile` — the whole on-disk roster, returned by
/// `peers_read`.
export interface PeersFile {
  schema_version: number;
  peers: Peer[];
}

/// Mirrors `commands.rs::P2pCapabilities` — the P2P integration-matrix
/// capability bits (spec R5-8), reconciled post-pivot by Task T-D (two bits
/// for cancelled surfaces were removed; the favorites↔contact-link bit was
/// renamed alongside T-B's peer_id→contact_id rename). See the Rust doc
/// comment for the UI-queried-vs-informational distinction; the frontend
/// only needs the two UI-queried bits (`finder_peers`, `map_peers`) to gate
/// rendering, but all six are mirrored so a caller can read any of them.
export interface P2pCapabilities {
  peer_store: boolean;
  finder_peers: boolean;
  map_peers: boolean;
  agent_find_peers: boolean;
  vara_engine_split: boolean;
  favorites_contact_link: boolean;
}
