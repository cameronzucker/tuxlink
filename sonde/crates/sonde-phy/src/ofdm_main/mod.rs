//! Bit-adaptive OFDM main throughput family.
//!
//! The OFDM family supplies the bulk of the throughput ladder above the
//! wide-band low-density floor (Phase 8). Three starting modes
//! (Narrow / Mid / Wide) live in [`ofdm_params`]; the transmitter and
//! receiver in [`transmitter`] / [`receiver`] move bits ↔ time-domain
//! audio samples. The single-tap frequency-domain equalizer in
//! [`equalizer`] uses pilot-aided channel estimation with linear
//! interpolation between pilots. Per-sub-carrier bit-loading
//! ([`bit_loader`]) lands in Phase 7.
//!
//! Design primitives only — no examination of VARA / ARDOP / FLDigi /
//! Trimode / Pat / wl2k-go internals ([ADR
//! 0014](../../../docs/adr/0014-clean-sheet-no-prior-art-examination.md)).

pub mod ofdm_params;
pub mod transmitter;
pub mod receiver;
pub mod bit_loader;
pub mod equalizer;
