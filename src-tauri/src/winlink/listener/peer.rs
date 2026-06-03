//! `PeerId` — the identity an inbound peer presents to the listener-arms gate.
//!
//! Different transports carry different identity fragments:
//! - Telnet: `SocketAddr` from `TcpStream::peer_addr` + a callsign declared
//!   in the `CALLSIGN :` prompt response (see `dev/scratch/winlink-re/findings/telnet-p2p.md`).
//! - Packet / ARDOP / VARA: a callsign carried in the link/modem-layer connect
//!   indication; no IP at the application layer.
//!
//! The `Both` variant exists for transports that present both — Telnet after the
//! `CALLSIGN :` exchange completes; the IP is the network peer and the callsign is
//! the operator's claim.
//!
//! Spec: `docs/design/2026-06-03-multi-transport-listener-architecture.md` §2.1
//! bd: tuxlink-3o2o

use std::net::SocketAddr;

use crate::winlink::ax25::frame::Address;

/// The identity of an inbound peer attempting to connect to a tuxlink listener.
///
/// Variants reflect what each transport carries at accept time. The accept-decision
/// path in `decide.rs` consults whichever fragments are present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerId {
    /// Callsign-only: AX.25 / ARDOP / VARA — link-layer or modem-control identity.
    Callsign(Address),
    /// IP-only: rare in practice (Telnet before the CALLSIGN exchange), but
    /// kept so an IP-only allowlist check stays expressible.
    SocketAddr(SocketAddr),
    /// Both fragments present — Telnet after CALLSIGN exchange completes.
    Both { callsign: Address, addr: SocketAddr },
}

impl PeerId {
    /// Borrow the callsign fragment, if present.
    pub fn callsign(&self) -> Option<&Address> {
        match self {
            PeerId::Callsign(c) => Some(c),
            PeerId::Both { callsign, .. } => Some(callsign),
            PeerId::SocketAddr(_) => None,
        }
    }

    /// Borrow the socket-address fragment, if present.
    pub fn socket_addr(&self) -> Option<&SocketAddr> {
        match self {
            PeerId::SocketAddr(a) => Some(a),
            PeerId::Both { addr, .. } => Some(addr),
            PeerId::Callsign(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn n7cpz() -> Address {
        Address { call: "N7CPZ".into(), ssid: 0 }
    }

    fn sa() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5)), 8774)
    }

    #[test]
    fn callsign_only_exposes_callsign_only() {
        let p = PeerId::Callsign(n7cpz());
        assert_eq!(p.callsign(), Some(&n7cpz()));
        assert_eq!(p.socket_addr(), None);
    }

    #[test]
    fn socket_only_exposes_addr_only() {
        let p = PeerId::SocketAddr(sa());
        assert_eq!(p.callsign(), None);
        assert_eq!(p.socket_addr(), Some(&sa()));
    }

    #[test]
    fn both_exposes_both() {
        let p = PeerId::Both { callsign: n7cpz(), addr: sa() };
        assert_eq!(p.callsign(), Some(&n7cpz()));
        assert_eq!(p.socket_addr(), Some(&sa()));
    }
}
