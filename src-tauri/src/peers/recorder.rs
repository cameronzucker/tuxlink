//! Shared peer-observation recorder (spec §3). There is NO single
//! chokepoint [R4-1] — each transport calls this from its own
//! attempt-conclusion site(s), via `ObservationGuard` so wedged /
//! aborted / early-return paths still record a fail [R3-11].
//!
//! **Task scoping:** this module currently carries the observation TYPES and
//! the pure phase-classifier only (consumed by the peer store, Task 8). The
//! `ObservationGuard` drop-guard and the central `record_peer_observation`
//! entry point (with its limiter wiring) land in Task 11; do not add them here
//! ahead of that task.

use crate::peers::model::{ChannelBandwidth, ChannelTransport, Direction, Provenance};

/// The conclusion phase of a single connection attempt. The recorder maps
/// these to a [`Classified`] bucket via [`classify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationPhase {
    DialAttempted,
    Connected,
    LoginFailed,
    B2fStarted,
    B2fOk,
    B2fFail,
    Accepted,
    Rejected,
    AbortedOrWedged,
}

/// The store-facing classification of a phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classified {
    Ok,
    Fail,
    /// Rejected/unauthorized inbound: an attacker knocking is not a peer.
    NoRecord,
}

/// Map an attempt-conclusion phase to its roster effect. Pure — the store's
/// single source of truth for "does this observation touch the roster, and as
/// an ok or a fail?".
pub fn classify(phase: ObservationPhase) -> Classified {
    match phase {
        ObservationPhase::B2fOk | ObservationPhase::Accepted => Classified::Ok,
        ObservationPhase::Rejected => Classified::NoRecord,
        ObservationPhase::DialAttempted
        | ObservationPhase::Connected
        | ObservationPhase::LoginFailed
        | ObservationPhase::B2fStarted
        | ObservationPhase::B2fFail
        | ObservationPhase::AbortedOrWedged => Classified::Fail,
    }
}

/// The reachability path an observation was made over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedPath {
    Rf {
        transport: ChannelTransport,
        via: Vec<String>,
        /// Incoming rows have no wire freq source (CONNECTED carries
        /// bandwidth, not frequency) — rig/CAT state if available, else
        /// None; never fabricated [R3-11].
        freq_hz: Option<u64>,
        bandwidth: Option<ChannelBandwidth>,
    },
    Telnet {
        host: String,
        port: u16,
        provenance: Provenance,
    },
}

/// One connection-attempt observation, routed to the roster by the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerObservation {
    pub path: ObservedPath,
    pub direction: Direction,
    /// Exact presented/SSID'd callsign of the far station.
    pub presented_target: String,
    pub phase: ObservationPhase,
}
