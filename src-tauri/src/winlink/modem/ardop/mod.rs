//! ARDOP (Amateur Radio Digital Open Protocol) modem client.
//!
//! Drives ardopcf (or any ARDOP-compatible TNC) over two TCP sockets:
//! - cmd socket (default 8515): `\r`-terminated ASCII command lines.
//! - data socket (default 8516): `[u16 BE length][3-byte type][payload]` inbound;
//!   raw bytes outbound (the TNC frames them for TX).
//!
//! This module is Phase 1 of ADR 0015 decision #2 (generic external-TCP-modem
//! client). Phase 1 = wire codec only (pure functions/structs, no I/O).
//! Phase 2 adds TCP sockets and `std::thread`-based concurrency.

pub mod command;
pub mod data;
pub mod frame;
pub mod session;
pub mod transport;
pub mod wire;
