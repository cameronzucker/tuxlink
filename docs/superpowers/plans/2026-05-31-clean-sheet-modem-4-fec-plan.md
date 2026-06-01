# Clean-sheet HF modem — Subsystem #4 (FEC) implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the forward-error-correction layer for tuxmodem's two-family PHY ladder (overview §5.A.1) — a short-block LDPC codec implemented as a discrete crate (`tuxmodem-fec`) inside the tuxmodem workspace, with two code families: a per-OFDM-symbol short-block LDPC for the bit-adaptive OFDM family and a single very-strong rate-1/4 LDPC for the wide-band low-density noise-floor mode. The FEC is the load-bearing margin for the noise-floor mode (per overview §5.A.1) and the rate-matched complement to bit-loading in the OFDM main family. Validate against the channel simulator (#1) per ITU-R F.520 "good/moderate/poor/flutter."

**Architecture (settled positions taken in this plan):**

1. **FEC is a peer subsystem, NOT folded into #3.** See §A below for the justification. Concretely: `tuxmodem-fec` is its own Rust crate inside the tuxmodem workspace; PHY (#3) depends on it. The crates ship together; the *separation* is at the API boundary, not the release boundary.
2. **Code family: LDPC only (short-block).** No turbo (decoder complexity + patent posture), no polar in v0.5+ (list-decoder complexity for the floor mode is questionable on a Pi 5 at the rate-1/4 strength needed; revisit in v0.6+). Reed-Solomon outer + convolutional inner — the classical concatenated stack — is explicitly rejected: modern short-block LDPC ships near-Shannon performance in a single layer at rates 1/4 through 5/6 at the block sizes HF voice-bandwidth modems can afford. Justification in §A.
3. **Two LDPC code instances, sharing one decoder:** a **rate-1/4 (n=2048, k=512)** "floor" code for the noise-floor robustness mode, and a **rate-adaptive (n=648 or 1296) WiFi-style family covering 1/2, 2/3, 3/4, 5/6** for the OFDM main-family per-OFDM-symbol payload. Both use sum-product-algorithm (SPA) belief-propagation decoding over a Tanner graph; the difference is the parity-check matrix and number of decoder iterations.
4. **Soft-decision input only.** The PHY (#3) MUST emit per-bit log-likelihood ratios (LLRs); the FEC API does NOT accept hard bits. This is non-negotiable for LDPC — hard-decision LDPC throws away 2–3 dB of coding gain and there is no reason to support it.
5. **CRC-32 outer per FEC block.** Per-block CRC is the contract the FEC exposes upward (to MAC/ARQ): "this block decoded AND the CRC checked." ARQ (#6) consumes the per-block ACK-or-NACK signal; without the CRC, an LDPC decoder that converges on a wrong codeword (rare but real) would silently corrupt data.
6. **Bit-interleaving inside the FEC layer.** Per-block bit interleaver after encode, de-interleaver before decode. Decorrelates HF burst errors before the LDPC decoder sees them. Interleaver is FEC's responsibility (not PHY's) because the interleaver depth is tied to the FEC block size, not the OFDM symbol structure.
7. **No HARQ in v0.5+.** Type-I HARQ (FEC redundancy stays fixed across retransmissions; ARQ is plain selective-repeat) is the v0.5+ posture. The FEC layer exposes hooks (rate-compatible puncturing on the rate-adaptive WiFi-family code) for v0.6+ Type-II/III incremental redundancy without requiring a v0.5+ implementation. See §8 watched failure modes.

**Tech Stack:**

- **Rust 2021 edition**, edition pinned to match the rest of the tuxmodem workspace.
- **Crate name:** `tuxmodem-fec`, AGPLv3-only (per overview §5.A.4).
- **Dependencies (verified AGPLv3-compatible):**
  - `bitvec` (MIT/Apache-2.0) — efficient bit-level slice manipulation for LDPC encode/decode.
  - `nalgebra` (BSD-3-Clause / Apache-2.0 / MIT, multi-licensed; pick Apache-2.0) — sparse matrix operations for the parity-check matrix.
  - `crc` (MIT/Apache-2.0) — CRC-32 (CRC-32-IEEE-802.3 polynomial 0x04C11DB7).
  - `rand` + `rand_chacha` (MIT/Apache-2.0) — deterministic PRNG for test vectors and channel simulator coupling.
  - `proptest` (MIT/Apache-2.0, dev-dep only) — property-based testing for invariants.
  - `criterion` (Apache-2.0, dev-dep only) — benchmarks.
- **No GNU Radio, no GPL-only LDPC libraries** (e.g., `gr-fec`'s implementations are GPL — out of scope per overview §5.A.4 + AGPL-incompatibility rationale in spec §1 footnote).
- **Channel simulator dependency:** depends on the `hf-channel-sim` crate (subsystem #1; AGPLv3) as a dev-dep for integration tests.

---

## §A. Position: FEC is a peer subsystem, not folded into #3

The overview §1 footnote ("FEC could fold into #3 or stay separate depending on architecture choice") and FEC spec §4.Q5 leave this open. This plan **takes the position: peer subsystem, separate crate.** Rationale (this position MUST be reconciled with #3's plan before either plan freezes; see §F coordination protocol below):

**Arguments for folding into #3:**

- FT8/JS8 fold them together (LDPC bits and Costas-array sync coexist in one frame).
- Single artifact is conceptually simpler.

**Arguments for a peer subsystem (taken here):**

1. **Two code families, one decoder.** The OFDM main family uses a rate-adaptive WiFi-style code (5+ code rates), the floor mode uses rate-1/4. Both share a single SPA decoder implementation. Folding into PHY would put a non-trivial decoder library inside the PHY crate, hurting reusability.
2. **The decoder is the load-bearing-on-CPU component.** The bulk of FEC implementation effort is the SPA decoder + its iteration tuning + its profiling. PHY's effort is in modulation/sync/equalization. Co-located in one crate, the FEC decoder optimization work would crowd PHY's work in the same file tree, the same review queue, the same benchmark harness.
3. **The validation harness differs.** FEC validation is "BER vs. SNR curve at fixed channel conditions" — pure pipe between channel simulator and FEC, no modulator/demodulator in the loop. Folding into PHY forces FEC tests to instantiate a PHY, which couples the two test suites.
4. **AI-native substrate principle (overview §4.6).** Subsystem decomposition makes work agent-scopeable. An agent can take "the FEC crate" as a unit of work bounded in context. Folded into PHY, an agent has to load both the PHY and FEC mental models simultaneously.
5. **Rust workspace convention.** Distinct concerns → distinct crates is the idiomatic posture. Reversing — folding FEC and PHY into one crate — is the unusual move that needs justification, not the default.
6. **HARQ optionality preserved.** Type-II/III HARQ (v0.6+) needs the FEC decoder to be addressable independently of the PHY frame timing. A folded design pre-commits to a single integration shape; a peer design keeps the seam open.

**Concretely:** `tuxmodem-fec` is its own crate. PHY (`tuxmodem-phy`) depends on it. They ship together in the `tuxmodem` workspace; the *separation* is the API boundary inside the workspace, not a release-time boundary. This is the same model `bitvec` + `nalgebra` use — separate crates, co-released.

**Coordination with #3 (per §F):** if #3's PHY plan takes the folded position, surface the disagreement at the parent agent's two-plan reconciliation step; do not silently resolve. The reconciliation should weigh the six arguments above against whatever the folded position offers.

---

## §B. File structure

The `tuxmodem-fec` crate sits at `crates/tuxmodem-fec/` inside the tuxmodem workspace. Files within the crate:

```
crates/tuxmodem-fec/
├── Cargo.toml                          # AGPL-3.0-only, edition 2021, deps as above
├── README.md                           # Purpose, citations (Gallager 1963, MacKay-Neal 1996, Richardson-Urbanke 2001), license
├── LICENSE                             # AGPL-3.0 full text
├── src/
│   ├── lib.rs                          # Crate root: pub-use the public API surface
│   ├── api.rs                          # Public encoder/decoder traits; FecBlock type; FecError enum
│   ├── llr.rs                          # Llr newtype wrapping f32; soft-bit ↔ LLR conversion helpers
│   ├── crc.rs                          # CRC-32 wrap; encode/decode/verify helpers
│   ├── interleaver.rs                  # Per-block bit interleaver (block-interleaver pattern, parameterized rows×cols)
│   ├── codes/
│   │   ├── mod.rs                      # Code-family enum + selection
│   │   ├── floor_rate14.rs             # rate-1/4 (n=2048, k=512) code: parity-check matrix construction + metadata
│   │   ├── ofdm_wifi_family.rs         # WiFi-style rate-compatible (n=648, n=1296) at rates 1/2, 2/3, 3/4, 5/6
│   │   └── parity_matrix.rs            # Sparse parity-check matrix representation (CSR sparse); shared utility
│   ├── encode.rs                       # LDPC systematic encoding via H-matrix back-substitution
│   ├── decode.rs                       # SPA belief-propagation decoder; max-iterations + early-termination
│   ├── puncture.rs                     # Rate-compatible puncturing patterns (for v0.6+ HARQ hook)
│   └── stats.rs                        # ResidualErrorStats type: surfaced to ARQ (subsystem #6)
├── tests/
│   ├── awgn_bers.rs                    # BER-vs-SNR curves under AWGN (sanity baseline; not Watterson)
│   ├── itu_f520_good.rs                # BER-vs-SNR under ITU-R F.520 "good" channel via hf-channel-sim
│   ├── itu_f520_moderate.rs            # Same, "moderate"
│   ├── itu_f520_poor.rs                # Same, "poor"
│   ├── itu_f520_flutter.rs             # Same, "flutter"
│   ├── crc_roundtrip.rs                # Encode → CRC → decode round-trip property tests
│   ├── interleaver_roundtrip.rs        # Bit-interleaver involution (interleave then de-interleave = identity)
│   └── api_contract.rs                 # FecEncoder/FecDecoder trait contract tests
├── benches/
│   ├── encode.rs                       # criterion benchmarks for encoder throughput
│   └── decode.rs                       # criterion benchmarks for decoder throughput at varying SNR
├── examples/
│   └── ber_curve.rs                    # CLI: produce BER-vs-SNR data for one code + channel condition combo
└── docs/
    ├── architecture.md                 # Internal docs: why LDPC, why two codes, why peer-subsystem
    ├── code-construction.md            # How the parity-check matrices are constructed (from-scratch derivation)
    └── decoder-tuning.md               # SPA iteration counts, early termination, profiling notes
```

**File responsibility boundaries:**

- `api.rs` defines the public surface; no one else does. `pub use` re-exports in `lib.rs`.
- `codes/` is the parity-check matrix factory; one file per code family. The decoder doesn't know which code it's decoding — it only knows the parity-check matrix it was handed.
- `encode.rs` and `decode.rs` are pure algorithm files; they take a matrix + payload/LLRs and produce codeword/decoded-bits. They do not touch the API types directly except through trait impls in `api.rs`.
- `stats.rs` is the seam to ARQ (subsystem #6); its public type is what ARQ consumes.

---

## §C. API surface to #3 (PHY) and #6 (ARQ)

### API to subsystem #3 (PHY)

**The PHY hands the FEC raw payload bytes; FEC returns a codeword as a `BitVec<u8>` to be modulated.**

```rust
// In api.rs:

/// Family selector. PHY hands this to the encoder/decoder to pick which code to use.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CodeFamily {
    /// Rate-1/4 floor code (n=2048, k=512). Used by the wide-band low-density OFDM mode.
    FloorRate14,
    /// Rate-adaptive WiFi-style family. n is 648 or 1296; rate is one of 1/2, 2/3, 3/4, 5/6.
    OfdmAdaptive { block_n: BlockN, rate: CodeRate },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BlockN { N648, N1296 }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CodeRate { R1_2, R2_3, R3_4, R5_6 }

/// Log-likelihood ratio. Sign carries the hard-decision bit; magnitude carries confidence.
/// LLR = log(P(bit=0) / P(bit=1)). Positive = more likely 0; negative = more likely 1.
#[derive(Copy, Clone, Debug)]
pub struct Llr(pub f32);

/// Encoder side. PHY calls this per FEC block.
pub trait FecEncoder {
    /// Encode `info_bits` (length k bits for the chosen code) into a length-n codeword.
    /// Includes CRC-32 over `info_bits` BEFORE LDPC encoding (CRC is part of the systematic
    /// info-bits region of the codeword).
    fn encode(&self, family: CodeFamily, info_bits: &BitSlice<u8>) -> Result<BitVec<u8>, FecError>;

    /// Return (n, k) for the chosen code so PHY knows how many modulated bits to allocate.
    fn block_size(&self, family: CodeFamily) -> (usize, usize);
}

/// Decoder side. PHY calls this with soft LLRs from its demodulator.
pub trait FecDecoder {
    /// Decode `llrs` (length n LLRs from PHY soft demod) into k information bits.
    /// Returns `Ok(DecodedBlock { info_bits, stats })` on successful decode + CRC pass;
    /// `Err(FecError::CrcFail | FecError::MaxIterationsExceeded)` otherwise.
    fn decode(&self, family: CodeFamily, llrs: &[Llr]) -> Result<DecodedBlock, FecError>;
}

pub struct DecodedBlock {
    pub info_bits: BitVec<u8>,
    pub stats: BlockDecodeStats,
}

pub struct BlockDecodeStats {
    pub iterations_used: u32,
    pub converged: bool,            // SPA found a valid codeword (parity checks all satisfied)
    pub crc_ok: bool,               // CRC-32 over info_bits matched
    pub estimated_post_decode_ber: Option<f32>, // optional: based on LLR magnitudes
}

#[derive(Debug)]
pub enum FecError {
    CrcFail { iterations_used: u32 },
    MaxIterationsExceeded,
    InvalidInputLength { expected: usize, got: usize },
    InternalDecoderFault(String),
}
```

**PHY-side responsibilities (NOT in this crate):**

- Producing soft LLRs from the demodulator. PHY's QAM/BPSK/FSK soft demod outputs LLRs in the units this crate expects (`log(P(0)/P(1))`).
- Handling sync, frame detection, equalization, channel-estimation before LLR production.
- Knowing how to bit-load a FEC codeword across OFDM sub-carriers (the bit-loading curve lives in PHY).

**FEC-side responsibilities (in this crate):**

- CRC-32 prepend before LDPC encode; CRC-32 verify after LDPC decode.
- Bit-interleaver after encode; de-interleaver before decode.
- LDPC encode/decode itself.
- Per-block stats surfaced to the decoder caller.

### API to subsystem #6 (ARQ)

ARQ consumes FEC's **per-block decode outcome** as the signal for ACK-or-NACK. ARQ does NOT directly call FEC; the MAC layer (#5) sits in between. But the data type that flows from FEC → MAC → ARQ is defined here so the contract is stable:

```rust
// In stats.rs:

/// Surfaced from FEC up through MAC to ARQ. ARQ uses these as input to the
/// retransmission decision: a CRC-fail block triggers NACK / selective-repeat retransmit.
#[derive(Clone, Debug)]
pub struct ResidualErrorStats {
    /// Did this block decode cleanly (CRC passed)?
    pub block_ok: bool,
    /// SPA iteration count used. ARQ may surface this as a channel-quality hint.
    pub iterations: u32,
    /// LLR-magnitude-derived confidence. None if the decoder doesn't compute it.
    pub confidence_score: Option<f32>,
    /// Time-since-frame-start for ARQ retransmission-timer accounting (filled in by MAC).
    pub frame_timestamp: Option<std::time::Instant>,
}

impl From<&BlockDecodeStats> for ResidualErrorStats {
    fn from(s: &BlockDecodeStats) -> Self {
        Self {
            block_ok: s.crc_ok,
            iterations: s.iterations_used,
            confidence_score: s.estimated_post_decode_ber,
            frame_timestamp: None, // MAC fills this
        }
    }
}
```

**The API contract to ARQ:** "if `block_ok == true`, the bits handed up are correct; if `block_ok == false`, the block did not decode and ARQ should request retransmission." That's it. ARQ does NOT inspect individual LLRs; FEC does NOT know about ARQ's window or sequence numbers.

### API to subsystem #1 (channel simulator)

The channel simulator is a dev-dep; FEC's integration tests use it to produce BER-vs-SNR curves. The seam is:

```rust
// In tests/itu_f520_*.rs (integration tests):

use hf_channel_sim::{Watterson, ItuF520Condition};
use tuxmodem_fec::{LdpcEncoder, LdpcDecoder, CodeFamily, Llr};

// 1. Encode N random info-blocks at the chosen rate.
// 2. BPSK-modulate the codeword (sign-bit only; this is the simplest channel-coupling).
// 3. Pass through Watterson channel at the chosen F.520 condition.
// 4. Soft-demodulate (compute LLRs from the noisy received samples).
// 5. Hand LLRs to FEC decoder.
// 6. Count bit errors in decoded info_bits vs. original.
// 7. Produce BER point at the chosen SNR.
// 8. Sweep SNR; plot the curve.
```

**The channel simulator must expose:** per-sample I/Q output given input I/Q + F.520 condition + SNR. This is in scope per subsystem #1's spec §3.6 (per-sub-carrier SNR estimation interface) and §3.5 (determinism / seeded RNG).

---

## §D. Multi-axis success criteria (per overview §0)

This FEC subsystem is gated on three measurable criteria, not one:

1. **OFDM-family code, "moderate" channel:** at the channel SNR where uncoded BPSK gives 10⁻² BER, the rate-1/2 OFDM-family LDPC code must achieve post-decode BER ≤ 10⁻⁵. This is the "competitive-with-VARA" margin — modern short-block LDPC at rate 1/2 routinely delivers 5–6 dB of coding gain in this regime.
2. **Floor-mode code, "poor" channel:** at per-sub-carrier SNR of -5 dB under ITU-R F.520 "poor," the rate-1/4 floor LDPC code must produce post-decode BER ≤ 10⁻⁴ across at least 80% of test runs. This is the "beat ARDOP's narrowest mode at the noise-floor case" gate translated to the FEC layer.
3. **Decoder real-time on Pi 5:** rate-1/2, n=648 LDPC decode must complete in ≤ 50 ms per block at max 50 SPA iterations. This is the decoder-complexity-budget forcing function from FEC spec §3.2; tested via `cargo bench --bench decode` on the dev Pi.

**Non-criteria explicitly:**

- We do NOT gate on "must strictly beat VARA's FEC." Per overview §0, that bar is risky to gate the program on. The bar is "close enough that operators find tuxmodem a reasonable choice."
- We do NOT gate on "decode is fastest possible on all hardware." Per overview §5.A.6, compute target is best-effort; Pi 5 is primary.

---

## §E. Watched failure modes

These extend FEC spec §8 with implementation-specific items.

1. **Optimistic AWGN-only BER projection.** AWGN performance of any LDPC code is a paper benchmark; HF Watterson performance is 2–5 dB worse for the same code. Every gate test uses `hf-channel-sim` at an F.520 condition, NOT AWGN.
2. **Decoder iteration runaway.** SPA can iterate until max-iter on a non-decodable block (channel below code's threshold). Capping max-iter is non-negotiable; we cap at 50 for the OFDM family and 100 for the floor mode (the floor mode trades decoder complexity for SNR-floor margin).
3. **CRC false-positive on converged-wrong codeword.** Rare but real: SPA can converge on a codeword that passes all parity checks but is not the transmitted codeword. CRC-32 catches this — that's why CRC is mandatory, not optional. CRC failure rate at design point should be <10⁻⁹.
4. **Interleaver-depth mismatch with HF burst length.** Block interleaver works only if depth >> burst length. HF burst lengths range from 10 ms (mild flutter) to 500 ms (deep fades). At 48 kHz sample rate, 500 ms = 24,000 samples. Block sizes (n=648 or n=2048) are smaller than this in symbol-time terms; interleaver-across-multiple-blocks is OUT of scope for v0.5+ (would couple FEC to the MAC layer's block-grouping). v0.5+ accepts that deep-fade bursts can wipe out a whole block; ARQ retransmits. This is a documented limitation, not a bug.
5. **HARQ scope creep.** Type-II/III HARQ is tempting once rate-compatible codes are in. We DO build the puncturing infrastructure (`puncture.rs`) but we DO NOT wire it to ARQ in v0.5+. The hook exists for v0.6+.
6. **VARA-shaped temptation around code parameters.** "How did VARA pick its LDPC parameters?" → STOP. Pick from Gallager 1963 + MacKay-Neal 1996 + WiFi 802.11n standard (open) for the WiFi-family code parameters. The WiFi 802.11n LDPC parameters are public-standard, not VARA-derived; using them is clean-sheet.
7. **GPL-only library temptation.** `gr-fec` from GNU Radio implements LDPC and is GPL-only. Tempting to wrap. Don't — AGPLv3 + GPLv3 is one-way compatible but adding a runtime GPL dep cascades, and our license posture is AGPLv3-only for tuxmodem. Implement LDPC from the foundational papers.
8. **Hardware-fast-decoder rabbit hole.** Production LDPC decoders use SIMD, GPU, or FPGA. v0.5+ stays scalar-float SPA on CPU. Profile under representative workloads first; optimize only the demonstrated bottlenecks.

---

## §F. Coordination with sibling subsystem plans

This plan was written in parallel with six sibling subsystem plans (#1 channel sim, #3 PHY, #5 link/MAC, #6 ARQ, #7 link adaptation, #8 host protocol). Three points need reconciliation at the parent agent's plan-merge step:

1. **Peer-vs-folded decision with #3.** §A above takes the peer-subsystem position. If #3's plan takes the folded position, surface the disagreement at the parent reconciliation step. Do NOT silently resolve.
2. **LLR units with #3.** §C above defines `Llr` as `log(P(0)/P(1))`. #3's PHY plan MUST match. If #3's plan defines LLR as `log(P(1)/P(0))` (the opposite sign convention), one of the two plans changes. The mathematically-conventional choice (and the one this plan picks) is `log(P(0)/P(1))` — sign matches the systematic-bit convention.
3. **`ResidualErrorStats` type with #6.** §C above defines a stats type ARQ consumes. #6's ARQ plan MUST match its consumer-side `From` impl to this producer-side type. If #6's plan invents a different name (`PerBlockResult`, `FrameDecodeOutcome`, etc.), unify at the parent step — pick one and update the other.

---

## §G. Phase overview (8 phases)

| Phase | Title |
|---|---|
| Phase 0 | Crate scaffold + workspace integration + license + README |
| Phase 1 | CRC-32 wrapper + property tests |
| Phase 2 | Bit interleaver (encode/decode involution) |
| Phase 3 | Parity-check matrix construction (floor rate-1/4 + WiFi-family rate-1/2..5/6) |
| Phase 4 | LDPC systematic encoder |
| Phase 5 | SPA belief-propagation decoder + max-iter + early-termination |
| Phase 6 | Public API (`FecEncoder` / `FecDecoder` traits) + roundtrip integration |
| Phase 7 | Channel-simulator-coupled BER-vs-SNR test suite (gate against §D criteria) |
| Phase 8 | Benchmarks, decoder tuning, README + design docs, final polish |

---

## Phase 0: Crate scaffold + workspace integration

**Files:**
- Create: `crates/tuxmodem-fec/Cargo.toml`
- Create: `crates/tuxmodem-fec/LICENSE` (full AGPL-3.0-only text)
- Create: `crates/tuxmodem-fec/README.md`
- Create: `crates/tuxmodem-fec/src/lib.rs`
- Modify: workspace-root `Cargo.toml` (add `crates/tuxmodem-fec` to `members`)

### Task 0.1: Write the workspace-root Cargo.toml diff

- [ ] **Step 1: Read the workspace-root `Cargo.toml`** at the tuxmodem workspace root (path to be confirmed at execution time; typically `tuxmodem/Cargo.toml` or `crates-tuxmodem/Cargo.toml` per the workspace's actual layout). Identify the `[workspace] members = [ ... ]` array.

- [ ] **Step 2: Add `"crates/tuxmodem-fec"` to the members list, alphabetized.**

Diff:

```toml
[workspace]
members = [
    "crates/hf-channel-sim",
    "crates/tuxmodem-fec",        # ADD THIS LINE
    "crates/tuxmodem-phy",
    # ... other crates as they exist
]
```

- [ ] **Step 3: Commit.**

```bash
git add Cargo.toml
git commit -m "feat(fec): register tuxmodem-fec crate in workspace

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 0.2: Create the Cargo.toml for the FEC crate

**File:** `crates/tuxmodem-fec/Cargo.toml`

- [ ] **Step 1: Write the file.**

```toml
[package]
name = "tuxmodem-fec"
version = "0.0.1"
edition = "2021"
license = "AGPL-3.0-only"
description = "LDPC forward error correction for tuxmodem (clean-sheet HF modem). Two code families: a rate-1/4 short-block code for the wide-band low-density noise-floor PHY mode, and a rate-adaptive WiFi-style family (rates 1/2..5/6) for the bit-adaptive OFDM main family."
repository = "https://github.com/cameronzucker/tuxlink"
keywords = ["ldpc", "fec", "hf", "modem", "ham-radio"]
categories = ["algorithms", "encoding"]

[dependencies]
bitvec = "1"
nalgebra = "0.32"
crc = "3"
rand = "0.8"
rand_chacha = "0.3"

[dev-dependencies]
hf-channel-sim = { path = "../hf-channel-sim" }
proptest = "1"
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "encode"
harness = false

[[bench]]
name = "decode"
harness = false
```

- [ ] **Step 2: Run `cargo check -p tuxmodem-fec`.**

Expected: PASS (empty crate, no source yet — `cargo check` succeeds against the empty `lib.rs` from the next task).

### Task 0.3: Create the LICENSE file (AGPL-3.0-only full text)

- [ ] **Step 1: Write `crates/tuxmodem-fec/LICENSE` with the full AGPL-3.0-only text.** Copy from `https://www.gnu.org/licenses/agpl-3.0.txt` verbatim (or from a sibling crate's LICENSE file if one already exists in the workspace). Cargo doesn't enforce this, but AGPL §13 + §15 require the license text to ship with the source.

- [ ] **Step 2: Commit Phase 0 scaffolding (Cargo.toml + LICENSE).**

```bash
git add crates/tuxmodem-fec/
git commit -m "feat(fec): scaffold tuxmodem-fec crate (Cargo.toml + AGPLv3 LICENSE)

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 0.4: Create the README + minimal lib.rs

- [ ] **Step 1: Write `crates/tuxmodem-fec/README.md`.**

```markdown
# tuxmodem-fec

LDPC forward error correction for tuxmodem (the clean-sheet HF modem in
the tuxlink project).

## Code families

This crate implements two LDPC code families, sharing one sum-product-algorithm
(SPA) belief-propagation decoder:

- **Floor code (rate-1/4, n=2048, k=512).** Used by the wide-band low-density
  OFDM PHY mode for noise-floor operation. Trades aggressive code rate for
  maximum coding gain.
- **OFDM-family rate-compatible codes (n=648, n=1296; rates 1/2, 2/3, 3/4, 5/6).**
  Used by the bit-adaptive OFDM main family. The WiFi 802.11n LDPC family is
  the open-standard reference; tuxmodem's parameters are independently derived
  per the program's clean-sheet posture (ADR 0014). See `docs/code-construction.md`.

## API

```rust
use tuxmodem_fec::{FecEncoder, FecDecoder, CodeFamily, Llr, BlockN, CodeRate};
use bitvec::prelude::*;

let encoder = LdpcEncoder::new();
let info_bits: BitVec<u8> = /* k bits from PHY's upper layer */;
let codeword = encoder.encode(
    CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 },
    info_bits.as_bitslice(),
)?;

// PHY modulates `codeword`; receiver demodulates to LLRs.

let decoder = LdpcDecoder::new();
let llrs: Vec<Llr> = /* n LLRs from PHY's soft demodulator */;
let decoded = decoder.decode(family, &llrs)?;
assert!(decoded.stats.crc_ok);
```

## License

AGPLv3-only (see LICENSE). Per the program-wide license posture, no GPL-only
runtime dependencies are permitted; this crate depends only on permissively-
licensed (MIT/Apache-2.0/BSD) Rust crates.

## Citations

This crate is a clean-sheet implementation from open foundational sources, per
ADR 0014. Key references:

- Gallager, R.G. "Low-Density Parity-Check Codes." Sc.D. thesis, MIT, 1963.
- MacKay, D.J.C., Neal, R.M. "Good Codes Based on Very Sparse Matrices."
  Cryptography and Coding, 1995.
- Richardson, T.J., Urbanke, R.L. "The Capacity of Low-Density Parity-Check
  Codes Under Message-Passing Decoding." IEEE Trans. Inf. Theory, 2001.
- IEEE 802.11n-2009 (WiFi LDPC code parameter family, public standard).

Full bibliography in the program's `docs/research/modem-foundations.md`.

NO VARA internals, leaked source, decompilation, or RE write-ups are
consulted. STOP rule per ADR 0014.
```

- [ ] **Step 2: Write `crates/tuxmodem-fec/src/lib.rs` minimal stub.**

```rust
//! tuxmodem-fec: LDPC forward error correction for the clean-sheet HF modem.
//!
//! See `README.md` for an overview. See `docs/architecture.md` for design
//! rationale. See ADR 0014 for the clean-sheet posture this crate is
//! implemented under.

#![deny(unsafe_code)]
#![warn(missing_docs)]

// Public API surface. Re-exported from submodules below.
pub use api::{
    BlockDecodeStats, BlockN, CodeFamily, CodeRate, DecodedBlock, FecDecoder,
    FecEncoder, FecError, Llr,
};
pub use stats::ResidualErrorStats;

mod api;
mod crc;
mod interleaver;
mod codes;
mod encode;
mod decode;
mod puncture;
mod stats;
mod llr;
```

- [ ] **Step 3: Verify `cargo check -p tuxmodem-fec` fails because submodules don't exist yet.**

Expected: FAIL with errors about missing modules `api`, `crc`, etc. That's the next phase's work.

- [ ] **Step 4: Skip the failing cargo check for now — the submodule stubs will land progressively in Phases 1–6. Commit the scaffolding.**

```bash
git add crates/tuxmodem-fec/
git commit -m "feat(fec): scaffold tuxmodem-fec lib.rs + README

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 1: CRC-32 wrapper + tests

**Files:**
- Create: `crates/tuxmodem-fec/src/crc.rs`
- Create: `crates/tuxmodem-fec/tests/crc_roundtrip.rs`

CRC-32 is the outer integrity check FEC reports to ARQ. CRC-32-IEEE-802.3
polynomial (0x04C11DB7) is the de-facto standard.

### Task 1.1: Write the failing CRC roundtrip test

**File:** `crates/tuxmodem-fec/tests/crc_roundtrip.rs`

- [ ] **Step 1: Write the test.**

```rust
//! Property: append_crc32() followed by verify_crc32() round-trips losslessly.

use bitvec::prelude::*;
use tuxmodem_fec::crc::{append_crc32, verify_crc32};

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

    // Flip a single bit anywhere.
    let mut bit = with_crc.get_mut(42).unwrap();
    let prev = *bit;
    *bit = !prev;

    assert!(verify_crc32(with_crc.as_bitslice()).is_err());
}
```

Note: this test imports `tuxmodem_fec::crc::{append_crc32, verify_crc32}`. To make this importable, expose the `crc` module as `pub mod crc` in `lib.rs` (the public API for tests is intentionally narrow but the CRC functions are needed by the encoder + downstream layers, so they're crate-level public).

- [ ] **Step 2: Adjust `lib.rs` to make the `crc` module public.**

Change `mod crc;` → `pub mod crc;` in `lib.rs`.

- [ ] **Step 3: Run `cargo test -p tuxmodem-fec --test crc_roundtrip` to verify it fails.**

Expected: FAIL with "unresolved module `crc`" or "function `append_crc32` not found."

### Task 1.2: Implement the CRC wrapper

**File:** `crates/tuxmodem-fec/src/crc.rs`

- [ ] **Step 1: Write `crc.rs`.**

```rust
//! CRC-32-IEEE-802.3 over bit slices.
//!
//! `append_crc32(bits)` → bits || crc32(bits)  (length = bits.len() + 32)
//! `verify_crc32(bits_with_crc)` → Ok(()) iff the trailing 32 bits match crc32(prefix).

use bitvec::prelude::*;
use crc::{Crc, CRC_32_ISO_HDLC}; // 0x04C11DB7 polynomial, common name

const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

/// Append CRC-32 (32 trailing bits) to the input bit slice.
pub fn append_crc32(info: &BitSlice<u8>) -> BitVec<u8> {
    // Convert info bits to bytes, padding the final byte with zeros if needed.
    let info_bytes = bits_to_bytes(info);
    let crc = CRC32.checksum(&info_bytes);

    let mut out: BitVec<u8> = info.to_bitvec();
    // Append the CRC most-significant-bit first.
    for i in (0..32).rev() {
        out.push((crc >> i) & 1 == 1);
    }
    out
}

/// Verify a CRC-appended bit slice. Returns Err if the CRC does not match.
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

#[derive(Debug, PartialEq, Eq)]
pub enum CrcError {
    TooShort,
    Mismatch { expected: u32, actual: u32 },
}

fn bits_to_bytes(bits: &BitSlice<u8>) -> Vec<u8> {
    // MSB-first packing within each byte. Pad the final byte with zero bits.
    let mut bytes = Vec::with_capacity((bits.len() + 7) / 8);
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
```

- [ ] **Step 2: Run the tests.**

Run: `cargo test -p tuxmodem-fec --test crc_roundtrip`
Expected: All three tests PASS.

- [ ] **Step 3: Commit.**

```bash
git add crates/tuxmodem-fec/src/crc.rs crates/tuxmodem-fec/src/lib.rs crates/tuxmodem-fec/tests/crc_roundtrip.rs
git commit -m "feat(fec): CRC-32 append + verify over bit slices

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 2: Bit interleaver

**Files:**
- Create: `crates/tuxmodem-fec/src/interleaver.rs`
- Create: `crates/tuxmodem-fec/tests/interleaver_roundtrip.rs`

Block interleaver: write input bits row-by-row into an R×C matrix; read column-by-column. De-interleaver is the inverse. Decorrelates burst errors before the LDPC decoder.

### Task 2.1: Write the failing interleaver tests

**File:** `crates/tuxmodem-fec/tests/interleaver_roundtrip.rs`

- [ ] **Step 1: Write the test.**

```rust
//! Property: interleave then de-interleave is identity. Also: interleaver
//! decorrelates positional clusters (sanity check for HF burst tolerance).

use bitvec::prelude::*;
use proptest::prelude::*;
use tuxmodem_fec::interleaver::{interleave, deinterleave};

proptest! {
    #[test]
    fn interleave_roundtrip(
        bits in proptest::collection::vec(any::<bool>(), 0..2048),
        rows in 4usize..32,
    ) {
        // Skip cases where the interleaver shape doesn't fit.
        let n = bits.len();
        if n < rows * 2 { return Ok(()); }
        let bv: BitVec<u8> = bits.into_iter().collect();

        let interleaved = interleave(bv.as_bitslice(), rows);
        let recovered = deinterleave(interleaved.as_bitslice(), rows);

        prop_assert_eq!(bv.len(), recovered.len());
        for (a, b) in bv.iter().zip(recovered.iter()) {
            prop_assert_eq!(*a, *b);
        }
    }
}

#[test]
fn burst_error_decorrelation() {
    // Put 1s in the first 16 positions (a burst); interleave; check that
    // after interleaving the 1s are spread across the output.
    let n = 256;
    let rows = 16;
    let mut input: BitVec<u8> = BitVec::repeat(false, n);
    for i in 0..16 { input.set(i, true); }

    let interleaved = interleave(input.as_bitslice(), rows);
    // Count 1s in each chunk-of-16 of the output. Each chunk should have
    // at most 1 set bit (the burst was fully spread).
    for chunk in interleaved.chunks(16) {
        let ones = chunk.iter().filter(|b| **b).count();
        assert!(ones <= 1, "burst was not decorrelated: chunk had {} ones", ones);
    }
}
```

- [ ] **Step 2: Make the interleaver module pub.** Change `mod interleaver;` → `pub mod interleaver;` in `lib.rs`.

- [ ] **Step 3: Run the test to verify it fails.**

Expected: FAIL — `interleave` / `deinterleave` not defined.

### Task 2.2: Implement the interleaver

**File:** `crates/tuxmodem-fec/src/interleaver.rs`

- [ ] **Step 1: Write `interleaver.rs`.**

```rust
//! Block bit interleaver. Writes input row-by-row into an R×C matrix
//! (where C = ceil(n / R)), reads column-by-column. De-interleaver is the inverse.
//!
//! Used between LDPC encode and channel modulation to decorrelate HF burst
//! errors before they reach the LDPC decoder.

use bitvec::prelude::*;

/// Interleave `input` using a block interleaver with `rows` rows.
/// Output length equals input length (pad bits are zero).
pub fn interleave(input: &BitSlice<u8>, rows: usize) -> BitVec<u8> {
    assert!(rows > 0);
    let n = input.len();
    let cols = (n + rows - 1) / rows;
    let total = rows * cols;

    // Fill a row-major matrix of length total, padding the tail with zeros.
    let mut matrix: BitVec<u8> = BitVec::repeat(false, total);
    for (i, bit) in input.iter().enumerate() {
        matrix.set(i, *bit);
    }

    // Read column-by-column into the output, truncating to n bits.
    let mut out: BitVec<u8> = BitVec::with_capacity(n);
    for col in 0..cols {
        for row in 0..rows {
            let idx = row * cols + col;
            if out.len() < n {
                out.push(matrix[idx]);
            }
        }
    }
    out.truncate(n);
    out
}

/// De-interleave: inverse of `interleave` with the same `rows` parameter.
pub fn deinterleave(input: &BitSlice<u8>, rows: usize) -> BitVec<u8> {
    assert!(rows > 0);
    let n = input.len();
    let cols = (n + rows - 1) / rows;
    let total = rows * cols;

    // Inverse: write column-by-column, read row-by-row.
    let mut matrix: BitVec<u8> = BitVec::repeat(false, total);
    let mut iter = input.iter();
    for col in 0..cols {
        for row in 0..rows {
            let idx = row * cols + col;
            if let Some(bit) = iter.next() {
                matrix.set(idx, *bit);
            }
        }
    }

    let mut out: BitVec<u8> = BitVec::with_capacity(n);
    for i in 0..n {
        out.push(matrix[i]);
    }
    out
}
```

- [ ] **Step 2: Run the tests.**

Run: `cargo test -p tuxmodem-fec --test interleaver_roundtrip`
Expected: Both tests PASS.

- [ ] **Step 3: Commit.**

```bash
git add crates/tuxmodem-fec/src/interleaver.rs crates/tuxmodem-fec/src/lib.rs crates/tuxmodem-fec/tests/interleaver_roundtrip.rs
git commit -m "feat(fec): block bit interleaver with burst-decorrelation test

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 3: Parity-check matrix construction

**Files:**
- Create: `crates/tuxmodem-fec/src/codes/mod.rs`
- Create: `crates/tuxmodem-fec/src/codes/parity_matrix.rs`
- Create: `crates/tuxmodem-fec/src/codes/floor_rate14.rs`
- Create: `crates/tuxmodem-fec/src/codes/ofdm_wifi_family.rs`
- Create: `crates/tuxmodem-fec/src/api.rs` (the enum types this phase needs)

The parity-check matrix H of an LDPC code is the source of both the encoder (via systematic-form transformation) and the decoder (via Tanner-graph BP). H is sparse — `nalgebra`'s sparse format wins on memory.

### Task 3.1: Write the public-type-enum stubs in api.rs

**File:** `crates/tuxmodem-fec/src/api.rs`

- [ ] **Step 1: Write the enums + types. (Trait stubs come in Phase 6.)**

```rust
//! Public API for tuxmodem-fec.

use bitvec::prelude::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CodeFamily {
    /// Rate-1/4 (n=2048, k=512) short-block code for the wide-band low-density
    /// OFDM floor mode. Maximum coding gain at the cost of throughput.
    FloorRate14,
    /// Rate-adaptive WiFi-style family for the bit-adaptive OFDM main family.
    OfdmAdaptive { block_n: BlockN, rate: CodeRate },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BlockN { N648, N1296 }

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CodeRate { R1_2, R2_3, R3_4, R5_6 }

impl CodeRate {
    /// Numerator and denominator of the code rate.
    pub fn ratio(self) -> (usize, usize) {
        match self {
            CodeRate::R1_2 => (1, 2),
            CodeRate::R2_3 => (2, 3),
            CodeRate::R3_4 => (3, 4),
            CodeRate::R5_6 => (5, 6),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Llr(pub f32);

/// Decoder output on a successful decode.
#[derive(Debug)]
pub struct DecodedBlock {
    pub info_bits: BitVec<u8>,
    pub stats: BlockDecodeStats,
}

#[derive(Copy, Clone, Debug)]
pub struct BlockDecodeStats {
    pub iterations_used: u32,
    pub converged: bool,
    pub crc_ok: bool,
    pub estimated_post_decode_ber: Option<f32>,
}

#[derive(Debug)]
pub enum FecError {
    CrcFail { iterations_used: u32 },
    MaxIterationsExceeded,
    InvalidInputLength { expected: usize, got: usize },
    InternalDecoderFault(String),
}

impl std::fmt::Display for FecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CrcFail { iterations_used } =>
                write!(f, "FEC decoded but CRC failed after {} iterations", iterations_used),
            Self::MaxIterationsExceeded =>
                write!(f, "FEC max iterations exceeded without convergence"),
            Self::InvalidInputLength { expected, got } =>
                write!(f, "FEC input length mismatch: expected {}, got {}", expected, got),
            Self::InternalDecoderFault(s) =>
                write!(f, "FEC internal decoder fault: {}", s),
        }
    }
}

impl std::error::Error for FecError {}

// FecEncoder and FecDecoder trait declarations are added in Phase 6.
```

- [ ] **Step 2: Run `cargo check -p tuxmodem-fec`.** Expected: still failing on missing `codes`, `encode`, `decode`, `stats`, `llr`, `puncture` modules. That's fine; this phase only adds `api.rs`.

- [ ] **Step 3: Commit.**

```bash
git add crates/tuxmodem-fec/src/api.rs
git commit -m "feat(fec): public type enums (CodeFamily, BlockN, CodeRate, Llr, ...)

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 3.2: Implement the sparse parity-matrix type

**File:** `crates/tuxmodem-fec/src/codes/parity_matrix.rs`

- [ ] **Step 1: Write the parity-check matrix type.**

```rust
//! Sparse parity-check matrix H for an LDPC code.
//!
//! H is an (n-k) × n binary matrix; codeword c satisfies H·c^T = 0 over GF(2).
//! Stored row-major as a list of column-indices per row (sparse representation
//! exploiting H's low density).

#[derive(Debug, Clone)]
pub struct ParityCheckMatrix {
    pub n: usize,                          // codeword length
    pub k: usize,                          // info-bits length
    pub rows: Vec<Vec<usize>>,             // rows[r] = sorted list of column indices where H[r][c] = 1
}

impl ParityCheckMatrix {
    pub fn n_minus_k(&self) -> usize { self.n - self.k }

    /// Check that c satisfies H·c^T = 0 over GF(2).
    pub fn parity_check(&self, codeword: &[bool]) -> bool {
        assert_eq!(codeword.len(), self.n);
        for row in &self.rows {
            let parity: bool = row.iter().fold(false, |acc, &col| acc ^ codeword[col]);
            if parity {
                return false;
            }
        }
        true
    }

    /// Count edges (1s) in H. For diagnostics + decoder-complexity estimation.
    pub fn edge_count(&self) -> usize {
        self.rows.iter().map(|r| r.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_check_rejects_nonzero_syndrome() {
        // Trivial 2×4 example: H = [[1,1,0,0],[0,0,1,1]]
        let h = ParityCheckMatrix {
            n: 4, k: 2,
            rows: vec![vec![0, 1], vec![2, 3]],
        };
        assert!(h.parity_check(&[false, false, false, false]));
        assert!(h.parity_check(&[true, true, false, false]));
        assert!(!h.parity_check(&[true, false, false, false]));
    }
}
```

- [ ] **Step 2: Run the unit test.**

Run: `cargo test -p tuxmodem-fec --lib codes::parity_matrix`
Expected: PASS.

- [ ] **Step 3: Commit.**

```bash
git add crates/tuxmodem-fec/src/codes/parity_matrix.rs
git commit -m "feat(fec): sparse parity-check matrix type for LDPC codes

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 3.3: Implement the floor rate-1/4 code construction

**File:** `crates/tuxmodem-fec/src/codes/floor_rate14.rs`

Use the **MacKay-Neal 3-4 regular construction** (regular column weight 3, regular row weight 4) for an (n=2048, k=512) code. Construction is from MacKay's 1996 paper (open) — random sparse H with constraints: every column has exactly 3 ones, every row has exactly 4 ones (since (n-k)/n × col_weight = row_weight, that's (1536/2048)*4 = 3 — checks out). Generated deterministically with a fixed seed for reproducibility.

- [ ] **Step 1: Write `floor_rate14.rs`.**

```rust
//! Floor rate-1/4 LDPC code: n=2048, k=512, regular (3,4) construction.
//!
//! Per Gallager 1963 + MacKay-Neal 1996, regular LDPC codes with sparse random
//! parity-check matrices approach Shannon capacity under sum-product decoding
//! at moderate iteration counts. The (3,4) regular construction balances
//! column weight (decoder cycle count per bit) and row weight (parity-check
//! density). Fixed seed → reproducible matrix → reproducible BER curves.

use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

use super::parity_matrix::ParityCheckMatrix;

const N: usize = 2048;
const K: usize = 512;
const COL_WEIGHT: usize = 3;
const ROW_WEIGHT: usize = 4;
const SEED: u64 = 0x_F_EC_FL_OOR_14_u64;

/// Construct the floor rate-1/4 parity-check matrix.
/// Deterministic given the SEED constant. Returns a (n-k) × n matrix.
pub fn build() -> ParityCheckMatrix {
    let m = N - K;
    debug_assert_eq!(N * COL_WEIGHT, m * ROW_WEIGHT, "regular construction balance");

    // Permutation construction: build a list of m*ROW_WEIGHT (= n*COL_WEIGHT)
    // edge-stubs, shuffle, then partition into rows. Each column gets exactly
    // COL_WEIGHT stubs; each row exactly ROW_WEIGHT.

    let total_edges = m * ROW_WEIGHT;
    debug_assert_eq!(total_edges, N * COL_WEIGHT);

    // Stub list: for each column c, COL_WEIGHT copies of c.
    let mut stubs: Vec<usize> = (0..N).flat_map(|c| std::iter::repeat(c).take(COL_WEIGHT)).collect();

    let mut rng = ChaCha8Rng::seed_from_u64(SEED);
    stubs.shuffle(&mut rng);

    // Partition shuffled stubs into m rows of ROW_WEIGHT each. Reject + retry
    // if a row would contain duplicate column indices (would be a degenerate
    // parity check). On a code this size, duplicate probability per row is
    // ~ROW_WEIGHT^2 / N = 16/2048 < 1%; total reshuffles expected <5.
    for attempt in 0..32 {
        let mut rows: Vec<Vec<usize>> = Vec::with_capacity(m);
        let mut ok = true;
        for r in 0..m {
            let mut row: Vec<usize> = stubs[r * ROW_WEIGHT..(r + 1) * ROW_WEIGHT].to_vec();
            row.sort();
            row.dedup();
            if row.len() != ROW_WEIGHT {
                ok = false;
                break;
            }
            rows.push(row);
        }
        if ok {
            return ParityCheckMatrix { n: N, k: K, rows };
        }
        stubs.shuffle(&mut rng);
        // (silent retry — `attempt` ignored beyond bounded-loop guard)
        let _ = attempt;
    }
    panic!("floor_rate14::build failed to construct a duplicate-free H after 32 retries; this is a code bug");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_h_has_correct_dimensions() {
        let h = build();
        assert_eq!(h.n, N);
        assert_eq!(h.k, K);
        assert_eq!(h.rows.len(), N - K);
    }

    #[test]
    fn floor_h_is_regular_row_weight() {
        let h = build();
        for row in &h.rows {
            assert_eq!(row.len(), ROW_WEIGHT);
            // sorted, no duplicates
            let mut sorted = row.clone();
            sorted.sort();
            assert_eq!(row, &sorted);
            sorted.dedup();
            assert_eq!(sorted.len(), row.len());
        }
    }

    #[test]
    fn floor_h_is_regular_column_weight() {
        let h = build();
        let mut col_weights = vec![0; N];
        for row in &h.rows {
            for &c in row {
                col_weights[c] += 1;
            }
        }
        for w in col_weights {
            assert_eq!(w, COL_WEIGHT);
        }
    }

    #[test]
    fn floor_h_is_deterministic() {
        let h1 = build();
        let h2 = build();
        assert_eq!(h1.rows, h2.rows);
    }
}
```

- [ ] **Step 2: Wire up `codes/mod.rs`.**

```rust
pub mod parity_matrix;
pub mod floor_rate14;
pub mod ofdm_wifi_family;
```

- [ ] **Step 3: Also create `codes/ofdm_wifi_family.rs` with a stub for the next task.**

```rust
//! WiFi 802.11n-style rate-compatible LDPC family.
//!
//! n ∈ {648, 1296}; rate ∈ {1/2, 2/3, 3/4, 5/6}. The construction follows
//! the IEEE 802.11n-2009 LDPC design pattern (quasi-cyclic block-diagonal H
//! with shifted identity sub-matrices), with tuxmodem-specific shift values
//! independently derived per the clean-sheet posture (ADR 0014).
//!
//! [STUB: filled in by Task 3.4]

use super::parity_matrix::ParityCheckMatrix;
use crate::api::{BlockN, CodeRate};

pub fn build(_block_n: BlockN, _rate: CodeRate) -> ParityCheckMatrix {
    todo!("Task 3.4")
}
```

- [ ] **Step 4: Run the floor-code tests.**

Run: `cargo test -p tuxmodem-fec --lib codes::floor_rate14`
Expected: All four tests PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/tuxmodem-fec/src/codes/
git commit -m "feat(fec): floor rate-1/4 LDPC code (n=2048, k=512, regular 3,4)

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 3.4: Implement the OFDM WiFi-family code construction

The WiFi 802.11n LDPC family uses a quasi-cyclic (QC) construction: H is a block matrix of Z×Z circulant sub-matrices (Z = 27 for n=648; Z = 54 for n=1296). Each block is either zero or a cyclic-shifted identity. The shift values are tabulated. Per ADR 0014, we don't use 802.11n's specific shift table verbatim (it's a public standard, so it'd be defensible — but we want independent provenance for the modem). Instead, we generate shift values from a deterministic PRNG seeded with a constant, using the WiFi-pattern construction *technique*.

- [ ] **Step 1: Replace the `ofdm_wifi_family.rs` stub with the real implementation.**

```rust
//! WiFi 802.11n-style rate-compatible LDPC family.
//!
//! Quasi-cyclic LDPC construction: H is a block matrix of Z×Z circulants
//! (Z = 27 for n=648; Z = 54 for n=1296). Each block is either the zero
//! matrix or a cyclic-shifted identity matrix.
//!
//! Shift values are generated deterministically from a fixed PRNG seed per
//! (block_n, rate) tuple. This is the construction PATTERN of IEEE 802.11n
//! (public standard); the specific shift values are tuxmodem-derived per
//! ADR 0014's clean-sheet provenance posture.

use rand::SeedableRng;
use rand::Rng;
use rand_chacha::ChaCha8Rng;

use super::parity_matrix::ParityCheckMatrix;
use crate::api::{BlockN, CodeRate};

const Z_648: usize = 27;
const Z_1296: usize = 54;
const SEED_BASE: u64 = 0x_F_EC_O_FD_M_WI_FI_u64;

/// Build the parity-check matrix for the given (block_n, rate) pair.
pub fn build(block_n: BlockN, rate: CodeRate) -> ParityCheckMatrix {
    let z = match block_n {
        BlockN::N648 => Z_648,
        BlockN::N1296 => Z_1296,
    };
    let n = z * 24; // 24 column-blocks per the WiFi-family convention
    let (rate_num, rate_den) = rate.ratio();
    let k = n * rate_num / rate_den;
    let m = n - k;
    let m_blocks = m / z;
    let n_blocks = n / z;

    debug_assert_eq!(m % z, 0);
    debug_assert_eq!(n % z, 0);

    let seed = SEED_BASE
        ^ ((block_n as u64) << 8)
        ^ ((rate as u64) << 16);
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    // Construct the block-shift matrix: m_blocks × n_blocks entries.
    // Each entry is either None (zero block) or Some(shift) (shifted-identity block).
    // Target column weight 3 in expectation for the rate-1/2 case (matches MacKay
    // regular-LDPC design); scale by code rate for higher rates.
    let target_col_weight_per_blockcol: f32 = 3.0;
    let p_nonzero: f32 = target_col_weight_per_blockcol / (m_blocks as f32);

    let mut block_shifts: Vec<Vec<Option<usize>>> = vec![vec![None; n_blocks]; m_blocks];
    for r in 0..m_blocks {
        for c in 0..n_blocks {
            if rng.gen::<f32>() < p_nonzero {
                let shift = rng.gen_range(0..z);
                block_shifts[r][c] = Some(shift);
            }
        }
    }

    // Ensure each column-block has at least column weight 2 (degree-1 columns
    // cause decoder convergence problems). Repair by adding shifts in random
    // rows where this is violated.
    for c in 0..n_blocks {
        let weight = (0..m_blocks).filter(|&r| block_shifts[r][c].is_some()).count();
        for _ in weight..2 {
            // Find a row that doesn't already have a nonzero block in this column.
            for _ in 0..16 {
                let r = rng.gen_range(0..m_blocks);
                if block_shifts[r][c].is_none() {
                    block_shifts[r][c] = Some(rng.gen_range(0..z));
                    break;
                }
            }
        }
    }

    // Expand block_shifts into the row-list ParityCheckMatrix representation.
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); m];
    for r_block in 0..m_blocks {
        for c_block in 0..n_blocks {
            if let Some(shift) = block_shifts[r_block][c_block] {
                // Shifted-identity block: for each row i within the block,
                // there's a 1 at column ((i + shift) mod z) within the c_block-th column-block.
                for i in 0..z {
                    let global_row = r_block * z + i;
                    let global_col = c_block * z + ((i + shift) % z);
                    rows[global_row].push(global_col);
                }
            }
        }
    }
    for row in rows.iter_mut() {
        row.sort();
        row.dedup();
    }

    ParityCheckMatrix { n, k, rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_family_n648_r12_dimensions() {
        let h = build(BlockN::N648, CodeRate::R1_2);
        assert_eq!(h.n, 648);
        assert_eq!(h.k, 324);
        assert_eq!(h.rows.len(), 324);
    }

    #[test]
    fn wifi_family_n1296_r34_dimensions() {
        let h = build(BlockN::N1296, CodeRate::R3_4);
        assert_eq!(h.n, 1296);
        assert_eq!(h.k, 972);
        assert_eq!(h.rows.len(), 324);
    }

    #[test]
    fn wifi_family_no_degree_one_columns() {
        let h = build(BlockN::N648, CodeRate::R1_2);
        let mut col_weights = vec![0; h.n];
        for row in &h.rows {
            for &c in row {
                col_weights[c] += 1;
            }
        }
        for (c, w) in col_weights.iter().enumerate() {
            assert!(*w >= 2, "column {} has weight {} (must be >=2)", c, w);
        }
    }

    #[test]
    fn wifi_family_deterministic() {
        let h1 = build(BlockN::N648, CodeRate::R1_2);
        let h2 = build(BlockN::N648, CodeRate::R1_2);
        assert_eq!(h1.rows, h2.rows);
    }
}
```

- [ ] **Step 2: Run the WiFi-family tests.**

Run: `cargo test -p tuxmodem-fec --lib codes::ofdm_wifi_family`
Expected: All four tests PASS.

- [ ] **Step 3: Commit.**

```bash
git add crates/tuxmodem-fec/src/codes/ofdm_wifi_family.rs
git commit -m "feat(fec): WiFi-pattern rate-compatible LDPC family (n=648/1296, rates 1/2..5/6)

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 3.5: Wire up the codes/mod.rs dispatch

**File:** `crates/tuxmodem-fec/src/codes/mod.rs`

- [ ] **Step 1: Update `codes/mod.rs` to provide the dispatch function.**

```rust
//! LDPC code families. Each submodule constructs its parity-check matrix.

pub mod parity_matrix;
pub mod floor_rate14;
pub mod ofdm_wifi_family;

use crate::api::CodeFamily;
use parity_matrix::ParityCheckMatrix;

/// Construct the parity-check matrix for the requested code family.
pub fn build(family: CodeFamily) -> ParityCheckMatrix {
    match family {
        CodeFamily::FloorRate14 => floor_rate14::build(),
        CodeFamily::OfdmAdaptive { block_n, rate } =>
            ofdm_wifi_family::build(block_n, rate),
    }
}
```

- [ ] **Step 2: Make `codes` and `api` modules accessible.** Edit `lib.rs`:

```rust
pub mod api;
pub mod codes;
pub mod crc;
pub mod interleaver;
```

(rest remain private until Phase 6).

- [ ] **Step 3: Commit.**

```bash
git add crates/tuxmodem-fec/src/codes/mod.rs crates/tuxmodem-fec/src/lib.rs
git commit -m "feat(fec): codes::build() dispatch over CodeFamily enum

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 4: LDPC systematic encoder

**Files:**
- Create: `crates/tuxmodem-fec/src/encode.rs`

LDPC systematic encoding: given H and k information bits, produce n codeword bits where the first k bits equal the input (systematic form). Done via Gaussian elimination on H to derive the generator-equivalent encoding, then back-substitution for the parity bits.

### Task 4.1: Write the failing encoder unit test

- [ ] **Step 1: Add a unit test block at the end of `encode.rs` after the implementation.** First we sketch the test in a stub file, then implement.

**File:** `crates/tuxmodem-fec/src/encode.rs` (initial stub)

```rust
//! LDPC systematic encoder.
//!
//! Given an (n,k) LDPC code with parity-check matrix H, the encoder produces
//! a length-n codeword c from a length-k info word u such that:
//!   - c[0..k] == u  (systematic form)
//!   - H * c^T == 0  (codeword satisfies parity checks)
//!
//! Implementation strategy: precompute the parity-bit equations from H via
//! Gaussian elimination once at encoder-construction time, then per-encode
//! is a sequence of XORs.

use bitvec::prelude::*;
use crate::codes::parity_matrix::ParityCheckMatrix;

/// Cached encoder state for one code (one H matrix).
pub struct Encoder {
    n: usize,
    k: usize,
    /// parity_eqs[p] = list of info-bit indices that XOR to produce parity bit p.
    parity_eqs: Vec<Vec<usize>>,
}

impl Encoder {
    /// Build an encoder from H. Performs the Gaussian-elimination preprocessing.
    pub fn new(h: &ParityCheckMatrix) -> Self {
        let n = h.n;
        let k = h.k;
        let m = n - k;

        // Build a dense (m × n) matrix view from H (for elimination).
        // Memory: m*n bits. For n=2048, m=1536 that's ~400KB; for n=648, m=324 it's ~25KB.
        let mut dense: Vec<Vec<bool>> = vec![vec![false; n]; m];
        for (r, row) in h.rows.iter().enumerate() {
            for &c in row {
                dense[r][c] = true;
            }
        }

        // Gaussian elimination: bring the right-half (columns k..n) of dense to row-echelon form
        // so that parity bits can be solved by back-substitution from info bits.
        // We're working in GF(2), so XOR is addition + subtraction.
        for col in k..n {
            let pivot_row_offset = col - k;
            // Find a row >= pivot_row_offset with a 1 in column `col`. Swap into place.
            let mut pivot = None;
            for r in pivot_row_offset..m {
                if dense[r][col] {
                    pivot = Some(r);
                    break;
                }
            }
            let Some(p) = pivot else {
                // No pivot — this column is dependent on earlier ones. For a well-formed
                // LDPC code this should not happen; if it does we fall back to a different
                // pivot strategy (column swap), but for v0.5+ we panic-with-context.
                panic!("Encoder::new: no pivot in column {} during elimination — code H is rank-deficient", col);
            };
            if p != pivot_row_offset {
                dense.swap(p, pivot_row_offset);
            }
            // Eliminate this column in all other rows.
            for r in 0..m {
                if r != pivot_row_offset && dense[r][col] {
                    for c2 in 0..n {
                        dense[r][c2] ^= dense[pivot_row_offset][c2];
                    }
                }
            }
        }

        // After elimination, each row r (0..m) has a single 1 in column (k + r)
        // and zero in all other parity columns. The info-bit columns (0..k) give
        // the XOR equation: parity_bit_r = XOR of info_bit[c] where dense[r][c]==1 for c<k.
        let mut parity_eqs: Vec<Vec<usize>> = Vec::with_capacity(m);
        for r in 0..m {
            let mut eq: Vec<usize> = Vec::new();
            for c in 0..k {
                if dense[r][c] {
                    eq.push(c);
                }
            }
            parity_eqs.push(eq);
        }

        Self { n, k, parity_eqs }
    }

    pub fn n(&self) -> usize { self.n }
    pub fn k(&self) -> usize { self.k }

    /// Encode info bits into a codeword.
    pub fn encode(&self, info: &BitSlice<u8>) -> BitVec<u8> {
        assert_eq!(info.len(), self.k, "info bits length {} != k {}", info.len(), self.k);

        let mut codeword: BitVec<u8> = BitVec::with_capacity(self.n);
        // Systematic: first k bits are the info bits.
        for bit in info.iter() {
            codeword.push(*bit);
        }
        // Parity bits: each is the XOR of the indicated info bits.
        for eq in &self.parity_eqs {
            let parity: bool = eq.iter().fold(false, |acc, &i| acc ^ info[i]);
            codeword.push(parity);
        }
        debug_assert_eq!(codeword.len(), self.n);
        codeword
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;
    use crate::api::{CodeFamily, BlockN, CodeRate};

    #[test]
    fn encoded_codeword_satisfies_parity() {
        let h = codes::build(CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 });
        let enc = Encoder::new(&h);

        let info: BitVec<u8> = (0..enc.k()).map(|i| (i % 3) == 0).collect();
        let codeword = enc.encode(info.as_bitslice());

        let cw_bools: Vec<bool> = codeword.iter().map(|b| *b).collect();
        assert!(h.parity_check(&cw_bools), "codeword failed parity check");
    }

    #[test]
    fn encoded_codeword_is_systematic() {
        let h = codes::build(CodeFamily::OfdmAdaptive { block_n: BlockN::N1296, rate: CodeRate::R3_4 });
        let enc = Encoder::new(&h);

        let info: BitVec<u8> = (0..enc.k()).map(|i| (i % 5) == 0).collect();
        let codeword = enc.encode(info.as_bitslice());

        // First k bits of codeword == info.
        for i in 0..enc.k() {
            assert_eq!(codeword[i], info[i], "systematic bit {} mismatch", i);
        }
    }

    #[test]
    fn encoder_handles_floor_rate14() {
        let h = codes::build(CodeFamily::FloorRate14);
        let enc = Encoder::new(&h);
        assert_eq!(enc.k(), 512);
        assert_eq!(enc.n(), 2048);

        let info: BitVec<u8> = BitVec::repeat(false, 512);
        let codeword = enc.encode(info.as_bitslice());
        assert_eq!(codeword.len(), 2048);

        // All-zero info bits → all-zero codeword (LDPC always contains the zero codeword).
        for i in 0..2048 {
            assert_eq!(codeword[i], false);
        }
    }
}
```

- [ ] **Step 2: Make the `encode` module public.** In `lib.rs`, add `pub mod encode;`.

- [ ] **Step 3: Run the encoder unit tests.**

Run: `cargo test -p tuxmodem-fec --lib encode`
Expected: PASS all three tests.

Note: If the floor_rate14 code's H matrix happens to be rank-deficient (small probability with the random construction), the `Encoder::new` `panic` triggers. If this surfaces, the fix is in `codes/floor_rate14.rs`: change the SEED constant by ±1 until a rank-full matrix is found. This is acceptable v0.5+ behavior; a more robust column-swap fallback is a v0.6+ improvement.

- [ ] **Step 4: Commit.**

```bash
git add crates/tuxmodem-fec/src/encode.rs crates/tuxmodem-fec/src/lib.rs
git commit -m "feat(fec): LDPC systematic encoder via Gaussian-eliminated H

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 5: SPA belief-propagation decoder

**Files:**
- Create: `crates/tuxmodem-fec/src/decode.rs`
- Create: `crates/tuxmodem-fec/src/llr.rs`

The Sum-Product Algorithm (SPA) belief-propagation decoder is the canonical
near-optimal LDPC decoder. It iterates messages between variable nodes
(codeword bits) and check nodes (parity equations) on the Tanner graph of H.

Per Richardson-Urbanke 2001 (open) and standard LDPC literature, SPA in
log-likelihood-ratio form is numerically robust and fast. Implementation
strategy: precompute Tanner-graph adjacency once; iterate up to `max_iters`,
checking for convergence (all parity checks satisfied) at the end of each
iteration.

### Task 5.1: Write the LLR helper module

**File:** `crates/tuxmodem-fec/src/llr.rs`

- [ ] **Step 1: Write the LLR utilities.**

```rust
//! Log-likelihood-ratio (LLR) helpers.
//!
//! Convention: LLR(b) = log(P(b=0) / P(b=1)).
//! Sign positive → hard-decision 0.  Sign negative → hard-decision 1.
//! |LLR| is the confidence.

use crate::api::Llr;

/// Hard-decide an LLR into a bit value.
pub fn hard_decide(llr: Llr) -> bool {
    // Positive LLR → bit 0 (false); negative → bit 1 (true).
    llr.0 < 0.0
}

/// Box-plus operator: combine two LLRs as if they were independent observations
/// of the same XOR sum. Used in the check-node update.
///   boxplus(a, b) = 2 * atanh( tanh(a/2) * tanh(b/2) )
/// Numerically-stable form via the sign-and-min approximation is acceptable
/// for v0.5+; we use the exact tanh form here for clarity. Profile in Phase 8.
pub fn boxplus(a: f32, b: f32) -> f32 {
    let ta = (a / 2.0).tanh();
    let tb = (b / 2.0).tanh();
    let prod = (ta * tb).clamp(-0.999_999_94, 0.999_999_94);
    2.0 * prod.atanh()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard_decide_positive_llr_is_zero() {
        assert_eq!(hard_decide(Llr(2.5)), false);
    }

    #[test]
    fn hard_decide_negative_llr_is_one() {
        assert_eq!(hard_decide(Llr(-2.5)), true);
    }

    #[test]
    fn boxplus_identity_with_infinite_certain() {
        // boxplus with +infinity reduces to the other argument.
        let result = boxplus(1.0, 100.0);
        assert!((result - 1.0).abs() < 0.01, "boxplus with high-confidence preserves other: got {}", result);
    }

    #[test]
    fn boxplus_sign_xor() {
        // Box-plus of two same-sign LLRs is same-sign.
        assert!(boxplus(2.0, 3.0) > 0.0);
        assert!(boxplus(-2.0, -3.0) > 0.0);  // (-)(-) = +
        // Mixed-sign → negative.
        assert!(boxplus(2.0, -3.0) < 0.0);
    }
}
```

- [ ] **Step 2: Make `llr` module public** in `lib.rs`: `pub mod llr;`.

- [ ] **Step 3: Run the LLR unit tests.**

Run: `cargo test -p tuxmodem-fec --lib llr`
Expected: All four tests PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/tuxmodem-fec/src/llr.rs crates/tuxmodem-fec/src/lib.rs
git commit -m "feat(fec): LLR helpers (hard_decide, boxplus)

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 5.2: Implement the SPA decoder — happy path

**File:** `crates/tuxmodem-fec/src/decode.rs`

- [ ] **Step 1: Write `decode.rs`.**

```rust
//! Sum-product-algorithm (SPA) belief-propagation LDPC decoder.
//!
//! Implementation in log-likelihood-ratio (LLR) form for numerical stability.
//! Per Richardson-Urbanke 2001 and standard LDPC references.
//!
//! Iteration structure:
//!   1. Initialize variable-to-check messages = channel LLRs.
//!   2. Check-to-variable update: for each check node c, for each adjacent variable v,
//!      M_{c->v} = boxplus over all other variables adjacent to c of M_{v'->c}.
//!   3. Variable-to-check update: for each variable v, for each adjacent check c,
//!      M_{v->c} = channel_llr[v] + sum over all other checks adjacent to v of M_{c'->v}.
//!   4. Posterior: posterior_llr[v] = channel_llr[v] + sum over all checks adjacent to v of M_{c->v}.
//!   5. Hard-decide; check if all parity checks pass; if yes, converged.

use crate::api::Llr;
use crate::codes::parity_matrix::ParityCheckMatrix;
use crate::llr::{boxplus, hard_decide};

pub struct Decoder {
    n: usize,
    k: usize,
    /// var_to_checks[v] = list of check-node indices adjacent to variable v.
    var_to_checks: Vec<Vec<usize>>,
    /// check_to_vars[c] = list of variable-node indices adjacent to check c (= h.rows[c]).
    check_to_vars: Vec<Vec<usize>>,
}

impl Decoder {
    pub fn new(h: &ParityCheckMatrix) -> Self {
        let n = h.n;
        let k = h.k;
        let m = n - k;

        // check_to_vars is just h.rows.
        let check_to_vars: Vec<Vec<usize>> = h.rows.clone();

        // Build the transposed adjacency: var_to_checks.
        let mut var_to_checks: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (c, row) in h.rows.iter().enumerate() {
            for &v in row {
                var_to_checks[v].push(c);
            }
        }

        Self { n, k, var_to_checks, check_to_vars }
    }

    pub fn n(&self) -> usize { self.n }
    pub fn k(&self) -> usize { self.k }

    /// Decode LLRs into hard-decision codeword bits.
    /// Returns `(decoded_bits, iterations_used, converged)`.
    pub fn decode(&self, llrs: &[Llr], max_iters: u32) -> DecodeOutcome {
        assert_eq!(llrs.len(), self.n, "llrs length {} != n {}", llrs.len(), self.n);

        let n = self.n;
        let channel: Vec<f32> = llrs.iter().map(|l| l.0).collect();

        // Per-edge messages. Indexed: for variable v and adjacency-index j (into var_to_checks[v]),
        // msg_v_to_c[v][j] is the message FROM v TO check var_to_checks[v][j].
        // Symmetrically, msg_c_to_v[c][i] is the message FROM check c TO variable check_to_vars[c][i].
        let mut msg_v_to_c: Vec<Vec<f32>> = self.var_to_checks.iter()
            .enumerate()
            .map(|(v, checks)| vec![channel[v]; checks.len()])
            .collect();
        let mut msg_c_to_v: Vec<Vec<f32>> = self.check_to_vars.iter()
            .map(|vars| vec![0.0; vars.len()])
            .collect();

        // To efficiently locate the (c -> v) index from the (v -> c) message: for each (v, j),
        // edge_inverse[v][j] = the i such that check_to_vars[var_to_checks[v][j]][i] == v.
        let mut edge_inverse: Vec<Vec<usize>> = vec![Vec::new(); n];
        for v in 0..n {
            edge_inverse[v] = self.var_to_checks[v].iter().map(|&c| {
                self.check_to_vars[c].iter().position(|&v2| v2 == v)
                    .expect("inconsistent Tanner graph: v not in check_to_vars[c]")
            }).collect();
        }

        let mut converged = false;
        let mut iter_count: u32 = 0;
        let mut decoded: Vec<bool> = vec![false; n];

        for iter in 0..max_iters {
            iter_count = iter + 1;

            // Check-to-variable update: for each check c, for each adjacent v at position i,
            // M_{c->v} = boxplus over all i' != i of msg_v_to_c at the (v', c) edge.
            for c in 0..self.check_to_vars.len() {
                let vars = &self.check_to_vars[c];
                // Collect incoming v-to-c messages for this check.
                let incoming: Vec<f32> = vars.iter().enumerate().map(|(i, &v)| {
                    // Find the (v, j) edge whose target is c.
                    let j = self.var_to_checks[v].iter().position(|&c2| c2 == c).unwrap();
                    msg_v_to_c[v][j]
                }).collect();

                for i in 0..vars.len() {
                    // boxplus of all incoming except index i.
                    let mut acc = f32::INFINITY;
                    for (i2, &m) in incoming.iter().enumerate() {
                        if i2 != i {
                            if acc.is_infinite() {
                                acc = m;
                            } else {
                                acc = boxplus(acc, m);
                            }
                        }
                    }
                    if acc.is_infinite() {
                        acc = 0.0; // degenerate row with only one variable; outgoing is 0
                    }
                    msg_c_to_v[c][i] = acc;
                }
            }

            // Variable-to-check update: for each variable v, for each adjacent check at position j,
            // M_{v->c} = channel[v] + sum over j' != j of msg_c_to_v at the (c', v) edge.
            for v in 0..n {
                let checks = &self.var_to_checks[v];
                let incoming: Vec<f32> = checks.iter().enumerate().map(|(j, &c)| {
                    let i = edge_inverse[v][j];
                    msg_c_to_v[c][i]
                }).collect();

                let total_sum: f32 = incoming.iter().sum();

                for j in 0..checks.len() {
                    msg_v_to_c[v][j] = channel[v] + total_sum - incoming[j];
                }
            }

            // Posterior + hard decision.
            for v in 0..n {
                let post: f32 = channel[v]
                    + self.var_to_checks[v].iter().enumerate()
                        .map(|(j, &c)| {
                            let i = edge_inverse[v][j];
                            msg_c_to_v[c][i]
                        })
                        .sum::<f32>();
                decoded[v] = post < 0.0;
            }

            // Check convergence: all parity checks satisfied?
            let all_satisfied = self.check_to_vars.iter().all(|vars| {
                vars.iter().fold(false, |acc, &v| acc ^ decoded[v]) == false
            });
            if all_satisfied {
                converged = true;
                break;
            }
        }

        DecodeOutcome {
            decoded,
            iterations_used: iter_count,
            converged,
        }
    }
}

pub struct DecodeOutcome {
    pub decoded: Vec<bool>,
    pub iterations_used: u32,
    pub converged: bool,
}
```

- [ ] **Step 2: Add `pub mod decode;` to `lib.rs`.**

- [ ] **Step 3: Run cargo check to verify compilation.**

Run: `cargo check -p tuxmodem-fec`
Expected: PASS (with possible warnings on unused `k` field; ignore for now).

- [ ] **Step 4: Commit.**

```bash
git add crates/tuxmodem-fec/src/decode.rs crates/tuxmodem-fec/src/lib.rs
git commit -m "feat(fec): SPA belief-propagation LDPC decoder (LLR-form)

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 5.3: Encode + decode roundtrip test (no channel noise)

- [ ] **Step 1: Write a unit test for the encode/decode roundtrip with zero channel noise.**

Append to the existing `#[cfg(test)] mod tests` in `decode.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{CodeFamily, BlockN, CodeRate};
    use crate::codes;
    use crate::encode::Encoder;
    use bitvec::prelude::*;

    fn codeword_to_llrs(codeword: &BitSlice<u8>, certainty: f32) -> Vec<Llr> {
        // BPSK-mapping: bit 0 → +certainty (positive LLR = bit 0); bit 1 → -certainty.
        codeword.iter().map(|b| Llr(if *b { -certainty } else { certainty })).collect()
    }

    #[test]
    fn decode_zero_noise_returns_input() {
        let h = codes::build(CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 });
        let enc = Encoder::new(&h);
        let dec = Decoder::new(&h);

        let info: BitVec<u8> = (0..enc.k()).map(|i| (i % 7) == 0).collect();
        let codeword = enc.encode(info.as_bitslice());

        // Pass the codeword through a noiseless channel: positive LLR for 0, negative for 1.
        let llrs = codeword_to_llrs(codeword.as_bitslice(), 10.0);

        let outcome = dec.decode(&llrs, 50);

        assert!(outcome.converged, "decoder did not converge in zero noise");
        assert_eq!(outcome.decoded.len(), enc.n());
        for i in 0..enc.k() {
            assert_eq!(outcome.decoded[i], info[i], "bit {} mismatch after zero-noise decode", i);
        }
    }

    #[test]
    fn decode_one_bit_flip_recovers() {
        let h = codes::build(CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 });
        let enc = Encoder::new(&h);
        let dec = Decoder::new(&h);

        let info: BitVec<u8> = (0..enc.k()).map(|i| (i % 5) == 0).collect();
        let codeword = enc.encode(info.as_bitslice());

        // Flip one bit in the LLR sequence.
        let mut llrs = codeword_to_llrs(codeword.as_bitslice(), 5.0);
        llrs[42].0 = -llrs[42].0; // hard flip with original confidence

        let outcome = dec.decode(&llrs, 50);

        assert!(outcome.converged, "decoder did not converge on single bit flip");
        for i in 0..enc.k() {
            assert_eq!(outcome.decoded[i], info[i], "bit {} mismatch after single flip recovery", i);
        }
    }
}
```

- [ ] **Step 2: Run the tests.**

Run: `cargo test -p tuxmodem-fec --lib decode`
Expected: Both PASS.

- [ ] **Step 3: Commit.**

```bash
git add crates/tuxmodem-fec/src/decode.rs
git commit -m "test(fec): SPA decoder zero-noise + single-flip recovery tests

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 6: Public API (FecEncoder / FecDecoder traits) + roundtrip

**Files:**
- Modify: `crates/tuxmodem-fec/src/api.rs` (add traits + impls)
- Create: `crates/tuxmodem-fec/src/stats.rs`
- Create: `crates/tuxmodem-fec/src/puncture.rs` (stub for v0.6+ HARQ hook)
- Create: `crates/tuxmodem-fec/tests/api_contract.rs`

This phase composes the encoder + interleaver + CRC + LDPC + decoder into the
public `FecEncoder` / `FecDecoder` traits and concrete implementations. The
CRC-32 is prepended to the info bits BEFORE LDPC encoding; bit interleaver
applied AFTER LDPC encoding. Reverse on decode.

### Task 6.1: Implement `stats.rs` and `puncture.rs` stubs

**File:** `crates/tuxmodem-fec/src/stats.rs`

- [ ] **Step 1: Write `stats.rs`.**

```rust
//! Residual-error statistics surfaced from FEC up through MAC to ARQ.
//!
//! These are the FEC layer's contract to subsystem #6 (ARQ): per-block,
//! did the block decode cleanly? How many iterations? Confidence?

use crate::api::BlockDecodeStats;

#[derive(Clone, Debug)]
pub struct ResidualErrorStats {
    /// Did this block decode cleanly (CRC passed)?
    pub block_ok: bool,
    /// SPA iteration count used. ARQ may surface this as a channel-quality hint.
    pub iterations: u32,
    /// LLR-magnitude-derived confidence. None if the decoder doesn't compute it.
    pub confidence_score: Option<f32>,
    /// Time-since-frame-start for ARQ retransmission-timer accounting (filled in by MAC).
    pub frame_timestamp: Option<std::time::Instant>,
}

impl From<&BlockDecodeStats> for ResidualErrorStats {
    fn from(s: &BlockDecodeStats) -> Self {
        Self {
            block_ok: s.crc_ok,
            iterations: s.iterations_used,
            confidence_score: s.estimated_post_decode_ber,
            frame_timestamp: None,
        }
    }
}
```

**File:** `crates/tuxmodem-fec/src/puncture.rs`

- [ ] **Step 2: Write `puncture.rs` v0.5+ stub.**

```rust
//! Rate-compatible puncturing patterns.
//!
//! Reserved for v0.6+ HARQ (Type-II / Type-III incremental redundancy). v0.5+
//! exposes this module as a hook only — the implementation is `todo!()`. ARQ
//! (subsystem #6) in v0.5+ uses Type-I HARQ (FEC redundancy stays fixed across
//! retransmissions, ARQ is plain selective-repeat); puncturing is not on the
//! v0.5+ critical path.
//!
//! See FEC plan §E.5 (watched failure mode: HARQ scope creep).

#[allow(dead_code)]
pub fn puncture_to_rate(_codeword: &bitvec::prelude::BitSlice<u8>, _target_rate: crate::api::CodeRate) -> bitvec::prelude::BitVec<u8> {
    todo!("v0.6+ HARQ: puncture a rate-1/2 codeword down to a higher rate")
}
```

- [ ] **Step 3: Make `stats` and `puncture` modules public** in `lib.rs`:

```rust
pub mod stats;
pub mod puncture;
```

- [ ] **Step 4: Run `cargo check -p tuxmodem-fec`.**

Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add crates/tuxmodem-fec/src/stats.rs crates/tuxmodem-fec/src/puncture.rs crates/tuxmodem-fec/src/lib.rs
git commit -m "feat(fec): stats type + v0.6+ puncture hook stub

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 6.2: Add the trait declarations to api.rs and the concrete impls

- [ ] **Step 1: Append to `crates/tuxmodem-fec/src/api.rs`.**

```rust
// ---- Trait declarations and concrete impls. ----

use bitvec::prelude::*;
use crate::codes::{self, parity_matrix::ParityCheckMatrix};
use crate::crc::{append_crc32, verify_crc32};
use crate::encode::Encoder as LdpcInternalEncoder;
use crate::decode::Decoder as LdpcInternalDecoder;
use crate::interleaver::{interleave, deinterleave};
use std::collections::HashMap;

/// Encoder side of the public FEC API.
pub trait FecEncoder {
    /// Encode k info bits into an n-bit codeword (including CRC + interleaving).
    /// Returns the codeword ready for the PHY to modulate.
    fn encode(&self, family: CodeFamily, info_bits: &BitSlice<u8>) -> Result<BitVec<u8>, FecError>;

    /// Return (n, k) for the chosen code.
    ///
    /// IMPORTANT: `k` reported here is the **payload k** (the size of `info_bits`
    /// the caller hands to `encode`), NOT the underlying LDPC code's k. The LDPC
    /// code's k includes 32 CRC bits prepended internally. Payload k = LDPC k - 32.
    fn block_size(&self, family: CodeFamily) -> (usize, usize);
}

/// Decoder side of the public FEC API.
pub trait FecDecoder {
    /// Decode n LLRs into k info bits. Returns Err if the codeword fails CRC.
    fn decode(&self, family: CodeFamily, llrs: &[Llr]) -> Result<DecodedBlock, FecError>;
}

/// Concrete implementation of `FecEncoder`. Caches the parity-check matrices
/// and the encoder objects per code family for performance.
pub struct LdpcEncoder {
    cache: std::sync::Mutex<HashMap<CodeFamily, std::sync::Arc<LdpcInternalEncoder>>>,
    h_cache: std::sync::Mutex<HashMap<CodeFamily, std::sync::Arc<ParityCheckMatrix>>>,
    interleaver_rows: usize,
}

impl LdpcEncoder {
    pub fn new() -> Self {
        Self {
            cache: Default::default(),
            h_cache: Default::default(),
            interleaver_rows: 16, // Block-interleaver default depth; see plan §E.4
        }
    }

    fn get_or_build(&self, family: CodeFamily) -> (std::sync::Arc<ParityCheckMatrix>, std::sync::Arc<LdpcInternalEncoder>) {
        let mut h_cache = self.h_cache.lock().unwrap();
        let h = h_cache.entry(family).or_insert_with(|| std::sync::Arc::new(codes::build(family))).clone();
        drop(h_cache);

        let mut enc_cache = self.cache.lock().unwrap();
        let enc = enc_cache.entry(family).or_insert_with(|| std::sync::Arc::new(LdpcInternalEncoder::new(&h))).clone();

        (h, enc)
    }
}

impl Default for LdpcEncoder {
    fn default() -> Self { Self::new() }
}

impl FecEncoder for LdpcEncoder {
    fn encode(&self, family: CodeFamily, info_bits: &BitSlice<u8>) -> Result<BitVec<u8>, FecError> {
        let (_h, enc) = self.get_or_build(family);
        let ldpc_k = enc.k();
        let payload_k = ldpc_k - 32;

        if info_bits.len() != payload_k {
            return Err(FecError::InvalidInputLength { expected: payload_k, got: info_bits.len() });
        }

        // Step 1: append CRC-32 to info bits.
        let with_crc = append_crc32(info_bits);
        debug_assert_eq!(with_crc.len(), ldpc_k);

        // Step 2: LDPC encode.
        let codeword = enc.encode(with_crc.as_bitslice());

        // Step 3: bit-interleave to decorrelate burst errors.
        let interleaved = interleave(codeword.as_bitslice(), self.interleaver_rows);

        Ok(interleaved)
    }

    fn block_size(&self, family: CodeFamily) -> (usize, usize) {
        let (_h, enc) = self.get_or_build(family);
        (enc.n(), enc.k() - 32)  // payload k excludes the 32-bit CRC
    }
}

/// Concrete implementation of `FecDecoder`.
pub struct LdpcDecoder {
    cache: std::sync::Mutex<HashMap<CodeFamily, std::sync::Arc<LdpcInternalDecoder>>>,
    h_cache: std::sync::Mutex<HashMap<CodeFamily, std::sync::Arc<ParityCheckMatrix>>>,
    interleaver_rows: usize,
    max_iters_ofdm: u32,
    max_iters_floor: u32,
}

impl LdpcDecoder {
    pub fn new() -> Self {
        Self {
            cache: Default::default(),
            h_cache: Default::default(),
            interleaver_rows: 16,
            max_iters_ofdm: 50,
            max_iters_floor: 100,
        }
    }

    fn get_or_build(&self, family: CodeFamily) -> (std::sync::Arc<ParityCheckMatrix>, std::sync::Arc<LdpcInternalDecoder>) {
        let mut h_cache = self.h_cache.lock().unwrap();
        let h = h_cache.entry(family).or_insert_with(|| std::sync::Arc::new(codes::build(family))).clone();
        drop(h_cache);

        let mut dec_cache = self.cache.lock().unwrap();
        let dec = dec_cache.entry(family).or_insert_with(|| std::sync::Arc::new(LdpcInternalDecoder::new(&h))).clone();

        (h, dec)
    }

    fn max_iters(&self, family: CodeFamily) -> u32 {
        match family {
            CodeFamily::FloorRate14 => self.max_iters_floor,
            CodeFamily::OfdmAdaptive { .. } => self.max_iters_ofdm,
        }
    }
}

impl Default for LdpcDecoder {
    fn default() -> Self { Self::new() }
}

impl FecDecoder for LdpcDecoder {
    fn decode(&self, family: CodeFamily, llrs: &[Llr]) -> Result<DecodedBlock, FecError> {
        let (_h, dec) = self.get_or_build(family);
        let ldpc_n = dec.n();

        if llrs.len() != ldpc_n {
            return Err(FecError::InvalidInputLength { expected: ldpc_n, got: llrs.len() });
        }

        // Step 1: de-interleave.
        let llrs_signs_as_bits: BitVec<u8> = llrs.iter().map(|l| l.0 < 0.0).collect();
        // For LLRs we de-interleave the LLR sequence by inverting the bit-interleaver
        // pattern on indices, then applying to the LLR array.
        let deint_llrs = deinterleave_llrs(llrs, self.interleaver_rows);
        let _ = llrs_signs_as_bits;

        // Step 2: SPA decode.
        let max_iters = self.max_iters(family);
        let outcome = dec.decode(&deint_llrs, max_iters);

        // Step 3: extract info+crc (first ldpc_k bits) and verify CRC.
        let ldpc_k = dec.k();
        let info_plus_crc: BitVec<u8> = outcome.decoded[..ldpc_k].iter().copied().collect();
        let crc_ok = verify_crc32(info_plus_crc.as_bitslice()).is_ok();

        let stats = BlockDecodeStats {
            iterations_used: outcome.iterations_used,
            converged: outcome.converged,
            crc_ok,
            estimated_post_decode_ber: None,
        };

        if !crc_ok {
            return Err(FecError::CrcFail { iterations_used: outcome.iterations_used });
        }

        // Strip the CRC tail; return the payload bits.
        let payload_k = ldpc_k - 32;
        let info_bits: BitVec<u8> = info_plus_crc[..payload_k].iter().copied().collect();

        Ok(DecodedBlock { info_bits, stats })
    }
}

/// De-interleave an LLR sequence using the same row count as the encoder's interleaver.
fn deinterleave_llrs(llrs: &[Llr], rows: usize) -> Vec<Llr> {
    let n = llrs.len();
    let cols = (n + rows - 1) / rows;
    // Inverse of `interleaver::interleave`: write column-by-column into a matrix,
    // read row-by-row.
    let mut matrix: Vec<Llr> = vec![Llr(0.0); rows * cols];
    let mut idx = 0;
    for col in 0..cols {
        for row in 0..rows {
            if idx < n {
                matrix[row * cols + col] = llrs[idx];
                idx += 1;
            }
        }
    }
    let mut out: Vec<Llr> = Vec::with_capacity(n);
    for i in 0..n {
        out.push(matrix[i]);
    }
    out
}
```

- [ ] **Step 2: Update `lib.rs` to re-export the encoder/decoder concrete types.**

```rust
pub use api::{
    BlockDecodeStats, BlockN, CodeFamily, CodeRate, DecodedBlock, FecDecoder,
    FecEncoder, FecError, LdpcDecoder, LdpcEncoder, Llr,
};
```

- [ ] **Step 3: Run `cargo check -p tuxmodem-fec`.**

Expected: PASS.

- [ ] **Step 4: Commit.**

```bash
git add crates/tuxmodem-fec/src/api.rs crates/tuxmodem-fec/src/lib.rs
git commit -m "feat(fec): FecEncoder + FecDecoder traits and LDPC concrete impls

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 6.3: API-contract roundtrip integration test

**File:** `crates/tuxmodem-fec/tests/api_contract.rs`

- [ ] **Step 1: Write the test.**

```rust
//! End-to-end FecEncoder + FecDecoder roundtrip via the public API.
//! Zero channel noise: must always round-trip exactly.

use bitvec::prelude::*;
use tuxmodem_fec::{
    BlockN, CodeFamily, CodeRate, FecDecoder, FecEncoder, LdpcDecoder, LdpcEncoder, Llr,
};

fn codeword_to_llrs(codeword: &BitSlice<u8>, certainty: f32) -> Vec<Llr> {
    codeword.iter().map(|b| Llr(if *b { -certainty } else { certainty })).collect()
}

#[test]
fn roundtrip_ofdm_n648_r12() {
    let enc = LdpcEncoder::new();
    let dec = LdpcDecoder::new();

    let family = CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 };
    let (_n, payload_k) = enc.block_size(family);

    let info: BitVec<u8> = (0..payload_k).map(|i| (i % 3) == 0).collect();
    let codeword = enc.encode(family, info.as_bitslice()).expect("encode");
    let llrs = codeword_to_llrs(codeword.as_bitslice(), 10.0);
    let decoded = dec.decode(family, &llrs).expect("decode");

    assert!(decoded.stats.crc_ok);
    assert!(decoded.stats.converged);
    assert_eq!(decoded.info_bits.len(), payload_k);
    for i in 0..payload_k {
        assert_eq!(decoded.info_bits[i], info[i], "bit {} mismatch", i);
    }
}

#[test]
fn roundtrip_floor_rate14() {
    let enc = LdpcEncoder::new();
    let dec = LdpcDecoder::new();

    let family = CodeFamily::FloorRate14;
    let (_n, payload_k) = enc.block_size(family);
    assert_eq!(payload_k, 512 - 32);

    let info: BitVec<u8> = (0..payload_k).map(|i| (i % 5) == 0).collect();
    let codeword = enc.encode(family, info.as_bitslice()).expect("encode");
    let llrs = codeword_to_llrs(codeword.as_bitslice(), 10.0);
    let decoded = dec.decode(family, &llrs).expect("decode");

    assert!(decoded.stats.crc_ok);
    assert_eq!(decoded.info_bits.len(), payload_k);
    for i in 0..payload_k {
        assert_eq!(decoded.info_bits[i], info[i], "bit {} mismatch", i);
    }
}

#[test]
fn wrong_length_input_errors() {
    let enc = LdpcEncoder::new();
    let family = CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 };
    let (_n, payload_k) = enc.block_size(family);

    let too_short: BitVec<u8> = BitVec::repeat(false, payload_k - 1);
    let err = enc.encode(family, too_short.as_bitslice()).unwrap_err();
    match err {
        tuxmodem_fec::FecError::InvalidInputLength { expected, got } => {
            assert_eq!(expected, payload_k);
            assert_eq!(got, payload_k - 1);
        }
        _ => panic!("wrong error variant"),
    }
}

#[test]
fn crc_fail_when_payload_is_corrupted_under_decode() {
    // Encode a payload, modify LLRs to flip enough bits that LDPC converges on
    // a wrong-but-valid codeword. With CRC, this should be caught (CRC_FAIL).
    // With small flips, the decoder corrects and we get the right answer
    // (covered by the roundtrip test). With LARGE flips, the decoder may
    // converge on a different codeword — CRC catches that case.

    let enc = LdpcEncoder::new();
    let dec = LdpcDecoder::new();
    let family = CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 };

    let (_n, payload_k) = enc.block_size(family);
    let info: BitVec<u8> = (0..payload_k).map(|i| (i % 3) == 0).collect();
    let codeword = enc.encode(family, info.as_bitslice()).expect("encode");

    // Adversarial corruption: flip the LLR sign on a large fraction of bits.
    // This forces the decoder either to fail max-iter or to find a wrong codeword.
    let mut llrs = codeword_to_llrs(codeword.as_bitslice(), 0.5);  // low confidence
    for i in 0..llrs.len() / 3 {
        llrs[i].0 = -llrs[i].0;
    }

    let result = dec.decode(family, &llrs);
    // Outcomes acceptable:
    //   - Err(CrcFail): decoder found a wrong codeword; CRC caught it.
    //   - Err(MaxIterationsExceeded): decoder didn't converge.
    //   - Ok(decoded) AND decoded.info_bits != info: this would be a silent corruption.
    //     Assert this case doesn't happen.
    match result {
        Err(_) => {} // good: either CRC or max-iter caught the failure
        Ok(decoded) => {
            // If we did decode, we MUST get the original info bits back (CRC would
            // have caught a wrong-codeword case).
            assert_eq!(decoded.info_bits, info, "silent corruption: decoded did not match original");
        }
    }
}
```

- [ ] **Step 2: Run the API-contract tests.**

Run: `cargo test -p tuxmodem-fec --test api_contract`
Expected: All four tests PASS.

- [ ] **Step 3: Commit.**

```bash
git add crates/tuxmodem-fec/tests/api_contract.rs
git commit -m "test(fec): FecEncoder/FecDecoder end-to-end roundtrip + error contract

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 7: Channel-simulator-coupled BER-vs-SNR tests

**Files:**
- Create: `crates/tuxmodem-fec/tests/awgn_bers.rs`
- Create: `crates/tuxmodem-fec/tests/itu_f520_good.rs`
- Create: `crates/tuxmodem-fec/tests/itu_f520_moderate.rs`
- Create: `crates/tuxmodem-fec/tests/itu_f520_poor.rs`
- Create: `crates/tuxmodem-fec/tests/itu_f520_flutter.rs`
- Create: `crates/tuxmodem-fec/examples/ber_curve.rs`

This phase couples the FEC to the channel simulator (#1) and validates against
the multi-axis success criteria in §D.

**Dependency:** the `hf-channel-sim` crate (subsystem #1) must already exist
and expose an API approximately like:

```rust
pub struct Watterson { /* ... */ }
impl Watterson {
    pub fn new(condition: ItuF520Condition, seed: u64) -> Self;
    pub fn apply(&mut self, input_iq: &[Complex32], snr_db: f32) -> Vec<Complex32>;
}
pub enum ItuF520Condition { Good, Moderate, Poor, Flutter }
```

If subsystem #1's plan diverges from this API, this phase's test code is the
caller-side adapter and updates accordingly.

### Task 7.1: AWGN baseline BER curve test

**File:** `crates/tuxmodem-fec/tests/awgn_bers.rs`

This is the "sanity baseline" — modern LDPC at rate 1/2 should deliver near-Shannon performance under AWGN. We expect BER ~10⁻⁵ at Eb/N0 around 2 dB for a well-constructed rate-1/2 LDPC.

- [ ] **Step 1: Write the AWGN test.**

```rust
//! AWGN baseline BER-vs-SNR sanity test.
//! Not gated on a specific BER target; verifies the decoder is working at all
//! under the simplest channel model.

use bitvec::prelude::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use tuxmodem_fec::{
    BlockN, CodeFamily, CodeRate, FecDecoder, FecEncoder, LdpcDecoder, LdpcEncoder, Llr,
};

/// BPSK over AWGN: add zero-mean Gaussian noise with std-dev sigma to each
/// codeword bit's BPSK-mapped amplitude (+1 / -1). Return per-bit LLRs.
fn awgn_channel(codeword: &BitSlice<u8>, ebn0_db: f32, rng: &mut ChaCha8Rng) -> Vec<Llr> {
    let ebn0 = 10f32.powf(ebn0_db / 10.0);
    let sigma = (1.0 / (2.0 * ebn0)).sqrt();
    codeword.iter().map(|b| {
        let amp: f32 = if *b { -1.0 } else { 1.0 };
        // Box-Muller; ChaCha8Rng provides uniform.
        let u1: f32 = rng.gen::<f32>().max(1e-9);
        let u2: f32 = rng.gen::<f32>();
        let noise = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos() * sigma;
        let received = amp + noise;
        // LLR for BPSK-AWGN: 2*received / sigma^2.
        Llr(2.0 * received / (sigma * sigma))
    }).collect()
}

#[test]
#[ignore] // long-running; run with `cargo test --release -- --ignored`
fn awgn_ber_curve_n648_r12() {
    let enc = LdpcEncoder::new();
    let dec = LdpcDecoder::new();
    let family = CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 };
    let (_n, payload_k) = enc.block_size(family);

    let mut rng = ChaCha8Rng::seed_from_u64(0x_AB_AB_FEC_AW_GN_u64);

    let mut all_block_ok = true;
    for ebn0_db_int in [1, 2, 3] {
        let ebn0_db = ebn0_db_int as f32;
        let trials = 100;
        let mut errors = 0u64;
        let mut total = 0u64;

        for _ in 0..trials {
            let info: BitVec<u8> = (0..payload_k).map(|_| rng.gen::<bool>()).collect();
            let codeword = enc.encode(family, info.as_bitslice()).expect("encode");
            let llrs = awgn_channel(codeword.as_bitslice(), ebn0_db, &mut rng);
            match dec.decode(family, &llrs) {
                Ok(decoded) => {
                    for i in 0..payload_k {
                        if decoded.info_bits[i] != info[i] { errors += 1; }
                    }
                }
                Err(_) => {
                    errors += payload_k as u64; // worst case: count as full block
                }
            }
            total += payload_k as u64;
        }

        let ber = errors as f64 / total as f64;
        eprintln!("AWGN Eb/N0={} dB: BER={:.6} ({}/{} bits)", ebn0_db, ber, errors, total);

        // Sanity gate at Eb/N0 = 3 dB: BER must be < 1e-2 (this is a loose sanity
        // floor; real near-Shannon performance is much better).
        if ebn0_db_int == 3 && ber > 1e-2 {
            all_block_ok = false;
        }
    }
    assert!(all_block_ok, "AWGN sanity gate failed: BER at 3 dB Eb/N0 too high");
}
```

- [ ] **Step 2: Run with `--ignored` to verify.**

Run: `cargo test --release -p tuxmodem-fec --test awgn_bers -- --ignored`
Expected: PASS; eprintln shows decreasing BER as Eb/N0 increases.

- [ ] **Step 3: Commit.**

```bash
git add crates/tuxmodem-fec/tests/awgn_bers.rs
git commit -m "test(fec): AWGN baseline BER-vs-SNR sanity test (ignored by default)

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 7.2: ITU-R F.520 channel tests (one file per condition)

For each ITU-R F.520 condition (good, moderate, poor, flutter), create a test
that runs a BER curve via the channel simulator. The criterion for "pass" is
per §D's multi-axis success criteria.

**File:** `crates/tuxmodem-fec/tests/itu_f520_moderate.rs` (the gated one)

- [ ] **Step 1: Write the moderate-channel test.**

```rust
//! ITU-R F.520 "moderate" channel BER test.
//!
//! GATE: at the SNR where uncoded BPSK gives BER ~10^-2, rate-1/2 OFDM-family
//! LDPC must give post-decode BER ≤ 10^-5 (per FEC plan §D.1).

use bitvec::prelude::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use tuxmodem_fec::{
    BlockN, CodeFamily, CodeRate, FecDecoder, FecEncoder, LdpcDecoder, LdpcEncoder, Llr,
};

// Stand-in for the hf-channel-sim API. When subsystem #1's crate API is finalized,
// replace this with `use hf_channel_sim::{Watterson, ItuF520Condition};`.
// For v0.5+ stub: this test is marked #[ignore] until the channel-sim crate exists.

#[test]
#[ignore = "requires hf-channel-sim crate (subsystem #1) to be implemented"]
fn itu_f520_moderate_gate() {
    // Channel sim sketch (real API once #1 lands):
    //
    //   use hf_channel_sim::{Watterson, ItuF520Condition};
    //   let mut chan = Watterson::new(ItuF520Condition::Moderate, seed=0xDEAD_BEEF);
    //   let received_iq = chan.apply(&tx_iq, snr_db);
    //
    // For each SNR in a sweep (e.g., 0..15 dB):
    //   1. Encode random info bits → codeword.
    //   2. BPSK-modulate codeword → tx I/Q at the modem sample rate.
    //   3. Pass tx_iq through Watterson channel.
    //   4. Soft-demodulate received I/Q → LLRs.
    //   5. Decode LLRs → decoded info bits.
    //   6. Count bit errors.
    //
    // Find the SNR where uncoded BPSK gives BER ~ 10^-2 (call it SNR_ref).
    // At SNR_ref + 0 dB (the coded-vs-uncoded comparison point), assert:
    //   coded_ber <= 1e-5.

    panic!("placeholder: implement once hf-channel-sim crate (subsystem #1) is available");
}
```

- [ ] **Step 2: Replicate the same pattern for `itu_f520_good.rs`, `itu_f520_poor.rs`, `itu_f520_flutter.rs`** with the appropriate condition + gate. The `poor` test is gated on FEC plan §D.2 (floor-mode + -5 dB SNR + post-decode BER ≤ 10⁻⁴ in 80%+ runs).

```rust
// itu_f520_poor.rs (key parts only — the rest is structural)

#[test]
#[ignore = "requires hf-channel-sim crate"]
fn itu_f520_poor_floor_mode_gate() {
    let family = CodeFamily::FloorRate14;
    let snr_db_target = -5.0; // per FEC plan §D.2
    let success_threshold = 0.80; // 80%+ runs must achieve BER <= 1e-4
    let ber_target = 1e-4;
    // ... (channel-sim stub as above; gate as documented)
    panic!("placeholder");
}
```

- [ ] **Step 3: Commit the placeholder tests.**

```bash
git add crates/tuxmodem-fec/tests/itu_f520_*.rs
git commit -m "test(fec): ITU-R F.520 channel BER gates (placeholder until #1 lands)

Tests are #[ignore]d; they panic with explanatory messages until the
hf-channel-sim crate (subsystem #1) is implemented. Gates encode the
multi-axis success criteria from plan §D.

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 7.3: BER-curve example CLI

**File:** `crates/tuxmodem-fec/examples/ber_curve.rs`

A simple CLI that produces a BER-vs-SNR curve for one (code family, channel condition) pair. Useful for ad-hoc tuning.

- [ ] **Step 1: Write the example.**

```rust
//! BER-vs-SNR curve generator.
//!
//! Usage:
//!   cargo run --release --example ber_curve -- --family floor --snr-start -10 --snr-end 0 --snr-step 1 --trials 100
//!   cargo run --release --example ber_curve -- --family ofdm-r12-n648 --snr-start 0 --snr-end 10 --snr-step 1 --trials 100

use bitvec::prelude::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use tuxmodem_fec::{
    BlockN, CodeFamily, CodeRate, FecDecoder, FecEncoder, LdpcDecoder, LdpcEncoder, Llr,
};

fn awgn(codeword: &BitSlice<u8>, snr_db: f32, rng: &mut ChaCha8Rng) -> Vec<Llr> {
    let snr_lin = 10f32.powf(snr_db / 10.0);
    let sigma = (1.0 / (2.0 * snr_lin)).sqrt();
    codeword.iter().map(|b| {
        let amp: f32 = if *b { -1.0 } else { 1.0 };
        let u1: f32 = rng.gen::<f32>().max(1e-9);
        let u2: f32 = rng.gen::<f32>();
        let noise = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos() * sigma;
        let received = amp + noise;
        Llr(2.0 * received / (sigma * sigma))
    }).collect()
}

fn parse_family(s: &str) -> Option<CodeFamily> {
    match s {
        "floor" => Some(CodeFamily::FloorRate14),
        "ofdm-r12-n648" => Some(CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 }),
        "ofdm-r23-n648" => Some(CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R2_3 }),
        "ofdm-r34-n648" => Some(CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R3_4 }),
        "ofdm-r56-n648" => Some(CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R5_6 }),
        "ofdm-r12-n1296" => Some(CodeFamily::OfdmAdaptive { block_n: BlockN::N1296, rate: CodeRate::R1_2 }),
        _ => None,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut family_arg = "ofdm-r12-n648".to_string();
    let mut snr_start = 0f32;
    let mut snr_end = 10f32;
    let mut snr_step = 1f32;
    let mut trials = 50u32;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--family" => { family_arg = args[i+1].clone(); i += 2; }
            "--snr-start" => { snr_start = args[i+1].parse().unwrap(); i += 2; }
            "--snr-end" => { snr_end = args[i+1].parse().unwrap(); i += 2; }
            "--snr-step" => { snr_step = args[i+1].parse().unwrap(); i += 2; }
            "--trials" => { trials = args[i+1].parse().unwrap(); i += 2; }
            _ => { eprintln!("unknown arg: {}", args[i]); std::process::exit(1); }
        }
    }

    let family = parse_family(&family_arg).expect("unknown family");
    let enc = LdpcEncoder::new();
    let dec = LdpcDecoder::new();
    let (_n, payload_k) = enc.block_size(family);

    let mut rng = ChaCha8Rng::seed_from_u64(0xBE_R_CU_R_VE_u64);

    println!("# family={} payload_k={}", family_arg, payload_k);
    println!("# snr_db, ber, errors, total");

    let mut snr = snr_start;
    while snr <= snr_end + 1e-6 {
        let mut errors = 0u64;
        let mut total = 0u64;
        for _ in 0..trials {
            let info: BitVec<u8> = (0..payload_k).map(|_| rng.gen::<bool>()).collect();
            let codeword = enc.encode(family, info.as_bitslice()).expect("encode");
            let llrs = awgn(codeword.as_bitslice(), snr, &mut rng);
            match dec.decode(family, &llrs) {
                Ok(decoded) => {
                    for i in 0..payload_k {
                        if decoded.info_bits[i] != info[i] { errors += 1; }
                    }
                }
                Err(_) => { errors += payload_k as u64; }
            }
            total += payload_k as u64;
        }
        let ber = errors as f64 / total as f64;
        println!("{:.2}, {:.6}, {}, {}", snr, ber, errors, total);
        snr += snr_step;
    }
}
```

- [ ] **Step 2: Build to confirm it compiles.**

Run: `cargo build --release --example ber_curve -p tuxmodem-fec`
Expected: PASS.

- [ ] **Step 3: Commit.**

```bash
git add crates/tuxmodem-fec/examples/ber_curve.rs
git commit -m "feat(fec): ber_curve CLI example for ad-hoc tuning

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## Phase 8: Benchmarks, decoder tuning, design docs, polish

**Files:**
- Create: `crates/tuxmodem-fec/benches/encode.rs`
- Create: `crates/tuxmodem-fec/benches/decode.rs`
- Create: `crates/tuxmodem-fec/docs/architecture.md`
- Create: `crates/tuxmodem-fec/docs/code-construction.md`
- Create: `crates/tuxmodem-fec/docs/decoder-tuning.md`

### Task 8.1: Criterion benchmarks

**File:** `crates/tuxmodem-fec/benches/encode.rs`

- [ ] **Step 1: Write the encode benchmark.**

```rust
use bitvec::prelude::*;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use tuxmodem_fec::{
    BlockN, CodeFamily, CodeRate, FecEncoder, LdpcEncoder,
};

fn bench_encode(c: &mut Criterion) {
    let enc = LdpcEncoder::new();

    let families = [
        ("floor", CodeFamily::FloorRate14),
        ("ofdm-n648-r12", CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 }),
        ("ofdm-n648-r56", CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R5_6 }),
        ("ofdm-n1296-r12", CodeFamily::OfdmAdaptive { block_n: BlockN::N1296, rate: CodeRate::R1_2 }),
    ];

    for (name, family) in families {
        let (_n, k) = enc.block_size(family);
        let info: BitVec<u8> = (0..k).map(|i| (i % 3) == 0).collect();

        c.bench_with_input(
            BenchmarkId::new("encode", name),
            &family,
            |b, &family| {
                b.iter(|| {
                    let _ = enc.encode(family, info.as_bitslice()).unwrap();
                });
            },
        );
    }
}

criterion_group!(benches, bench_encode);
criterion_main!(benches);
```

**File:** `crates/tuxmodem-fec/benches/decode.rs`

- [ ] **Step 2: Write the decode benchmark — GATE this against §D.3 (rate-1/2 n=648 ≤ 50 ms per block on Pi 5).**

```rust
use bitvec::prelude::*;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::time::Duration;
use tuxmodem_fec::{
    BlockN, CodeFamily, CodeRate, FecDecoder, FecEncoder, LdpcDecoder, LdpcEncoder, Llr,
};

fn awgn(codeword: &BitSlice<u8>, snr_db: f32, rng: &mut ChaCha8Rng) -> Vec<Llr> {
    let snr_lin = 10f32.powf(snr_db / 10.0);
    let sigma = (1.0 / (2.0 * snr_lin)).sqrt();
    codeword.iter().map(|b| {
        let amp: f32 = if *b { -1.0 } else { 1.0 };
        let u1: f32 = rng.gen::<f32>().max(1e-9);
        let u2: f32 = rng.gen::<f32>();
        let noise = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos() * sigma;
        let received = amp + noise;
        Llr(2.0 * received / (sigma * sigma))
    }).collect()
}

fn bench_decode(c: &mut Criterion) {
    let enc = LdpcEncoder::new();
    let dec = LdpcDecoder::new();
    let mut rng = ChaCha8Rng::seed_from_u64(0xDE_C_BE_NC_u64);

    // Three SNR points: near threshold (slow, many iterations), comfortable (~3 dB above),
    // and very high (1-2 iterations, basically free).
    let configs = [
        ("ofdm-n648-r12-snr2dB", CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 }, 2.0),
        ("ofdm-n648-r12-snr5dB", CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 }, 5.0),
        ("ofdm-n648-r12-snr10dB", CodeFamily::OfdmAdaptive { block_n: BlockN::N648, rate: CodeRate::R1_2 }, 10.0),
        ("floor-snr-minus5dB", CodeFamily::FloorRate14, -5.0),
    ];

    for (name, family, snr_db) in configs {
        let (n, k) = enc.block_size(family);
        let info: BitVec<u8> = (0..k).map(|_| rng.gen::<bool>()).collect();
        let codeword = enc.encode(family, info.as_bitslice()).unwrap();
        let llrs = awgn(codeword.as_bitslice(), snr_db, &mut rng);

        let mut group = c.benchmark_group(format!("decode-{}", name));
        group.measurement_time(Duration::from_secs(20));
        group.throughput(Throughput::Bytes(n as u64 / 8));
        group.bench_function(BenchmarkId::from_parameter(snr_db), |b| {
            b.iter(|| {
                let _ = dec.decode(family, &llrs);
            });
        });
        group.finish();
    }
}

criterion_group!(benches, bench_decode);
criterion_main!(benches);
```

- [ ] **Step 3: Run encode bench to confirm it works.**

Run: `cargo bench --bench encode -p tuxmodem-fec`
Expected: PASS (numbers will vary by host).

- [ ] **Step 4: Run decode bench at the §D.3 gate point.**

Run: `cargo bench --bench decode -p tuxmodem-fec`
Expected: For `decode-ofdm-n648-r12-snr5dB` (the §D.3 gate at moderate SNR), median time must be ≤ 50 ms on Pi 5. If above, document in `decoder-tuning.md` and either tune (reduce max_iters or switch to min-sum approximation) or escalate to the next session.

- [ ] **Step 5: Commit.**

```bash
git add crates/tuxmodem-fec/benches/
git commit -m "bench(fec): criterion benchmarks for encode + decode (gates §D.3)

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 8.2: Write the architecture + code-construction + decoder-tuning docs

**File:** `crates/tuxmodem-fec/docs/architecture.md`

- [ ] **Step 1: Write architecture.md.**

```markdown
# tuxmodem-fec — internal architecture

## Why LDPC

Modern short-block LDPC is the best fit for tuxmodem's two-family PHY:

- **Near-Shannon performance at rates 1/2 through 5/6** with iterative
  belief-propagation decoding. Closes ~70% of the gap to capacity at the
  block sizes HF voice-bandwidth modems can afford.
- **Rate-compatible families** (the WiFi-style construction used here)
  let link adaptation (subsystem #7) step among code rates without
  changing decoder code paths.
- **Soft-decision native** — the SPA decoder consumes LLRs directly,
  matching the PHY's soft-bit demodulator output.

Reed-Solomon + convolutional inner ("classical concatenated") was
considered and rejected:
- 1.5–2 dB worse than LDPC at the same block size.
- Two code layers, two decoders, more complexity to maintain.
- No clear win on burst-error performance at our block sizes once the
  bit interleaver is in place.

Turbo codes were considered and rejected:
- Comparable LDPC performance with similar complexity, but:
- The patent landscape around turbo is more contested than LDPC
  (Berrou's patents lapsed in 2013, but residual encumbrances exist).
- LDPC is the implementation we know best from open literature.

Polar codes were considered for the floor mode and rejected for v0.5+:
- List-decoder complexity at the (n, rate) point we need is
  questionable on a Pi 5.
- LDPC at rate 1/4 with regular (3,4) construction gets us where we
  need to be.
- Polar is a v0.6+ candidate if profiling shows LDPC is too slow.

## Why two code instances

The two PHY families have different constraints:

- **Bit-adaptive OFDM family** wants rate-compatible codes covering 1/2
  through 5/6 so link adaptation can step rates without protocol churn.
  WiFi 802.11n's LDPC family (n ∈ {648, 1296}; 4 rates each) is the
  open-standard reference.
- **Wide-band low-density OFDM floor mode** wants maximum coding gain
  in a single, fixed mode. Rate-1/4 LDPC at n=2048 gets us 7–8 dB of
  coding gain over uncoded BPSK at the noise floor.

Two code instances. One decoder. The decoder doesn't know which code
it's decoding — it consumes the parity-check matrix and the LLRs and
runs SPA.

## Why peer subsystem (not folded into #3)

See plan §A.

## Why CRC-32

LDPC SPA can converge on a wrong-but-valid codeword (rare under high
SNR, more frequent at the threshold). The CRC-32 layer catches this
case and surfaces a clean ACK-or-NACK to ARQ. Without CRC, silent
corruption would pollute the data plane.

## Why bit interleaver

HF burst errors are bursty. The LDPC decoder works best on
near-independent bit errors. Bit interleaving spreads bursts across
codeword positions before the decoder sees them. Interleaver lives in
the FEC layer (not PHY) because depth ties to FEC block size.

## Limitations (documented, not bugs)

- Interleaver depth is per-FEC-block (n=648 or 2048 bits). Deep-fade
  bursts longer than the block can wipe out a block; ARQ retransmits.
  Cross-block interleaving is out of v0.5+ scope.
- No HARQ in v0.5+. Type-I (FEC fixed across retransmissions). Hook
  exists in `puncture.rs` for v0.6+ Type-II/III.
- Decoder is scalar f32 SPA. SIMD-vectorized min-sum is v0.6+ if Pi 5
  profiling shows it's needed.
```

**File:** `crates/tuxmodem-fec/docs/code-construction.md`

- [ ] **Step 2: Write code-construction.md.**

```markdown
# Code construction reference

## Floor rate-1/4 (n=2048, k=512)

Regular Gallager-style (3,4) construction:
- Column weight = 3
- Row weight = 4

Permutation-based stub assignment (Gallager 1963). Deterministic via
fixed PRNG seed. Construction is in `src/codes/floor_rate14.rs`.

## OFDM rate-compatible family (n=648 or n=1296)

Quasi-cyclic LDPC construction following the IEEE 802.11n PATTERN
(circulant block matrix; block size Z=27 for n=648, Z=54 for n=1296).
Shift values generated by deterministic PRNG seeded per (n, rate); NOT
copied verbatim from the 802.11n shift tables, per the clean-sheet
posture in ADR 0014.

Construction is in `src/codes/ofdm_wifi_family.rs`. The 802.11n pattern
itself is a public standard (IEEE 802.11n-2009); using the pattern is
clean-sheet. The specific shift values are tuxmodem-derived.
```

**File:** `crates/tuxmodem-fec/docs/decoder-tuning.md`

- [ ] **Step 3: Write decoder-tuning.md.**

```markdown
# Decoder tuning notes

## Max-iteration counts

| Code family | Max iters | Rationale |
|---|---|---|
| OfdmAdaptive (all rates) | 50 | Standard SPA convergence point under
  moderate channels; ~3% throughput cost vs. 30 iterations at modest SNR
  benefit |
| FloorRate14 | 100 | Floor-mode operates near the code threshold;
  more iterations trade decode time for coding gain |

Both can be tuned downward if Phase 8 benchmarks show Pi 5 misses the
§D.3 gate (50 ms per block at rate-1/2 n=648). First lever: drop max
iters from 50 to 30 (saves ~40% decode time, ~0.3 dB BER penalty at the
threshold).

## Boxplus form

Current implementation uses the exact `2 * atanh(tanh(a/2) * tanh(b/2))`
form. The min-sum approximation (`sign(a)*sign(b)*min(|a|,|b|)`) is ~2x
faster but ~0.3 dB BER penalty. v0.5+ uses exact; v0.6+ may switch.

## Early termination

After each iteration, the decoder hard-decides the codeword and checks
all parity constraints. If all are satisfied, terminate early. This is
the dominant runtime win at high SNR (1-2 iterations to converge).

## Numerical clamps

`boxplus` clamps the inner product to ±0.999_999_94 to prevent
`atanh` blow-up. f32 precision is sufficient at the block sizes used.
```

- [ ] **Step 4: Commit all docs.**

```bash
git add crates/tuxmodem-fec/docs/
git commit -m "docs(fec): architecture, code construction, decoder tuning

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

### Task 8.3: Final polish — clippy, format, unused-import cleanup

- [ ] **Step 1: Run clippy across the crate.**

Run: `cargo clippy -p tuxmodem-fec --all-targets -- -D warnings`
Expected: PASS (or surface specific warnings to fix one-by-one).

- [ ] **Step 2: Run rustfmt.**

Run: `cargo fmt -p tuxmodem-fec`
Expected: No output (format is clean) or formatting applied.

- [ ] **Step 3: Run the full test suite (excluding ignored).**

Run: `cargo test -p tuxmodem-fec`
Expected: All non-ignored tests PASS.

- [ ] **Step 4: Final commit.**

```bash
git add -A crates/tuxmodem-fec/
git commit -m "chore(fec): clippy + rustfmt pass; ship-ready v0.5+ FEC crate

Agent: opossum-pine-spruce
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>"
```

---

## §H. Self-review (run after writing this plan, before handoff)

### Spec coverage

Walking through the FEC spec (`2026-05-31-clean-sheet-modem-4-fec.md`)
section by section:

- §1 Role (FEC composes with ARQ; may fold into PHY) → §A above takes a peer-vs-folded position; the encode/decode pipeline (CRC → LDPC → interleave) is the composition.
- §2 What it's NOT → respected: no code-family pre-decision beyond plan-time picks (LDPC); interleaving is in FEC per Q4-resolved-this-plan; no ARQ logic in this crate (we surface stats only).
- §3 Forcing functions:
  - §3.1 F.520 performance → covered by Phase 7 + §D criteria.
  - §3.2 Decoder complexity budget → covered by §D.3 + Phase 8 benchmarks.
  - §3.3 Code-rate flexibility → covered by WiFi-family rate-compatible codes (1/2..5/6).
  - §3.4 Block size vs. latency → addressed in §E.4; v0.5+ accepts block-level latency (deep-fade limitation).
  - §3.5 HF burst-error pattern → covered by bit interleaver (Phase 2).
  - §3.6 No examination of VARA's FEC → enforced; cited as a watched failure mode §E.6.
- §4 Open design questions:
  - Q1 Code family → resolved: LDPC only (justified in §A + architecture.md).
  - Q2 Code rates → resolved: fixed rate-1/4 for floor + WiFi-family 1/2..5/6 for OFDM.
  - Q3 Block size → resolved: 2048 / 648 / 1296.
  - Q4 Interleaving → resolved: inside FEC layer (Phase 2 + architecture.md).
  - Q5 FEC discrete vs. folded → resolved: peer subsystem (§A; reconciled with #3 per §F).
  - Q6 Soft-decision input → resolved: required (LLR-only API).
  - Q7 HARQ type → resolved: Type-I in v0.5+; hooks for v0.6+ Type-II/III.
- §5 Citations → covered in README + architecture.md.
- §6 Dependencies → matches §C.
- §7 No-impl-choice markers → SUPERSEDED by this plan, which takes positions.
- §8 Watched failure modes → covered by §E (plus implementation-specific additions).

All spec sections covered. No gaps.

### Placeholder scan

Searched for: "TBD", "TODO", "implement later", "fill in", "appropriate", "add validation", "handle edge cases".

- One `todo!()` in `puncture.rs` — intentional, documented as a v0.6+ HARQ hook in §E.5 + the file comment.
- One `panic!("placeholder")` per ITU-R F.520 test file — intentional, documented as "requires hf-channel-sim crate (#1)" with `#[ignore]` attribute.
- One `panic!("...rank-deficient")` in `floor_rate14.rs` Encoder construction — intentional, documented in-code as a v0.5+ acceptable failure mode with a SEED-tweak workaround.

No silent placeholders.

### Type consistency

- `Llr(pub f32)` defined once in `api.rs`; used in `decode.rs`, `llr.rs`, tests, examples.
- `CodeFamily`, `BlockN`, `CodeRate` enums defined once in `api.rs`; used by `codes/mod.rs`, `codes/floor_rate14.rs`, `codes/ofdm_wifi_family.rs`, tests, examples.
- `FecEncoder::encode` signature: `(family, info_bits) -> Result<BitVec<u8>, FecError>` — consistent.
- `FecDecoder::decode` signature: `(family, llrs) -> Result<DecodedBlock, FecError>` — consistent.
- `BlockDecodeStats` → `ResidualErrorStats` via `From` impl — consistent.
- `Encoder::n() / k()` vs. `Decoder::n() / k()` — match. PHY-facing `block_size()` returns (LDPC n, payload k = LDPC k - 32 for CRC overhead). Note in `block_size` doc-comment surfaces this.
- `interleaver::interleave / deinterleave` parameters are (bits, rows) — consistent.
- `crc::append_crc32 / verify_crc32` — consistent.

No inconsistencies found.

---

## §I. Execution handoff

Plan complete and saved to
`docs/superpowers/plans/2026-05-31-clean-sheet-modem-4-fec-plan.md`.

Per parent agent's spec, execution choice is deferred to the parent
reconciliation step. The plan is ready for either:

1. **Subagent-Driven** — fresh subagent per task; two-stage review
   between tasks; FEC + #3 PHY reconcile §F coordination points before
   either implementation lands.
2. **Inline Execution** — batch the eight phases with checkpoints at
   each phase boundary; reconcile §F coordination at the Phase 0
   checkpoint.

Recommended: subagent-driven for Phases 0–6 (clear unit boundaries);
inline for Phases 7–8 (the channel-simulator integration in Phase 7
needs interactive iteration with #1's actual API surface).

---

Agent: opossum-pine-spruce
