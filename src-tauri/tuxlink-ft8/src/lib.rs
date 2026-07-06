//! `tuxlink-ft8` — clean-room, pure-Rust FT-8 decoder (Station Intelligence L0 spike).
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! Implemented ONLY from: the QEX 2020 FT4/FT8 protocol paper
//! (Franke/Somerville/Taylor); the WB2FKO "Synchronization in FT8" paper; and the
//! MIT-licensed references `ft8_lib` (kgoba) + `RustFT8` (jl1nie). The GPL
//! `wsjtr`/WSJT-X is a **binary test oracle only** — its source is never read, its
//! binary never `strings`/`objdump`-ed, and its `generator.dat`/`parity.dat` are
//! never copied (the LDPC matrix comes from MIT `ft8_lib` or is regenerated).
//!
//! Layers land bottom-up per `docs/plans/2026-07-05-station-intel-l0-ft8-decoder-plan.md`:
//! message pack/unpack → CRC-14 → Gray/Costas framing → LDPC encode/syndrome →
//! soft demapper + min-sum decode → channelize → sync → full pipeline → oracle diff.

#![forbid(unsafe_code)]

/// FT-8 frame constants. Provenance: QEX 2020 §4 "Channel Symbols and Modulation".
pub mod consts {
    /// Costas 7-tone synchronization array (QEX 2020 §4; `ft8_lib` `constants.c`, MIT).
    pub const COSTAS: [u8; 7] = [3, 1, 4, 0, 6, 5, 2];

    /// Total transmitted symbols per FT-8 frame: `{Costas, 29 info, Costas, 29 info, Costas}`.
    pub const FRAME_SYMBOLS: usize = 79;

    /// Info-carrying symbols (58 channel symbols = 174 bits / 3 bits-per-symbol).
    pub const INFO_SYMBOLS: usize = 58;

    /// Payload bits conveyed by every FT-8 message (QEX 2020 §2).
    pub const PAYLOAD_BITS: usize = 77;

    /// LDPC codeword length (174,91) (QEX 2020 §3).
    pub const CODEWORD_BITS: usize = 174;

    /// Message + CRC bits before LDPC parity is appended (QEX 2020 §3).
    pub const MSG_CRC_BITS: usize = 91;

    /// Tone spacing / symbol rate in Hz: `h / T = 1 / 0.160` (QEX 2020 §4).
    pub const TONE_SPACING_HZ: f64 = 6.25;

    /// Symbol duration in seconds (QEX 2020 Table 4).
    pub const SYMBOL_SECS: f64 = 0.160;

    /// Canonical decode audio sample rate in Hz.
    pub const SAMPLE_RATE_HZ: u32 = 12_000;
}

/// FT-8 77-bit message pack/unpack (source encoding). Provenance: QEX 2020
/// Table 1/2 + MIT `ft8_lib` `message.c`/`text.c` (see `message` module docs).
pub mod message;

#[cfg(test)]
mod tests {
    use super::consts;

    /// QEX 2020 §4: the frame is `{Costas, 29, Costas, 29, Costas}` = 79 symbols,
    /// and the 174-bit codeword maps to 58 channel symbols at 3 bits each.
    #[test]
    fn frame_geometry_matches_spec() {
        assert_eq!(consts::COSTAS.len() * 3 + 29 * 2, consts::FRAME_SYMBOLS);
        assert_eq!(consts::INFO_SYMBOLS * 3, consts::CODEWORD_BITS);
        assert_eq!(consts::MSG_CRC_BITS + 83, consts::CODEWORD_BITS);
    }
}
