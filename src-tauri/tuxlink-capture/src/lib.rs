//! Station Intelligence L2 pure-logic leaf crate (tuxlink-b026z.3).
//!
//! std-only by design: everything here compiles and TDDs on the dev Pi in
//! seconds. The main crate's `src/ft8/` module (ALSA, threads, Tauri) wires
//! these pieces at Phase C. Design authority:
//! docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md.

pub mod bands;
pub mod wavwrite;
