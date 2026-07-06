//! FT-8 soft-demapper: 8 per-symbol FSK tone powers → 174 variance-normalized
//! codeword-bit log-likelihood ratios (LLRs).
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! Implemented ONLY from:
//! - the QEX 2020 "The FT4 and FT8 Communication Protocols" paper
//!   (Franke/Somerville/Taylor), **§6** ("soft symbol" — the per-symbol bit
//!   metric `Lj = K·(max|Ci| over xj=1 − max|Ci| over xj=0)`); and
//! - the MIT-licensed `ft8_lib` (kgoba) reference `ft8/decode.c` —
//!   `ft8_extract_symbol` (the max-log demap `l0/l1/l2` formulas) and
//!   `ftx_normalize_logl` (the `sqrt(24 / variance)` scaling), re-expressed in
//!   idiomatic Rust, constants cited below.
//!
//! # LLR sign convention (pinned — the #1 trap)
//!
//! Throughout this crate:
//!
//! > **`llr[i] = log( P(bit_i = 1) / P(bit_i = 0) )`. Positive LLR ⟹ bit 1.
//! > Hard decision: `bit_i = llr[i] > 0.0`.**
//!
//! The `ft8_lib` source carries a stale/wrong comment
//! (`codeword[i] = log(P(x=0)/P(x=1))`) that does NOT match its arithmetic; we
//! deliberately do **not** inherit that confusion. The max-log formulas below
//! are used with **no sign flip** — `max(tones where bit=1) − max(tones where
//! bit=0)` — so a larger power on a bit=1 tone drives the LLR positive, exactly
//! matching the convention. The `demap_one_hot_round_trip` KAT in the tests
//! pins this empirically against [`crate::symbols::symbols_to_bits`].
//!
//! # What this module does
//!
//! For each of the 58 info symbols the input is `[f32; 8]` — a power (received
//! energy) per FSK tone 0..=7. Each symbol yields 3 bit LLRs (MSB-first, in the
//! same order [`crate::symbols::symbols_to_bits`] emits them), for 58 × 3 = 174
//! LLRs in codeword-bit order. The full vector is then variance-normalized.

use crate::consts::{CODEWORD_BITS, INFO_SYMBOLS};
use crate::symbols::GRAY_MAP;

/// Experimentally-tuned normalization coefficient from `ft8_lib`. The LLR vector
/// is scaled by `sqrt(NORM_COEFF / variance)` so its spread matches the
/// calibration that makes the published −20.8 dB FT-8 decode threshold
/// reachable. It is a tuning constant, not a first-principles value.
/// provenance: `ft8_lib` `ft8/decode.c` `ftx_normalize_logl` literal `24.0f`
/// (MIT).
pub const NORM_COEFF: f32 = 24.0;

/// The three max-log bit LLRs for a single info symbol, given the 8 tone powers
/// `p` (index = tone 0..=7). Returns `[l0, l1, l2]` where `l0` is the MSB of the
/// 3-bit Gray group (matching [`crate::symbols::symbols_to_bits`]'s first bit for
/// this symbol), in the pinned `log(P1/P0)` convention (positive ⟹ bit 1).
///
/// The tone powers are first reordered by the Gray map into
/// `s2[j] = p[GRAY_MAP[j]]` (so `s2` is indexed by the 3-bit group value `j`),
/// then each bit's LLR is `max(powers over groups where that bit = 1) −
/// max(powers over groups where that bit = 0)`.
/// provenance: `ft8_lib` `ft8/decode.c` `ft8_extract_symbol` `logl[0..3]` (MIT);
/// QEX 2020 §6 soft-symbol metric.
pub fn demap_symbol(p: &[f32; 8]) -> [f32; 3] {
    // Reorder tone powers by the Gray map: s2 is indexed by 3-bit group value j.
    // provenance: `ft8_lib` `ft8/decode.c` `s2[j] = mag(wf[kFT8_Gray_map[j]])`.
    let mut s2 = [0.0f32; 8];
    for (j, s) in s2.iter_mut().enumerate() {
        *s = p[GRAY_MAP[j] as usize];
    }

    // Bit b is set in group values {j : j has bit b set}. For a 3-bit group
    // value j = (b0 b1 b2) MSB-first: b0 set ⇒ j ∈ {4,5,6,7}, b1 set ⇒ j ∈
    // {2,3,6,7}, b2 set ⇒ j ∈ {1,3,5,7}. max-log LLR = max(bit=1) − max(bit=0).
    let l0 = max4(s2[4], s2[5], s2[6], s2[7]) - max4(s2[0], s2[1], s2[2], s2[3]);
    let l1 = max4(s2[2], s2[3], s2[6], s2[7]) - max4(s2[0], s2[1], s2[4], s2[5]);
    let l2 = max4(s2[1], s2[3], s2[5], s2[7]) - max4(s2[0], s2[2], s2[4], s2[6]);
    [l0, l1, l2]
}

/// Maximum of four `f32` values.
/// provenance: `ft8_lib` `ft8/decode.c` `max4` (MIT).
#[inline]
fn max4(a: f32, b: f32, c: f32, d: f32) -> f32 {
    a.max(b).max(c).max(d)
}

/// Soft-demap 58 info symbols' tone powers into 174 variance-normalized
/// codeword-bit LLRs (pinned `log(P1/P0)` convention; positive ⟹ bit 1).
///
/// Each symbol contributes its 3 bit LLRs in codeword-bit order (symbol `i` →
/// codeword bits `3i, 3i+1, 3i+2`), matching [`crate::symbols::symbols_to_bits`].
/// The assembled vector is then scaled by `sqrt(NORM_COEFF / variance)`.
///
/// # Degenerate input
///
/// If every LLR is equal (variance ≤ 0), e.g. all tone powers identical, the
/// scale factor would be `NaN`/`inf`; in that case the un-scaled LLRs are
/// returned instead (all-zero for a flat input), so the decoder receives a
/// finite, information-free vector rather than `NaN`.
/// provenance: `ft8_lib` `ft8/decode.c` `ft8_extract_symbol` +
/// `ftx_normalize_logl` (MIT).
pub fn soft_demap(tone_powers: &[[f32; 8]; INFO_SYMBOLS]) -> [f32; CODEWORD_BITS] {
    let mut llr = [0.0f32; CODEWORD_BITS];
    for (i, p) in tone_powers.iter().enumerate() {
        let [l0, l1, l2] = demap_symbol(p);
        let b = i * 3;
        llr[b] = l0;
        llr[b + 1] = l1;
        llr[b + 2] = l2;
    }
    normalize_llr(&mut llr);
    llr
}

/// Variance-normalize an LLR vector in place: scale every entry by
/// `sqrt(NORM_COEFF / variance)`. Guards the degenerate `variance ≤ 0` case
/// (all entries equal) by leaving the vector unscaled to avoid `NaN`/`inf`.
/// provenance: `ft8_lib` `ft8/decode.c` `ftx_normalize_logl` (MIT).
pub fn normalize_llr(llr: &mut [f32; CODEWORD_BITS]) {
    let n = CODEWORD_BITS as f32;
    let mut sum = 0.0f32;
    let mut sum2 = 0.0f32;
    for &x in llr.iter() {
        sum += x;
        sum2 += x * x;
    }
    let mean = sum / n;
    let variance = sum2 / n - mean * mean;
    if variance <= 0.0 {
        // Degenerate all-equal input: scaling by sqrt(k/0) is NaN/inf. Leave the
        // vector as-is (a flat input carries no bit information anyway).
        return;
    }
    let factor = (NORM_COEFF / variance).sqrt();
    for x in llr.iter_mut() {
        *x *= factor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc::add_crc;
    use crate::ldpc::ldpc_encode;
    use crate::symbols::{bits_to_symbols, symbols_to_bits};
    use crate::consts::PAYLOAD_BITS;

    /// Three deterministic 174-bit codewords (valid FT-8 codewords) to demap
    /// against. Reuses the T0.x KAT payloads so the fixtures are known-good.
    fn kat_codewords() -> [[bool; CODEWORD_BITS]; 3] {
        let payloads: [[u8; 10]; 3] = [
            [0x00, 0x00, 0x00, 0x20, 0x4d, 0xef, 0x1a, 0x8a, 0x19, 0x88], // CQ K1ABC FN42
            [0x09, 0xbd, 0xe3, 0x50, 0x61, 0x49, 0xdc, 0x1f, 0xa9, 0xc8], // K1ABC W9XYZ -12
            [0x55, 0xaa, 0x13, 0x9c, 0x00, 0xff, 0x42, 0x7e, 0x81, 0x00], // arbitrary spread
        ];
        let mut out = [[false; CODEWORD_BITS]; 3];
        for (o, p) in out.iter_mut().zip(payloads.iter()) {
            let mut payload = [false; PAYLOAD_BITS];
            for (i, bit) in payload.iter_mut().enumerate() {
                *bit = p[i / 8] & (0x80 >> (i % 8)) != 0;
            }
            *o = ldpc_encode(&add_crc(&payload));
        }
        out
    }

    /// Build one-hot tone powers from 58 true tones: `high` on the true tone,
    /// `low` on every other tone.
    fn one_hot_powers(symbols: &[u8; INFO_SYMBOLS], high: f32, low: f32) -> [[f32; 8]; INFO_SYMBOLS] {
        let mut powers = [[low; 8]; INFO_SYMBOLS];
        for (sym, tone) in powers.iter_mut().zip(symbols.iter()) {
            sym[*tone as usize] = high;
        }
        powers
    }

    // ── KAT 2: Demapper one-hot round-trip (pins sign + bit-order) ───────────
    //
    // For each of ≥3 codewords, at every one of the 58 symbol positions, build
    // one-hot tone powers for the TRUE tone, demap, hard-decide (> 0), and assert
    // the 3 recovered bits equal `symbols_to_bits` for that symbol. If the sign
    // convention were inverted or the bit order wrong, this fails immediately.
    #[test]
    fn demap_one_hot_round_trip() {
        for cw in kat_codewords() {
            let symbols = bits_to_symbols(&cw);
            let truth = symbols_to_bits(&symbols); // == cw
            assert_eq!(truth, cw, "sanity: symbols_to_bits round-trips the codeword");

            // Whole-codeword one-hot demap: every symbol's true tone gets power
            // 1.0, others 0.0. Hard decisions must equal the true codeword bits.
            let powers = one_hot_powers(&symbols, 1.0, 0.0);
            let llr = soft_demap(&powers);
            for (i, &bit) in cw.iter().enumerate() {
                let decided = llr[i] > 0.0;
                assert_eq!(decided, bit, "one-hot demap bit {i} mismatch");
            }

            // Per-symbol check too: isolate each symbol's 3 bits.
            for (i, &tone) in symbols.iter().enumerate() {
                let mut p = [0.0f32; 8];
                p[tone as usize] = 1.0;
                let [l0, l1, l2] = demap_symbol(&p);
                let b = i * 3;
                assert_eq!(l0 > 0.0, cw[b], "symbol {i} bit0");
                assert_eq!(l1 > 0.0, cw[b + 1], "symbol {i} bit1");
                assert_eq!(l2 > 0.0, cw[b + 2], "symbol {i} bit2");
            }
        }
    }

    /// The normalization is a positive scale: it preserves every sign (and thus
    /// every hard decision) and rescales the variance to `NORM_COEFF`.
    #[test]
    fn normalize_preserves_signs_and_sets_variance() {
        let cw = kat_codewords()[0];
        let symbols = bits_to_symbols(&cw);
        let powers = one_hot_powers(&symbols, 3.0, 1.0);

        // Un-normalized per-symbol assembly for comparison.
        let mut raw = [0.0f32; CODEWORD_BITS];
        for (i, p) in powers.iter().enumerate() {
            let [l0, l1, l2] = demap_symbol(p);
            raw[i * 3] = l0;
            raw[i * 3 + 1] = l1;
            raw[i * 3 + 2] = l2;
        }
        let norm = soft_demap(&powers);

        // Sign preserved for every bit.
        for i in 0..CODEWORD_BITS {
            assert_eq!(raw[i] > 0.0, norm[i] > 0.0, "sign flipped at {i} by normalize");
        }

        // Variance of the normalized vector is NORM_COEFF (to float tolerance).
        let n = CODEWORD_BITS as f32;
        let sum: f32 = norm.iter().sum();
        let sum2: f32 = norm.iter().map(|x| x * x).sum();
        let mean = sum / n;
        let variance = sum2 / n - mean * mean;
        assert!((variance - NORM_COEFF).abs() < 1e-2, "normalized variance {variance} != {NORM_COEFF}");
    }

    /// Degenerate zero-variance input (all tone powers identical ⇒ all LLRs
    /// equal ⇒ variance 0) must NOT produce NaN/inf; the guard returns the
    /// unscaled (all-zero) vector.
    #[test]
    fn zero_variance_input_is_finite() {
        let flat = [[2.5f32; 8]; INFO_SYMBOLS];
        let llr = soft_demap(&flat);
        for (i, &x) in llr.iter().enumerate() {
            assert!(x.is_finite(), "LLR {i} not finite: {x}");
            // Flat input ⇒ every max4 term equal ⇒ every raw LLR is exactly 0.
            assert_eq!(x, 0.0, "flat input should give zero LLRs, got {x} at {i}");
        }
    }

    /// `demap_symbol` on a flat single symbol yields all-zero LLRs (each max4
    /// difference is zero), independent of the flat level.
    #[test]
    fn demap_symbol_flat_is_zero() {
        for level in [0.0f32, 1.0, 7.5] {
            let p = [level; 8];
            assert_eq!(demap_symbol(&p), [0.0, 0.0, 0.0], "flat level {level}");
        }
    }
}
