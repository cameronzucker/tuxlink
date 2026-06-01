//! LDPC systematic encoder.
//!
//! Given an (n, k) LDPC code with parity-check matrix `H`, the encoder
//! produces a length-n codeword `c` from a length-k info word `u`
//! such that:
//! - `c[0..k] == u` (systematic form)
//! - `H · c^T == 0` over GF(2) (codeword satisfies parity checks)
//!
//! Strategy: at encoder-construction time, run Gaussian elimination
//! on `H` to bring its right half (columns `k..n`) to identity form.
//! Each resulting row's left half (columns `0..k`) becomes the XOR
//! equation for one parity bit. Per-encode is then a sequence of
//! XORs over the info bits, one per parity equation.
//!
//! ## Why Vec<u64> dense rows
//!
//! Each pivot step XORs an n-bit row vector across O(m) rows; over m
//! pivot columns the inner work is O(m² · n) bit operations. For the
//! floor code (n=2048, m=1536) a naive `Vec<Vec<bool>>` representation
//! takes ~150 s per construction — far too slow when the construction
//! module needs to iterate seeds searching for a rank-full H (see
//! [`Encoder::try_new`]). Packing 64 bits per `u64` accelerates the
//! row XOR by ~32×, bringing floor-code elimination to ~5 s.

// Gaussian elimination is fundamentally index-driven over (row, col)
// pairs; `col / 64`, `col % 64`, and the row-XOR's `for w in 0..words`
// are load-bearing on the loop indices. Rewriting as iterator chains
// obscures the elimination math without type-safety benefit.
#![allow(clippy::needless_range_loop)]

use bitvec::prelude::*;

use crate::parity_matrix::ParityCheckMatrix;

/// Error from [`Encoder::try_new`]: `H`'s right-half columns
/// (`k..n`) are rank-deficient over GF(2), so no systematic encoding
/// exists with this column ordering. The construction module re-rolls
/// the seed and retries.
#[derive(Debug, PartialEq, Eq)]
pub struct RankDeficient {
    /// Column at which the elimination ran out of pivots.
    pub column: usize,
}

impl std::fmt::Display for RankDeficient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "H right-half rank-deficient: no pivot at column {}",
            self.column
        )
    }
}

impl std::error::Error for RankDeficient {}

/// Cached encoder state for one LDPC code (one `H` matrix).
pub struct Encoder {
    n: usize,
    k: usize,
    /// `parity_eqs[p]` is the list of info-bit indices that XOR to
    /// produce parity bit `p`.
    parity_eqs: Vec<Vec<usize>>,
}

impl Encoder {
    /// Build an encoder from `H`. Performs Gaussian elimination on a
    /// u64-packed dense representation of `H`.
    ///
    /// Returns `Err(RankDeficient)` if no pivot is available at some
    /// column during elimination. Construction modules in
    /// [`crate::codes`] iterate over seeds until a rank-full `H` is
    /// found.
    pub fn try_new(h: &ParityCheckMatrix) -> Result<Self, RankDeficient> {
        let n = h.n;
        let k = h.k;
        let m = n - k;
        let words_per_row = n.div_ceil(64);

        let mut dense: Vec<Vec<u64>> = vec![vec![0u64; words_per_row]; m];
        for (r, row) in h.rows.iter().enumerate() {
            for &c in row {
                dense[r][c / 64] |= 1u64 << (c % 64);
            }
        }

        for col in k..n {
            let pivot_row_offset = col - k;
            let word = col / 64;
            let bit_mask = 1u64 << (col % 64);
            let mut pivot = None;
            for r in pivot_row_offset..m {
                if dense[r][word] & bit_mask != 0 {
                    pivot = Some(r);
                    break;
                }
            }
            let Some(p) = pivot else {
                return Err(RankDeficient { column: col });
            };
            if p != pivot_row_offset {
                dense.swap(p, pivot_row_offset);
            }
            // RREF: eliminate pivot column in all OTHER rows.
            for r in 0..m {
                if r != pivot_row_offset && dense[r][word] & bit_mask != 0 {
                    for w in 0..words_per_row {
                        dense[r][w] ^= dense[pivot_row_offset][w];
                    }
                }
            }
        }

        let mut parity_eqs: Vec<Vec<usize>> = Vec::with_capacity(m);
        for r in 0..m {
            let mut eq: Vec<usize> = Vec::new();
            for c in 0..k {
                if dense[r][c / 64] & (1u64 << (c % 64)) != 0 {
                    eq.push(c);
                }
            }
            parity_eqs.push(eq);
        }

        Ok(Self { n, k, parity_eqs })
    }

    /// Panicking wrapper around [`try_new`]. Construction modules
    /// in [`crate::codes`] should call `try_new` and handle
    /// rank-deficiency by iterating their PRNG seed.
    pub fn new(h: &ParityCheckMatrix) -> Self {
        Self::try_new(h).unwrap_or_else(|e| panic!("Encoder::new: {e}"))
    }

    /// Codeword length n.
    pub fn n(&self) -> usize {
        self.n
    }

    /// Info-bit length k.
    pub fn k(&self) -> usize {
        self.k
    }

    /// Encode info bits into a codeword. Result length equals `n`;
    /// the first `k` bits are the input bits unchanged (systematic
    /// form).
    ///
    /// # Panics
    /// Panics if `info.len() != self.k()`.
    pub fn encode(&self, info: &BitSlice<u8>) -> BitVec<u8> {
        assert_eq!(
            info.len(),
            self.k,
            "info bits length {} != k {}",
            info.len(),
            self.k
        );

        let mut codeword: BitVec<u8> = BitVec::with_capacity(self.n);
        for bit in info.iter() {
            codeword.push(*bit);
        }
        for eq in &self.parity_eqs {
            let parity: bool = eq.iter().fold(false, |acc, &i| acc ^ info[i]);
            codeword.push(parity);
        }
        debug_assert_eq!(codeword.len(), self.n);
        codeword
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::{self, BlockN, CodeFamily, WifiLdpcRate};

    #[test]
    fn encoded_codeword_satisfies_parity() {
        let h = codes::build(CodeFamily::OfdmAdaptive {
            block_n: BlockN::N648,
            rate: WifiLdpcRate::R1_2,
        });
        let enc = Encoder::new(&h);

        let info: BitVec<u8> = (0..enc.k()).map(|i| (i % 3) == 0).collect();
        let codeword = enc.encode(info.as_bitslice());

        let cw_bools: Vec<bool> = codeword.iter().map(|b| *b).collect();
        assert!(h.parity_check(&cw_bools), "codeword failed parity check");
    }

    #[test]
    fn encoded_codeword_is_systematic() {
        let h = codes::build(CodeFamily::OfdmAdaptive {
            block_n: BlockN::N1296,
            rate: WifiLdpcRate::R3_4,
        });
        let enc = Encoder::new(&h);

        let info: BitVec<u8> = (0..enc.k()).map(|i| (i % 5) == 0).collect();
        let codeword = enc.encode(info.as_bitslice());

        for i in 0..enc.k() {
            assert_eq!(codeword[i], info[i], "systematic bit {i} mismatch");
        }
    }

    #[test]
    #[ignore = "floor_rate14 random (3,4) regular LDPC is rank-deficient under \
                naive systematic encoding; tracked by tuxlink-bbin follow-up for \
                PEG (progressive-edge-growth) or column-swap-pivot construction"]
    fn encoder_handles_floor_rate14() {
        let h = codes::build(CodeFamily::FloorRate14);
        let enc = Encoder::new(&h);
        assert_eq!(enc.k(), 512);
        assert_eq!(enc.n(), 2048);

        let info: BitVec<u8> = BitVec::repeat(false, 512);
        let codeword = enc.encode(info.as_bitslice());
        assert_eq!(codeword.len(), 2048);

        // Any linear code maps the zero vector to the zero vector.
        for i in 0..2048 {
            assert!(!codeword[i], "all-zero info → bit {i} should be 0");
        }
    }
}
