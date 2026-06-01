//! OFDM transmitter: map sub-carrier bit-allocations + payload bits to
//! a sequence of complex sub-carriers, IFFT to time domain, prepend
//! cyclic prefix, and emit real-valued audio samples.
//!
//! The audio-channel simplification — take only the real part of the
//! complex IFFT output — preserves zero-BER round-trip on a clean
//! channel because the matching real-cast on the RX side halves both
//! pilot and data sub-carriers identically; the pilot-aided equalizer
//! cancels the halving. The cost is a fixed 3 dB SNR penalty, surfaced
//! only in Phase 11 BER/SNR sweeps.

use crate::constellations::{Constellation, Mapper};
use crate::ofdm_main::ofdm_params::OfdmParams;
use num_complex::Complex;
use rustfft::FftPlanner;

/// Single-symbol OFDM transmitter bound to one resolved
/// [`OfdmParams`] mode.
pub struct OfdmTransmitter<'a> {
    params: &'a OfdmParams,
}

impl<'a> OfdmTransmitter<'a> {
    /// Construct a transmitter bound to the given mode parameters.
    pub fn new(params: &'a OfdmParams) -> Self {
        Self { params }
    }

    /// Render one OFDM symbol's time-domain samples (CP + body).
    ///
    /// `payload_bits` are the data bits across data sub-carriers in
    /// transmission order. `bits_per_subcarrier` has length equal to
    /// [`OfdmParams::subcarrier_indices`] and indexes the same
    /// ascending order; entries at pilot positions are ignored (they
    /// may be zero). A `bits_per_subcarrier` entry of 0 skips a data
    /// sub-carrier entirely (no bits consumed, frequency bin left at
    /// 0+0j).
    pub fn modulate_one_symbol(
        &self,
        payload_bits: &[u8],
        bits_per_subcarrier: &[u8],
    ) -> Vec<f32> {
        let p = self.params;
        let mut freq_bins = vec![Complex::new(0.0_f32, 0.0); p.fft_size()];

        // 1) Drop pilots at known positions (constant +1+0j for now).
        for &pi in p.pilot_indices() {
            freq_bins[pi] = Complex::new(1.0, 0.0);
        }

        // 2) Walk data sub-carriers in order, popping bits per the
        //    bit-loading.
        let mut bit_cursor = 0usize;
        let subcarriers = p.subcarrier_indices();
        let pilot_set: std::collections::HashSet<usize> =
            p.pilot_indices().iter().copied().collect();
        for (idx_in_sc, &sc) in subcarriers.iter().enumerate() {
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
            let slice = &payload_bits[bit_cursor..bit_cursor + bpc];
            let sym = mapper.map(slice);
            freq_bins[sc] = sym[0];
            bit_cursor += bpc;
        }

        // 3) IFFT to time domain.
        let mut planner = FftPlanner::<f32>::new();
        let ifft = planner.plan_fft_inverse(p.fft_size());
        let mut td = freq_bins.clone();
        ifft.process(&mut td);
        let scale = 1.0 / (p.fft_size() as f32).sqrt();
        let samples_complex: Vec<Complex<f32>> = td.iter().map(|c| c * scale).collect();

        // 4) Prepend cyclic prefix.
        let cp = samples_complex[p.fft_size() - p.cp_len()..].to_vec();
        let mut full = cp;
        full.extend_from_slice(&samples_complex);

        // 5) Real-valued output: take real part. (Audio channel.)
        full.into_iter().map(|c| c.re).collect()
    }
}
