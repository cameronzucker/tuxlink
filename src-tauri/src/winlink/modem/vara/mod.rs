//! VARA TCP modem transport.
//!
//! Sibling to the [`ardop`](super::ardop) module under the same
//! [`ModemTransport`](super::ModemTransport) abstraction. VARA exposes
//! its host protocol over two TCP sockets (cmd + data, defaulting to
//! 8300/8301); see [`command`] for the wire-level enum + parser and
//! [`transport`] for the socket pair holder.
//!
//! Status: Phase 1 of the VARA wire-up (`tuxlink-hblz`) — codec +
//! transport layer + smoke binary. Full
//! [`ModemTransport`](super::ModemTransport) trait impl + session-
//! layer integration with the B2F exchanger arrives in a follow-up
//! once the operator validates the TCP roundtrip against a real
//! VARA instance.
//!
//! ## Wire summary
//!
//! - Cmd socket: ASCII commands, one per line, `\r` terminated.
//! - Data socket: raw bytes both directions (VARA frames them for TX).
//! - Setter commands (`MYCALL`, `BW`, `LISTEN`, `COMPRESSION`, `CWID`,
//!   `PUBLIC`) get echoed back by VARA on success.
//! - Asynchronous status events: `PTT ON/OFF`, `BUFFER <n>`,
//!   `CONNECTED <mycall> <target> [bw]`, `DISCONNECTED`, `PENDING`,
//!   `CANCELPENDING`, `LINK REGISTERED`, `IAMALIVE`, `OFFLINE`,
//!   error variants (`MISSING SOUNDCARD`, `WRONG CALLSIGN`).
//!
//! ## RADIO-1
//!
//! Opening the sockets does not transmit. `CONNECT` does. The smoke
//! probe + unit tests only exercise the TCP layer; `CONNECT` is
//! reserved for operator-driven flows.

pub mod command;
pub mod commands;
pub mod transport;
pub mod wire;

pub use command::{
    Bandwidth, CommandParseError, Compression, InboundCommand, OutboundCommand,
};
pub use commands::{PlatformInfo, VaraSession, VaraState, VaraStatus};
pub use transport::{VaraConfig, VaraTransport};
