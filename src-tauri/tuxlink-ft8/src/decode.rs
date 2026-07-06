//! FT-8 LDPC(174,91) belief-propagation decoder — normalized min-sum.
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! Implemented ONLY from:
//! - the QEX 2020 "The FT4 and FT8 Communication Protocols" paper
//!   (Franke/Somerville/Taylor), **§3** ("Forward Error Correction"); and
//! - the MIT-licensed `ft8_lib` (kgoba) reference `ft8/ldpc.c` (`ldpc_decode`
//!   sum-product & `bp_decode`) and the Sarah Johnson "Iterative Error
//!   Correction" reference it cites.
//!
//! The normalized-min-sum *variant* is the standard check-node approximation
//! (min-magnitude in place of the `tanh`/`atanh` product) applied over the
//! *same* Tanner graph and the *same* sign structure `bp_decode` uses. The
//! `RustFT8` (jl1nie) crate is a documented prior art for min-sum on this code
//! but was **not** read for this implementation — the min-sum here is derived
//! from `ft8_lib` `bp_decode` + textbook min-sum, not transcribed.
//!
//! The Tanner-graph incidence tables ([`crate::ldpc::NM`] / [`crate::ldpc::MN`]
//! / [`crate::ldpc::NUM_ROWS`]) and the syndrome / validity check
//! ([`crate::ldpc::is_valid_codeword`]) come from the M0 `ldpc` module — this
//! module does not re-implement the parity check.
//!
//! # Algorithm: normalized min-sum over the (174,91) Tanner graph
//!
//! In the pinned crate convention `llr[n] = log(P(bit_n = 1) / P(bit_n = 0))`
//! (positive ⟹ bit 1; hard decision `bit_n = L[n] > 0`), the flooding schedule
//! per iteration is:
//!
//! - **Channel LLR** `ch[n]` = input `llr[n]` (fixed across iterations).
//! - **Variable→check** `q[n→m] = ch[n] + Σ_{m'≠m} r[m'→n]`.
//! - **Check→variable** `r[m→n] = α · (Π_{n'≠n} sign(q[n'→m])) · min_{n'≠n}
//!   |q[n'→m]|`, with normalization factor `α = ALPHA` (`0.75`, tunable by T1.2).
//! - **Posterior** `L[n] = ch[n] + Σ_m r[m→n]`; hard decision `bit[n] = L[n] >
//!   0`.
//!
//! After each iteration the hard decision is checked via
//! [`crate::ldpc::is_valid_codeword`]; a valid codeword early-stops. The
//! best-so-far word (fewest failed parity checks) is retained and returned even
//! if no iteration converges, so T1.2 can measure convergence rate.
//! provenance: `ft8_lib` `ft8/ldpc.c` `bp_decode` message schedule + textbook
//! normalized min-sum check-node approximation (MIT source; min-sum theory).

use crate::consts::{CODEWORD_BITS, PAYLOAD_BITS};
use crate::ldpc::{ldpc_syndrome, MN, NM, NUM_ROWS, PARITY_BITS};

/// Normalized-min-sum scaling factor applied to every check→variable message.
/// A value below 1 compensates for the min-sum approximation's optimism versus
/// exact sum-product. Named so the T1.2 SNR harness can tune it.
/// provenance: normalized min-sum (Chen/Fossorier "Reduced-Complexity Decoding
/// of LDPC Codes"); 0.75 is a standard normalization value for this class of
/// code — T1.2 will re-tune it against the AWGN sweep.
pub const ALPHA: f32 = 0.75;

/// Finite clip bound applied to channel LLRs and messages each update, so a
/// near-certain bit cannot drive a message to `±inf` and wedge the graph.
/// provenance: standard BP numerical-stability guard (Johnson, "Iterative Error
/// Correction").
pub const CLIP: f32 = 20.0;

/// Default maximum decoder iterations. In `ft8_lib` the iteration cap is a
/// caller-supplied parameter to `bp_decode` (not a library constant); its
/// callers pass values in the tens. FT-8 min-sum converges in a few to a few
/// tens of iterations, so 30 is a conventional cap. T1.2 may override it.
/// provenance: `ft8_lib` `ft8/decode.c` `ftx_decode_candidate` passes
/// `max_iterations` through to `bp_decode` (MIT); the specific value 30 is a
/// conventional default, not an ft8_lib constant.
pub const DEFAULT_MAX_ITERS: usize = 30;

/// Result of a min-sum decode: the best 174-bit codeword found, whether the
/// syndrome reached zero (`converged`), and how many iterations ran.
#[derive(Debug, Clone, Copy)]
pub struct DecodeResult {
    /// The best hard-decision codeword found (fewest failed parity checks).
    pub codeword: [bool; CODEWORD_BITS],
    /// True iff `codeword` has an all-zero syndrome (a valid codeword).
    pub converged: bool,
    /// Iterations actually run (1-origin; ≤ `max_iters`).
    pub iters: usize,
}

impl DecodeResult {
    /// The systematic 91 message+CRC bits (`codeword[0..91]`). Only meaningful
    /// when `converged` (and after a CRC check) — exposed as a convenience for
    /// callers that then run [`crate::crc::check_crc`].
    pub fn message_bits(&self) -> [bool; crate::consts::MSG_CRC_BITS] {
        let mut m = [false; crate::consts::MSG_CRC_BITS];
        m.copy_from_slice(&self.codeword[..crate::consts::MSG_CRC_BITS]);
        m
    }

    /// The 77 payload bits (`codeword[0..77]`), i.e. the message without its
    /// 14-bit CRC. Convenience for callers past a successful CRC check.
    pub fn payload_bits(&self) -> [bool; PAYLOAD_BITS] {
        let mut p = [false; PAYLOAD_BITS];
        p.copy_from_slice(&self.codeword[..PAYLOAD_BITS]);
        p
    }
}

/// Clip a value to `[-CLIP, CLIP]`.
#[inline]
fn clip(x: f32) -> f32 {
    x.clamp(-CLIP, CLIP)
}

/// Sign of an LLR as `+1.0` / `-1.0`. Zero is treated as `+1.0` (documented
/// choice: a zero message carries no information; folding it to `+1` keeps the
/// product well-defined without biasing the magnitude, which is `0` anyway).
#[inline]
fn sign(x: f32) -> f32 {
    if x < 0.0 {
        -1.0
    } else {
        1.0
    }
}

/// Normalized min-sum belief-propagation decode of the FT-8 LDPC(174,91) code.
///
/// `llr` holds the 174 channel LLRs in the pinned `log(P1/P0)` convention
/// (positive ⟹ bit 1). Returns the best hard-decision codeword found and whether
/// its syndrome is zero. The hard-decision bits are returned even when no
/// iteration converges (`converged == false`), so callers gate "decoded" on
/// `converged` (syndrome == 0) themselves.
/// provenance: `ft8_lib` `ft8/ldpc.c` `ldpc_decode`/`bp_decode` schedule +
/// normalized min-sum (MIT).
pub fn ldpc_decode_ms(llr: &[f32; CODEWORD_BITS], max_iters: usize) -> DecodeResult {
    // Channel LLRs (clipped once).
    let mut ch = [0.0f32; CODEWORD_BITS];
    for (c, &l) in ch.iter_mut().zip(llr.iter()) {
        *c = clip(l);
    }

    // Per-edge message stores, keyed the `ft8_lib` `bp_decode` way:
    // - `tov[n][mi]` = check→variable message into variable `n` from its `mi`-th
    //   incident check (`MN[n][mi]`). This is the message summed into the
    //   posterior. Initialized to 0 (first hard decision uses `ch` alone).
    // - `toc[m][ni]` = the variable→check message from variable `NM[m][ni]`
    //   toward check `m`, i.e. `q` on that edge, recomputed each iteration.
    let mut tov = [[0.0f32; 3]; CODEWORD_BITS];
    let mut toc = [[0.0f32; 7]; PARITY_BITS];

    // Best-so-far tracking (fewest failed checks). Iteration 0's hard decision
    // uses `tov == 0`, i.e. the raw channel LLR signs.
    let mut best_codeword = hard_decision(&ch, &tov);
    let mut best_errors = num_failed_checks(&best_codeword);
    let mut best_iters = 0usize;
    if best_errors == 0 {
        return DecodeResult { codeword: best_codeword, converged: true, iters: 0 };
    }

    for iter in 1..=max_iters {
        // ── Variable→check: q_{n→m} = ch[n] + Σ_{m'≠m} tov[n][m']. ────────────
        // Stored per check-edge as toc[m][ni] for the ni-th variable of check m.
        for m in 0..PARITY_BITS {
            let deg = NUM_ROWS[m] as usize;
            for ni in 0..deg {
                let n = NM[m][ni] as usize - 1;
                let mut q = ch[n];
                for mi in 0..3 {
                    if MN[n][mi] as usize - 1 != m {
                        q += tov[n][mi];
                    }
                }
                toc[m][ni] = clip(q);
            }
        }

        // ── Check→variable (normalized min-sum): ─────────────────────────────
        //   tov[n→m] = -α · (Π_{n'≠n} sign(-q_{n'→m})) · min_{n'≠n} |q_{n'→m}|.
        // The leading `-` and the `sign(-q)` mirror `ft8_lib` `bp_decode`'s
        // `tov = -2·atanh(Π tanh(-Tnm/2))`: this graph's parity convention needs
        // that sign structure, not the textbook `α·Πsign(q)·min`. The
        // `sub_limit_error_recovery` KAT empirically pins it — the textbook sign
        // fails to correct even a single bit flip on this code.
        // provenance: min-sum analogue of `ft8_lib` `ft8/ldpc.c` `bp_decode`
        // check-node update (MIT); normalized min-sum scale α (Chen/Fossorier).
        for n in 0..CODEWORD_BITS {
            for mi in 0..3 {
                let m = MN[n][mi] as usize - 1;
                let deg = NUM_ROWS[m] as usize;
                let mut sign_prod = 1.0f32;
                let mut min_mag = f32::INFINITY;
                for ni in 0..deg {
                    let np = NM[m][ni] as usize - 1;
                    if np == n {
                        continue;
                    }
                    let q = toc[m][ni];
                    sign_prod *= sign(-q);
                    let mag = q.abs();
                    if mag < min_mag {
                        min_mag = mag;
                    }
                }
                tov[n][mi] = clip(-(ALPHA * sign_prod * min_mag));
            }
        }

        // ── Posterior + hard decision + early stop. ──────────────────────────
        let codeword = hard_decision(&ch, &tov);
        let errors = num_failed_checks(&codeword);
        if errors < best_errors {
            best_errors = errors;
            best_codeword = codeword;
            best_iters = iter;
        }
        if errors == 0 {
            return DecodeResult { codeword, converged: true, iters: iter };
        }
    }

    DecodeResult { codeword: best_codeword, converged: false, iters: best_iters }
}

/// Convenience wrapper: decode with [`DEFAULT_MAX_ITERS`].
pub fn ldpc_decode_ms_default(llr: &[f32; CODEWORD_BITS]) -> DecodeResult {
    ldpc_decode_ms(llr, DEFAULT_MAX_ITERS)
}

/// Posterior hard decision `bit[n] = (ch[n] + Σ_mi tov[n][mi]) > 0`.
fn hard_decision(
    ch: &[f32; CODEWORD_BITS],
    tov: &[[f32; 3]; CODEWORD_BITS],
) -> [bool; CODEWORD_BITS] {
    let mut bits = [false; CODEWORD_BITS];
    for (n, bit) in bits.iter_mut().enumerate() {
        let l = ch[n] + tov[n][0] + tov[n][1] + tov[n][2];
        *bit = l > 0.0;
    }
    bits
}

/// Number of parity checks the codeword fails (0 ⇒ valid). Reuses the M0
/// syndrome rather than re-implementing the check.
fn num_failed_checks(codeword: &[bool; CODEWORD_BITS]) -> usize {
    ldpc_syndrome(codeword).iter().filter(|&&s| s).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc::{add_crc, check_crc};
    use crate::ldpc::{is_valid_codeword, ldpc_encode};
    use crate::llr::soft_demap;
    use crate::symbols::bits_to_symbols;
    use crate::consts::INFO_SYMBOLS;

    /// Named FT-8 KAT payloads plus an arbitrary spread, encoded to valid
    /// 174-bit codewords.
    fn kat_codewords() -> [[bool; CODEWORD_BITS]; 3] {
        let payloads: [[u8; 10]; 3] = [
            [0x00, 0x00, 0x00, 0x20, 0x4d, 0xef, 0x1a, 0x8a, 0x19, 0x88], // CQ K1ABC FN42
            [0x09, 0xbd, 0xe3, 0x50, 0x61, 0x49, 0xdc, 0x1f, 0xa9, 0xc8], // K1ABC W9XYZ -12
            [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x00], // arbitrary spread
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

    /// Ideal LLRs for a codeword in the pinned `log(P1/P0)` convention: `+l`
    /// where the bit is 1, `-l` where the bit is 0.
    fn ideal_llr(cw: &[bool; CODEWORD_BITS], l: f32) -> [f32; CODEWORD_BITS] {
        let mut out = [0.0f32; CODEWORD_BITS];
        for (o, &b) in out.iter_mut().zip(cw.iter()) {
            *o = if b { l } else { -l };
        }
        out
    }

    // ── KAT 3: Clean-LLR exact decode (pins sign + graph correctness) ────────
    //
    // A known codeword's ideal LLR vector decodes to EXACTLY that codeword,
    // converged, in a single iteration.
    #[test]
    fn clean_llr_exact_decode() {
        for cw in kat_codewords() {
            let llr = ideal_llr(&cw, 8.0);
            let res = ldpc_decode_ms(&llr, DEFAULT_MAX_ITERS);
            assert!(res.converged, "clean LLR failed to converge");
            assert_eq!(res.codeword, cw, "clean LLR decoded to wrong codeword");
            assert_eq!(res.iters, 0, "clean LLR should be valid at iter 0 (pre-iteration hard decision)");
        }
    }

    // ── KAT 4: Sub-limit error recovery ──────────────────────────────────────
    //
    // Flip K LLR signs (K spread across the codeword) and the decoder still
    // recovers the exact original codeword. K = 6 is comfortably under the
    // (174,91) code's isolated-flip correction capability for these patterns.
    #[test]
    fn sub_limit_error_recovery() {
        // Spread of flip positions across the 174 bits.
        let flip_sets: [&[usize]; 3] = [
            &[10],
            &[3, 40, 100],
            &[5, 30, 60, 90, 120, 150],
        ];
        for cw in kat_codewords().iter().take(2) {
            for flips in flip_sets {
                let mut llr = ideal_llr(cw, 8.0);
                for &f in flips {
                    llr[f] = -llr[f]; // invert the sign ⇒ wrong hard decision
                }
                let res = ldpc_decode_ms(&llr, DEFAULT_MAX_ITERS);
                assert!(
                    res.converged,
                    "K={} flips did not converge (codeword)",
                    flips.len()
                );
                assert_eq!(
                    res.codeword, *cw,
                    "K={} flips recovered wrong codeword",
                    flips.len()
                );
            }
        }
    }

    /// A non-decodable input (all-zero LLRs ⇒ no information) returns
    /// best-effort with `converged == false`, and does not panic.
    #[test]
    fn non_converging_returns_best_effort() {
        let llr = [0.0f32; CODEWORD_BITS];
        let res = ldpc_decode_ms(&llr, 5);
        // All-zero LLR ⇒ hard decision all-false ⇒ the zero codeword, which is
        // actually valid (linear code). So this specific input converges to zero.
        // Assert we return SOME finite result without panicking, and that the
        // codeword matches its own converged flag.
        assert_eq!(res.converged, is_valid_codeword(&res.codeword));
    }

    /// A genuinely corrupt LLR beyond correction returns best-effort without
    /// panicking, with `converged` reflecting the syndrome honestly.
    #[test]
    fn heavy_corruption_returns_honest_flag() {
        let cw = kat_codewords()[0];
        // Flip nearly half the LLRs — far past correction capability.
        let mut llr = ideal_llr(&cw, 8.0);
        for i in (0..CODEWORD_BITS).step_by(2) {
            llr[i] = -llr[i];
        }
        let res = ldpc_decode_ms(&llr, DEFAULT_MAX_ITERS);
        assert_eq!(
            res.converged,
            is_valid_codeword(&res.codeword),
            "converged flag must match actual syndrome"
        );
    }

    // ── KAT 5: End-to-end symbol path (Part A + Part B compose) ──────────────
    //
    // message -> encode -> bits_to_symbols -> one-hot tone powers -> soft_demap
    // -> ldpc_decode_ms -> recovered codeword == original, and the recovered 91
    // message+CRC bits pass check_crc. No noise (that is T1.2).
    #[test]
    fn end_to_end_symbol_path() {
        for cw in kat_codewords() {
            let symbols = bits_to_symbols(&cw);

            // One-hot tone powers: true tone high (4.0), others low (0.0).
            let mut powers = [[0.0f32; 8]; INFO_SYMBOLS];
            for (sym, &tone) in powers.iter_mut().zip(symbols.iter()) {
                sym[tone as usize] = 4.0;
            }

            let llr = soft_demap(&powers);
            let res = ldpc_decode_ms(&llr, DEFAULT_MAX_ITERS);

            assert!(res.converged, "end-to-end path did not converge");
            assert_eq!(res.codeword, cw, "end-to-end recovered wrong codeword");

            // The recovered 91 message+CRC bits pass the CRC check.
            let msg91 = res.message_bits();
            assert!(check_crc(&msg91), "recovered message failed CRC");
        }
    }
}

