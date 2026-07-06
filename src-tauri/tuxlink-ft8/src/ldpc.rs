//! FT-8 LDPC(174,91) forward-error-correction: systematic encode + syndrome.
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! Implemented ONLY from:
//! - the QEX 2020 "The FT4 and FT8 Communication Protocols" paper
//!   (Franke/Somerville/Taylor), **§3** ("Forward Error Correction"), which
//!   specifies the (174,91) regular LDPC code with column weight 3 that protects
//!   the 91-bit message+CRC; and
//! - the MIT-licensed `ft8_lib` (kgoba) reference — the generator matrix, the
//!   parity-check incidence tables, and the `encode174` algorithm are
//!   transcribed as **data constants** (allowed: MIT tables) and re-expressed in
//!   idiomatic Rust. The GPL WSJT-X `generator.dat` / `parity.dat` are **never**
//!   read or copied.
//!
//! # What this module builds (task T0.5)
//!
//! - [`ldpc_encode`] — append the 83 LDPC parity bits to the 91 systematic
//!   message+CRC bits, producing the 174-bit codeword.
//! - [`ldpc_syndrome`] — the 83 parity-check sums (mod 2); all-zero iff the
//!   174-bit word is a valid codeword.
//! - [`is_valid_codeword`] — syndrome all zero.
//!
//! The belief-propagation / min-sum **decoder** is a later task (T1.1) and is
//! deliberately **not** implemented here.
//!
//! # Codeword bit ordering (the pinned decision)
//!
//! The codeword is **systematic-first**: bits `0..91` are the 91 message+CRC
//! bits (identical to [`crate::crc::add_crc`]'s output — 77 payload bits then 14
//! CRC bits, MSB-first), and bits `91..174` are the 83 parity bits, in
//! parity-check order (parity bit `i` is check `i`'s generator dot-product).
//!
//! This matches `ft8_lib`'s `encode174` byte layout exactly. There, the 174-bit
//! codeword is packed MSB-first into `FTX_LDPC_N_BYTES = 22` bytes; the first
//! `FTX_LDPC_K_BYTES = 12` bytes are copied verbatim from the 91-bit message
//! (with the low 5 bits of byte 11 unused), and the 83 checksum bits are written
//! starting at bit index `FTX_LDPC_K = 91` (byte 11, mask `0x80 >> (91 % 8)` =
//! `0x04`), advancing MSB-first. So codeword bit `n` for `n < 91` is message bit
//! `n`, and codeword bit `91 + i` is the `i`-th parity/check bit.
//! provenance: `ft8_lib` `ft8/encode.c` `encode174` + `ft8/constants.h`
//! `FTX_LDPC_{N,K,M,N_BYTES,K_BYTES}` (MIT).

use crate::consts::{CODEWORD_BITS, MSG_CRC_BITS};

/// Number of LDPC parity (checksum) bits: `174 - 91 = 83`.
/// provenance: `ft8_lib` `ft8/constants.h` `FTX_LDPC_M = 83` (MIT); QEX §3.
pub const PARITY_BITS: usize = CODEWORD_BITS - MSG_CRC_BITS; // 83

/// Bytes needed to hold the 91 systematic message+CRC bits (`ceil(91/8) = 12`).
/// provenance: `ft8_lib` `ft8/constants.h` `FTX_LDPC_K_BYTES = 12` (MIT).
const K_BYTES: usize = 12;

/// The LDPC(174,91) **generator** matrix, one row per parity/check bit.
///
/// Each of the [`PARITY_BITS`] (83) rows is 91 bits packed MSB-first into
/// [`K_BYTES`] (12) bytes (the low 5 bits of the last byte are unused). Parity
/// bit `i` is the GF(2) dot-product of row `i` with the 91-bit message+CRC
/// vector — i.e. the parity of the bitwise-AND of the message with the row.
///
/// provenance: `ft8_lib` `ft8/constants.c` `kFTX_LDPC_generator`
/// `[FTX_LDPC_M][FTX_LDPC_K_BYTES]` (MIT). This is an MIT-licensed data table;
/// the GPL WSJT-X `generator.dat` is NOT the source.
#[rustfmt::skip]
const GENERATOR: [[u8; K_BYTES]; PARITY_BITS] = [
    [0x83, 0x29, 0xce, 0x11, 0xbf, 0x31, 0xea, 0xf5, 0x09, 0xf2, 0x7f, 0xc0],
    [0x76, 0x1c, 0x26, 0x4e, 0x25, 0xc2, 0x59, 0x33, 0x54, 0x93, 0x13, 0x20],
    [0xdc, 0x26, 0x59, 0x02, 0xfb, 0x27, 0x7c, 0x64, 0x10, 0xa1, 0xbd, 0xc0],
    [0x1b, 0x3f, 0x41, 0x78, 0x58, 0xcd, 0x2d, 0xd3, 0x3e, 0xc7, 0xf6, 0x20],
    [0x09, 0xfd, 0xa4, 0xfe, 0xe0, 0x41, 0x95, 0xfd, 0x03, 0x47, 0x83, 0xa0],
    [0x07, 0x7c, 0xcc, 0xc1, 0x1b, 0x88, 0x73, 0xed, 0x5c, 0x3d, 0x48, 0xa0],
    [0x29, 0xb6, 0x2a, 0xfe, 0x3c, 0xa0, 0x36, 0xf4, 0xfe, 0x1a, 0x9d, 0xa0],
    [0x60, 0x54, 0xfa, 0xf5, 0xf3, 0x5d, 0x96, 0xd3, 0xb0, 0xc8, 0xc3, 0xe0],
    [0xe2, 0x07, 0x98, 0xe4, 0x31, 0x0e, 0xed, 0x27, 0x88, 0x4a, 0xe9, 0x00],
    [0x77, 0x5c, 0x9c, 0x08, 0xe8, 0x0e, 0x26, 0xdd, 0xae, 0x56, 0x31, 0x80],
    [0xb0, 0xb8, 0x11, 0x02, 0x8c, 0x2b, 0xf9, 0x97, 0x21, 0x34, 0x87, 0xc0],
    [0x18, 0xa0, 0xc9, 0x23, 0x1f, 0xc6, 0x0a, 0xdf, 0x5c, 0x5e, 0xa3, 0x20],
    [0x76, 0x47, 0x1e, 0x83, 0x02, 0xa0, 0x72, 0x1e, 0x01, 0xb1, 0x2b, 0x80],
    [0xff, 0xbc, 0xcb, 0x80, 0xca, 0x83, 0x41, 0xfa, 0xfb, 0x47, 0xb2, 0xe0],
    [0x66, 0xa7, 0x2a, 0x15, 0x8f, 0x93, 0x25, 0xa2, 0xbf, 0x67, 0x17, 0x00],
    [0xc4, 0x24, 0x36, 0x89, 0xfe, 0x85, 0xb1, 0xc5, 0x13, 0x63, 0xa1, 0x80],
    [0x0d, 0xff, 0x73, 0x94, 0x14, 0xd1, 0xa1, 0xb3, 0x4b, 0x1c, 0x27, 0x00],
    [0x15, 0xb4, 0x88, 0x30, 0x63, 0x6c, 0x8b, 0x99, 0x89, 0x49, 0x72, 0xe0],
    [0x29, 0xa8, 0x9c, 0x0d, 0x3d, 0xe8, 0x1d, 0x66, 0x54, 0x89, 0xb0, 0xe0],
    [0x4f, 0x12, 0x6f, 0x37, 0xfa, 0x51, 0xcb, 0xe6, 0x1b, 0xd6, 0xb9, 0x40],
    [0x99, 0xc4, 0x72, 0x39, 0xd0, 0xd9, 0x7d, 0x3c, 0x84, 0xe0, 0x94, 0x00],
    [0x19, 0x19, 0xb7, 0x51, 0x19, 0x76, 0x56, 0x21, 0xbb, 0x4f, 0x1e, 0x80],
    [0x09, 0xdb, 0x12, 0xd7, 0x31, 0xfa, 0xee, 0x0b, 0x86, 0xdf, 0x6b, 0x80],
    [0x48, 0x8f, 0xc3, 0x3d, 0xf4, 0x3f, 0xbd, 0xee, 0xa4, 0xea, 0xfb, 0x40],
    [0x82, 0x74, 0x23, 0xee, 0x40, 0xb6, 0x75, 0xf7, 0x56, 0xeb, 0x5f, 0xe0],
    [0xab, 0xe1, 0x97, 0xc4, 0x84, 0xcb, 0x74, 0x75, 0x71, 0x44, 0xa9, 0xa0],
    [0x2b, 0x50, 0x0e, 0x4b, 0xc0, 0xec, 0x5a, 0x6d, 0x2b, 0xdb, 0xdd, 0x00],
    [0xc4, 0x74, 0xaa, 0x53, 0xd7, 0x02, 0x18, 0x76, 0x16, 0x69, 0x36, 0x00],
    [0x8e, 0xba, 0x1a, 0x13, 0xdb, 0x33, 0x90, 0xbd, 0x67, 0x18, 0xce, 0xc0],
    [0x75, 0x38, 0x44, 0x67, 0x3a, 0x27, 0x78, 0x2c, 0xc4, 0x20, 0x12, 0xe0],
    [0x06, 0xff, 0x83, 0xa1, 0x45, 0xc3, 0x70, 0x35, 0xa5, 0xc1, 0x26, 0x80],
    [0x3b, 0x37, 0x41, 0x78, 0x58, 0xcc, 0x2d, 0xd3, 0x3e, 0xc3, 0xf6, 0x20],
    [0x9a, 0x4a, 0x5a, 0x28, 0xee, 0x17, 0xca, 0x9c, 0x32, 0x48, 0x42, 0xc0],
    [0xbc, 0x29, 0xf4, 0x65, 0x30, 0x9c, 0x97, 0x7e, 0x89, 0x61, 0x0a, 0x40],
    [0x26, 0x63, 0xae, 0x6d, 0xdf, 0x8b, 0x5c, 0xe2, 0xbb, 0x29, 0x48, 0x80],
    [0x46, 0xf2, 0x31, 0xef, 0xe4, 0x57, 0x03, 0x4c, 0x18, 0x14, 0x41, 0x80],
    [0x3f, 0xb2, 0xce, 0x85, 0xab, 0xe9, 0xb0, 0xc7, 0x2e, 0x06, 0xfb, 0xe0],
    [0xde, 0x87, 0x48, 0x1f, 0x28, 0x2c, 0x15, 0x39, 0x71, 0xa0, 0xa2, 0xe0],
    [0xfc, 0xd7, 0xcc, 0xf2, 0x3c, 0x69, 0xfa, 0x99, 0xbb, 0xa1, 0x41, 0x20],
    [0xf0, 0x26, 0x14, 0x47, 0xe9, 0x49, 0x0c, 0xa8, 0xe4, 0x74, 0xce, 0xc0],
    [0x44, 0x10, 0x11, 0x58, 0x18, 0x19, 0x6f, 0x95, 0xcd, 0xd7, 0x01, 0x20],
    [0x08, 0x8f, 0xc3, 0x1d, 0xf4, 0xbf, 0xbd, 0xe2, 0xa4, 0xea, 0xfb, 0x40],
    [0xb8, 0xfe, 0xf1, 0xb6, 0x30, 0x77, 0x29, 0xfb, 0x0a, 0x07, 0x8c, 0x00],
    [0x5a, 0xfe, 0xa7, 0xac, 0xcc, 0xb7, 0x7b, 0xbc, 0x9d, 0x99, 0xa9, 0x00],
    [0x49, 0xa7, 0x01, 0x6a, 0xc6, 0x53, 0xf6, 0x5e, 0xcd, 0xc9, 0x07, 0x60],
    [0x19, 0x44, 0xd0, 0x85, 0xbe, 0x4e, 0x7d, 0xa8, 0xd6, 0xcc, 0x7d, 0x00],
    [0x25, 0x1f, 0x62, 0xad, 0xc4, 0x03, 0x2f, 0x0e, 0xe7, 0x14, 0x00, 0x20],
    [0x56, 0x47, 0x1f, 0x87, 0x02, 0xa0, 0x72, 0x1e, 0x00, 0xb1, 0x2b, 0x80],
    [0x2b, 0x8e, 0x49, 0x23, 0xf2, 0xdd, 0x51, 0xe2, 0xd5, 0x37, 0xfa, 0x00],
    [0x6b, 0x55, 0x0a, 0x40, 0xa6, 0x6f, 0x47, 0x55, 0xde, 0x95, 0xc2, 0x60],
    [0xa1, 0x8a, 0xd2, 0x8d, 0x4e, 0x27, 0xfe, 0x92, 0xa4, 0xf6, 0xc8, 0x40],
    [0x10, 0xc2, 0xe5, 0x86, 0x38, 0x8c, 0xb8, 0x2a, 0x3d, 0x80, 0x75, 0x80],
    [0xef, 0x34, 0xa4, 0x18, 0x17, 0xee, 0x02, 0x13, 0x3d, 0xb2, 0xeb, 0x00],
    [0x7e, 0x9c, 0x0c, 0x54, 0x32, 0x5a, 0x9c, 0x15, 0x83, 0x6e, 0x00, 0x00],
    [0x36, 0x93, 0xe5, 0x72, 0xd1, 0xfd, 0xe4, 0xcd, 0xf0, 0x79, 0xe8, 0x60],
    [0xbf, 0xb2, 0xce, 0xc5, 0xab, 0xe1, 0xb0, 0xc7, 0x2e, 0x07, 0xfb, 0xe0],
    [0x7e, 0xe1, 0x82, 0x30, 0xc5, 0x83, 0xcc, 0xcc, 0x57, 0xd4, 0xb0, 0x80],
    [0xa0, 0x66, 0xcb, 0x2f, 0xed, 0xaf, 0xc9, 0xf5, 0x26, 0x64, 0x12, 0x60],
    [0xbb, 0x23, 0x72, 0x5a, 0xbc, 0x47, 0xcc, 0x5f, 0x4c, 0xc4, 0xcd, 0x20],
    [0xde, 0xd9, 0xdb, 0xa3, 0xbe, 0xe4, 0x0c, 0x59, 0xb5, 0x60, 0x9b, 0x40],
    [0xd9, 0xa7, 0x01, 0x6a, 0xc6, 0x53, 0xe6, 0xde, 0xcd, 0xc9, 0x03, 0x60],
    [0x9a, 0xd4, 0x6a, 0xed, 0x5f, 0x70, 0x7f, 0x28, 0x0a, 0xb5, 0xfc, 0x40],
    [0xe5, 0x92, 0x1c, 0x77, 0x82, 0x25, 0x87, 0x31, 0x6d, 0x7d, 0x3c, 0x20],
    [0x4f, 0x14, 0xda, 0x82, 0x42, 0xa8, 0xb8, 0x6d, 0xca, 0x73, 0x35, 0x20],
    [0x8b, 0x8b, 0x50, 0x7a, 0xd4, 0x67, 0xd4, 0x44, 0x1d, 0xf7, 0x70, 0xe0],
    [0x22, 0x83, 0x1c, 0x9c, 0xf1, 0x16, 0x94, 0x67, 0xad, 0x04, 0xb6, 0x80],
    [0x21, 0x3b, 0x83, 0x8f, 0xe2, 0xae, 0x54, 0xc3, 0x8e, 0xe7, 0x18, 0x00],
    [0x5d, 0x92, 0x6b, 0x6d, 0xd7, 0x1f, 0x08, 0x51, 0x81, 0xa4, 0xe1, 0x20],
    [0x66, 0xab, 0x79, 0xd4, 0xb2, 0x9e, 0xe6, 0xe6, 0x95, 0x09, 0xe5, 0x60],
    [0x95, 0x81, 0x48, 0x68, 0x2d, 0x74, 0x8a, 0x38, 0xdd, 0x68, 0xba, 0xa0],
    [0xb8, 0xce, 0x02, 0x0c, 0xf0, 0x69, 0xc3, 0x2a, 0x72, 0x3a, 0xb1, 0x40],
    [0xf4, 0x33, 0x1d, 0x6d, 0x46, 0x16, 0x07, 0xe9, 0x57, 0x52, 0x74, 0x60],
    [0x6d, 0xa2, 0x3b, 0xa4, 0x24, 0xb9, 0x59, 0x61, 0x33, 0xcf, 0x9c, 0x80],
    [0xa6, 0x36, 0xbc, 0xbc, 0x7b, 0x30, 0xc5, 0xfb, 0xea, 0xe6, 0x7f, 0xe0],
    [0x5c, 0xb0, 0xd8, 0x6a, 0x07, 0xdf, 0x65, 0x4a, 0x90, 0x89, 0xa2, 0x00],
    [0xf1, 0x1f, 0x10, 0x68, 0x48, 0x78, 0x0f, 0xc9, 0xec, 0xdd, 0x80, 0xa0],
    [0x1f, 0xbb, 0x53, 0x64, 0xfb, 0x8d, 0x2c, 0x9d, 0x73, 0x0d, 0x5b, 0xa0],
    [0xfc, 0xb8, 0x6b, 0xc7, 0x0a, 0x50, 0xc9, 0xd0, 0x2a, 0x5d, 0x03, 0x40],
    [0xa5, 0x34, 0x43, 0x30, 0x29, 0xea, 0xc1, 0x5f, 0x32, 0x2e, 0x34, 0xc0],
    [0xc9, 0x89, 0xd9, 0xc7, 0xc3, 0xd3, 0xb8, 0xc5, 0x5d, 0x75, 0x13, 0x00],
    [0x7b, 0xb3, 0x8b, 0x2f, 0x01, 0x86, 0xd4, 0x66, 0x43, 0xae, 0x96, 0x20],
    [0x26, 0x44, 0xeb, 0xad, 0xeb, 0x44, 0xb9, 0x46, 0x7d, 0x1f, 0x42, 0xc0],
    [0x60, 0x8c, 0xc8, 0x57, 0x59, 0x4b, 0xfb, 0xb5, 0x5d, 0x69, 0x60, 0x00],
];

/// The LDPC parity-check incidence table `Nm`: for each of the [`PARITY_BITS`]
/// (83) parity checks, the codeword bit indices (1-origin) whose XOR must be
/// zero. Rows with only 6 incident bits use a `0` sentinel in the 7th slot.
///
/// provenance: `ft8_lib` `ft8/constants.c` `kFTX_LDPC_Nm[FTX_LDPC_M][7]` (MIT).
/// This is an MIT-licensed data table; the GPL WSJT-X `parity.dat` is NOT the
/// source.
#[rustfmt::skip]
const NM: [[u8; 7]; PARITY_BITS] = [
    [4, 31, 59, 91, 92, 96, 153],
    [5, 32, 60, 93, 115, 146, 0],
    [6, 24, 61, 94, 122, 151, 0],
    [7, 33, 62, 95, 96, 143, 0],
    [8, 25, 63, 83, 93, 96, 148],
    [6, 32, 64, 97, 126, 138, 0],
    [5, 34, 65, 78, 98, 107, 154],
    [9, 35, 66, 99, 139, 146, 0],
    [10, 36, 67, 100, 107, 126, 0],
    [11, 37, 67, 87, 101, 139, 158],
    [12, 38, 68, 102, 105, 155, 0],
    [13, 39, 69, 103, 149, 162, 0],
    [8, 40, 70, 82, 104, 114, 145],
    [14, 41, 71, 88, 102, 123, 156],
    [15, 42, 59, 106, 123, 159, 0],
    [1, 33, 72, 106, 107, 157, 0],
    [16, 43, 73, 108, 141, 160, 0],
    [17, 37, 74, 81, 109, 131, 154],
    [11, 44, 75, 110, 121, 166, 0],
    [45, 55, 64, 111, 130, 161, 173],
    [8, 46, 71, 112, 119, 166, 0],
    [18, 36, 76, 89, 113, 114, 143],
    [19, 38, 77, 104, 116, 163, 0],
    [20, 47, 70, 92, 138, 165, 0],
    [2, 48, 74, 113, 128, 160, 0],
    [21, 45, 78, 83, 117, 121, 151],
    [22, 47, 58, 118, 127, 164, 0],
    [16, 39, 62, 112, 134, 158, 0],
    [23, 43, 79, 120, 131, 145, 0],
    [19, 35, 59, 73, 110, 125, 161],
    [20, 36, 63, 94, 136, 161, 0],
    [14, 31, 79, 98, 132, 164, 0],
    [3, 44, 80, 124, 127, 169, 0],
    [19, 46, 81, 117, 135, 167, 0],
    [7, 49, 58, 90, 100, 105, 168],
    [12, 50, 61, 118, 119, 144, 0],
    [13, 51, 64, 114, 118, 157, 0],
    [24, 52, 76, 129, 148, 149, 0],
    [25, 53, 69, 90, 101, 130, 156],
    [20, 46, 65, 80, 120, 140, 170],
    [21, 54, 77, 100, 140, 171, 0],
    [35, 82, 133, 142, 171, 174, 0],
    [14, 30, 83, 113, 125, 170, 0],
    [4, 29, 68, 120, 134, 173, 0],
    [1, 4, 52, 57, 86, 136, 152],
    [26, 51, 56, 91, 122, 137, 168],
    [52, 84, 110, 115, 145, 168, 0],
    [7, 50, 81, 99, 132, 173, 0],
    [23, 55, 67, 95, 172, 174, 0],
    [26, 41, 77, 109, 141, 148, 0],
    [2, 27, 41, 61, 62, 115, 133],
    [27, 40, 56, 124, 125, 126, 0],
    [18, 49, 55, 124, 141, 167, 0],
    [6, 33, 85, 108, 116, 156, 0],
    [28, 48, 70, 85, 105, 129, 158],
    [9, 54, 63, 131, 147, 155, 0],
    [22, 53, 68, 109, 121, 174, 0],
    [3, 13, 48, 78, 95, 123, 0],
    [31, 69, 133, 150, 155, 169, 0],
    [12, 43, 66, 89, 97, 135, 159],
    [5, 39, 75, 102, 136, 167, 0],
    [2, 54, 86, 101, 135, 164, 0],
    [15, 56, 87, 108, 119, 171, 0],
    [10, 44, 82, 91, 111, 144, 149],
    [23, 34, 71, 94, 127, 153, 0],
    [11, 49, 88, 92, 142, 157, 0],
    [29, 34, 87, 97, 147, 162, 0],
    [30, 50, 60, 86, 137, 142, 162],
    [10, 53, 66, 84, 112, 128, 165],
    [22, 57, 85, 93, 140, 159, 0],
    [28, 32, 72, 103, 132, 166, 0],
    [28, 29, 84, 88, 117, 143, 150],
    [1, 26, 45, 80, 128, 147, 0],
    [17, 27, 89, 103, 116, 153, 0],
    [51, 57, 98, 163, 165, 172, 0],
    [21, 37, 73, 138, 152, 169, 0],
    [16, 47, 76, 130, 137, 154, 0],
    [3, 24, 30, 72, 104, 139, 0],
    [9, 40, 90, 106, 134, 151, 0],
    [15, 58, 60, 74, 111, 150, 163],
    [18, 42, 79, 144, 146, 152, 0],
    [25, 38, 65, 99, 122, 160, 0],
    [17, 42, 75, 129, 170, 172, 0],
];

/// Per-check incident-bit count for each of the [`PARITY_BITS`] (83) checks —
/// how many of the 7 [`NM`] entries are real (6 or 7). Redundant with the `0`
/// sentinels in [`NM`], but pinned as a table for a clippy-clean loop bound and
/// as a self-check against the sentinels.
///
/// provenance: `ft8_lib` `ft8/constants.c` `kFTX_LDPC_Num_rows[FTX_LDPC_M]`
/// (MIT).
#[rustfmt::skip]
const NUM_ROWS: [u8; PARITY_BITS] = [
    7, 6, 6, 6, 7, 6, 7, 6, 6, 7, 6, 6, 7, 7, 6, 6,
    6, 7, 6, 7, 6, 7, 6, 6, 6, 7, 6, 6, 6, 7, 6, 6,
    6, 6, 7, 6, 6, 6, 7, 7, 6, 6, 6, 6, 7, 7, 6, 6,
    6, 6, 7, 6, 6, 6, 7, 6, 6, 6, 6, 7, 6, 6, 6, 7,
    6, 6, 6, 7, 7, 6, 6, 7, 6, 6, 6, 6, 6, 6, 6, 7,
    6, 6, 6,
];

/// Number of set bits in a byte, modulo 2 (GF(2) parity of the byte).
/// provenance: `ft8_lib` `ft8/encode.c` `parity8` (MIT), re-expressed via
/// [`u8::count_ones`].
#[inline]
fn parity8(x: u8) -> u8 {
    (x.count_ones() & 1) as u8
}

/// Compute a single LDPC parity bit: the GF(2) dot-product of generator row `i`
/// with the 91-bit message packed MSB-first into 12 bytes.
/// provenance: `ft8_lib` `ft8/encode.c` `encode174` inner loop (MIT).
#[inline]
fn parity_bit(msg_bytes: &[u8; K_BYTES], row: usize) -> bool {
    let mut nsum: u8 = 0;
    for (m, g) in msg_bytes.iter().zip(GENERATOR[row].iter()) {
        nsum ^= parity8(m & g);
    }
    nsum & 1 != 0
}

/// LDPC(174,91) systematic encode: append the 83 parity bits to the 91
/// message+CRC bits, producing the 174-bit codeword.
///
/// `msg91` is the 91-bit message+CRC array (MSB-first), exactly as produced by
/// [`crate::crc::add_crc`]. The returned codeword is **systematic-first**:
/// `[0..91]` are `msg91` verbatim and `[91..174]` are the parity bits (see the
/// module-level "Codeword bit ordering" note).
/// provenance: `ft8_lib` `ft8/encode.c` `encode174` (MIT); QEX §3.
pub fn ldpc_encode(msg91: &[bool; MSG_CRC_BITS]) -> [bool; CODEWORD_BITS] {
    // Pack the 91 message bits MSB-first into 12 bytes for the generator AND
    // (the low 5 bits of byte 11 stay zero, matching ft8_lib).
    let mut msg_bytes = [0u8; K_BYTES];
    for (i, &b) in msg91.iter().enumerate() {
        if b {
            msg_bytes[i / 8] |= 0x80 >> (i % 8);
        }
    }

    let mut codeword = [false; CODEWORD_BITS];
    // Systematic part: the 91 message+CRC bits verbatim.
    codeword[..MSG_CRC_BITS].copy_from_slice(msg91);
    // Parity part: one dot-product per check, appended after bit 91.
    for i in 0..PARITY_BITS {
        codeword[MSG_CRC_BITS + i] = parity_bit(&msg_bytes, i);
    }
    codeword
}

/// LDPC(174,91) syndrome: the 83 parity-check sums (mod 2). A valid codeword has
/// an all-zero syndrome; check `m` is the XOR of the codeword bits listed in
/// [`NM`] row `m` (1-origin indices).
/// provenance: `ft8_lib` `ft8/ldpc.c` `ldpc_check` (MIT).
pub fn ldpc_syndrome(codeword: &[bool; CODEWORD_BITS]) -> [bool; PARITY_BITS] {
    let mut syndrome = [false; PARITY_BITS];
    for m in 0..PARITY_BITS {
        let mut x = false;
        for k in 0..(NUM_ROWS[m] as usize) {
            // NM entries are 1-origin codeword-bit indices.
            let bit = NM[m][k] as usize - 1;
            x ^= codeword[bit];
        }
        syndrome[m] = x;
    }
    syndrome
}

/// True iff `codeword` satisfies all 83 LDPC parity checks (syndrome all zero).
/// provenance: `ft8_lib` `ft8/ldpc.c` `ldpc_check == 0` (MIT).
pub fn is_valid_codeword(codeword: &[bool; CODEWORD_BITS]) -> bool {
    ldpc_syndrome(codeword).iter().all(|&s| !s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc::add_crc;
    use crate::consts::PAYLOAD_BITS;

    /// Convert 10 MSB-first payload bytes (the `message.rs` layout) to 77 bools.
    fn payload_bytes_to_bits(bytes: [u8; 10]) -> [bool; PAYLOAD_BITS] {
        let mut bits = [false; PAYLOAD_BITS];
        for (i, bit) in bits.iter_mut().enumerate() {
            *bit = bytes[i / 8] & (0x80 >> (i % 8)) != 0;
        }
        bits
    }

    /// Pack a 174-bit codeword into 22 MSB-first bytes (ft8_lib's layout), so we
    /// can KAT against `ft8_lib`'s `encode174` byte output.
    fn codeword_to_bytes(cw: &[bool; CODEWORD_BITS]) -> [u8; 22] {
        let mut out = [0u8; 22];
        for (i, &b) in cw.iter().enumerate() {
            if b {
                out[i / 8] |= 0x80 >> (i % 8);
            }
        }
        out
    }

    /// The three composed test payloads reused from the T0.2 message / T0.4 CRC
    /// KATs (`crc.rs`): known-good payload bytes for "CQ K1ABC FN42",
    /// "K1ABC W9XYZ -12", and the all-zero payload.
    fn kat_payloads() -> [[u8; 10]; 3] {
        [
            [0x00, 0x00, 0x00, 0x20, 0x4d, 0xef, 0x1a, 0x8a, 0x19, 0x88], // CQ K1ABC FN42
            [0x09, 0xbd, 0xe3, 0x50, 0x61, 0x49, 0xdc, 0x1f, 0xa9, 0xc8], // K1ABC W9XYZ -12
            [0x00; 10],                                                   // all-zero
        ]
    }

    // ── Table shape / structure sanity ──────────────────────────────────────
    //
    // GENERATOR SANITY: 83 rows, each spanning 91 bits (12 bytes, low 5 bits of
    // byte 11 unused); NM covers all 174 variables; NUM_ROWS agrees with the NM
    // sentinels. provenance: ft8_lib constants.c FTX_LDPC_{M,K,N} (MIT).
    #[test]
    fn table_shapes_match_spec() {
        assert_eq!(PARITY_BITS, 83);
        assert_eq!(GENERATOR.len(), PARITY_BITS);
        assert_eq!(NM.len(), PARITY_BITS);
        assert_eq!(NUM_ROWS.len(), PARITY_BITS);
        assert_eq!(K_BYTES, 12);
        // Each generator row is 12 bytes = 91 bits + 5 unused; the low 5 bits of
        // the last byte must be zero (91 % 8 == 3 used bits in byte 11).
        for row in &GENERATOR {
            assert_eq!(row.len(), K_BYTES);
            assert_eq!(row[11] & 0x1F, 0, "generator row has bits set past bit 91");
        }
        // NUM_ROWS matches the count of non-zero NM entries per check, and every
        // real index is a valid 1-origin codeword bit (1..=174).
        for m in 0..PARITY_BITS {
            let n = NUM_ROWS[m] as usize;
            assert!(n == 6 || n == 7, "check {m} has {n} incident bits");
            // Slots [0, n) are real (nonzero) indices in range.
            for &idx in &NM[m][..n] {
                assert!((1..=174).contains(&(idx as usize)), "NM[{m}] index {idx} out of range");
            }
            // A 6-bit check's 7th slot is the 0 sentinel.
            if n == 6 {
                assert_eq!(NM[m][6], 0, "6-bit check {m} lacks 0 sentinel");
            }
        }
    }

    /// PARITY STRUCTURE COVERS ALL 174 VARIABLES: every codeword bit index
    /// 1..=174 appears in at least one NM check (the code is regular, column
    /// weight 3). provenance: ft8_lib constants.c kFTX_LDPC_Nm / column weight 3.
    #[test]
    fn parity_structure_covers_all_variables() {
        let mut seen = [0u32; CODEWORD_BITS];
        for m in 0..PARITY_BITS {
            for k in 0..(NUM_ROWS[m] as usize) {
                seen[NM[m][k] as usize - 1] += 1;
            }
        }
        // Every variable is covered, and (regular code) exactly 3 times.
        for (v, &count) in seen.iter().enumerate() {
            assert_eq!(count, 3, "variable {} covered {} times, expected 3", v + 1, count);
        }
    }

    // ── ENCODE → SYNDROME = 0 ────────────────────────────────────────────────
    //
    // For each known payload, add_crc then ldpc_encode must yield an all-zero
    // syndrome. provenance: ft8_lib encode174 -> ldpc_check == 0 (MIT).
    #[test]
    fn encode_produces_zero_syndrome() {
        for p in kat_payloads() {
            let payload = payload_bytes_to_bits(p);
            let a91 = add_crc(&payload);
            let cw = ldpc_encode(&a91);
            let syn = ldpc_syndrome(&cw);
            assert!(syn.iter().all(|&s| !s), "nonzero syndrome for payload {p:02x?}");
            assert!(is_valid_codeword(&cw));
            // Systematic-first: the first 91 codeword bits equal the message+CRC.
            assert_eq!(&cw[..MSG_CRC_BITS], &a91[..]);
        }
    }

    // ── FULL-CODEWORD KAT (byte-exact) ───────────────────────────────────────
    //
    // The 22-byte codeword for each payload, cross-verified via a scratch Python
    // transcription of ft8_lib's `ftx_add_crc` + `encode174` (MIT). The first 12
    // bytes are the systematic a91 (payload + CRC), the remainder the parity.
    // A wrong generator, wrong bit order, or parity-first layout fails this.
    #[test]
    fn full_codeword_byte_kats() {
        // CQ K1ABC FN42
        let cw = ldpc_encode(&add_crc(&payload_bytes_to_bits(kat_payloads()[0])));
        assert_eq!(
            codeword_to_bytes(&cw),
            [
                0x00, 0x00, 0x00, 0x20, 0x4d, 0xef, 0x1a, 0x8a, 0x19, 0x89, 0x65,
                0xd5, 0x04, 0x8d, 0xe1, 0xe0, 0x74, 0xb7, 0xd3, 0x48, 0x52, 0x98,
            ]
        );
        // K1ABC W9XYZ -12
        let cw2 = ldpc_encode(&add_crc(&payload_bytes_to_bits(kat_payloads()[1])));
        assert_eq!(
            codeword_to_bytes(&cw2),
            [
                0x09, 0xbd, 0xe3, 0x50, 0x61, 0x49, 0xdc, 0x1f, 0xa9, 0xcb, 0x8f,
                0x56, 0xab, 0xf8, 0x50, 0x28, 0xc6, 0x2d, 0x2a, 0x9e, 0x0f, 0x98,
            ]
        );
    }

    // ── CORRUPTION → NONZERO SYNDROME ────────────────────────────────────────
    //
    // Flipping 1–3 codeword bits makes the syndrome nonzero (is_valid_codeword
    // == false). provenance: LDPC parity property; ft8_lib ldpc_check (MIT).
    #[test]
    fn corruption_makes_syndrome_nonzero() {
        let a91 = add_crc(&payload_bytes_to_bits(kat_payloads()[0]));
        let cw = ldpc_encode(&a91);
        assert!(is_valid_codeword(&cw));

        // Single-bit flip at each position -> invalid.
        for i in 0..CODEWORD_BITS {
            let mut bad = cw;
            bad[i] = !bad[i];
            assert!(!is_valid_codeword(&bad), "single flip at bit {i} left codeword valid");
        }

        // A representative 2-bit and 3-bit flip -> invalid.
        let mut two = cw;
        two[3] = !two[3];
        two[100] = !two[100];
        assert!(!is_valid_codeword(&two));

        let mut three = cw;
        three[7] = !three[7];
        three[90] = !three[90];
        three[173] = !three[173];
        assert!(!is_valid_codeword(&three));
    }

    // ── ROUND-TRIP over a table of messages ─────────────────────────────────
    //
    // encode -> syndrome == 0 for a spread of payloads, including one built by a
    // single-bit sweep from the all-zero payload, guarding against a
    // generator/message misalignment that only shows on set bits.
    #[test]
    fn round_trip_zero_syndrome_over_table() {
        // The three named payloads plus every single-bit payload.
        for p in kat_payloads() {
            let cw = ldpc_encode(&add_crc(&payload_bytes_to_bits(p)));
            assert!(is_valid_codeword(&cw));
        }
        for bit in 0..PAYLOAD_BITS {
            let mut payload = [false; PAYLOAD_BITS];
            payload[bit] = true;
            let cw = ldpc_encode(&add_crc(&payload));
            assert!(is_valid_codeword(&cw), "encode->syndrome nonzero for single-bit payload {bit}");
        }
    }

    /// The all-zero message is a valid codeword (the LDPC code is linear, so the
    /// zero word encodes to zero parity). Documents the linear-code baseline.
    #[test]
    fn zero_message_is_valid_codeword() {
        let cw = ldpc_encode(&[false; MSG_CRC_BITS]);
        assert_eq!(cw, [false; CODEWORD_BITS]);
        assert!(is_valid_codeword(&cw));
    }
}
