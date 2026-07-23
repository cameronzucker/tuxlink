//! App-crate half of the `find_stations` agent-native redesign (bd tuxlink-m0n38).
//!
//! The tool-boundary *shapes* (the intent-tagged request, the tagged response
//! union, the bounded primitives) live in `tuxlink_mcp_core::station_query` — see
//! that module's docs for why. This module holds the parts that need live app
//! state and cannot live in the transport-free core crate:
//!
//! - [`snapshot`] — a normalized, TTL'd population snapshot + store, so
//!   `explore` / `lookup` narrow against a *stable* population (counts don't
//!   drift between calls) rather than re-fetching and re-counting each time.
//! - `engine` (P5) — the `StationQueryEngine` that resolves app-owned facts
//!   (operator grid, current time, transports, hours, propagation, FT-8, history)
//!   and builds a bounded [`tuxlink_mcp_core::station_query::FindStationsResponse`]
//!   from a request. Added in P5.

pub mod engine;
pub mod snapshot;
