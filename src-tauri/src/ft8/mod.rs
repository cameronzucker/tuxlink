//! FT8 Station Intelligence L2 — the persistent listening service
//! (tuxlink-b026z.3). ALSA capture → 48k→12k decimation → wall-clock-true
//! 15 s slot assembly → tmpfs WAV → jt9 decode, with the full service state
//! machine, modem yield/resume arbitration, and opt-in CAT sweep.
//!
//! Layering: pure logic lives in the `tuxlink-capture` leaf crate; this
//! module is everything that touches ALSA, threads, Tauri, tux-rig, or
//! process lifecycle. Spec:
//! docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md.

pub mod alsa_source;
pub mod arbiter;
pub mod clock;
pub mod commands;
pub mod events;
pub mod meter;
pub mod records;
pub mod service;
pub mod sweep;
pub mod traits;
pub mod waterfall;

#[cfg(test)]
pub mod testutil;

#[cfg(test)]
mod e2e_tests;
