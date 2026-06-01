# tuxmodem-fec

LDPC forward error correction for tuxmodem (the clean-sheet HF modem in
the tuxlink project).

## Code families

Two LDPC code families share one sum-product-algorithm (SPA) belief-propagation
decoder:

- **Floor code (rate-1/4, n=2048, k=512).** Used by the wide-band low-density
  OFDM PHY mode for noise-floor operation. Trades aggressive code rate for
  maximum coding gain.
- **OFDM-family rate-compatible codes (n=648, n=1296; rates 1/2, 2/3, 3/4, 5/6).**
  Used by the bit-adaptive OFDM main family. The WiFi 802.11n LDPC family
  is the open-standard reference; tuxmodem's parameters are independently
  derived per the program's clean-sheet posture (ADR 0014). See
  `docs/code-construction.md`.

## API

Concrete LDPC codec types implement the `FecCodec` trait from
`tuxmodem-phy::coded_modulation`. The trait is the bus contract; this
crate provides the concrete implementations.

```rust
use tuxmodem_fec::{FloorRate14Codec, OfdmAdaptiveCodec, WifiLdpcRate};
use tuxmodem_phy::coded_modulation::FecCodec;

// Floor-mode codec (rate-1/4):
let codec = FloorRate14Codec::new();
let info_bits: Vec<u8> = /* one bit per byte, length 512 */;
let codeword = codec.encode(&info_bits);

// OFDM-family codec (rate-1/2 over n=648):
let codec = OfdmAdaptiveCodec::new(648, WifiLdpcRate::R1_2);
let llrs: Vec<f32> = /* n LLRs from PHY soft demod */;
let recovered = codec.decode_soft(&llrs)?;
```

## License

AGPLv3-only (see LICENSE). Per the program-wide license posture, no GPL-only
runtime dependencies are permitted; this crate depends only on permissively-
licensed (MIT/Apache-2.0/BSD) Rust crates plus the in-workspace
AGPLv3 `tuxmodem-phy`.

## Citations

Clean-sheet implementation from open foundational sources per ADR 0014. Key
references:

- Gallager, R.G. "Low-Density Parity-Check Codes." Sc.D. thesis, MIT, 1963.
- MacKay, D.J.C., Neal, R.M. "Good Codes Based on Very Sparse Matrices."
  Cryptography and Coding, 1995.
- Richardson, T.J., Urbanke, R.L. "The Capacity of Low-Density Parity-Check
  Codes Under Message-Passing Decoding." IEEE Trans. Inf. Theory, 2001.
- IEEE 802.11n-2009 (WiFi LDPC code parameter family, public standard).

Full bibliography in the program's `docs/research/modem-foundations.md`.

NO VARA internals, leaked source, decompilation, or RE write-ups are
consulted. STOP rule per ADR 0014.
