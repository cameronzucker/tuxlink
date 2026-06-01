//! Phase 2 acceptance tests: block interleaver involution + burst
//! decorrelation.

use bitvec::prelude::*;
use proptest::prelude::*;
use tuxmodem_fec::interleaver::{deinterleave, interleave};

proptest! {
    #[test]
    fn interleave_roundtrip(
        bits in proptest::collection::vec(any::<bool>(), 0..2048),
        rows in 4usize..32,
    ) {
        let n = bits.len();
        if n < rows * 2 { return Ok(()); }
        let bv: BitVec<u8> = bits.into_iter().collect();

        let interleaved = interleave(bv.as_bitslice(), rows);
        let recovered = deinterleave(interleaved.as_bitslice(), rows, n);

        prop_assert_eq!(bv.len(), recovered.len());
        for (a, b) in bv.iter().zip(recovered.iter()) {
            prop_assert_eq!(*a, *b);
        }
    }
}

#[test]
fn burst_error_decorrelation() {
    // Put 1s in the first 16 positions (a burst); interleave; check that
    // each 16-bit chunk of the output contains at most 1 set bit (the
    // burst was fully spread across columns).
    let n = 256;
    let rows = 16;
    let mut input: BitVec<u8> = BitVec::repeat(false, n);
    for i in 0..16 {
        input.set(i, true);
    }

    let interleaved = interleave(input.as_bitslice(), rows);
    for chunk in interleaved.chunks(16) {
        let ones = chunk.iter().filter(|b| **b).count();
        assert!(
            ones <= 1,
            "burst was not decorrelated: chunk had {ones} ones"
        );
    }
}

#[test]
fn output_length_is_rows_times_cols() {
    // Plan errata: interleave emits the full R·C-bit matrix. Verify
    // explicitly so a future reader sees the contract.
    let input: BitVec<u8> = BitVec::repeat(false, 64);
    let interleaved = interleave(input.as_bitslice(), 30);
    // ceil(64/30) = 3, total = 30·3 = 90
    assert_eq!(interleaved.len(), 90);
}
