//! Per-sub-carrier single-tap frequency-domain equalizer.
//!
//! The channel estimate is derived from pilot positions (transmitted as
//! `+1+0j`); data-position estimates come from linear interpolation
//! between adjacent pilots in bin order. The interpolation assumes the
//! channel's coherence bandwidth is wider than the pilot spacing — for
//! the every-4th-sub-carrier grid at 23–47 Hz bin width (Wide / Narrow
//! modes), that holds across the FT-818-class SSB passband under
//! ITU-R F.520 "moderate" multipath. Phase 11 may revisit for floors
//! with denser pilot grids.

use num_complex::Complex;

/// Pilot-aided single-tap frequency-domain equalizer bound to a fixed
/// FFT size and pilot-index set.
pub struct OfdmEqualizer {
    pilot_positions: Vec<usize>,
    n_bins: usize,
}

impl OfdmEqualizer {
    /// Construct an equalizer for a given pilot-position set and FFT
    /// bin count.
    pub fn new(pilot_positions: Vec<usize>, n_bins: usize) -> Self {
        Self { pilot_positions, n_bins }
    }

    /// Estimate the channel from pilot bins (which the transmitter
    /// emits as `+1+0j`) and equalize all bins by linear interpolation
    /// of the channel estimate between adjacent pilots. Returns the
    /// equalized full-spectrum vector.
    ///
    /// `freq_bins.len()` must equal the `n_bins` passed at
    /// construction.
    pub fn equalize(&self, freq_bins: &[Complex<f32>]) -> Vec<Complex<f32>> {
        assert_eq!(freq_bins.len(), self.n_bins);
        let mut chan_est = vec![Complex::new(1.0_f32, 0.0); self.n_bins];
        for &pi in &self.pilot_positions {
            chan_est[pi] = freq_bins[pi]; // pilot was 1, so observed = channel.
        }
        // Linear interpolation between consecutive pilot positions.
        for window in self.pilot_positions.windows(2) {
            let a = window[0];
            let b = window[1];
            if b <= a + 1 {
                continue;
            }
            let h_a = chan_est[a];
            let h_b = chan_est[b];
            let span = (b - a) as f32;
            // `k` carries the load-bearing absolute bin index for the
            // interpolation weight `t = (k - a) / span`; rephrasing as
            // `iter_mut().enumerate().skip(...).take(...)` obscures
            // the math without buying type safety.
            #[allow(clippy::needless_range_loop)]
            for k in (a + 1)..b {
                let t = (k - a) as f32 / span;
                chan_est[k] = h_a * (1.0 - t) + h_b * t;
            }
        }
        // Apply zero-forcing division: y * conj(h) / |h|^2, with a
        // small floor to keep divisions numerically tame on bins where
        // the channel collapses to near-zero.
        freq_bins
            .iter()
            .zip(chan_est.iter())
            .map(|(r, h)| {
                let h2 = h.norm_sqr().max(1e-9);
                r * h.conj() / h2
            })
            .collect()
    }
}
