//! sonde-fec — LDPC forward error correction for the clean-sheet HF modem.
//!
//! Concrete LDPC codec types implement the [`FecCodec`] trait from
//! [`sonde_phy::coded_modulation`] (the bus contract landed by
//! subsystem #3, PR #188). Two code families share one
//! sum-product-algorithm decoder.
//!
//! See `README.md` for an overview, `docs/architecture.md` for design
//! rationale (post-Phase-8), and ADR 0014 for the clean-sheet posture.
//!
//! ## Integration with subsystem #3
//!
//! Subsystem #4's plan was authored in parallel with subsystem #3's
//! and proposed its own richer `FecEncoder` / `FecDecoder` traits with
//! a `Llr` newtype + family-dispatched methods. Subsystem #3 landed a
//! simpler [`FecCodec`] bus contract (`&[u8]` info bits, `&[f32]`
//! LLRs, `Vec<u8>` outputs) in PR #188 before this implementation
//! started. Per the plan's §F reconciliation protocol, the already-
//! landed bus contract wins. This crate's concrete types implement
//! that trait; richer per-family types (rate enums, decode stats)
//! live alongside as implementation detail and surface to ARQ via
//! [`stats::ResidualErrorStats`].
//!
//! [`FecCodec`]: sonde_phy::coded_modulation::FecCodec

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod codec;
pub mod codes;
pub mod crc;
pub mod decode;
pub mod encode;
pub mod interleaver;
pub mod llr;
pub mod parity_matrix;
pub mod puncture;
pub mod stats;

// Internal note: codes::floor_rate14 and codes::ofdm_wifi_family
// depend on encode::Encoder::try_new for the rank-deficiency seed
// iteration. This creates an intra-crate dep: codes → encode →
// parity_matrix. No public cycle.
