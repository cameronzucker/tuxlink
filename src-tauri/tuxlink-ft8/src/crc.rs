//! FT-8 CRC-14 over the source-encoded message.
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! Implemented ONLY from:
//! - the QEX 2020 "The FT4 and FT8 Communication Protocols" paper
//!   (Franke/Somerville/Taylor), **§3** ("Forward Error Correction"), which
//!   states the 14-bit CRC is computed on the source-encoded message
//!   **zero-extended from 77 to 82 bits**, forming (with the 14 CRC bits) the
//!   91-bit message that the LDPC(174,91) code protects; and
//! - the MIT-licensed `ft8_lib` (kgoba) reference `ft8/crc.c` +
//!   `ft8/constants.h` — algorithm re-expressed in idiomatic Rust, constants
//!   cited below.
//!
//! # Polynomial representation (the KAT-pinned decision)
//!
//! The FT-8 CRC-14 polynomial is
//! `x^14 + x^13 + x^10 + x^9 + x^6 + x^5 + x^4 + x^2 + x + 1`.
//! Two representations of the *same* polynomial appear in the literature:
//!
//! - QEX §3 prints `0x6757`, which **includes** the implicit `x^14` term
//!   (bit 14 set: `0x6757 = 0b110_0111_0101_0111`).
//! - `ft8_lib` `constants.h` uses `FT8_CRC_POLYNOMIAL = 0x2757`, the **low-14**
//!   representation with the leading `x^14` dropped
//!   (`0x2757 = 0b10_0111_0101_0111`); `0x6757 & 0x3FFF == 0x2757`.
//!
//! This module pins the **`0x2757` low-14 form** because the division algorithm
//! is transcribed from `ft8_lib`'s `ftx_compute_crc`, whose `remainder` is a
//! 14-bit register: when the top bit (`x^13`, mask `0x2000`) is set before the
//! shift, XOR-ing the low-14 poly `0x2757` after the `<< 1` reproduces the
//! modulo-2 division against the full `0x6757` poly. Using `0x6757` here (or a
//! reflected/LSB-first order) fails the KATs in this file. See the module tests.
//!
//! # Covered bits & bit order
//!
//! CRC input is **MSB-first** (matching the crate's canonical bit order, see
//! [`crate::message`]). Per QEX §3 / `ft8_lib` `ftx_add_crc`, the CRC is
//! computed over the **82-bit** message = the 77 payload bits followed by 5
//! zero bits (the "zero-extension from 77 to 82"). In `ft8_lib`'s byte layout
//! (payload = 10 bytes, MSB-first, low 3 bits of byte 9 unused) this is
//! `ftx_compute_crc(a91, 96 - 14) == ftx_compute_crc(a91, 82)` after clearing
//! the 3 tail bits of byte 9 and zeroing byte 10.

use crate::consts::PAYLOAD_BITS;

/// CRC-14 polynomial, low-14 representation (leading `x^14` term dropped).
/// provenance: `ft8_lib` `ft8/constants.h` `FT8_CRC_POLYNOMIAL = 0x2757u`
/// ("CRC-14 polynomial without the leading (MSB) 1"), MIT; QEX §3 prints the
/// same polynomial as `0x6757` (with the `x^14` term); `0x6757 & 0x3FFF == 0x2757`.
pub const CRC_POLYNOMIAL: u16 = 0x2757;

/// CRC width in bits.
/// provenance: `ft8_lib` `ft8/constants.h` `FT8_CRC_WIDTH = 14`, MIT; QEX §3.
pub const CRC_WIDTH: u32 = 14;

/// Number of bits the CRC is computed over: 77 payload bits zero-extended to 82.
/// provenance: QEX §3 ("zero-extended from 77 to 82 bits"); `ft8_lib` `ftx_add_crc`
/// (`ftx_compute_crc(a91, 96 - 14)` == 82 bits), MIT.
pub const CRC_INPUT_BITS: usize = 82;

/// Mask selecting the top bit of the 14-bit remainder register (`x^13`).
const TOP_BIT: u16 = 1 << (CRC_WIDTH - 1); // 0x2000
/// Mask selecting the low 14 bits.
const CRC_MASK: u16 = (1 << CRC_WIDTH) - 1; // 0x3FFF

/// Compute the FT-8 CRC-14 of a bit sequence, MSB-first, `num_bits` long.
///
/// This is the byte-oriented core, a direct re-expression of `ft8_lib`'s
/// `ftx_compute_crc`: bytes are consumed MSB-first, the 14-bit remainder is fed
/// one bit at a time, and whenever the top bit (`x^13`) is set before the shift
/// the low-14 polynomial [`CRC_POLYNOMIAL`] is XOR-ed in after the shift.
///
/// `message` must hold at least `num_bits.div_ceil(8)` bytes. The returned value
/// is a 14-bit CRC in the low bits of a `u16`.
/// provenance: `ft8_lib` `ft8/crc.c` `ftx_compute_crc` (MIT).
pub fn crc14_bytes(message: &[u8], num_bits: usize) -> u16 {
    let mut remainder: u16 = 0;
    let mut idx_byte = 0usize;
    for idx_bit in 0..num_bits {
        if idx_bit % 8 == 0 {
            // Bring the next byte into the top of the remainder (WIDTH - 8 = 6).
            remainder ^= (message[idx_byte] as u16) << (CRC_WIDTH - 8);
            idx_byte += 1;
        }
        if remainder & TOP_BIT != 0 {
            remainder = (remainder << 1) ^ CRC_POLYNOMIAL;
        } else {
            remainder <<= 1;
        }
        // The C `uint16_t` naturally masks to 16 bits; the final `& CRC_MASK`
        // below discards the spilled bits, so no intermediate mask is required.
    }
    remainder & CRC_MASK
}

/// Compute the FT-8 CRC-14 of a bit sequence given as MSB-first booleans.
///
/// The bits are packed MSB-first into bytes (bit 0 -> bit 7 of byte 0, matching
/// [`crate::message::Payload`]'s convention), then [`crc14_bytes`] is applied.
/// `bits.len()` need not be a multiple of 8; the trailing bits of the final byte
/// are implicitly zero, which is exactly the zero-extension the FT-8 CRC relies
/// on when `bits.len()` is 77 (or 82).
/// provenance: `ft8_lib` `ft8/crc.c` `ftx_compute_crc` (MIT); QEX §3.
pub fn crc14_bits(bits: &[bool]) -> u16 {
    let nbytes = bits.len().div_ceil(8);
    let mut bytes = vec![0u8; nbytes.max(1)];
    for (i, &b) in bits.iter().enumerate() {
        if b {
            bytes[i / 8] |= 0x80 >> (i % 8);
        }
    }
    crc14_bytes(&bytes, bits.len())
}

/// Compute the FT-8 CRC-14 of the 77-bit source-encoded message.
///
/// Per QEX §3 the message is zero-extended from 77 to 82 bits before the CRC is
/// taken; this function performs that extension, so `payload_bits` should be the
/// 77 payload bits (MSB-first). Extra bits beyond 77 are ignored; fewer than 77
/// are zero-padded (both the tail extension and any short input are handled by
/// running the division over exactly [`CRC_INPUT_BITS`] = 82 bits with the input
/// left-aligned).
/// provenance: `ft8_lib` `ft8/crc.c` `ftx_add_crc` (MIT); QEX §3.
pub fn crc14(payload_bits: &[bool]) -> u16 {
    // Pack the (up to) 77 payload bits MSB-first, zero-extend to 82 bits, and
    // run the division over exactly 82 bits.
    let mut bytes = [0u8; 11]; // 82 bits -> ceil(82/8) = 11 bytes; tail is zero.
    for (i, &b) in payload_bits.iter().take(PAYLOAD_BITS).enumerate() {
        if b {
            bytes[i / 8] |= 0x80 >> (i % 8);
        }
    }
    crc14_bytes(&bytes, CRC_INPUT_BITS)
}

/// Append the 14-bit CRC to the 77 payload bits, forming the 91-bit
/// message+CRC bit array (MSB-first): bits 0..77 are the payload, bits 77..91
/// are the CRC MSB-first.
/// provenance: `ft8_lib` `ft8/crc.c` `ftx_add_crc` (MIT); QEX §3 (91 = 77 + 14).
pub fn add_crc(payload_bits: &[bool; PAYLOAD_BITS]) -> [bool; 91] {
    let crc = crc14(payload_bits);
    let mut out = [false; 91];
    out[..PAYLOAD_BITS].copy_from_slice(payload_bits);
    for i in 0..(CRC_WIDTH as usize) {
        // CRC bit i (MSB-first): bit (13 - i) of the 14-bit CRC.
        out[PAYLOAD_BITS + i] = (crc >> (CRC_WIDTH as usize - 1 - i)) & 1 != 0;
    }
    out
}

/// Extract the 14-bit CRC carried in a 91-bit message+CRC bit array (bits 77..91,
/// MSB-first).
/// provenance: `ft8_lib` `ft8/crc.c` `ftx_extract_crc` (MIT).
pub fn extract_crc(a91: &[bool; 91]) -> u16 {
    let mut crc = 0u16;
    for i in 0..(CRC_WIDTH as usize) {
        crc = (crc << 1) | a91[PAYLOAD_BITS + i] as u16;
    }
    crc
}

/// Verify a 91-bit message+CRC array: recompute the CRC over its 77 payload bits
/// and compare against the carried CRC. `true` if they match.
/// provenance: `ft8_lib` decode path (`ftx_extract_crc` vs recomputed CRC), MIT.
pub fn check_crc(a91: &[bool; 91]) -> bool {
    let mut payload = [false; PAYLOAD_BITS];
    payload.copy_from_slice(&a91[..PAYLOAD_BITS]);
    crc14(&payload) == extract_crc(a91)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convert 10 MSB-first payload bytes (the `message.rs` layout) to 77 bools.
    fn payload_bytes_to_bits(bytes: [u8; 10]) -> [bool; PAYLOAD_BITS] {
        let mut bits = [false; PAYLOAD_BITS];
        for (i, bit) in bits.iter_mut().enumerate() {
            *bit = bytes[i / 8] & (0x80 >> (i % 8)) != 0;
        }
        bits
    }

    // ── Poly-representation pin ─────────────────────────────────────────────
    //
    // 0x6757 (QEX, with x^14) & 0x3FFF == 0x2757 (ft8_lib, low-14). The division
    // uses the low-14 form; asserting this relationship documents the pin.
    #[test]
    fn poly_representations_agree() {
        assert_eq!(CRC_POLYNOMIAL, 0x2757);
        assert_eq!(0x6757u16 & CRC_MASK, CRC_POLYNOMIAL);
        assert_eq!(CRC_INPUT_BITS, 82);
        assert_eq!(TOP_BIT, 0x2000);
        assert_eq!(CRC_MASK, 0x3FFF);
    }

    // ── Byte-core KATs ──────────────────────────────────────────────────────
    //
    // Expected values transcribed from ft8_lib `ftx_compute_crc` (MIT) via a
    // scratch Python transcription (poly 0x2757, MSB-first, 14-bit register).
    // A wrong poly representation or bit order fails these.
    #[test]
    fn crc14_bytes_kats() {
        // Single byte 0x80 as an 8-bit message.
        assert_eq!(crc14_bytes(&[0x80], 8), 0x2f7e);
        // Two 0xFF bytes as a 16-bit message.
        assert_eq!(crc14_bytes(&[0xFF, 0xFF], 16), 0x0f6a);
    }

    // ── Payload KATs ────────────────────────────────────────────────────────
    //
    // The payload bytes are the T0.2 message KATs (known-good). Expected CRCs
    // derived from ft8_lib `ftx_add_crc` (82-bit zero-extended input), MIT.
    #[test]
    fn crc14_payload_kats() {
        // All-zero payload: CRC of 82 zero bits is 0.
        assert_eq!(crc14(&[false; PAYLOAD_BITS]), 0x0000);

        // "CQ K1ABC FN42" payload -> 0x0b2e.
        let p1 = payload_bytes_to_bits([0x00, 0x00, 0x00, 0x20, 0x4d, 0xef, 0x1a, 0x8a, 0x19, 0x88]);
        assert_eq!(crc14(&p1), 0x0b2e);

        // "K1ABC W9XYZ -12" payload -> 0x1c7a.
        let p2 = payload_bytes_to_bits([0x09, 0xbd, 0xe3, 0x50, 0x61, 0x49, 0xdc, 0x1f, 0xa9, 0xc8]);
        assert_eq!(crc14(&p2), 0x1c7a);
    }

    // ── 1-bit-flip sensitivity ──────────────────────────────────────────────
    #[test]
    fn single_bit_flip_changes_crc() {
        let base = [false; PAYLOAD_BITS];
        let base_crc = crc14(&base);
        // Flipping any single payload bit must change the CRC.
        for i in 0..PAYLOAD_BITS {
            let mut flipped = base;
            flipped[i] = true;
            assert_ne!(crc14(&flipped), base_crc, "bit {i} flip left CRC unchanged");
        }
        // Concrete: flip payload bit 0 of the zero payload -> 0x2bf8.
        let mut f0 = [false; PAYLOAD_BITS];
        f0[0] = true;
        assert_eq!(crc14(&f0), 0x2bf8);
    }

    // ── A wrong poly / bit order MUST fail the KATs ─────────────────────────
    #[test]
    fn wrong_poly_fails_kat() {
        // Re-run the division with the full 0x6757 poly (bug: includes x^14) and
        // confirm it does NOT match the pinned KAT, proving the KAT discriminates.
        fn crc_with_poly(message: &[u8], num_bits: usize, poly: u16) -> u16 {
            let mut rem: u16 = 0;
            let mut ib = 0;
            for i in 0..num_bits {
                if i % 8 == 0 {
                    rem ^= (message[ib] as u16) << (CRC_WIDTH - 8);
                    ib += 1;
                }
                if rem & TOP_BIT != 0 {
                    rem = (rem << 1) ^ poly;
                } else {
                    rem <<= 1;
                }
            }
            rem & CRC_MASK
        }
        assert_eq!(crc_with_poly(&[0x80], 8, CRC_POLYNOMIAL), 0x2f7e);
        assert_ne!(crc_with_poly(&[0x80], 8, 0x6757), 0x2f7e);
    }

    // ── add_crc / extract_crc / check_crc round-trip ────────────────────────
    #[test]
    fn add_extract_check_round_trip() {
        let p = payload_bytes_to_bits([0x00, 0x00, 0x00, 0x20, 0x4d, 0xef, 0x1a, 0x8a, 0x19, 0x88]);
        let a91 = add_crc(&p);
        // Payload bits preserved.
        assert_eq!(&a91[..PAYLOAD_BITS], &p[..]);
        // Extracted CRC equals the computed CRC.
        assert_eq!(extract_crc(&a91), 0x0b2e);
        assert_eq!(extract_crc(&a91), crc14(&p));
        // check_crc accepts the valid frame.
        assert!(check_crc(&a91));
        // Corrupting a payload bit makes check_crc reject.
        let mut bad = a91;
        bad[3] = !bad[3];
        assert!(!check_crc(&bad));
        // Corrupting a CRC bit also makes check_crc reject.
        let mut bad_crc = a91;
        bad_crc[PAYLOAD_BITS + 2] = !bad_crc[PAYLOAD_BITS + 2];
        assert!(!check_crc(&bad_crc));
    }
}
