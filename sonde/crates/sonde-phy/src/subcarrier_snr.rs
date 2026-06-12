//! Per-sub-carrier SNR estimation for the bit-loading layer (subsystem
//! #7 link adaptation) and the channel-quality report (PHY API).
//!
//! Two methods supported:
//! - **Pilot-aided:** known-symbol pilots are inserted on a grid in
//!   the OFDM symbol; the SNR per pilot bin is the ratio of expected
//!   signal energy to residual error energy. Pilot grid choice is
//!   per-mode (see ofdm_main/ofdm_params.rs).
//! - **Decision-directed:** after frame decode succeeds, recovered
//!   symbols become "pilots" for the next characterization window.
//!
//! Phase 5 implements the pilot-aided estimator; decision-directed is
//! added in Phase 7 when the OFDM receiver exists.

use num_complex::Complex;

/// Per-sub-carrier SNR estimator. Stateless; configured once with the
/// expected number of bins.
pub struct SubcarrierSnrEstimator {
    n_bins: usize,
}

impl SubcarrierSnrEstimator {
    /// Construct an estimator expecting `n_bins` frequency bins.
    pub fn new(n_bins: usize) -> Self {
        Self { n_bins }
    }

    /// Estimate per-bin SNR (dB) from a parallel pair of received +
    /// reference (pilot) symbol vectors. Returns one dB value per bin.
    pub fn estimate_from_pilots(
        &self,
        received: &[Complex<f32>],
        pilots: &[Complex<f32>],
    ) -> Vec<f32> {
        assert_eq!(received.len(), self.n_bins);
        assert_eq!(pilots.len(), self.n_bins);
        received
            .iter()
            .zip(pilots.iter())
            .map(|(r, p)| {
                let signal_energy = p.norm_sqr().max(1e-12);
                let error = r - p;
                let noise_energy = error.norm_sqr().max(1e-12);
                10.0 * (signal_energy / noise_energy).log10()
            })
            .collect()
    }
}
