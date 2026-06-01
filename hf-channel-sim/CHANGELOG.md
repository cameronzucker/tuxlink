# Changelog

All notable changes to hf-channel-sim are documented here.
Format adapted from Keep a Changelog. Versions follow SemVer.

## [0.1.0] — 2026-05-31

Initial public release. Independent-creation provenance: implemented
from Watterson (1970), ITU-R F.520-2, ITU-R F.1487, Davies' *Ionospheric
Radio*, and Proakis & Salehi's *Digital Communications*. No closed-source
HF modem (VARA, ARDOP binary distributions, Trimode, etc.) consulted
in any form.

### Added
- Two-tap Watterson HF channel model with ITU-R F.520 standardized
  conditions (Good / Moderate / Poor / Flutter).
- Deterministic seeded RNG; same seed + same input + same params
  produces bit-identical output.
- Per-sub-carrier SNR analyzer for bit-adaptive OFDM characterization.
- AWGN injection decoupled from channel (per F.1487 methodology).
- End-to-end `CharacterizationReport` with JSON serialization.
- CLI binary `hf-channel-sim-cli` for pipe-friendly characterization.
- AGPL-3.0-only license.

### Limitations
- 2-tap Watterson only; multi-tap / frequency-selective extension is
  a future release.
- Cross-validation against GNU Radio `fading_model` is harness-ready but
  the reference fixtures are generated offline by the operator (see
  `docs/cross-validation.md`).
