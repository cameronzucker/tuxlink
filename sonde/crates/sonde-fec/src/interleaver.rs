//! Block bit interleaver. Writes input row-by-row into an R×C matrix
//! (where C = ceil(n / R)), reads column-by-column. The de-interleaver
//! is the inverse.
//!
//! Used between LDPC encode and channel modulation to decorrelate HF
//! burst errors before they reach the LDPC decoder. The decoder makes
//! its independent-errors assumption explicit at the Tanner-graph
//! level — bursts violate it; an interleaver of depth > burst length
//! recovers the assumption.
//!
//! ## Output length
//!
//! When `n % rows != 0` the R×C matrix has `R·C - n` padding cells at
//! the linear tail. To remain lossless (so every input bit survives
//! the column-major read), the interleaver emits the **full R·C
//! bits**, not the original n. [`deinterleave`] takes the original
//! length as a separate argument so it can drop the trailing padding.
//! When the FEC layer composes interleaver with a fixed-block LDPC
//! codeword (n is a multiple of rows by construction), this padding
//! is empty and the output length equals the input.

use bitvec::prelude::*;

/// Interleave `input` using a block interleaver with `rows` rows.
/// Output length is `rows · ceil(n / rows)` bits (the full matrix);
/// for inputs whose length is a multiple of `rows`, this equals `n`.
///
/// # Panics
/// Panics if `rows == 0`.
pub fn interleave(input: &BitSlice<u8>, rows: usize) -> BitVec<u8> {
    assert!(rows > 0, "interleaver row count must be positive");
    let n = input.len();
    let cols = n.div_ceil(rows).max(1);
    let total = rows * cols;

    let mut matrix: BitVec<u8> = BitVec::repeat(false, total);
    for (i, bit) in input.iter().enumerate() {
        matrix.set(i, *bit);
    }

    let mut out: BitVec<u8> = BitVec::with_capacity(total);
    for col in 0..cols {
        for row in 0..rows {
            out.push(matrix[row * cols + col]);
        }
    }
    out
}

/// De-interleave: inverse of [`interleave`].
///
/// `input.len()` must equal `rows · cols` where `cols = ceil(original_len / rows)`.
/// Returns the first `original_len` bits of the reconstructed matrix.
///
/// # Panics
/// Panics if `rows == 0` or if `input.len()` is not a multiple of
/// `rows` consistent with `original_len`.
pub fn deinterleave(input: &BitSlice<u8>, rows: usize, original_len: usize) -> BitVec<u8> {
    assert!(rows > 0, "interleaver row count must be positive");
    let cols = original_len.div_ceil(rows).max(1);
    let total = rows * cols;
    assert_eq!(
        input.len(),
        total,
        "deinterleave input length {} does not match expected {} (rows·cols)",
        input.len(),
        total
    );

    let mut matrix: BitVec<u8> = BitVec::repeat(false, total);
    let mut iter = input.iter();
    for col in 0..cols {
        for row in 0..rows {
            let idx = row * cols + col;
            if let Some(bit) = iter.next() {
                matrix.set(idx, *bit);
            }
        }
    }

    let mut out: BitVec<u8> = BitVec::with_capacity(original_len);
    for i in 0..original_len {
        out.push(matrix[i]);
    }
    out
}
