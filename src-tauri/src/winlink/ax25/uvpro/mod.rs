//! Native UV-Pro "Benshi/Vero" control profile (tuxlink-nx95, APRS-chat Phase 2).
//!
//! On-screen device control of the BTECH UV-Pro over its native Bluetooth
//! protocol (RFCOMM + GAIA framing) — read status (channel/freq/mode/battery/RSSI)
//! and send control (set channel/frequency/mode, connect/disconnect). This is the
//! "Layer 2" capability profile from the APRS tactical chat epic (tuxlink-2f2n):
//! a second profile over the same UV-Pro Bluetooth link the KISS path uses, so
//! only one is active at a time (single-Bluetooth-host arbitration).
//!
//! Protocol reverse-engineered from benlink + HTCommander source (sanctioned RE
//! per the winlink-RE-authoritative-sources rule — the opposite of the clean-sheet
//! VARA modem rule). Spec + golden vectors:
//! `docs/design/2026-06-12-uvpro-benshi-control-phase2-design.md`,
//! `docs/design/uvpro-benshi-golden-vectors.md`.
//!
//! Attribution + license review (benlink + HTCommander, both Apache-2.0;
//! independent Rust reimplementation, no source copied):
//! `docs/reference/uvpro-benshi-protocol-attribution.md`.
//!
//! RADIO-1 / ADR 0018: control commands do NOT key the transmitter; this profile
//! exposes no transmit command and is non-transmitting by construction. Abort =
//! drop the RFCOMM socket. No auto-reconnect (a drop → disconnected; the operator
//! re-connects). The agent never transmits; the operator runs the on-air smoke.

pub mod bits;
pub mod commands;
pub mod gaia;
pub mod message;
pub mod model;
pub mod rf_ch;
pub mod session;
pub mod settings;

/// Errors surfaced to the command layer / frontend. The `kind` (variant name) is
/// what the UI switches on; the payload carries operator-facing detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UvproError {
    /// The UV-Pro Bluetooth link is already held (by the KISS/packet path or an
    /// existing native session).
    LinkBusy { holder: String },
    /// No active connection.
    NotConnected,
    /// A command got no reply within its timeout.
    Timeout,
    /// A frame could not be parsed / an unexpected reply arrived.
    Protocol(String),
    /// The radio replied with a non-SUCCESS status.
    RadioRejected(String),
    /// Socket / I/O failure.
    Io(String),
    /// The configured / supplied MAC is malformed.
    BadMac,
}

impl std::fmt::Display for UvproError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UvproError::LinkBusy { holder } => {
                write!(f, "UV-Pro Bluetooth link is in use by {holder}")
            }
            UvproError::NotConnected => write!(f, "not connected to the UV-Pro"),
            UvproError::Timeout => write!(f, "the radio did not respond in time"),
            UvproError::Protocol(d) => write!(f, "protocol error: {d}"),
            UvproError::RadioRejected(d) => write!(f, "the radio rejected the request: {d}"),
            UvproError::Io(d) => write!(f, "Bluetooth I/O error: {d}"),
            UvproError::BadMac => write!(f, "invalid Bluetooth MAC address"),
        }
    }
}

impl std::error::Error for UvproError {}

impl UvproError {
    /// Stable machine-readable kind for the frontend to switch on.
    pub fn kind(&self) -> &'static str {
        match self {
            UvproError::LinkBusy { .. } => "LinkBusy",
            UvproError::NotConnected => "NotConnected",
            UvproError::Timeout => "Timeout",
            UvproError::Protocol(_) => "Protocol",
            UvproError::RadioRejected(_) => "RadioRejected",
            UvproError::Io(_) => "Io",
            UvproError::BadMac => "BadMac",
        }
    }
}
