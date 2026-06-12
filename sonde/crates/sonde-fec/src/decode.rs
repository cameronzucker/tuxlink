//! Sum-product-algorithm (SPA) belief-propagation LDPC decoder.
//!
//! Implementation in log-likelihood-ratio (LLR) form for numerical
//! stability. Per Richardson-Urbanke 2001 and standard LDPC references.
//!
//! Iteration structure (per Tanner-graph message passing):
//! 1. Initialize variable-to-check messages = channel LLRs.
//! 2. **Check-to-variable** update: for each check c, for each adjacent
//!    variable v at position i in `check_to_vars[c]`:
//!    `M_{c→v} = boxplus over all i' != i of M_{v'→c}`.
//! 3. **Variable-to-check** update: for each variable v, for each
//!    adjacent check c at position j in `var_to_checks[v]`:
//!    `M_{v→c} = channel[v] + sum over all j' != j of M_{c'→v}`.
//! 4. Posterior: `posterior[v] = channel[v] + sum over all c adjacent
//!    to v of M_{c→v}`.
//! 5. Hard-decide; check if all parity equations satisfied; if yes,
//!    converged.

#![allow(clippy::needless_range_loop)]

use crate::llr::boxplus;
use crate::parity_matrix::ParityCheckMatrix;

/// Outcome of one [`Decoder::decode`] run.
pub struct DecodeOutcome {
    /// Hard-decided bit vector of length n.
    pub decoded: Vec<bool>,
    /// Number of iterations actually run (1..=max_iters).
    pub iterations_used: u32,
    /// `true` if all parity checks were satisfied at termination.
    pub converged: bool,
}

/// Cached decoder state for one LDPC code (one `H` matrix). Builds
/// Tanner-graph adjacency lists + edge-inverse maps once; per-decode
/// is then pure SPA iteration.
pub struct Decoder {
    n: usize,
    k: usize,
    /// `var_to_checks[v]` = checks adjacent to variable v.
    var_to_checks: Vec<Vec<usize>>,
    /// `check_to_vars[c]` = variables adjacent to check c (=`h.rows[c]`).
    check_to_vars: Vec<Vec<usize>>,
    /// `v_edge_pos[c][i]` = the j-index such that
    /// `var_to_checks[check_to_vars[c][i]][j] == c`. Lets the
    /// check-update step look up the corresponding outgoing-from-
    /// variable message position in O(1).
    v_edge_pos: Vec<Vec<usize>>,
    /// `c_edge_pos[v][j]` = the i-index such that
    /// `check_to_vars[var_to_checks[v][j]][i] == v`. Lets the
    /// variable-update step look up the corresponding outgoing-from-
    /// check message position in O(1).
    c_edge_pos: Vec<Vec<usize>>,
}

impl Decoder {
    /// Build a decoder from `H`. Precomputes both directions of the
    /// Tanner-graph adjacency + edge-inverse maps.
    pub fn new(h: &ParityCheckMatrix) -> Self {
        let n = h.n;
        let k = h.k;

        let check_to_vars: Vec<Vec<usize>> = h.rows.clone();

        let mut var_to_checks: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (c, row) in h.rows.iter().enumerate() {
            for &v in row {
                var_to_checks[v].push(c);
            }
        }

        // For each (c, i) edge → which j-index in var_to_checks[v]?
        let mut v_edge_pos: Vec<Vec<usize>> = Vec::with_capacity(check_to_vars.len());
        for (c, vars) in check_to_vars.iter().enumerate() {
            let row: Vec<usize> = vars
                .iter()
                .map(|&v| {
                    var_to_checks[v]
                        .iter()
                        .position(|&c2| c2 == c)
                        .expect("inconsistent Tanner graph: c not in var_to_checks[v]")
                })
                .collect();
            v_edge_pos.push(row);
        }

        // For each (v, j) edge → which i-index in check_to_vars[c]?
        let mut c_edge_pos: Vec<Vec<usize>> = vec![Vec::new(); n];
        for v in 0..n {
            c_edge_pos[v] = var_to_checks[v]
                .iter()
                .map(|&c| {
                    check_to_vars[c]
                        .iter()
                        .position(|&v2| v2 == v)
                        .expect("inconsistent Tanner graph: v not in check_to_vars[c]")
                })
                .collect();
        }

        Self {
            n,
            k,
            var_to_checks,
            check_to_vars,
            v_edge_pos,
            c_edge_pos,
        }
    }

    /// Codeword length n.
    pub fn n(&self) -> usize {
        self.n
    }

    /// Info-bit length k.
    pub fn k(&self) -> usize {
        self.k
    }

    /// Decode channel LLRs (length n) into hard-decision bits. Runs
    /// up to `max_iters` SPA iterations; early-terminates as soon as
    /// all parity checks are satisfied.
    ///
    /// # Panics
    /// Panics if `llrs.len() != self.n()`.
    pub fn decode(&self, llrs: &[f32], max_iters: u32) -> DecodeOutcome {
        assert_eq!(
            llrs.len(),
            self.n,
            "llrs length {} != n {}",
            llrs.len(),
            self.n
        );

        let n = self.n;
        let channel: Vec<f32> = llrs.to_vec();

        // Per-edge messages.
        //   msg_v_to_c[v][j] = message FROM v TO var_to_checks[v][j].
        //   msg_c_to_v[c][i] = message FROM c TO check_to_vars[c][i].
        let mut msg_v_to_c: Vec<Vec<f32>> = self
            .var_to_checks
            .iter()
            .enumerate()
            .map(|(v, checks)| vec![channel[v]; checks.len()])
            .collect();
        let mut msg_c_to_v: Vec<Vec<f32>> = self
            .check_to_vars
            .iter()
            .map(|vars| vec![0.0_f32; vars.len()])
            .collect();

        let mut converged = false;
        let mut iter_count: u32 = 0;
        let mut decoded: Vec<bool> = vec![false; n];

        for iter in 0..max_iters {
            iter_count = iter + 1;

            // Check-to-variable update.
            for c in 0..self.check_to_vars.len() {
                let vars = &self.check_to_vars[c];
                // Collect incoming v→c messages for this check (O(1) via v_edge_pos).
                let incoming: Vec<f32> = vars
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| msg_v_to_c[v][self.v_edge_pos[c][i]])
                    .collect();

                for i in 0..vars.len() {
                    let mut acc = f32::INFINITY;
                    for (i2, &m) in incoming.iter().enumerate() {
                        if i2 != i {
                            if acc.is_infinite() {
                                acc = m;
                            } else {
                                acc = boxplus(acc, m);
                            }
                        }
                    }
                    if acc.is_infinite() {
                        // Degenerate row with only one variable; outgoing is 0.
                        acc = 0.0;
                    }
                    msg_c_to_v[c][i] = acc;
                }
            }

            // Variable-to-check update.
            for v in 0..n {
                let checks = &self.var_to_checks[v];
                let incoming: Vec<f32> = checks
                    .iter()
                    .enumerate()
                    .map(|(j, &c)| msg_c_to_v[c][self.c_edge_pos[v][j]])
                    .collect();

                let total_sum: f32 = incoming.iter().sum();

                for j in 0..checks.len() {
                    msg_v_to_c[v][j] = channel[v] + total_sum - incoming[j];
                }
            }

            // Posterior + hard decision.
            for v in 0..n {
                let post: f32 = channel[v]
                    + self
                        .var_to_checks[v]
                        .iter()
                        .enumerate()
                        .map(|(j, &c)| msg_c_to_v[c][self.c_edge_pos[v][j]])
                        .sum::<f32>();
                decoded[v] = post < 0.0;
            }

            // Convergence: all parity checks satisfied?
            let all_satisfied = self
                .check_to_vars
                .iter()
                .all(|vars| !vars.iter().fold(false, |acc, &v| acc ^ decoded[v]));
            if all_satisfied {
                converged = true;
                break;
            }
        }

        DecodeOutcome {
            decoded,
            iterations_used: iter_count,
            converged,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::{self, BlockN, CodeFamily, WifiLdpcRate};
    use crate::encode::Encoder;
    use bitvec::prelude::*;

    fn codeword_to_llrs(codeword: &BitSlice<u8>, certainty: f32) -> Vec<f32> {
        // BPSK mapping: bit 0 → +certainty (positive LLR = bit 0);
        // bit 1 → -certainty.
        codeword
            .iter()
            .map(|b| if *b { -certainty } else { certainty })
            .collect()
    }

    #[test]
    fn decode_zero_noise_returns_input() {
        let h = codes::build(CodeFamily::OfdmAdaptive {
            block_n: BlockN::N648,
            rate: WifiLdpcRate::R1_2,
        });
        let enc = Encoder::new(&h);
        let dec = Decoder::new(&h);

        let info: BitVec<u8> = (0..enc.k()).map(|i| (i % 7) == 0).collect();
        let codeword = enc.encode(info.as_bitslice());

        let llrs = codeword_to_llrs(codeword.as_bitslice(), 10.0);

        let outcome = dec.decode(&llrs, 50);

        assert!(outcome.converged, "decoder did not converge in zero noise");
        assert_eq!(outcome.decoded.len(), enc.n());
        for i in 0..enc.k() {
            assert_eq!(
                outcome.decoded[i],
                info[i],
                "bit {i} mismatch after zero-noise decode"
            );
        }
    }

    #[test]
    fn decode_one_bit_flip_recovers() {
        let h = codes::build(CodeFamily::OfdmAdaptive {
            block_n: BlockN::N648,
            rate: WifiLdpcRate::R1_2,
        });
        let enc = Encoder::new(&h);
        let dec = Decoder::new(&h);

        let info: BitVec<u8> = (0..enc.k()).map(|i| (i % 5) == 0).collect();
        let codeword = enc.encode(info.as_bitslice());

        let mut llrs = codeword_to_llrs(codeword.as_bitslice(), 5.0);
        llrs[42] = -llrs[42]; // hard flip with original confidence

        let outcome = dec.decode(&llrs, 50);

        assert!(
            outcome.converged,
            "decoder did not converge on single bit flip (iters={})",
            outcome.iterations_used
        );
        for i in 0..enc.k() {
            assert_eq!(
                outcome.decoded[i],
                info[i],
                "bit {i} mismatch after single-flip recovery"
            );
        }
    }
}
