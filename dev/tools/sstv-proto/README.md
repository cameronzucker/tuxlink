# sstv-proto

Standalone fast-iteration harness for the SSTV PCM↔image codec
(bd **tuxlink-st5n**), part of SSTV inline images for the UV-Pro native chat
(bd **tuxlink-bcsy**).

A pure-Rust, std-only port of HTCommander's SSTV implementation (itself a port of
<https://github.com/xdsopl/robot36>), kept as a separate crate so it compiles in
~1 s for tight TDD loops (the `no_cold_cargo` rule targets the multi-minute
`src-tauri` build, not a small leaf crate). The validated codec is intended to be
promoted into `src-tauri/src/winlink/ax25/uvpro/audio/` once the inline-image UI
(bd tuxlink-yfyn) consumes it.

## What it does

- **Encode** a packed-ARGB image → 32 kHz mono `f32` PCM (Robot 36 + PD modes).
- **Decode** PCM → image via a free-running FM discriminator with sync-pulse
  detection and VIS-header mode lock (`decoder.rs`). NOTE: despite the bd issue's
  "STFT decode" wording, the real xdsopl/HTCommander decode path is *time-domain*
  FM demodulation — the FFT/STFT classes are dead code there and are not ported.

## Decisive gate (the reason this harness exists)

`cargo test` includes a full-image round-trip through the **actual shipped**
`UvproSbcCodec` (vendored verbatim into `src/sbc_vendored.rs` for the test):

```
image → SSTV-encode → s16 PCM → SBC encode → SBC decode → s16 PCM → SSTV-decode → image
```

Result: pure-color quadrants reconstruct at MAE **0.0**, gray at ~2.4. SSTV is
frequency-modulated, so the SBC encoder's amplitude error (the ~MAE-156 concern
from tuxlink-vgvn) does not move the recovered frequencies — **no SBC encoder
refinement is needed for SSTV.**

## CLI

```bash
cargo run --features harness -- encode    in.png  out.pcm  [robot36|pd120|pd90]
cargo run --features harness -- decode    in.pcm  out.png
cargo run --features harness -- roundtrip in.png  out.png  [robot36|pd120|pd90]
```

PCM files are raw 32 kHz mono s16le — the wire format SSTV produces/consumes; the
SBC codec sits between this PCM and the radio link.
