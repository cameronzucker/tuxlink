//! Synchronization infrastructure shared across mode families.
//!
//! Per PHY spec §3 forcing function 3, sync infrastructure is shared
//! between OFDM and FSK families. This module owns preamble design,
//! carrier-frequency-offset estimation, symbol-timing recovery, and
//! frame-sync detection.

pub mod preamble;
pub mod carrier_offset;
pub mod symbol_timing;
pub mod frame_sync;
