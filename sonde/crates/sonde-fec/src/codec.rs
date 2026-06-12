//! Public LDPC codec wrappers implementing PR #188's `FecCodec` bus
//! contract (`sonde_phy::coded_modulation::FecCodec`).
//!
//! ## Composition
//!
//! Encode:
//! 1. Convert `&[u8]` info bits (one bit per byte) into `BitVec<u8>`.
//! 2. Append CRC-32 (32 trailing bits, MSB-first).
//! 3. LDPC systematic encode to an n-bit codeword.
//! 4. Block bit-interleave (rows = 8 — divides 648 / 1296 / 2048
//!    exactly so the output length equals n).
//! 5. Convert `BitVec<u8>` → `Vec<u8>` (one bit per byte).
//!
//! Decode is the inverse: pack LLRs through deinterleave → SPA →
//! verify CRC → strip CRC → return info bits.
//!
//! ## Bit-layout convention
//!
//! Matches PR #188's `FecCodec::encode`: each `u8` in the info-bit /
//! coded-bit slices carries exactly one bit (value 0 or 1) in its
//! least-significant bit; bytes elsewhere are zero. Same convention
//! `IdentityFec` uses.

use bitvec::prelude::*;
use sonde_phy::coded_modulation::{CodeRate, FecCodec, FecError};

use crate::codes::{self, BlockN, CodeFamily, WifiLdpcRate};
use crate::crc::{append_crc32, verify_crc32};
use crate::decode::Decoder as LdpcDecoder;
use crate::encode::Encoder as LdpcEncoder;
use crate::interleaver::interleave;
use crate::parity_matrix::ParityCheckMatrix;

const INTERLEAVER_ROWS: usize = 8;
const MAX_ITERS_OFDM: u32 = 50;

/// LDPC codec for the WiFi-style rate-compatible family
/// (`OfdmAdaptive` in [`CodeFamily`]). Constructs the parity-check
/// matrix + encoder + decoder once at codec-construction time;
/// per-encode and per-decode are then pure data-plane work.
pub struct OfdmAdaptiveCodec {
    family: CodeFamily,
    h: ParityCheckMatrix,
    encoder: LdpcEncoder,
    decoder: LdpcDecoder,
    n: usize,
    /// Underlying LDPC info-bit count = payload bits + 32 (CRC).
    ldpc_k: usize,
    /// Code rate as expressed in PR #188's struct form.
    rate: CodeRate,
}

impl OfdmAdaptiveCodec {
    /// Build a codec for the given WiFi-family `(block_n, rate)`
    /// pair. Constructs `H`, the LDPC encoder, and the SPA decoder
    /// up-front.
    pub fn new(block_n: BlockN, rate: WifiLdpcRate) -> Self {
        let family = CodeFamily::OfdmAdaptive { block_n, rate };
        let h = codes::build(family);
        let encoder = LdpcEncoder::new(&h);
        let decoder = LdpcDecoder::new(&h);
        let n = h.n;
        let ldpc_k = h.k;
        let (num, den) = rate.ratio();
        Self {
            family,
            h,
            encoder,
            decoder,
            n,
            ldpc_k,
            rate: CodeRate {
                num: num as u32,
                den: den as u32,
            },
        }
    }

    /// Number of LDPC payload bits per block (excluding the 32-bit
    /// CRC prepended by the encoder).
    pub fn payload_bits(&self) -> usize {
        self.ldpc_k - 32
    }

    /// Borrowed access to the underlying parity-check matrix
    /// (diagnostics; not part of the `FecCodec` contract).
    pub fn parity_check_matrix(&self) -> &ParityCheckMatrix {
        &self.h
    }

    /// Which `CodeFamily` enum value this codec was constructed for.
    pub fn family(&self) -> CodeFamily {
        self.family
    }
}

fn bytes_to_bitvec(bits: &[u8]) -> BitVec<u8> {
    bits.iter().map(|&b| b != 0).collect()
}

fn bitvec_to_bytes(bv: &BitSlice<u8>) -> Vec<u8> {
    bv.iter().map(|b| u8::from(*b)).collect()
}

impl FecCodec for OfdmAdaptiveCodec {
    fn encode(&self, info_bits: &[u8]) -> Vec<u8> {
        assert_eq!(
            info_bits.len(),
            self.payload_bits(),
            "OfdmAdaptiveCodec::encode: info_bits length {} != payload k {}",
            info_bits.len(),
            self.payload_bits()
        );
        let info = bytes_to_bitvec(info_bits);
        let with_crc = append_crc32(info.as_bitslice());
        debug_assert_eq!(with_crc.len(), self.ldpc_k);

        let codeword = self.encoder.encode(with_crc.as_bitslice());
        debug_assert_eq!(codeword.len(), self.n);

        let interleaved = interleave(codeword.as_bitslice(), INTERLEAVER_ROWS);
        debug_assert_eq!(
            interleaved.len(),
            self.n,
            "interleaver output length {} != n {} — interleaver_rows must divide n",
            interleaved.len(),
            self.n
        );

        bitvec_to_bytes(interleaved.as_bitslice())
    }

    fn decode_soft(&self, llr: &[f32]) -> Result<Vec<u8>, FecError> {
        if llr.len() != self.n {
            return Err(FecError::DecodeFailure(format!(
                "decode_soft: llr.len() {} != n {}",
                llr.len(),
                self.n
            )));
        }

        // Sign-mapped bit form for deinterleave; the interleaver
        // operates on bits not LLR magnitudes, but the channel order
        // is what matters. Apply deinterleave to the LLR sequence by
        // permuting in the same shape as the bit form.
        let llr_bitvec_signs: BitVec<u8> = llr.iter().map(|x| *x < 0.0).collect();
        // De-interleave a parallel index permutation, then read LLR
        // values back in the permuted order.
        let perm = deinterleave_index_perm(self.n, INTERLEAVER_ROWS);
        let deint_llrs: Vec<f32> = (0..self.n).map(|i| llr[perm[i]]).collect();
        let _ = llr_bitvec_signs; // sanity-check witness; could remove

        let outcome = self.decoder.decode(&deint_llrs, MAX_ITERS_OFDM);

        // First ldpc_k bits of the decoded codeword are info+CRC.
        let info_plus_crc: BitVec<u8> =
            outcome.decoded[..self.ldpc_k].iter().copied().collect();
        verify_crc32(info_plus_crc.as_bitslice()).map_err(|e| {
            FecError::DecodeFailure(format!(
                "CRC mismatch after {} iterations: {e}",
                outcome.iterations_used
            ))
        })?;

        let payload_k = self.payload_bits();
        Ok(bitvec_to_bytes(&info_plus_crc[..payload_k]))
    }

    fn rate(&self) -> CodeRate {
        self.rate
    }

    fn block_info_bits(&self) -> usize {
        self.payload_bits()
    }

    fn block_coded_bits(&self) -> usize {
        self.n
    }
}

/// Compute the permutation that maps a deinterleaved index to the
/// corresponding interleaved-input index. `perm[i] = j` means
/// "output index i comes from input index j".
///
/// Mirrors [`crate::interleaver::interleave`]: the interleaved stream
/// at index `col * rows + row` is the matrix cell at `row * cols +
/// col`. So `perm[k] = (k / rows) + (k % rows) * cols`. The
/// `deinterleave` op reads the interleaved stream into the matrix in
/// the same column-major order and reads back row-major; we invert
/// that here for the LLR vector.
fn deinterleave_index_perm(n: usize, rows: usize) -> Vec<usize> {
    let cols = n / rows;
    debug_assert_eq!(n % rows, 0, "interleaver_rows must divide n");
    let mut perm = vec![0usize; n];
    for col in 0..cols {
        for row in 0..rows {
            let interleaved_idx = col * rows + row;
            let original_idx = row * cols + col;
            perm[original_idx] = interleaved_idx;
        }
    }
    perm
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_bits(n: usize, seed: u64) -> Vec<u8> {
        let mut state = seed;
        (0..n)
            .map(|_| {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                ((state >> 33) & 1) as u8
            })
            .collect()
    }

    #[test]
    fn round_trip_n648_r12_zero_noise() {
        let codec = OfdmAdaptiveCodec::new(BlockN::N648, WifiLdpcRate::R1_2);
        let payload = random_bits(codec.payload_bits(), 0x1234_5678);
        let encoded = codec.encode(&payload);
        assert_eq!(encoded.len(), codec.block_coded_bits());

        // Zero-noise channel: bit b maps to LLR +1 if 0, -1 if 1.
        let llrs: Vec<f32> = encoded
            .iter()
            .map(|&b| if b == 0 { 10.0 } else { -10.0 })
            .collect();
        let recovered = codec.decode_soft(&llrs).expect("decode_soft");
        assert_eq!(recovered, payload, "round-trip should be lossless");
    }

    #[test]
    fn round_trip_n1296_r34_zero_noise() {
        let codec = OfdmAdaptiveCodec::new(BlockN::N1296, WifiLdpcRate::R3_4);
        let payload = random_bits(codec.payload_bits(), 0xDEAD_BEEF);
        let encoded = codec.encode(&payload);
        assert_eq!(encoded.len(), codec.block_coded_bits());

        let llrs: Vec<f32> = encoded
            .iter()
            .map(|&b| if b == 0 { 10.0 } else { -10.0 })
            .collect();
        let recovered = codec.decode_soft(&llrs).expect("decode_soft");
        assert_eq!(recovered, payload);
    }

    #[test]
    fn block_size_includes_crc_overhead() {
        let codec = OfdmAdaptiveCodec::new(BlockN::N648, WifiLdpcRate::R1_2);
        // LDPC k = 324; payload k = 324 - 32 = 292.
        assert_eq!(codec.block_info_bits(), 292);
        assert_eq!(codec.block_coded_bits(), 648);
    }

    #[test]
    fn rate_reports_num_den() {
        let codec = OfdmAdaptiveCodec::new(BlockN::N648, WifiLdpcRate::R1_2);
        let r = codec.rate();
        assert_eq!(r.num, 1);
        assert_eq!(r.den, 2);
    }

    #[test]
    fn corrupted_codeword_returns_decode_failure() {
        let codec = OfdmAdaptiveCodec::new(BlockN::N648, WifiLdpcRate::R1_2);
        let payload = random_bits(codec.payload_bits(), 42);
        let encoded = codec.encode(&payload);

        // Flip enough bits that LDPC + CRC fail to recover.
        let mut llrs: Vec<f32> = encoded
            .iter()
            .map(|&b| if b == 0 { 1.0 } else { -1.0 })
            .collect();
        // 1/3 of bits flipped — far above the noise-floor mode's correctable threshold.
        for i in (0..llrs.len()).step_by(3) {
            llrs[i] = -llrs[i];
        }
        let result = codec.decode_soft(&llrs);
        assert!(
            result.is_err(),
            "heavily corrupted codeword should fail decode + CRC verify"
        );
    }
}
