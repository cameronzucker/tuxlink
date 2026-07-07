//! `tuxlink-ft8` — clean-room, pure-Rust FT-8 decoder (Station Intelligence L0 spike).
//!
//! # Clean-room provenance (see `PROVENANCE.md`)
//!
//! FT8-protocol-specific expression (LDPC tables, Gray/Costas arrays, CRC,
//! message layout, demapper/min-sum form) is taken ONLY from the QEX 2020 FT4/FT8
//! protocol paper (Franke/Somerville/Taylor), the WB2FKO "Synchronization in FT8"
//! paper, and the MIT-licensed `ft8_lib` (kgoba). `RustFT8` (jl1nie, MIT) is an
//! available permitted reference but was **not** read/transcribed for this code.
//! Standard published algorithms and public-domain code (noncoherent-MFSK error
//! probability, min-sum, BP, SplitMix64, Box–Muller) are cited to their own
//! literature — see `PROVENANCE.md` for the two-tier rule. The GPL `wsjtr`/WSJT-X
//! is a **binary test oracle only** — its source is never read, its binary never
//! `strings`/`objdump`-ed, and its `generator.dat`/`parity.dat` are never copied
//! (the LDPC matrix comes from MIT `ft8_lib` or is regenerated).
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

/// FT-8 CRC-14 over the source-encoded message (77 bits zero-extended to 82,
/// forming the 91-bit message+CRC). Provenance: QEX 2020 §3 + MIT `ft8_lib`
/// `crc.c`/`constants.h` (poly `0x2757` low-14 / `0x6757` with `x^14`); see
/// `crc` module docs.
pub mod crc;

/// FT-8 codeword-bit <-> channel-symbol mapping: the Gray code (QEX 2020 Table 3)
/// and Costas framing (`{Costas, 29 info, Costas, 29 info, Costas}` = 79 symbols).
/// Provenance: QEX 2020 §4/Table 3 + MIT `ft8_lib` `constants.c`/`encode.c`; see
/// `symbols` module docs.
pub mod symbols;

/// FT-8 LDPC(174,91) forward error correction: systematic encode (append 83
/// parity bits to the 91 message+CRC bits) and syndrome / codeword validity.
/// The belief-propagation decoder is a later task (T1.1) and is not here.
/// Provenance: QEX 2020 §3 + MIT `ft8_lib` `constants.c` (generator + `Nm`
/// tables) / `encode.c` (`encode174`) / `ldpc.c` (`ldpc_check`); see `ldpc`
/// module docs.
pub mod ldpc;

/// FT-8 soft-demapper: 8 per-symbol FSK tone powers -> 174 variance-normalized
/// codeword-bit LLRs (`log(P1/P0)` convention, positive => bit 1). Provenance:
/// QEX 2020 §6 (soft-symbol metric) + MIT `ft8_lib` `decode.c`
/// (`ft8_extract_symbol` max-log demap + `ftx_normalize_logl`); see `llr`
/// module docs.
pub mod llr;

/// FT-8 channelization (M2): 12 kHz real audio → short-time power spectrogram +
/// a single-bin DFT primitive for sub-bin tone extraction. Provenance: WB2FKO
/// "Synchronization in FT8" spectrogram construction + MIT `ft8_lib` waterfall
/// oversampling model; Hann window + Goertzel are public-domain DSP; see
/// `channelize` module docs + `PROVENANCE.md`.
pub mod channelize;

/// FT-8 Costas synchronization (M2): coarse 2-D `(fc, t0)` search + ranked/deduped
/// candidates + fine time/frequency refinement + per-symbol tone-power extraction,
/// and the full real-WAV decode pipeline feeding the M1 demapper/decoder.
/// Provenance: WB2FKO "Synchronization in FT8" (`Sabc`/`Sbc`, fine `ft8b`/`sync8d`)
/// + MIT `ft8_lib` `decode.c` (`ftx_find_candidates`, `ft8_extract_symbol`); see
/// `sync` module docs + `PROVENANCE.md`.
pub mod sync;

/// FT-8 LDPC(174,91) belief-propagation (normalized min-sum) decoder: 174 LLRs
/// -> corrected 174-bit codeword. Provenance: QEX 2020 §3 + MIT `ft8_lib`
/// `ldpc.c` (`ldpc_decode`/`bp_decode`) + S. Johnson "Iterative Error
/// Correction" + textbook normalized min-sum; see `decode` module docs.
pub mod decode;

/// T1.2 AWGN-vs-SNR go/no-go harness (test-only): the noncoherent 8-FSK AWGN
/// tone model, the SNR-in-2500-Hz conversion, a dependency-free deterministic
/// PRNG, the self-verifying calibration against closed-form FSK theory, and the
/// coded decode-probability sweep. Compiled only under `cfg(test)`. Provenance:
/// QEX 2020 §4/§6/§8 + Table 5/6; Proakis noncoherent-MFSK Pe; SplitMix64 +
/// Box–Muller (public domain); see `awgn` module docs + `PROVENANCE.md`.
#[cfg(test)]
mod awgn;

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
