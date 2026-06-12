//! CRC-32-IEEE-802.3 over bit slices.
//!
//! [`append_crc32`] returns `bits || crc32(bits)` (length = `bits.len() + 32`).
//! [`verify_crc32`] returns `Ok(())` iff the trailing 32 bits match
//! `crc32(prefix)`.
//!
//! The polynomial is 0x04C11DB7 (IEEE 802.3 / ISO-HDLC), matching plan §C
//! and the most common deployed CRC-32.

use bitvec::prelude::*;
use crc::{Crc, CRC_32_ISO_HDLC};

const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

/// CRC verification errors.
#[derive(Debug, PartialEq, Eq)]
pub enum CrcError {
    /// Input was shorter than the 32-bit CRC tail.
    TooShort,
    /// Computed CRC did not match the trailing 32 bits.
    Mismatch {
        /// CRC the function computed over the prefix.
        expected: u32,
        /// CRC parsed from the trailing 32 bits.
        actual: u32,
    },
}

impl std::fmt::Display for CrcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "input too short for CRC verification"),
            Self::Mismatch { expected, actual } => write!(
                f,
                "CRC-32 mismatch: expected {expected:#010x}, got {actual:#010x}"
            ),
        }
    }
}

impl std::error::Error for CrcError {}

/// Append CRC-32 (32 trailing bits, MSB-first) to the input bit slice.
pub fn append_crc32(info: &BitSlice<u8>) -> BitVec<u8> {
    let info_bytes = bits_to_bytes(info);
    let crc = CRC32.checksum(&info_bytes);

    let mut out: BitVec<u8> = info.to_bitvec();
    for i in (0..32).rev() {
        out.push((crc >> i) & 1 == 1);
    }
    out
}

/// Verify a CRC-appended bit slice. Returns `Err` if the CRC does not match.
pub fn verify_crc32(bits_with_crc: &BitSlice<u8>) -> Result<(), CrcError> {
    if bits_with_crc.len() < 32 {
        return Err(CrcError::TooShort);
    }
    let split = bits_with_crc.len() - 32;
    let info = &bits_with_crc[..split];
    let crc_tail = &bits_with_crc[split..];

    let info_bytes = bits_to_bytes(info);
    let expected = CRC32.checksum(&info_bytes);
    let actual = bits_to_u32_msbfirst(crc_tail);

    if expected == actual {
        Ok(())
    } else {
        Err(CrcError::Mismatch { expected, actual })
    }
}

/// MSB-first byte packing. The final byte is zero-padded if the bit
/// count is not a multiple of 8.
fn bits_to_bytes(bits: &BitSlice<u8>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(bits.len().div_ceil(8));
    for chunk in bits.chunks(8) {
        let mut b: u8 = 0;
        for (i, bit) in chunk.iter().enumerate() {
            if *bit {
                b |= 1 << (7 - i);
            }
        }
        bytes.push(b);
    }
    bytes
}

/// MSB-first big-endian decode of a 32-bit slice.
fn bits_to_u32_msbfirst(bits: &BitSlice<u8>) -> u32 {
    debug_assert_eq!(bits.len(), 32);
    let mut v: u32 = 0;
    for (i, bit) in bits.iter().enumerate() {
        if *bit {
            v |= 1 << (31 - i);
        }
    }
    v
}
