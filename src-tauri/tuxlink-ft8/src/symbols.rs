//! FT-8 codeword-bit <-> channel-symbol mapping (Gray code + Costas framing).
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! Implemented ONLY from:
//! - the QEX 2020 "The FT4 and FT8 Communication Protocols" paper
//!   (Franke/Somerville/Taylor), **§4** ("Channel Symbols and Modulation") and
//!   **Table 3** (the Gray-coded symbol <-> 3-bit map); and
//! - the MIT-licensed `ft8_lib` (kgoba) reference `ft8/constants.c` +
//!   `ft8/encode.c` — the `kFT8_Gray_map` table and Costas framing loop
//!   re-expressed in idiomatic Rust, constants cited below.
//!
//! # What this module does
//!
//! The LDPC(174,91) codeword is 174 bits. FT-8 conveys them as 58 "info"
//! channel symbols (58 × 3 bits = 174), each 3 bits mapped through the Gray code
//! to a tone 0..7. The 58 info symbols are split into two 29-symbol groups and
//! interleaved with three 7-symbol Costas synchronization arrays to form the
//! 79-symbol transmitted frame:
//!
//! ```text
//! frame = { Costas(7), info[0..29], Costas(7), info[29..58], Costas(7) }
//!           |0.......6 |7........35 |36.....42 |43........71 |72.....78
//! ```
//!
//! # Gray map orientation (matches QEX Table 3 and `ft8_lib`)
//!
//! `ft8_lib` stores one table, `kFT8_Gray_map[bits3] = tone`, i.e. it maps the
//! 3-bit value (0..7) to the transmitted tone. This module's [`gray_encode`] is
//! that direction (bits -> tone). [`gray_decode`] is its inverse (tone -> bits),
//! which is exactly QEX Table 3 read as "channel symbol -> bits":
//!
//! | tone | bits (decode) |   | bits | tone (encode) |
//! |------|---------------|---|------|---------------|
//! | 0    | 000 (0)       |   | 0    | 0             |
//! | 1    | 001 (1)       |   | 1    | 1             |
//! | 2    | 011 (3)       |   | 2    | 3             |
//! | 3    | 010 (2)       |   | 3    | 2             |
//! | 4    | 110 (6)       |   | 4    | 5             |
//! | 5    | 100 (4)       |   | 5    | 6             |
//! | 6    | 101 (5)       |   | 6    | 4             |
//! | 7    | 111 (7)       |   | 7    | 7             |
//!
//! (The two columns are inverses: `gray_decode(gray_encode(b)) == b`.)

use crate::consts::{COSTAS, FRAME_SYMBOLS, INFO_SYMBOLS};

/// Gray-code map from 3-bit value (0..7) to transmitted tone (0..7).
/// provenance: `ft8_lib` `ft8/constants.c` `kFT8_Gray_map[8] = {0,1,3,2,5,6,4,7}`
/// (MIT); QEX 2020 Table 3.
pub const GRAY_MAP: [u8; 8] = [0, 1, 3, 2, 5, 6, 4, 7];

/// Number of info symbols per 29-symbol group (there are two such groups).
/// provenance: QEX 2020 §4 frame layout `{7, 29, 7, 29, 7}`.
pub const INFO_GROUP: usize = 29;

/// Symbol-index offsets at which each 7-symbol Costas array is inserted.
/// provenance: `ft8_lib` `ft8/constants.h` `FT8_SYNC_OFFSET = 36`, three sync
/// groups at 0, 36, 72 (MIT); QEX 2020 §4.
pub const COSTAS_OFFSETS: [usize; 3] = [0, 36, 72];

/// The three Costas block offsets (0, 36, 72) as a value.
/// provenance: `ft8_lib` `ft8/constants.h` sync-group offsets (MIT); QEX §4.
pub fn costas_positions() -> [usize; 3] {
    COSTAS_OFFSETS
}

/// Gray-encode a 3-bit value (0..7) to its transmitted tone (0..7).
///
/// The low 3 bits of `bits3` are used; higher bits are ignored (masked).
/// provenance: `ft8_lib` `ft8/encode.c` `tones[i] = kFT8_Gray_map[bits3]` (MIT).
pub fn gray_encode(bits3: u8) -> u8 {
    GRAY_MAP[(bits3 & 0x07) as usize]
}

/// Gray-decode a transmitted tone (0..7) back to its 3-bit value (0..7).
///
/// This is the inverse of [`gray_encode`] (QEX Table 3 read tone -> bits).
/// provenance: inverse of `ft8_lib` `kFT8_Gray_map` (MIT); QEX 2020 Table 3.
pub fn gray_decode(tone: u8) -> u8 {
    // Inverse lookup over the 8-entry map; the map is a bijection on 0..8.
    let t = tone & 0x07;
    GRAY_MAP.iter().position(|&m| m == t).unwrap_or(0) as u8
}

/// Map a 174-bit codeword (MSB-first booleans, the crate's canonical order) to
/// its 58 info channel symbols: each consecutive triad of bits (MSB-first) is
/// Gray-encoded to one tone.
/// provenance: `ft8_lib` `ft8/encode.c` info-symbol loop (MIT); QEX 2020 §4.
pub fn bits_to_symbols(codeword: &[bool; 174]) -> [u8; INFO_SYMBOLS] {
    let mut symbols = [0u8; INFO_SYMBOLS];
    for (i, sym) in symbols.iter_mut().enumerate() {
        let b = i * 3;
        let bits3 = ((codeword[b] as u8) << 2) | ((codeword[b + 1] as u8) << 1) | (codeword[b + 2] as u8);
        *sym = gray_encode(bits3);
    }
    symbols
}

/// Inverse of [`bits_to_symbols`]: map 58 info tones back to the 174-bit
/// codeword (MSB-first booleans).
/// provenance: inverse of `ft8_lib` `ft8/encode.c` info-symbol loop (MIT).
pub fn symbols_to_bits(symbols: &[u8; INFO_SYMBOLS]) -> [bool; 174] {
    let mut codeword = [false; 174];
    for (i, &tone) in symbols.iter().enumerate() {
        let bits3 = gray_decode(tone);
        let b = i * 3;
        codeword[b] = bits3 & 0x04 != 0;
        codeword[b + 1] = bits3 & 0x02 != 0;
        codeword[b + 2] = bits3 & 0x01 != 0;
    }
    codeword
}

/// Assemble the 79-symbol transmitted frame from the 58 info tones by inserting
/// the 7-symbol Costas array at symbol blocks 0-6, 36-42, and 72-78:
/// `{ Costas, info[0..29], Costas, info[29..58], Costas }`.
/// provenance: `ft8_lib` `ft8/encode.c` frame-assembly loop (MIT); QEX 2020 §4.
pub fn assemble_frame(info58: &[u8; INFO_SYMBOLS]) -> [u8; FRAME_SYMBOLS] {
    let mut frame = [0u8; FRAME_SYMBOLS];
    // First Costas sync group: symbols 0..7.
    frame[0..7].copy_from_slice(&COSTAS);
    // First info group: symbols 7..36 = info[0..29].
    frame[7..36].copy_from_slice(&info58[0..INFO_GROUP]);
    // Second Costas sync group: symbols 36..43.
    frame[36..43].copy_from_slice(&COSTAS);
    // Second info group: symbols 43..72 = info[29..58].
    frame[43..72].copy_from_slice(&info58[INFO_GROUP..INFO_SYMBOLS]);
    // Third Costas sync group: symbols 72..79.
    frame[72..79].copy_from_slice(&COSTAS);
    frame
}

/// Strip the three Costas sync groups from a 79-symbol frame, returning the 58
/// info tones (`info[0..29]` from symbols 7-35, `info[29..58]` from symbols 43-71).
/// provenance: inverse of `ft8_lib` `ft8/encode.c` frame assembly (MIT); QEX §4.
pub fn disassemble_frame(frame: &[u8; FRAME_SYMBOLS]) -> [u8; INFO_SYMBOLS] {
    let mut info58 = [0u8; INFO_SYMBOLS];
    info58[0..INFO_GROUP].copy_from_slice(&frame[7..36]);
    info58[INFO_GROUP..INFO_SYMBOLS].copy_from_slice(&frame[43..72]);
    info58
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Gray-map KATs (QEX Table 3 / ft8_lib kFT8_Gray_map) ─────────────────
    #[test]
    fn gray_map_matches_table3() {
        // encode: bits -> tone (kFT8_Gray_map).
        assert_eq!(GRAY_MAP, [0, 1, 3, 2, 5, 6, 4, 7]);
        let encode = [0u8, 1, 3, 2, 5, 6, 4, 7];
        for bits in 0u8..8 {
            assert_eq!(gray_encode(bits), encode[bits as usize], "encode bits={bits}");
        }
        // decode: tone -> bits (QEX Table 3, inverse of encode).
        let decode = [0u8, 1, 3, 2, 6, 4, 5, 7];
        for tone in 0u8..8 {
            assert_eq!(gray_decode(tone), decode[tone as usize], "decode tone={tone}");
        }
    }

    #[test]
    fn gray_round_trip_all_symbols() {
        for v in 0u8..8 {
            assert_eq!(gray_decode(gray_encode(v)), v, "encode/decode round-trip v={v}");
            assert_eq!(gray_encode(gray_decode(v)), v, "decode/encode round-trip v={v}");
        }
    }

    // ── bits <-> symbols round-trip ─────────────────────────────────────────
    #[test]
    fn codeword_symbols_round_trip() {
        // A deterministic pseudo-random-ish 174-bit codeword.
        let mut cw = [false; 174];
        for (i, b) in cw.iter_mut().enumerate() {
            *b = (i * 37 + 11) % 3 == 0;
        }
        let syms = bits_to_symbols(&cw);
        assert_eq!(syms.len(), 58);
        // Every symbol is a valid tone 0..7.
        assert!(syms.iter().all(|&t| t < 8));
        let back = symbols_to_bits(&syms);
        assert_eq!(back, cw, "174-bit codeword round-trip through 58 symbols");
    }

    #[test]
    fn bits_to_symbols_known_triads() {
        // First triad 000 -> tone 0; a triad of 010 -> tone gray_encode(2)=3.
        let mut cw = [false; 174];
        // symbol 0 bits = 000 -> tone 0 (already).
        // symbol 1 bits (indices 3,4,5) = 010 -> tone 3.
        cw[4] = true;
        let syms = bits_to_symbols(&cw);
        assert_eq!(syms[0], 0);
        assert_eq!(syms[1], 3);
    }

    // ── Frame assemble / disassemble ────────────────────────────────────────
    #[test]
    fn assemble_places_costas_at_correct_indices() {
        // Distinguishable info tones: fill each info symbol with (index % 8) + a
        // marker so we can tell info from Costas. Use tone = (i % 7) which never
        // needs to equal a specific Costas value for the position check.
        let mut info = [0u8; INFO_SYMBOLS];
        for (i, s) in info.iter_mut().enumerate() {
            *s = (i % 8) as u8;
        }
        let frame = assemble_frame(&info);
        assert_eq!(frame.len(), 79);

        // Costas tones at blocks 0, 36, 72.
        for &off in &COSTAS_OFFSETS {
            for k in 0..7 {
                assert_eq!(frame[off + k], COSTAS[k], "Costas mismatch at {}", off + k);
            }
        }
        assert_eq!(costas_positions(), [0, 36, 72]);

        // Info symbols fill 7..36 and 43..72.
        for k in 0..INFO_GROUP {
            assert_eq!(frame[7 + k], info[k], "info group 1 at frame[{}]", 7 + k);
            assert_eq!(frame[43 + k], info[INFO_GROUP + k], "info group 2 at frame[{}]", 43 + k);
        }
    }

    #[test]
    fn assemble_disassemble_round_trip() {
        let mut info = [0u8; INFO_SYMBOLS];
        for (i, s) in info.iter_mut().enumerate() {
            *s = ((i * 5 + 3) % 8) as u8;
        }
        let frame = assemble_frame(&info);
        let back = disassemble_frame(&frame);
        assert_eq!(back, info, "assemble/disassemble round-trip");
    }

    #[test]
    fn full_codeword_to_frame_to_codeword() {
        // End-to-end: 174 bits -> 58 symbols -> 79-symbol frame -> 58 -> 174.
        let mut cw = [false; 174];
        for (i, b) in cw.iter_mut().enumerate() {
            *b = (i * 13 + 7) % 5 < 2;
        }
        let syms = bits_to_symbols(&cw);
        let frame = assemble_frame(&syms);
        let syms2 = disassemble_frame(&frame);
        let cw2 = symbols_to_bits(&syms2);
        assert_eq!(cw2, cw);
    }
}
