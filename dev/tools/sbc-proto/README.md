# sbc-proto — SBC encoder dev/iteration harness (tuxlink-vgvn)

Standalone fast-iteration crate (builds in ~3s; NOT part of the app — no root
workspace, so `cargo build` of src-tauri ignores it). Develops the pure-Rust SBC
**encoder** against the `mini_sbc` decode oracle + ffmpeg golden vectors.

`cargo run --release` here encodes `src/in.pcm` (1 kHz sine) → SBC → decodes via
mini_sbc → reports round-trip MAE.

## Status (2026-06-13, opossum-yew-juniper)
- Analysis filterbank + Loudness bit-allocation (ported from mini_sbc `calculate_bits`)
  + quantize (inverse of mini_sbc dequant) + bitstream pack: IMPLEMENTED.
- **scale = 4.0** is the calibrated analysis→dequant factor (= 2^SBCDEC_FIXED_EXTRA_BITS).
- Round-trip: steady-state **MAE 156** (avg) but **peak-err ~9850** + overshoot
  (out-peak 22862 vs in-peak 16383). So it produces *decodable* SBC but the analysis
  is NOT yet the exact inverse of mini_sbc's synthesis — a convention mismatch
  (sample-insertion order / cosine offset `(2k+1)(i-4)π/16` / window index mapping
  `c(i+16j)=PROTO[n/8][n%8]`). REMAINING: match the convention to mini_sbc's
  `decode16` (sbc.rs) so peak-err → low, before this feeds transmitted audio.
- CRC not yet emitted (tests use `FrameDecoder::new_skip_crc`); add CRC-8 (mini_sbc
  `crc.rs` is the reference) when wiring the real `SbcCodec`.
