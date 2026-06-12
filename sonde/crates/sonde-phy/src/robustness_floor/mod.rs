//! Robustness floor family. Two architecturally-distinct modes:
//!
//! - [`wideband_lowdensity`] — DEFAULT. Wide-band OFDM with BPSK per
//!   sub-carrier + strong FEC. Per overview §5.A.1, the strategic
//!   posture is "go wider, not denser" — outperforms FT8-class
//!   narrow-FSK at the same per-Hz SNR via Shannon-driven design.
//!   The competence gate is "beat ARDOP's narrowest mode at the
//!   noise-floor case."
//!
//! - [`narrow_fsk`] — SITUATIONAL. M-FSK conceptual primitive borrowed
//!   from FT8/JS8 weak-signal design. Reserved for crowded-band slots
//!   where wide-band isn't available.

pub mod wideband_lowdensity;
pub mod narrow_fsk;
