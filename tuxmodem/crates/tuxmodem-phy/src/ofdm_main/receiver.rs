//! OFDM receiver: time-domain samples → CP stripping → FFT →
//! pilot-aided equalization → per-bit LLR computation across data
//! sub-carriers.

use crate::constellations::{Constellation, Mapper};
use crate::ofdm_main::equalizer::OfdmEqualizer;
use crate::ofdm_main::ofdm_params::OfdmParams;
use num_complex::Complex;
use rustfft::FftPlanner;

/// Single-symbol OFDM receiver bound to one resolved [`OfdmParams`]
/// mode.
pub struct OfdmReceiver<'a> {
    params: &'a OfdmParams,
}

impl<'a> OfdmReceiver<'a> {
    /// Construct a receiver bound to the given mode parameters.
    pub fn new(params: &'a OfdmParams) -> Self {
        Self { params }
    }

    /// Demodulate one OFDM symbol: drop the CP, FFT, equalize against
    /// pilot positions, then emit per-bit LLRs across the data
    /// sub-carriers (in the same transmission order the matching
    /// [`crate::ofdm_main::transmitter::OfdmTransmitter::modulate_one_symbol`]
    /// consumed).
    ///
    /// `samples.len()` must equal `params.fft_size() + params.cp_len()`.
    /// `bits_per_subcarrier` follows the same indexing as the
    /// transmitter side.
    pub fn demodulate_one_symbol(
        &self,
        samples: &[f32],
        bits_per_subcarrier: &[u8],
    ) -> Vec<f32> {
        let p = self.params;
        let expected = p.fft_size() + p.cp_len();
        assert_eq!(samples.len(), expected, "OFDM RX symbol length mismatch");

        // Drop CP, promote to complex baseband for FFT.
        let body: Vec<Complex<f32>> = samples[p.cp_len()..]
            .iter()
            .map(|s| Complex::new(*s, 0.0))
            .collect();
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(p.fft_size());
        let mut freq = body;
        fft.process(&mut freq);
        let scale = 1.0 / (p.fft_size() as f32).sqrt();
        for c in freq.iter_mut() {
            *c *= scale;
        }

        // Equalize.
        let eq = OfdmEqualizer::new(p.pilot_indices().to_vec(), p.fft_size());
        let equalized = eq.equalize(&freq);

        // LLR per data sub-carrier in transmission order.
        let pilot_set: std::collections::HashSet<usize> =
            p.pilot_indices().iter().copied().collect();
        let mut all_llr = Vec::new();
        for (idx_in_sc, &sc) in p.subcarrier_indices().iter().enumerate() {
            if pilot_set.contains(&sc) {
                continue;
            }
            let bpc = bits_per_subcarrier[idx_in_sc] as usize;
            if bpc == 0 {
                continue;
            }
            let constellation = match bpc {
                1 => Constellation::Bpsk,
                2 => Constellation::Qpsk,
                4 => Constellation::Qam16,
                6 => Constellation::Qam64,
                _ => panic!("unsupported bit-loading: {bpc}"),
            };
            let mapper = Mapper::new(constellation);
            // Noise-variance proxy. Phase 11 refines via the residual
            // after hard decision; for clean-channel round-trip the
            // value only sets LLR magnitude, not sign.
            let n0 = 0.1_f32;
            let llrs = mapper.compute_llr(&[equalized[sc]], n0);
            all_llr.extend_from_slice(&llrs);
        }
        all_llr
    }
}
