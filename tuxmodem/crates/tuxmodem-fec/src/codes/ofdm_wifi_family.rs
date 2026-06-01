//! WiFi 802.11n-style rate-compatible LDPC family.
//!
//! Quasi-cyclic LDPC construction: `H` is a block matrix of Z×Z
//! circulants (Z = 27 for n=648; Z = 54 for n=1296). Each block is
//! either the zero matrix or a cyclic-shifted identity matrix.
//!
//! Shift values are generated deterministically from a fixed PRNG
//! seed per `(block_n, rate)` tuple. This is the construction
//! PATTERN of IEEE 802.11n (public standard); the specific shift
//! values are tuxmodem-derived per ADR 0014's clean-sheet provenance
//! posture.

// The quasi-cyclic construction is fundamentally index-driven:
// `global_row = r_block * z + i` and `global_col = c_block * z +
// ((i + shift) % z)` are the load-bearing identities. Rewriting as
// iterator chains obscures the block-coordinate math without buying
// type safety.
#![allow(clippy::needless_range_loop)]

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use super::{BlockN, WifiLdpcRate};
use crate::encode::Encoder;
use crate::parity_matrix::ParityCheckMatrix;

const Z_648: usize = 27;
const Z_1296: usize = 54;
const SEED_BASE: u64 = 0xFEC0_0FD0_F1F1_u64;
const MAX_SEED_ITERATIONS: u64 = 64;

/// Build the parity-check matrix for the given `(block_n, rate)` pair.
/// Iterates seeds until a rank-full `H` is found (so systematic
/// encoding via [`Encoder::try_new`] succeeds).
pub fn build(block_n: BlockN, rate: WifiLdpcRate) -> ParityCheckMatrix {
    for delta in 0..MAX_SEED_ITERATIONS {
        let base = SEED_BASE.wrapping_add(delta);
        let h = build_with_seed(block_n, rate, base);
        if Encoder::try_new(&h).is_ok() {
            return h;
        }
    }
    panic!(
        "ofdm_wifi_family::build({block_n:?}, {rate:?}): no rank-full H found in {MAX_SEED_ITERATIONS} seed iterations"
    );
}

fn build_with_seed(block_n: BlockN, rate: WifiLdpcRate, seed_base: u64) -> ParityCheckMatrix {
    let z = match block_n {
        BlockN::N648 => Z_648,
        BlockN::N1296 => Z_1296,
    };
    let n = z * 24; // 24 column-blocks per the WiFi-family convention
    let (rate_num, rate_den) = rate.ratio();
    let k = n * rate_num / rate_den;
    let m = n - k;
    let m_blocks = m / z;
    let n_blocks = n / z;

    debug_assert_eq!(m % z, 0);
    debug_assert_eq!(n % z, 0);

    let seed = seed_base ^ ((block_n as u64) << 8) ^ ((rate as u64) << 16);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    // Construct the block-shift matrix: m_blocks × n_blocks entries.
    // Each entry is either None (zero block) or Some(shift) (shifted
    // identity block). Target column weight 3 in expectation for the
    // rate-1/2 case (matches MacKay regular-LDPC design); scale by
    // m_blocks so the per-column probability lands.
    let target_col_weight_per_blockcol: f32 = 3.0;
    let p_nonzero: f32 = target_col_weight_per_blockcol / (m_blocks as f32);

    let mut block_shifts: Vec<Vec<Option<usize>>> = vec![vec![None; n_blocks]; m_blocks];
    for r in 0..m_blocks {
        for c in 0..n_blocks {
            if rng.gen::<f32>() < p_nonzero {
                let shift = rng.gen_range(0..z);
                block_shifts[r][c] = Some(shift);
            }
        }
    }

    // Ensure each column-block has at least block-weight 2 (degree-1
    // columns cause decoder convergence problems). Repair by adding
    // shifts in random rows where this is violated.
    for c in 0..n_blocks {
        let weight = (0..m_blocks).filter(|&r| block_shifts[r][c].is_some()).count();
        for _ in weight..2 {
            for _try in 0..16 {
                let r = rng.gen_range(0..m_blocks);
                if block_shifts[r][c].is_none() {
                    block_shifts[r][c] = Some(rng.gen_range(0..z));
                    break;
                }
            }
        }
    }

    // Expand block_shifts into the row-list ParityCheckMatrix
    // representation.
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); m];
    for r_block in 0..m_blocks {
        for c_block in 0..n_blocks {
            if let Some(shift) = block_shifts[r_block][c_block] {
                // Shifted-identity block: for each row i within the
                // block, there's a 1 at column ((i + shift) mod z)
                // within the c_block-th column-block.
                for i in 0..z {
                    let global_row = r_block * z + i;
                    let global_col = c_block * z + ((i + shift) % z);
                    rows[global_row].push(global_col);
                }
            }
        }
    }
    for row in rows.iter_mut() {
        row.sort_unstable();
        row.dedup();
    }

    ParityCheckMatrix { n, k, rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_family_n648_r12_dimensions() {
        let h = build(BlockN::N648, WifiLdpcRate::R1_2);
        assert_eq!(h.n, 648);
        assert_eq!(h.k, 324);
        assert_eq!(h.rows.len(), 324);
    }

    #[test]
    fn wifi_family_n1296_r34_dimensions() {
        let h = build(BlockN::N1296, WifiLdpcRate::R3_4);
        assert_eq!(h.n, 1296);
        assert_eq!(h.k, 972);
        assert_eq!(h.rows.len(), 324);
    }

    #[test]
    fn wifi_family_no_degree_one_columns() {
        let h = build(BlockN::N648, WifiLdpcRate::R1_2);
        let mut col_weights = vec![0usize; h.n];
        for row in &h.rows {
            for &c in row {
                col_weights[c] += 1;
            }
        }
        for (c, w) in col_weights.iter().enumerate() {
            assert!(*w >= 2, "column {c} has weight {w} (must be >= 2)");
        }
    }

    #[test]
    fn wifi_family_deterministic() {
        let h1 = build(BlockN::N648, WifiLdpcRate::R1_2);
        let h2 = build(BlockN::N648, WifiLdpcRate::R1_2);
        assert_eq!(h1.rows, h2.rows);
    }
}
