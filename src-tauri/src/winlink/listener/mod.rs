//! Shared listener-arms layer — the WHO + WHAT-PASSWORD + WHEN-WINDOW gate that
//! every multi-transport P2P listener consults before handing a stream to the
//! B2F answerer.
//!
//! Architecture: `docs/design/2026-06-03-multi-transport-listener-architecture.md` §2.1
//!
//! ## Module layout
//!
//! - [`peer`] — `PeerId` enum (Callsign / SocketAddr / Both).
//! - [`transport`] — `TransportKind` enum (Telnet / Packet / Ardop / VaraHf / VaraFm / Pactor).
//! - [`allowed_stations`] — `AllowedStations` allowlist + wildcard matching.
//!   DIVERGES from WLE: defaults to `Allow All Connections: FALSE`.
//! - [`station_password`] — `StationPassword` keyring-backed per-listener
//!   password (DIVERGES from WLE's plaintext INI storage). Constant-time verify.
//! - [`arms_record`] — `ListenerArmsRecord` per-arm-event consent metadata with
//!   TTL + UUID + JSONL forensics log.
//! - [`decide`] — `listener_decide` integration function + `ListenerDecision`
//!   enum: the single entry point every transport adapter calls.
//!
//! ## Wiring contract
//!
//! Each transport adapter (Telnet / Packet / ARDOP / VARA) does, in order:
//!
//! 1. Receive an inbound connection at the transport layer.
//! 2. Build a `PeerId` from the transport's identity fragments.
//! 3. (Telnet only) prompt for callsign + optional password.
//! 4. Call `listener_decide(&peer, password_input, &allowed, &password, &arms)`.
//! 5. On `Accept`: hand the connected stream to
//!    `winlink::session::run_exchange_with_role(ExchangeRole::Answer, ..)`.
//! 6. On any `Reject*` variant: close the connection with the appropriate
//!    transport-specific signal and log the reject for the forensics log.
//!
//! The wiring INTO each transport (tuxlink-xehu Telnet, tuxlink-inde Packet,
//! tuxlink-dhbl ARDOP, tuxlink-xnoy VARA) is each a separate bd issue; this
//! module ships only the foundation.
//!
//! bd: tuxlink-3o2o

pub mod allowed_stations;
pub mod arms_record;
pub mod decide;
pub mod packet_gate;
pub mod peer;
pub mod station_password;
pub mod transport;

// ──────────────────────────────────────────────────────────────
// Convenience re-exports — the "stable surface" for callers
// ──────────────────────────────────────────────────────────────

pub use allowed_stations::AllowedStations;
pub use arms_record::{ListenerArmsRecord, DEFAULT_TTL, NO_EXPIRY};
pub use decide::{listener_decide, listener_decide_at, ListenerDecision};
pub use peer::PeerId;
pub use station_password::StationPassword;
pub use transport::TransportKind;
