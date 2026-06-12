//! Floor rate-1/4 LDPC code: n=2048, k=512, regular (3,4) construction.
//!
//! Per Gallager 1963 + MacKay-Neal 1996, regular LDPC codes with sparse
//! random parity-check matrices approach Shannon capacity under sum-
//! product decoding at moderate iteration counts. The (3,4) regular
//! construction balances column weight (decoder cycle count per bit)
//! and row weight (parity-check density). Fixed seed → reproducible
//! matrix → reproducible BER curves.

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::parity_matrix::ParityCheckMatrix;

const N: usize = 2048;
const K: usize = 512;
const COL_WEIGHT: usize = 3;
const ROW_WEIGHT: usize = 4;
const SEED: u64 = 0xFEC0_F100_0014_u64;

/// Construct the floor rate-1/4 parity-check matrix.
///
/// Deterministic given the [`SEED`] constant. Returns an (n-k) × n
/// matrix with regular column weight 3 and regular row weight 4.
///
/// NOTE: this configuration-model construction does not guarantee
/// rank-full `H` — the floor code (n=2048, k=512) reliably produces
/// rank-deficient matrices under random sampling. The
/// [`crate::encode::Encoder`] currently panics on the floor code;
/// the proper fix is a PEG (progressive-edge-growth) or
/// column-swap-pivot construction tracked by the tuxlink-bbin
/// follow-up. Structural tests (weights, determinism) work on the
/// rank-deficient H as-is.
pub fn build() -> ParityCheckMatrix {
    build_with_seed(SEED).expect("structural construction must succeed with the pinned SEED")
}

/// Single seed attempt at the configuration-model construction.
/// Returns `None` if `MAX_RESHUFFLE` attempts exhausted without
/// producing a duplicate-free row partition.
fn build_with_seed(seed: u64) -> Option<ParityCheckMatrix> {
    let m = N - K;
    debug_assert_eq!(N * COL_WEIGHT, m * ROW_WEIGHT, "regular construction balance");

    let total_edges = m * ROW_WEIGHT;
    debug_assert_eq!(total_edges, N * COL_WEIGHT);

    // Configuration-model: for each column c, emit COL_WEIGHT stubs
    // labeled c; shuffle all stubs; partition into m rows of
    // ROW_WEIGHT each. Each column ends up with exactly COL_WEIGHT
    // stubs (in some rows); each row ends up with exactly ROW_WEIGHT
    // stubs (some columns).
    let mut stubs: Vec<usize> = (0..N)
        .flat_map(|c| std::iter::repeat(c).take(COL_WEIGHT))
        .collect();

    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    stubs.shuffle(&mut rng);

    for _attempt in 0..32 {
        let mut rows: Vec<Vec<usize>> = Vec::with_capacity(m);
        let mut ok = true;
        for r in 0..m {
            let mut row: Vec<usize> = stubs[r * ROW_WEIGHT..(r + 1) * ROW_WEIGHT].to_vec();
            row.sort_unstable();
            let pre_dedup = row.len();
            row.dedup();
            if row.len() != pre_dedup {
                ok = false;
                break;
            }
            rows.push(row);
        }
        if ok {
            return Some(ParityCheckMatrix { n: N, k: K, rows });
        }
        stubs.shuffle(&mut rng);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_h_has_correct_dimensions() {
        let h = build();
        assert_eq!(h.n, N);
        assert_eq!(h.k, K);
        assert_eq!(h.rows.len(), N - K);
    }

    #[test]
    fn floor_h_is_regular_row_weight() {
        let h = build();
        for row in &h.rows {
            assert_eq!(row.len(), ROW_WEIGHT);
            let mut sorted = row.clone();
            sorted.sort_unstable();
            assert_eq!(row, &sorted, "rows must be sorted");
            sorted.dedup();
            assert_eq!(sorted.len(), row.len(), "rows must be duplicate-free");
        }
    }

    #[test]
    fn floor_h_is_regular_column_weight() {
        let h = build();
        let mut col_weights = vec![0usize; N];
        for row in &h.rows {
            for &c in row {
                col_weights[c] += 1;
            }
        }
        for (c, w) in col_weights.iter().enumerate() {
            assert_eq!(*w, COL_WEIGHT, "column {c} weight {w}");
        }
    }

    #[test]
    fn floor_h_is_deterministic() {
        let h1 = build();
        let h2 = build();
        assert_eq!(h1.rows, h2.rows);
    }
}
