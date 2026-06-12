//! sonde-phy — clean-sheet HF PHY waveform layer.
//!
//! Subordinate to `docs/superpowers/specs/2026-05-31-clean-sheet-modem-3-phy-waveform.md`
//! in the tuxlink repo. No examination of VARA / ARDOP / FLDigi / Trimode /
//! Pat / wl2k-go internals (ADR 0014).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod modes;
pub mod phy_api;
pub mod audio_io;
#[cfg(feature = "audio-device")]
pub mod audio_device;
pub mod constellations;
pub mod sync;
pub mod subcarrier_snr;
pub mod ofdm_main;
pub mod robustness_floor;
pub mod coded_modulation;
pub mod error;
pub use error::PhyError;
