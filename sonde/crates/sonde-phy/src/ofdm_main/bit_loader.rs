//! Per-sub-carrier bit allocation.
//!
//! Design primitive (per foundation doc §2.2 Shannon + §2.3 OFDM and
//! ITU-T G.992/993 DSL bit-loading literature): allocate `b` bits per
//! symbol per sub-carrier where the per-bit SNR margin exceeds the
//! constellation's BER threshold. Water-filling over the per-bin SNR
//! vector yields the allocation. Supported constellations are BPSK,
//! QPSK, 16-QAM, 64-QAM ⇒ bit-counts `{1, 2, 4, 6}`.
//!
//! The threshold table here is a Phase-7 placeholder pinned from BER-
//! curve theory at target BER 1e-3 before FEC. Phase 11 re-pegs it
//! against channel-sim BER/SNR sweeps once the real FEC composition
//! lands.

/// Greedy per-sub-carrier bit allocator over a fixed BER-threshold
/// table.
///
/// The "water-filling" framing is conceptual: the per-bin SNR sets the
/// achievable constellation density independently per sub-carrier; the
/// per-bin allocations sum to the symbol's total payload-bit budget.
pub struct WaterfillingBitLoader;

impl WaterfillingBitLoader {
    /// Construct a bit loader. No tunable state today — the threshold
    /// table is internal; Phase 11 will lift it into a config struct
    /// once channel-sim sweeps drive a re-peg.
    pub fn new() -> Self {
        Self
    }

    /// Allocate bits per sub-carrier given per-bin SNR in dB.
    ///
    /// `max_bits` caps each entry at the supplied constellation
    /// density (1 = BPSK, 2 = QPSK, 4 = 16-QAM, 6 = 64-QAM). Returns
    /// one entry per input SNR, in the same order.
    pub fn allocate(&self, snr_db: &[f32], max_bits: u8) -> Vec<u8> {
        snr_db
            .iter()
            .map(|s| self.bits_for_snr(*s, max_bits))
            .collect()
    }

    /// Threshold table: `(dB, SNR ≥) → bits/symbol`. Derived from BER-
    /// curve theory at target BER 1e-3 before FEC, allowing typical-
    /// FEC headroom downstream. Phase 11 re-pegs against channel-sim
    /// sweeps.
    fn bits_for_snr(&self, snr_db: f32, max_bits: u8) -> u8 {
        let candidates: [(f32, u8); 4] = [
            (3.0, 1),  // BPSK above ~3 dB
            (8.0, 2),  // QPSK above ~8 dB
            (15.0, 4), // 16-QAM above ~15 dB
            (22.0, 6), // 64-QAM above ~22 dB
        ];
        let mut best = 0u8;
        for (thresh, bits) in candidates {
            if snr_db >= thresh && bits <= max_bits {
                best = bits;
            }
        }
        best
    }
}

impl Default for WaterfillingBitLoader {
    fn default() -> Self {
        Self::new()
    }
}
