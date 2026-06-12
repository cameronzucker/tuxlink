//! Phase 1 acceptance tests: CRC-32 append + verify round-trip.

use bitvec::prelude::*;
use sonde_fec::crc::{append_crc32, verify_crc32};

#[test]
fn crc_roundtrip_zero_bits() {
    let info: BitVec<u8> = BitVec::new();
    let with_crc = append_crc32(info.as_bitslice());
    assert!(verify_crc32(with_crc.as_bitslice()).is_ok());
}

#[test]
fn crc_roundtrip_512_bits() {
    let info: BitVec<u8> = (0..512u32).map(|i| (i % 7) == 0).collect();
    let with_crc = append_crc32(info.as_bitslice());
    assert_eq!(with_crc.len(), 512 + 32);
    assert!(verify_crc32(with_crc.as_bitslice()).is_ok());
}

#[test]
fn crc_detects_single_bit_flip() {
    let info: BitVec<u8> = (0..256u32).map(|i| (i % 3) == 0).collect();
    let mut with_crc = append_crc32(info.as_bitslice());

    // Flip a single bit anywhere. Scope the mutable borrow so it
    // drops before the verify_crc32 immutable re-borrow.
    {
        let mut bit = with_crc.get_mut(42).unwrap();
        let prev = *bit;
        *bit = !prev;
    }

    assert!(verify_crc32(with_crc.as_bitslice()).is_err());
}
