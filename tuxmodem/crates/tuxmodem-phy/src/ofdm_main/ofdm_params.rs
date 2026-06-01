//! Per-mode OFDM parameter table.
//!
//! Three starting modes per overview §5.A.1 ladder ("ARDOP uses 4;
//! tuxmodem may use fewer or more"). Phase 11 sweeps may add or
//! remove modes informed by channel-sim characterization. Parameters
//! are derived from primitives — sub-carrier orthogonality, CP-as-
//! delay-spread-budget — not from any prior-art HF modem.
//!
//! Sample-rate is the crate-wide constant 48 kHz (see
//! [`crate::audio_io::SAMPLE_RATE_HZ`]).

use crate::audio_io::SAMPLE_RATE_HZ;

/// Named OFDM mode in the throughput ladder.
///
/// Each variant maps to a distinct FFT size / occupied bandwidth /
/// sub-carrier grid in [`OfdmParams::for_mode`]. Phase 11 may add
/// further modes informed by channel-sim BER/SNR sweeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfdmModeName {
    /// Narrow: ~500 Hz occupied bandwidth, FFT=1024. Conservative
    /// mode for poor channels.
    Narrow,
    /// Mid: ~1000 Hz occupied bandwidth, FFT=1024. Default sustained
    /// mode for typical mid-latitude HF paths.
    Mid,
    /// Wide: ~2300 Hz occupied bandwidth, FFT=2048. Fills the full
    /// FT-818-class SSB passband when SNR permits.
    Wide,
}

/// Resolved per-mode OFDM parameters: FFT size, cyclic-prefix length,
/// occupied sub-carrier indices, and pilot-symbol positions within the
/// occupied band.
///
/// Construct via [`OfdmParams::for_mode`]; the resulting struct is
/// shared by the [`crate::ofdm_main::transmitter`],
/// [`crate::ofdm_main::receiver`], and
/// [`crate::ofdm_main::equalizer`].
pub struct OfdmParams {
    fft_size: usize,
    cp_len: usize,
    subcarrier_indices: Vec<usize>,
    pilot_indices: Vec<usize>,
}

impl OfdmParams {
    /// Build the parameter set for a named mode.
    ///
    /// Design primitives applied (per overview §4 rule 1, foundation
    /// doc §2.3 OFDM):
    ///
    /// - At 48 kHz sample rate, FFT size N gives bin width 48000/N Hz.
    /// - FFT size is picked so a contiguous slab of bins covers the
    ///   desired bandwidth inside 250–2700 Hz (FT-818 SSB passband).
    /// - CP length covers ITU-R F.520 "moderate" multipath delay-spread
    ///   budget (~2 ms) — empirically settled in Phase 11; this
    ///   skeleton pins CP = 25 % of FFT.
    /// - Pilot grid is every 4th occupied sub-carrier.
    pub fn for_mode(mode: OfdmModeName) -> Self {
        let (fft_size, bandwidth_hz) = match mode {
            OfdmModeName::Narrow => (1024usize, 500.0f32),
            OfdmModeName::Mid    => (1024,     1000.0),
            OfdmModeName::Wide   => (2048,     2300.0),
        };
        let cp_len = fft_size / 4;
        let bin_width = SAMPLE_RATE_HZ as f32 / fft_size as f32;
        let center_hz = 1500.0_f32;
        let half_bins = (bandwidth_hz / 2.0 / bin_width).floor() as usize;
        let center_bin = (center_hz / bin_width).round() as usize;
        let start_bin = center_bin.saturating_sub(half_bins);
        let end_bin = center_bin + half_bins;
        let subcarrier_indices: Vec<usize> = (start_bin..=end_bin).collect();

        let pilot_indices: Vec<usize> = subcarrier_indices
            .iter()
            .copied()
            .step_by(4)
            .collect();

        Self { fft_size, cp_len, subcarrier_indices, pilot_indices }
    }

    /// FFT size in samples. Always a power of two.
    pub fn fft_size(&self) -> usize { self.fft_size }

    /// Cyclic-prefix length in samples, prepended to each OFDM symbol.
    pub fn cp_len(&self) -> usize { self.cp_len }

    /// All occupied sub-carrier bin indices in ascending order
    /// (data + pilot positions combined).
    pub fn subcarrier_indices(&self) -> &[usize] { &self.subcarrier_indices }

    /// Pilot sub-carrier bin indices (a subset of
    /// [`Self::subcarrier_indices`]).
    pub fn pilot_indices(&self) -> &[usize] { &self.pilot_indices }

    /// Data sub-carrier bin indices — the occupied bins that are not
    /// pilots, returned in ascending order.
    pub fn data_indices(&self) -> Vec<usize> {
        let pilot: std::collections::HashSet<usize> =
            self.pilot_indices.iter().copied().collect();
        self.subcarrier_indices
            .iter()
            .copied()
            .filter(|i| !pilot.contains(i))
            .collect()
    }
}
