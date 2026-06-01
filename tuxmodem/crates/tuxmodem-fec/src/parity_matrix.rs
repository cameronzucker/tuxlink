//! Sparse parity-check matrix `H` for an LDPC code.
//!
//! `H` is an `(n-k) × n` binary matrix; codeword `c` satisfies
//! `H · c^T = 0` over GF(2). Stored row-major as a list of column-
//! indices per row — sparse representation exploiting `H`'s low density
//! (≈ 3 ones per column for the constructions in [`crate::codes`]).
//!
//! The Tanner-graph bipartite representation used by the SPA decoder
//! (Phase 5) is derived directly from this row-list: each row is a
//! check node; each column index in the row is a variable-to-check
//! edge.

/// Sparse parity-check matrix.
#[derive(Debug, Clone)]
pub struct ParityCheckMatrix {
    /// Codeword length n (number of columns).
    pub n: usize,
    /// Info-bits length k (number of systematic bits in the codeword).
    pub k: usize,
    /// `rows[r]` is the sorted list of column indices where
    /// `H[r][c] = 1`.
    pub rows: Vec<Vec<usize>>,
}

impl ParityCheckMatrix {
    /// Number of parity rows in `H` (= `n - k`).
    pub fn n_minus_k(&self) -> usize {
        self.n - self.k
    }

    /// Check that `codeword` satisfies `H · c^T = 0` over GF(2). Each
    /// row is XOR'd across its column positions; all rows must yield
    /// false for the codeword to be valid.
    pub fn parity_check(&self, codeword: &[bool]) -> bool {
        assert_eq!(codeword.len(), self.n, "codeword length mismatch");
        for row in &self.rows {
            let parity: bool = row.iter().fold(false, |acc, &col| acc ^ codeword[col]);
            if parity {
                return false;
            }
        }
        true
    }

    /// Count edges (1s) in `H`. Useful for diagnostics + decoder
    /// complexity estimation (the SPA decoder's per-iteration cost
    /// scales with edge count).
    pub fn edge_count(&self) -> usize {
        self.rows.iter().map(|r| r.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_check_accepts_valid_codeword() {
        // Trivial 2×4 example: H = [[1,1,0,0],[0,0,1,1]]
        let h = ParityCheckMatrix {
            n: 4,
            k: 2,
            rows: vec![vec![0, 1], vec![2, 3]],
        };
        assert!(h.parity_check(&[false, false, false, false]));
        assert!(h.parity_check(&[true, true, false, false]));
        assert!(h.parity_check(&[false, false, true, true]));
    }

    #[test]
    fn parity_check_rejects_nonzero_syndrome() {
        let h = ParityCheckMatrix {
            n: 4,
            k: 2,
            rows: vec![vec![0, 1], vec![2, 3]],
        };
        assert!(!h.parity_check(&[true, false, false, false]));
        assert!(!h.parity_check(&[false, false, true, false]));
    }

    #[test]
    fn edge_count_sums_row_lengths() {
        let h = ParityCheckMatrix {
            n: 4,
            k: 2,
            rows: vec![vec![0, 1], vec![2, 3]],
        };
        assert_eq!(h.edge_count(), 4);
    }
}
