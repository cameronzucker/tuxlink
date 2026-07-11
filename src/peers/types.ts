// Frontend DTOs for the P2P integration-matrix capability bits — Task 22,
// re-sourced by Task T-E.
//
// The peers store died with the operator pivot (T-B, 2026-07-10/11): a peer IS
// a contact now, and the Peer / Channel / Endpoint / PeersFile shapes this
// file used to mirror live on `../contacts/types` instead (Contact's `tier` /
// `origin` / `grid` / `channels` / `endpoints`). ONLY `P2pCapabilities`
// survives here — it never moved to contacts/commands.rs's Contact model, and
// `useP2pCapabilities` (peers/usePeers.ts) is explicitly UNTOUCHED by T-E.

/// Mirrors `contacts/commands.rs::P2pCapabilities` — the P2P integration-matrix
/// capability bits (spec R5-8), relocated verbatim by Task T-B (the peers
/// module that used to own this command died) and reconciled by Task T-D (two
/// bits for cancelled surfaces were removed; the favorites↔contact-link bit
/// was renamed alongside T-B's peer_id→contact_id rename). See the Rust doc
/// comment for the UI-queried-vs-informational distinction; the frontend only
/// needs the two UI-queried bits (`finder_peers`, `map_peers`) to gate
/// rendering, but all six are mirrored so a caller can read any of them.
export interface P2pCapabilities {
  peer_store: boolean;
  finder_peers: boolean;
  map_peers: boolean;
  agent_find_peers: boolean;
  vara_engine_split: boolean;
  favorites_contact_link: boolean;
}
