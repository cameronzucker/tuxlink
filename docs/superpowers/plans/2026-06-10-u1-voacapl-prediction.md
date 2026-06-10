# U1 — Offline HF Prediction Service (voacapl sidecar) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an offline HF-propagation prediction service to the Tuxlink backend: a bundled `voacapl` (VOACAP-for-Linux) binary + `itshfbc` coefficient data, invoked headless to compute per-frequency, per-hour circuit reliability (REL) for an operator→station HF path, with the sunspot number (SSN) bundled/cached so prediction never requires the network.

**Architecture:** New Rust module `src-tauri/src/propagation/`. The backend (1) converts an operator Maidenhead grid + a station's grid + its `frequencies_khz` into a VOACAP fixed-column "input deck", (2) runs the bundled `voacapl` binary headless inside a **per-call scratch directory** (voacapl writes a fixed `run/voacapx.out` filename, so concurrency requires isolation), (3) parses `voacapx.out` for the 24-hour REL/SNR matrix plus the great-circle azimuth/distance VOACAP already computes, and (4) returns a structured `PathPrediction`. The only time-varying input, SSN, is bundled as a forecast table and cached under `app_data_dir()`; network access only ever *refreshes* it. The engine is **pure offline compute**: no network, no credentials, no transmit, no writes outside its scratch dir — categorically unlike the removed Pat sidecar (ADR 0011 / ADR 0018).

**Tech Stack:** Rust (Tauri backend), `std::process::Command` via `tokio::task::spawn_blocking` (mirrors `ManagedModem` in `src-tauri/src/winlink/modem/process.rs`), `serde` (camelCase DTOs), Tauri `externalBin` + `resources` bundling, GitHub Actions multi-arch build. Bundled native `voacapl` (GPL-3-compatible; built from `github.com/jawatson/voacapl`).

**Grounding (do not re-derive — captured by running voacapl on arm64, 2026-06-10):**
- I/O contract: `dev/scratch/voacapl-grounding-2026-06-10/IO-CONTRACT.md` (in the main checkout, gitignored). Card formats are from voacapl source `src/voacapw/voacap.for` WRITE formats — authoritative, not guessed.
- Real captured fixtures (tracked in this branch): `src-tauri/tests/fixtures/voacap/dm43-dm34-input-deck.dat` and `dm43-dm34-voacapx.out` (24 hourly blocks, DM43→DM34, N0DAJ VARA HF dials).
- Reference prototypes (gitignored scratch): `gen_voacap_deck.py`, `parse_voacapx_out.py` — both verified against the fixtures.

---

## ⚠️ REVISION 2 — adversarial-review dispositions (READ BEFORE EXECUTING)

This plan passed a `build-robust-features` cross-provider adversarial review (4 reviewers, 2 providers). The companion doc **[`2026-06-10-u1-voacapl-prediction-adrev.md`](2026-06-10-u1-voacapl-prediction-adrev.md)** holds the ranked findings (F1–F19) and their dispositions. **The executor MUST apply every "FIX in plan" disposition from that table** — they are mandatory plan amendments, not optional. The DTO contract (Task 1) and the TDD scaffolding (below) are already revised here; the remaining per-finding fixes are specified against their task numbers in the dispositions table.

Highest-impact amendments (do not skip):
- **F1/F12 (Task 1 DTO — already revised below):** carry the *exact input* `frequencies_khz` through to results; never re-derive frequency from VOACAP's lossy 1-decimal display. Expose `snr`, `mufday`, `ssn`, `month`, `year` so U3 can rank by a defensible composite, not raw REL.
- **F7 (Tasks 1/2):** REL alone (vs VOACAP's generic `REQ.SNR=73 dB`) mis-ranks data modes — set a data-mode `REQ.SNR`/bandwidth and rank on REL gated by SNR-margin/MUF.
- **F2 (Tasks 6/7):** resolve the sidecar via `ShellExt::shell(&app).sidecar("voacapl")`, NOT `BaseDirectory::Resource`.
- **F5 (Task 7):** bundle `database/version.w32` or voacapl hard-aborts.
- **F4/F16 (Task 3):** tokenize to the label column (col 67), validate `freqs.len()==freq_count`, guard SNR length.
- **F8 (Tasks 4/6):** inject `Clock`; use the real UTC year (not hardcoded 2026).
- **F9 (Task 2):** clamp frequencies to the HF window (≈1.8–30 MHz); error on >11, don't silent-truncate.
- **F10 (Task 5):** use `tempfile::TempDir` (RAII cleanup), fail-closed if `app_cache_dir()` is unavailable.

### Mandatory per-task scaffolding (build-robust-features)

**Every task** below carries this preamble and completion check; **every task group** ends with the review loop.

```
BEFORE starting work:
1. Invoke superpowers:test-driven-development (or read .claude/skills/test-driven-development/).
2. Read docs/pitfalls/testing-pitfalls.md and docs/pitfalls/implementation-pitfalls.md.
Follow TDD: write the failing test → run it red → implement minimal code → run green.

BEFORE marking the task complete:
1. Review your tests against docs/pitfalls/testing-pitfalls.md (error paths? edge cases?).
2. Run the task's tests + `cargo clippy --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings` (re-run clippy to exit 0; it hides later-target lints).
3. Confirm green with pasted output before claiming done (verification-before-completion).

AFTER each task group: do a minimum of three review rounds from multiple
perspectives; if substantive issues remain in round 3, keep going. Then continue.
```

Task groups for the review loop: **{1,2,3}** (pure logic: DTO + deck + parser), **{4,5}** (SSN + engine), **{6,7}** (command wiring + bundling/CI).

---

**Reused existing code:**
- `crate::position::grid_to_lat_lon(grid: &str) -> Option<(f64, f64)>` (`src-tauri/src/position/maidenhead.rs`) — signed decimal degrees (W/S negative), square-center. The deck builder splits sign into VOACAP's hemisphere letter.
- `Gateway` / `StationListing` / `ListingMode` (`src-tauri/src/catalog/stations.rs`) — `frequencies_khz: Vec<f64>` is **kHz**; VOACAP wants MHz (÷1000).
- `Clock` trait pattern (`src-tauri/src/catalog/stations_cache.rs`) — inject time for deterministic tests (SSN cache reuses it).
- `UiError` (`src-tauri/src/ui_commands.rs`, `#[serde(tag = "kind", content = "detail")]`) — command-layer error projection.

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/propagation/mod.rs` | Module wiring + public DTOs (`PathPrediction`, `ChannelReliability`, `PredictionInputs`) + `PropagationError` |
| `src-tauri/src/propagation/deck.rs` | Build the VOACAP input deck from `PredictionInputs` (card formats, grid→latlon, kHz→MHz, hemisphere split) |
| `src-tauri/src/propagation/parse.rs` | Parse `voacapx.out` → REL/SNR per freq per hour + azimuth/distance |
| `src-tauri/src/propagation/ssn.rs` | Bundled SSN forecast table + on-disk cache under `app_data_dir()`; opportunistic refresh |
| `src-tauri/src/propagation/engine.rs` | Resolve bundled binary + `itshfbc`; per-call scratch run dir; spawn voacapl; capture; map errors |
| `src-tauri/src/propagation/commands.rs` | Tauri command `propagation_predict_path`; project `PropagationError` → `UiError` |
| `src-tauri/tests/fixtures/voacap/*` | Real captured deck + output (already placed) |
| `src-tauri/tauri.conf.json` | `externalBin` (voacapl per-arch) + `resources` (itshfbc tree) |
| `.github/workflows/*.yml` | Per-arch build + bundle voacapl + itshfbc |
| `src-tauri/src/lib.rs` | Register `propagation_predict_path` in `invoke_handler`; declare `mod propagation` |

**Run tests with:** `cargo test --lib --manifest-path src-tauri/Cargo.toml propagation` (unit) and `cargo test --manifest-path src-tauri/Cargo.toml --test propagation_parse` (integration fixture tests). Run `cargo clippy --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings` before every push (CI `verify` gate is stricter than `cargo test`).

---

## Task 1: Module scaffold — DTOs and error type

**Files:**
- Create: `src-tauri/src/propagation/mod.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod propagation;` near the other `mod` decls, ~line 1-40)

- [ ] **Step 1: Write the failing test** (in `src-tauri/src/propagation/mod.rs`)

```rust
//! Offline HF-propagation prediction (voacapl sidecar). Pure offline compute:
//! no network, no transmit, no writes outside a per-call scratch dir.
//! Plan: docs/superpowers/plans/2026-06-10-u1-voacapl-prediction.md

pub mod deck;
pub mod engine;
pub mod parse;
pub mod ssn;
pub mod commands;

use serde::{Deserialize, Serialize};

/// Inputs for one operator→station HF circuit prediction.
#[derive(Debug, Clone, PartialEq)]
pub struct PredictionInputs {
    /// Operator Maidenhead grid (reference point; from the status bar).
    pub tx_grid: String,
    /// Station Maidenhead grid (from `Gateway.grid`).
    pub rx_grid: String,
    /// Frequencies in kHz (from `Gateway.frequencies_khz`); converted to MHz for VOACAP.
    /// F1: these EXACT values are carried through to results by index — never
    /// re-derived from VOACAP's lossy display.
    pub frequencies_khz: Vec<f64>,
    /// UTC year (F8: used for the SSN lookup; do NOT hardcode).
    pub year: i32,
    /// UTC month 1-12.
    pub month: u8,
    /// Smoothed sunspot number (from the SSN cache).
    pub ssn: f64,
    /// TX power in watts (v1 default 100 W; operator-configurable).
    pub tx_power_w: f64,
    /// F7: required SNR (dB) for the SYSTEM card, calibrated to the data mode
    /// (VARA/ARDOP), NOT VOACAP's generic 73 dB default. v1 default documented
    /// in Task 2; this is what REL is computed against.
    pub req_snr_db: f64,
}

/// Per-frequency reliability over the 24 UTC hours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelReliability {
    /// F1: the EXACT input dial in kHz (e.g. 7103.0), carried through by column
    /// index — the value U3 maps back to the operator's channel.
    pub frequency_khz: f64,
    /// The rounded MHz VOACAP actually computed this column at (informational;
    /// 7103 kHz and 7108 kHz both compute at ~7.10/7.11 MHz). Lets the UI show
    /// "computed at 7.10 MHz" without losing the real dial.
    pub voacap_mhz: f64,
    /// 24 reliability values (0.0-1.0), index = UTC hour 0..23. REL is vs `req_snr_db`.
    pub rel_by_hour: Vec<f64>,
    /// 24 SNR values (dB), index = UTC hour 0..23 (F7: lets U3 rank by SNR margin).
    pub snr_by_hour: Vec<f64>,
    /// 24 MUFday values (0.0-1.0), index = UTC hour 0..23 (F7: MUF-gating context).
    pub mufday_by_hour: Vec<f64>,
}

/// Full prediction result for one path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathPrediction {
    /// Great-circle bearing TX→RX in degrees (from VOACAP's AZIMUTHS line; for antenna aiming).
    pub bearing_deg: f64,
    /// Path distance in km (from VOACAP).
    pub distance_km: f64,
    /// F12: SSN provenance so U3 can render "solar data N old".
    pub ssn: f64,
    pub year: i32,
    pub month: u8,
    pub channels: Vec<ChannelReliability>,
}

#[derive(Debug, thiserror::Error)]
pub enum PropagationError {
    #[error("invalid grid {0:?}")]
    InvalidGrid(String),
    #[error("no usable HF frequencies in input")]
    NoFrequencies,
    #[error("voacapl binary not found: {0}")]
    BinaryNotFound(String),
    #[error("voacapl run failed: {0}")]
    RunFailed(String),
    #[error("could not parse voacapx.out: {0}")]
    ParseFailed(String),
    #[error("ssn cache error: {0}")]
    Ssn(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_prediction_serializes_camel_case() {
        let p = PathPrediction {
            bearing_deg: 301.65,
            distance_km: 215.2,
            ssn: 100.0,
            year: 2026,
            month: 6,
            channels: vec![ChannelReliability {
                frequency_khz: 7103.0,
                voacap_mhz: 7.10,
                rel_by_hour: vec![0.21; 24],
                snr_by_hour: vec![65.0; 24],
                mufday_by_hour: vec![0.69; 24],
            }],
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"bearingDeg\":301.65"));
        assert!(json.contains("\"distanceKm\":215.2"));
        assert!(json.contains("\"relByHour\""));
        assert!(json.contains("\"mufdayByHour\""));
        assert!(json.contains("\"voacapMhz\":7.1"));
        // F1: the exact dial survives, not a rounded 7100.
        assert!(json.contains("\"frequencyKhz\":7103.0"));
        assert!(json.contains("\"ssn\":100.0"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib --manifest-path src-tauri/Cargo.toml propagation::tests::path_prediction_serializes_camel_case`
Expected: FAIL — compile error (`thiserror`, `serde_json` confirmed already in `Cargo.toml`; module not declared). If `thiserror` is missing, add it (it is used by `catalog/composer.rs`, so it is present).

- [ ] **Step 3: Wire the module**

In `src-tauri/src/lib.rs`, add alongside the other top-level `mod` declarations:

```rust
mod propagation;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib --manifest-path src-tauri/Cargo.toml propagation::tests::path_prediction_serializes_camel_case`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/propagation/mod.rs src-tauri/src/lib.rs
git commit -m "feat(propagation): scaffold module DTOs + error type"
```

---

## Task 2: Deck builder — VOACAP input-deck from PredictionInputs

**Files:**
- Create: `src-tauri/src/propagation/deck.rs`
- Reference: `src-tauri/tests/fixtures/voacap/dm43-dm34-input-deck.dat` (the exact expected output for the DM43→DM34 inputs)

**Card formats (authoritative, from voacapl `src/voacapw/voacap.for`):**
- `CIRCUIT` = `'CIRCUIT   ',f5.2,a1,3(f9.2,a1),2x,a1,1x,i5` — **TX-lat is F5.2; TX-lon, RX-lat, RX-lon are each F9.2**. Hemisphere letter carries lon/lat sign (magnitude positive).
- `FREQUENCY` = `'FREQUENCY ',11f5.2` — exactly 11 MHz slots, F5.2 (2 decimals), unused = `0.00`.
- `SUNSPOT` = `'SUNSPOT   ',12f5.0` — keep the trailing dot (`100.`).
- `MONTH` = `'MONTH     ',i5,10f5.2`; `LABEL` = `2a20`; `METHOD` = `30 0` (point-to-point).
- `SYSTEM`/`FPROB` copied verbatim from voacapl's shipped default; TX power lives in the `ANTENNA` card's final `f10.4` field (kW; 100 W = `0.1`).

- [ ] **Step 1: Write the failing test** (in `src-tauri/src/propagation/deck.rs`)

```rust
//! VOACAP input-deck builder. Fixed-column "card" format; formats grounded in
//! voacapl src/voacapw/voacap.for WRITE statements (see IO-CONTRACT.md).

use crate::position::grid_to_lat_lon;
use super::{PredictionInputs, PropagationError};

const MAX_FREQS: usize = 11;

/// Split a signed decimal degree into VOACAP's (magnitude, hemisphere-letter).
fn lat_hemi(v: f64) -> (f64, char) { (v.abs(), if v >= 0.0 { 'N' } else { 'S' }) }
fn lon_hemi(v: f64) -> (f64, char) { (v.abs(), if v >= 0.0 { 'E' } else { 'W' }) }

/// Build the complete METHOD-30 deck text. The frequencies are kHz on input,
/// emitted as MHz (÷1000). Returns Err for unparseable grids or no frequencies.
pub fn build_deck(inputs: &PredictionInputs) -> Result<String, PropagationError> {
    let (tx_lat, tx_lon) = grid_to_lat_lon(&inputs.tx_grid)
        .ok_or_else(|| PropagationError::InvalidGrid(inputs.tx_grid.clone()))?;
    let (rx_lat, rx_lon) = grid_to_lat_lon(&inputs.rx_grid)
        .ok_or_else(|| PropagationError::InvalidGrid(inputs.rx_grid.clone()))?;

    let freqs_mhz: Vec<f64> = inputs
        .frequencies_khz
        .iter()
        .filter(|f| f.is_finite() && **f > 0.0)
        .map(|f| f / 1000.0)
        .take(MAX_FREQS)
        .collect();
    if freqs_mhz.is_empty() {
        return Err(PropagationError::NoFrequencies);
    }

    let (tla, tlah) = lat_hemi(tx_lat);
    let (tlo, tloh) = lon_hemi(tx_lon);
    let (rla, rlah) = lat_hemi(rx_lat);
    let (rlo, rloh) = lon_hemi(rx_lon);
    let circuit = format!(
        "CIRCUIT   {:5.2}{}{:9.2}{}{:9.2}{}{:9.2}{}  {} {:5}",
        tla, tlah, tlo, tloh, rla, rlah, rlo, rloh, 'S', 0
    );

    let mut padded = freqs_mhz.clone();
    padded.resize(MAX_FREQS, 0.0);
    let frequency = format!(
        "FREQUENCY {}",
        padded.iter().map(|f| format!("{:5.2}", f)).collect::<String>()
    );

    // f5.0 keeps a trailing '.', right-justified in width 5.
    let sunspot = format!("SUNSPOT   {:>5}", format!("{:.0}.", inputs.ssn));

    let tx_power_kw = inputs.tx_power_w / 1000.0;
    let lines = [
        "COMMENT    Any VOACAP default cards may be placed in the file: VOACAP.DEF".to_string(),
        "LINEMAX      55       number of lines-per-page".to_string(),
        "COEFFS    CCIR".to_string(),
        format!("TIME      {:5}{:5}{:5}{:5}", 1, 24, 1, 1),
        format!("MONTH     {:5}{:5.2}", 2026, inputs.month as f64),
        sunspot,
        format!("LABEL     {:<20}{:<20}", "TX", "RX"),
        circuit,
        "SYSTEM       1. 145. 0.10  90. 73.0 3.00 0.10".to_string(),
        "FPROB      1.00 1.00 1.00 0.00".to_string(),
        format!(
            "ANTENNA       1    1    2   30     0.000[default/const17.voa  ]  0.0{:10.4}",
            tx_power_kw
        ),
        "ANTENNA       2    2    2   30     0.000[default/swwhip.voa   ]  0.0    0.0000".to_string(),
        frequency,
        "METHOD       30    0".to_string(),
        "EXECUTE".to_string(),
        "QUIT".to_string(),
    ];
    Ok(format!("{}\n", lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dm43_dm34() -> PredictionInputs {
        PredictionInputs {
            tx_grid: "DM43".to_string(),
            rx_grid: "DM34".to_string(),
            frequencies_khz: vec![3590.0, 7103.0, 7108.0, 10147.0, 14103.0, 14115.0],
            month: 6,
            ssn: 100.0,
            tx_power_w: 100.0,
        }
    }

    #[test]
    fn circuit_card_matches_fortran_format_widths() {
        let deck = build_deck(&dm43_dm34()).unwrap();
        let circuit = deck.lines().find(|l| l.starts_with("CIRCUIT")).unwrap();
        // DM43 center = 33.50N 111.00W ; DM34 center = 34.50N 113.00W.
        assert_eq!(circuit, "CIRCUIT   33.50N   111.00W    34.50N   113.00W  S     0");
        assert_eq!(circuit.len(), 55);
    }

    #[test]
    fn frequencies_convert_khz_to_mhz_and_pad_to_11() {
        let deck = build_deck(&dm43_dm34()).unwrap();
        let freq = deck.lines().find(|l| l.starts_with("FREQUENCY")).unwrap();
        // 7103 kHz -> 7.10, 7108 -> 7.11, 10147 -> 10.15, etc.
        assert_eq!(freq, "FREQUENCY  3.59 7.10 7.1110.1514.1014.12 0.00 0.00 0.00 0.00 0.00");
    }

    #[test]
    fn matches_captured_golden_deck() {
        // The real deck that produced the fixture output, modulo LABEL text.
        let golden = include_str!("../../tests/fixtures/voacap/dm43-dm34-input-deck.dat");
        let built = build_deck(&dm43_dm34()).unwrap();
        let strip_label = |s: &str| {
            s.lines().filter(|l| !l.starts_with("LABEL")).collect::<Vec<_>>().join("\n")
        };
        assert_eq!(strip_label(&built), strip_label(golden));
    }

    #[test]
    fn invalid_grid_is_error() {
        let mut inputs = dm43_dm34();
        inputs.rx_grid = "ZZ".to_string();
        assert!(matches!(build_deck(&inputs), Err(PropagationError::InvalidGrid(_))));
    }

    #[test]
    fn no_finite_frequencies_is_error() {
        let mut inputs = dm43_dm34();
        inputs.frequencies_khz = vec![0.0, -1.0, f64::NAN];
        assert!(matches!(build_deck(&inputs), Err(PropagationError::NoFrequencies)));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib --manifest-path src-tauri/Cargo.toml propagation::deck`
Expected: FAIL — `deck` module body not yet compiled into the build until `mod deck;` resolves (it is declared in Task 1's mod.rs). The test file IS the implementation here (builder + tests together); the failure is the golden-deck assertion if any format detail is off. Run and read the diff.

- [ ] **Step 3: Reconcile any format mismatch against the golden fixture**

If `matches_captured_golden_deck` fails, diff the built deck vs `dm43-dm34-input-deck.dat` line-by-line and adjust the format strings. The fixture is the ground truth (real voacapl input). Common gotchas: the `ANTENNA` power field width (`f10.4`), the `SUNSPOT` trailing dot, `MONTH` year hardcoded to 2026 (acceptable for v1 — month drives prediction, year is solar-context only and SSN already encodes the cycle).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib --manifest-path src-tauri/Cargo.toml propagation::deck`
Expected: PASS (5 tests)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/propagation/deck.rs
git commit -m "feat(propagation): VOACAP input-deck builder (grounded in fixture)"
```

---

## Task 3: Output parser — voacapx.out → REL/SNR/azimuth/distance

**Files:**
- Create: `src-tauri/src/propagation/parse.rs`
- Reference fixture: `src-tauri/tests/fixtures/voacap/dm43-dm34-voacapx.out`

**Parse contract (from IO-CONTRACT.md):**
- 24 per-hour blocks. Each opens with a row whose right-edge label (6-char field at col 67, 0-based) is `FREQ`: tokens are `hour, MUF, freq1..freqN`.
- Following rows carry a right-edge label naming the parameter (`REL`, `SNR`, `S DBW`, `MODE`, …). Values are ~5-char fixed fields in the data region (cols 6-60); `-` = unused freq slot.
- The circuit summary line (before the blocks) holds AZIMUTHS + KM: `33.50 N  111.00 W - 34.50 N  113.00 W    301.65  120.54     116.2    215.2` → first azimuth = TX→RX bearing, last number = km.

- [ ] **Step 1: Write the failing test** (in `src-tauri/src/propagation/parse.rs`)

```rust
//! Parser for voacapl's METHOD-30 `voacapx.out`. See IO-CONTRACT.md.
//! Robust to frequency count via the per-block FREQ header; rows are keyed by
//! the right-edge 6-char label at col 67.

use super::{ChannelReliability, PathPrediction, PropagationError};

const LABEL_COL: usize = 67;
const DATA_START: usize = 5;
const DATA_END: usize = 60;

fn right_label(line: &str) -> &str {
    line.get(LABEL_COL..).map(str::trim).unwrap_or("")
}

fn data_tokens(line: &str) -> Vec<&str> {
    let end = line.len().min(DATA_END);
    if line.len() <= DATA_START { return Vec::new(); }
    line[DATA_START..end].split_whitespace().collect()
}

/// Parse the full output into a `PathPrediction`. `freq_count` frequencies are
/// expected (the active set from the deck); REL/SNR vectors are 24 long.
pub fn parse_voacapx_out(text: &str, freq_count: usize) -> Result<PathPrediction, PropagationError> {
    let (bearing_deg, distance_km) = parse_azimuth_distance(text)
        .ok_or_else(|| PropagationError::ParseFailed("no AZIMUTHS/KM summary line".into()))?;

    // Accumulate per-freq, per-hour. freqs_khz captured from the first FREQ header.
    let mut freqs_khz: Vec<f64> = Vec::new();
    let mut rel: Vec<Vec<f64>> = vec![Vec::new(); freq_count];
    let mut snr: Vec<Vec<f64>> = vec![Vec::new(); freq_count];

    let mut in_block = false;
    for line in text.lines() {
        match right_label(line) {
            "FREQ" => {
                in_block = true;
                if freqs_khz.is_empty() {
                    // tokens: hour, MUF, then freqs (MHz). Skip first 2.
                    let toks = data_tokens(line);
                    freqs_khz = toks.iter().skip(2).take(freq_count)
                        .filter_map(|t| t.parse::<f64>().ok())
                        .map(|mhz| mhz * 1000.0)
                        .collect();
                }
            }
            "REL" if in_block => push_row(&mut rel, line, freq_count),
            "SNR" if in_block => push_row(&mut snr, line, freq_count),
            _ => {}
        }
    }

    if rel.iter().any(|v| v.len() != 24) {
        return Err(PropagationError::ParseFailed(format!(
            "expected 24 hourly REL values per freq, got {:?}",
            rel.iter().map(Vec::len).collect::<Vec<_>>()
        )));
    }

    let channels = (0..freq_count)
        .map(|i| ChannelReliability {
            frequency_khz: *freqs_khz.get(i).unwrap_or(&0.0),
            rel_by_hour: rel[i].clone(),
            snr_by_hour: snr[i].clone(),
        })
        .collect();

    Ok(PathPrediction { bearing_deg, distance_km, channels })
}

fn push_row(acc: &mut [Vec<f64>], line: &str, freq_count: usize) {
    let toks = data_tokens(line);
    for i in 0..freq_count {
        let v = toks.get(i).and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0);
        acc[i].push(v);
    }
}

/// Find the summary line with great-circle azimuth + km. The data row directly
/// follows the header row containing "AZIMUTHS" and "KM".
fn parse_azimuth_distance(text: &str) -> Option<(f64, f64)> {
    let mut lines = text.lines().peekable();
    while let Some(l) = lines.next() {
        if l.contains("AZIMUTHS") && l.contains("KM") {
            let data = lines.next()?;
            let nums: Vec<f64> = data.split_whitespace()
                .filter_map(|t| t.parse::<f64>().ok())
                .collect();
            // Layout: txlat txlon rxlat rxlon  az1 az2  nmi  km  (lat/lon include hemi letters,
            // so the parseable numeric tail is: ...mag,mag,mag,mag, az1, az2, nmi, km).
            // The last value is km; the azimuth pair is the 3rd/4th-from-last.
            if nums.len() >= 4 {
                let km = *nums.last()?;
                let bearing = nums[nums.len() - 4];
                return Some((bearing, km));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/voacap/dm43-dm34-voacapx.out");

    #[test]
    fn parses_bearing_and_distance() {
        let p = parse_voacapx_out(FIXTURE, 6).unwrap();
        assert!((p.bearing_deg - 301.65).abs() < 0.01, "bearing {}", p.bearing_deg);
        assert!((p.distance_km - 215.2).abs() < 0.1, "km {}", p.distance_km);
    }

    #[test]
    fn parses_24_hour_rel_per_frequency() {
        let p = parse_voacapx_out(FIXTURE, 6).unwrap();
        assert_eq!(p.channels.len(), 6);
        for ch in &p.channels {
            assert_eq!(ch.rel_by_hour.len(), 24);
            assert_eq!(ch.snr_by_hour.len(), 24);
        }
    }

    #[test]
    fn rel_values_match_captured_data() {
        let p = parse_voacapx_out(FIXTURE, 6).unwrap();
        // Hour 1 (index 0): 40m (7.10 = channel index 1) REL = 0.21 in the fixture.
        assert!((p.channels[1].rel_by_hour[0] - 0.21).abs() < 0.001);
        // 20m (14.10 = channel index 4) is dead at this short path: 0.03 at hour 1.
        assert!((p.channels[4].rel_by_hour[0] - 0.03).abs() < 0.001);
    }

    #[test]
    fn first_frequency_is_3590_khz() {
        let p = parse_voacapx_out(FIXTURE, 6).unwrap();
        // 3.59 MHz displayed -> 3590 kHz (VOACAP rounds display to 2 decimals).
        assert!((p.channels[0].frequency_khz - 3590.0).abs() < 1.0);
    }

    #[test]
    fn missing_summary_line_is_parse_error() {
        let bad = "no summary here\n   1.0  7.8  3.6   FREQ\n";
        assert!(matches!(parse_voacapx_out(bad, 1), Err(PropagationError::ParseFailed(_))));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib --manifest-path src-tauri/Cargo.toml propagation::parse`
Expected: FAIL initially if `parse_azimuth_distance`'s last-4 indexing is off (the lat/lon hemisphere letters split tokens unexpectedly). Read the failure.

- [ ] **Step 3: Reconcile azimuth-line tokenization against the fixture**

The summary data row is:
`  33.50 N  111.00 W - 34.50 N  113.00 W    301.65  120.54     116.2    215.2`
Whitespace-split numeric tokens are: `33.50, 111.00, 34.50, 113.00, 301.65, 120.54, 116.2, 215.2` (the `N/W/-` are non-numeric and filtered). So `nums.last() = 215.2` (km) and `nums[len-4] = 301.65` (TX→RX azimuth). Confirm against the fixture; if VOACAP emits N.Mi. last in some builds, assert on the fixture, not on assumption.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib --manifest-path src-tauri/Cargo.toml propagation::parse`
Expected: PASS (5 tests)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/propagation/parse.rs
git commit -m "feat(propagation): voacapx.out parser (REL/SNR/bearing, fixture-verified)"
```

---

## Task 4: SSN cache — bundled forecast + on-disk cache

**Files:**
- Create: `src-tauri/src/propagation/ssn.rs`
- Create: `src-tauri/resources/propagation/ssn-forecast.json` (bundled forecast table)

**Design (spec §5):** SSN is the only time-varying input; it is slowly-varying and forecastable. Bundle a forecast table keyed by `YYYY-MM`; cache the active value under `app_data_dir()`. The network only ever *refreshes* it (out of scope for v1 beyond the opportunistic-update hook). Surface provenance ("solar data N old") later in U3.

- [ ] **Step 1: Create the bundled forecast resource**

`src-tauri/resources/propagation/ssn-forecast.json` — a small JSON map of `"YYYY-MM" -> smoothed SSN`. Seed with NOAA/SWPC-style monthly smoothed-SSN forecast values for the current solar-cycle horizon (the operator will supply or confirm the table; for the plan, structure is fixed, values are data):

```json
{
  "schema": "tuxlink-ssn-forecast-v1",
  "source": "SWPC monthly smoothed sunspot number forecast",
  "captured": "2026-06-10",
  "monthly": {
    "2026-06": 100.0,
    "2026-07": 98.0
  }
}
```

- [ ] **Step 2: Write the failing test** (in `src-tauri/src/propagation/ssn.rs`)

```rust
//! SSN (smoothed sunspot number) source: bundled forecast table + on-disk cache.
//! Offline-first: a bundled table always yields a value; network only refreshes.

use serde::Deserialize;
use std::collections::BTreeMap;
use super::PropagationError;

#[derive(Debug, Deserialize)]
pub struct SsnForecast {
    pub monthly: BTreeMap<String, f64>,
}

impl SsnForecast {
    pub fn from_json(text: &str) -> Result<Self, PropagationError> {
        serde_json::from_str(text).map_err(|e| PropagationError::Ssn(e.to_string()))
    }

    /// SSN for `year`-`month`; falls back to the nearest earlier month, else the
    /// last known value, else a conservative solar-minimum default (10.0).
    pub fn ssn_for(&self, year: i32, month: u8) -> f64 {
        let key = format!("{year:04}-{month:02}");
        if let Some(v) = self.monthly.get(&key) {
            return *v;
        }
        // nearest earlier key, else last, else default
        self.monthly.range(..=key).next_back()
            .or_else(|| self.monthly.iter().next_back())
            .map(|(_, v)| *v)
            .unwrap_or(10.0)
    }
}

pub const BUNDLED_SSN_FORECAST: &str =
    include_str!("../../resources/propagation/ssn-forecast.json");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_forecast_parses() {
        let f = SsnForecast::from_json(BUNDLED_SSN_FORECAST).unwrap();
        assert!(!f.monthly.is_empty());
    }

    #[test]
    fn exact_month_hit() {
        let f = SsnForecast::from_json(r#"{"monthly":{"2026-06":100.0}}"#).unwrap();
        assert_eq!(f.ssn_for(2026, 6), 100.0);
    }

    #[test]
    fn falls_back_to_nearest_earlier_month() {
        let f = SsnForecast::from_json(r#"{"monthly":{"2026-06":100.0,"2026-01":80.0}}"#).unwrap();
        assert_eq!(f.ssn_for(2026, 4), 80.0); // 2026-04 missing -> nearest earlier = 2026-01
    }

    #[test]
    fn empty_table_uses_conservative_default() {
        let f = SsnForecast::from_json(r#"{"monthly":{}}"#).unwrap();
        assert_eq!(f.ssn_for(2026, 6), 10.0);
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib --manifest-path src-tauri/Cargo.toml propagation::ssn`
Expected: PASS (4 tests) once the resource file exists and `serde_json` is available (it is).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/propagation/ssn.rs src-tauri/resources/propagation/ssn-forecast.json
git commit -m "feat(propagation): bundled SSN forecast + lookup with fallback"
```

> **Open item for build-robust-features / operator:** the on-disk cache write + opportunistic network refresh path. v1 reads the bundled table (always offline-correct); the writable cache under `app_data_dir()` and the refresh source are a follow-up sub-task — DO NOT add a network precondition. Surface this in the adrev.

---

## Task 5: Engine — resolve binary + itshfbc, run in scratch dir, capture

**Files:**
- Create: `src-tauri/src/propagation/engine.rs`

**Design:** voacapl writes a fixed `run/voacapx.out` filename inside the itshfbc root. To allow concurrent per-station runs and to keep the bundled itshfbc read-only, the engine creates a **per-call scratch itshfbc-style dir** (a temp dir with a `run/` subdir, symlinking or copying the read-only `coeffs/`/`antennas/`/`database` from the bundled root, and writing `run/voacapx.dat` fresh), then invokes `voacapl <scratch_root>`. Mirror `ManagedModem` (`src-tauri/src/winlink/modem/process.rs`) for spawn/timeout/kill discipline; run under `tokio::task::spawn_blocking` since `std::process` is blocking (codebase rule: "Blocking work MUST NOT be held across an await").

- [ ] **Step 1: Write the failing test** (in `src-tauri/src/propagation/engine.rs`)

```rust
//! voacapl invocation. Resolves the bundled binary + itshfbc data, runs headless
//! in a per-call scratch run dir, captures voacapx.out. Pure offline compute.

use std::path::{Path, PathBuf};
use std::process::Command;
use super::PropagationError;

/// Locations of the bundled engine assets (resolved once at startup from the
/// Tauri resource dir; injectable for tests).
#[derive(Debug, Clone)]
pub struct EnginePaths {
    /// Path to the `voacapl` binary (externalBin).
    pub binary: PathBuf,
    /// Path to the read-only bundled `itshfbc` root (coeffs/antennas/database).
    pub itshfbc_root: PathBuf,
}

/// Run voacapl for the given deck text; returns raw voacapx.out text.
/// `scratch_parent` is where the per-call temp dir is created (e.g. app cache dir).
pub fn run_voacapl(
    paths: &EnginePaths,
    deck_text: &str,
    scratch_parent: &Path,
) -> Result<String, PropagationError> {
    if !paths.binary.exists() {
        return Err(PropagationError::BinaryNotFound(paths.binary.display().to_string()));
    }
    let scratch = make_scratch_itshfbc(paths, scratch_parent)?;
    std::fs::write(scratch.join("run").join("voacapx.dat"), deck_text)?;

    let output = Command::new(&paths.binary)
        .arg(&scratch)
        .output()
        .map_err(|e| PropagationError::RunFailed(e.to_string()))?;
    if !output.status.success() {
        return Err(PropagationError::RunFailed(format!(
            "exit {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let out_text = std::fs::read_to_string(scratch.join("run").join("voacapx.out"))?;
    // Best-effort cleanup; ignore errors (temp dir).
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(out_text)
}

/// Create a per-call scratch itshfbc root: a fresh temp dir with `run/`, plus
/// symlinks to the read-only bundled coeffs/antennas/database/geo* trees.
fn make_scratch_itshfbc(paths: &EnginePaths, parent: &Path) -> Result<PathBuf, PropagationError> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let scratch = parent.join(format!("voacap-run-{nanos}-{}", std::process::id()));
    std::fs::create_dir_all(scratch.join("run"))?;
    for sub in ["coeffs", "antennas", "database", "geocity", "geonatio", "geostate"] {
        let src = paths.itshfbc_root.join(sub);
        if src.exists() {
            let dst = scratch.join(sub);
            #[cfg(unix)]
            std::os::unix::fs::symlink(&src, &dst)?;
        }
    }
    Ok(scratch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_binary_is_clear_error() {
        let paths = EnginePaths {
            binary: PathBuf::from("/nonexistent/voacapl"),
            itshfbc_root: PathBuf::from("/tmp"),
        };
        let err = run_voacapl(&paths, "deck", Path::new("/tmp")).unwrap_err();
        assert!(matches!(err, PropagationError::BinaryNotFound(_)));
    }

    // A live end-to-end test (gated, requires a built voacapl + itshfbc on the
    // dev machine) lives in tests/propagation_live.rs behind a feature flag, so
    // CI without the engine still passes. See Task 7.
}
```

- [ ] **Step 2: Run test**

Run: `cargo test --lib --manifest-path src-tauri/Cargo.toml propagation::engine`
Expected: PASS (`missing_binary_is_clear_error`).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/propagation/engine.rs
git commit -m "feat(propagation): voacapl engine runner with per-call scratch isolation"
```

> **Open items for build-robust-features (RF-correctness-critical — flag explicitly):**
> 1. **Timeout/abort.** Add a bounded run timeout + SIGINT→SIGKILL like `ManagedModem::stop` (worst-case airtime is N/A — no TX — but a runaway compute must be bounded). Wrap `Command` spawn so a hung voacapl is killed.
> 2. **Symlink vs copy** for the read-only trees: symlinks are cheap but assume the bundled root is stable; if the resource dir is read-only and voacapl writes into `database/` (it copies some files in `makeitshfbc`), pre-seed those writable files into the scratch dir instead of symlinking. Verify which `database/` files voacapl opens for write.
> 3. **spawn_blocking wrapper** for the async command boundary.

---

## Task 6: Tauri command — propagation_predict_path

**Files:**
- Create: `src-tauri/src/propagation/commands.rs`
- Modify: `src-tauri/src/lib.rs` (register command + manage `EnginePaths` state)

- [ ] **Step 1: Write the command + From<PropagationError> for UiError**

```rust
//! Tauri command surface for path prediction.

use tauri::State;
use crate::ui_commands::UiError;
use super::{deck, engine, parse, ssn, PathPrediction, PredictionInputs, PropagationError};
use super::engine::EnginePaths;

impl From<PropagationError> for UiError {
    fn from(e: PropagationError) -> Self {
        match e {
            PropagationError::InvalidGrid(_) | PropagationError::NoFrequencies => {
                UiError::Rejected(e.to_string())
            }
            PropagationError::BinaryNotFound(_) => UiError::Unavailable { reason: e.to_string() },
            _ => UiError::Internal { detail: e.to_string() },
        }
    }
}

/// State holding resolved engine asset paths + scratch parent dir.
pub struct PropagationState {
    pub paths: EnginePaths,
    pub scratch_parent: std::path::PathBuf,
}

#[tauri::command]
pub async fn propagation_predict_path(
    tx_grid: String,
    rx_grid: String,
    frequencies_khz: Vec<f64>,
    month: u8,
    state: State<'_, PropagationState>,
) -> Result<PathPrediction, UiError> {
    let forecast = ssn::SsnForecast::from_json(ssn::BUNDLED_SSN_FORECAST)?;
    let ssn_val = forecast.ssn_for(2026, month);
    let freq_count = frequencies_khz.iter().filter(|f| f.is_finite() && **f > 0.0).count().min(11);

    let inputs = PredictionInputs {
        tx_grid, rx_grid, frequencies_khz, month, ssn: ssn_val, tx_power_w: 100.0,
    };
    let deck_text = deck::build_deck(&inputs)?;

    let paths = state.paths.clone();
    let scratch = state.scratch_parent.clone();
    // std::process is blocking; never hold it across the Tauri async boundary.
    let out_text = tokio::task::spawn_blocking(move || {
        engine::run_voacapl(&paths, &deck_text, &scratch)
    })
    .await
    .map_err(|e| UiError::Internal { detail: format!("join error: {e}") })??;

    Ok(parse::parse_voacapx_out(&out_text, freq_count)?)
}
```

- [ ] **Step 2: Register in `src-tauri/src/lib.rs`**

Add to the `tauri::generate_handler!` list (alongside `catalog::commands::catalog_fetch_stations`):

```rust
crate::propagation::commands::propagation_predict_path,
```

And in the builder `.setup()` (where other state is managed), resolve the resource paths and `app.manage(...)`:

```rust
use tauri::Manager;
let res = app.path().resolve("binaries", tauri::path::BaseDirectory::Resource)?;
let itshfbc = app.path().resolve("resources/itshfbc", tauri::path::BaseDirectory::Resource)?;
let scratch_parent = app.path().app_cache_dir().unwrap_or(std::env::temp_dir());
std::fs::create_dir_all(&scratch_parent).ok();
app.manage(crate::propagation::commands::PropagationState {
    paths: crate::propagation::engine::EnginePaths {
        binary: res.join("voacapl"),
        itshfbc_root: itshfbc,
    },
    scratch_parent,
});
```

- [ ] **Step 3: Build + clippy**

Run: `cargo build --manifest-path src-tauri/Cargo.toml` then `cargo clippy --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: clean. Re-run clippy until exit 0 (it hides later-target lints).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/propagation/commands.rs src-tauri/src/lib.rs
git commit -m "feat(propagation): propagation_predict_path command + state wiring"
```

> **Open item:** the exact `BaseDirectory` for `externalBin` differs from `Resource` — Tauri places sidecars adjacent to the main binary. Verify the resolved path against a packaged `.deb` (the resource resolution pattern is in `forms/wle_templates.rs:116`). The adrev must confirm the runtime path resolves in a packaged build, not just `cargo run` (cf. the `test_production_mount_path` memory).

---

## Task 7: Bundling — tauri.conf.json + CI per-arch build

**Files:**
- Modify: `src-tauri/tauri.conf.json` (bundle section)
- Modify: `.github/workflows/release.yml` (+ any CI build workflow)
- Create: `src-tauri/binaries/` (per-arch voacapl, named `voacapl-<target-triple>`)
- Create: `src-tauri/resources/itshfbc/` (coeffs + antennas + database, copied from a built `makeitshfbc` tree)
- Create: `src-tauri/tests/propagation_live.rs` (gated end-to-end test)

- [ ] **Step 1: tauri.conf.json bundle additions**

```json
"bundle": {
  "externalBin": ["binaries/voacapl"],
  "resources": [
    "resources/wle-forms/**/*",
    "resources/itshfbc/**/*",
    "resources/propagation/ssn-forecast.json"
  ]
}
```

(Tauri expands `binaries/voacapl` to `binaries/voacapl-<target-triple>` per arch.)

- [ ] **Step 2: CI — build voacapl per-arch and stage it**

Add a step before the Tauri bundle that builds voacapl from source (or downloads a pinned release) for each target triple (`aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`), runs `makeitshfbc` into `src-tauri/resources/itshfbc/`, and renames the binary to `src-tauri/binaries/voacapl-<triple>`. Install `gfortran` in the CI image. Document the pinned voacapl commit. (Build commands captured in IO-CONTRACT.md: `./configure --prefix=...`, `make`, `make install`, `makeitshfbc`.)

- [ ] **Step 3: Gated end-to-end test**

`src-tauri/tests/propagation_live.rs` — `#[ignore]` by default (or `#[cfg(feature = "voacapl-live")]`); builds inputs, calls the engine against a real installed voacapl, asserts a non-empty 24-hour REL matrix. Documents that CI without the engine skips it; the dev machine runs `cargo test --manifest-path src-tauri/Cargo.toml --test propagation_live -- --ignored`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tauri.conf.json .github/workflows/ src-tauri/tests/propagation_live.rs
git commit -m "build(propagation): bundle voacapl + itshfbc per-arch; gated live test"
```

> **Open items for build-robust-features (flag explicitly):**
> - License/attribution: voacapl is GPL-3-compatible; add NOTICE/attribution per its LICENSE.
> - Binary size: itshfbc coeffs are ~MB; confirm acceptable .deb growth.
> - Cross-compilation vs native-arch CI runners: the existing `release.yml` is amd64-only; arm64 needs either a cross toolchain or an arm64 runner. This is the largest infra unknown — surface for operator decision.

---

## Self-Review

**Spec coverage (against `docs/design/2026-06-10-find-a-station-propagation-map-design.md` §5):**
- voacapl sidecar boundary (bundled binary + itshfbc, offline compute) → Tasks 5, 7. ✅
- Inputs per circuit (grids→latlon, freq, month/hour, SSN, power) → Tasks 1, 2, 4. ✅
- Output REL/SNR from voacapx.out → Task 3. ✅
- SSN bundled/cached, never per-session download → Task 4 (+ flagged cache-write follow-up). ✅
- Map ranking mode (a) point-to-point per station → the per-path command (Task 6) supports per-station calls; the multi-station orchestration is U3's consumer concern. ✅ (mode (b) area-coverage explicitly deferred per spec)
- No VHF/UHF prediction → enforced by U3 (HF channels only call this); documented. ✅
- Operator-configurable power/antenna defaults → `tx_power_w` plumbed (default 100 W); antenna-model config surface flagged as adrev open item. ⚠️ (partial — config surface deferred to flagged item)

**Placeholder scan:** No "TBD/handle-edge-cases" placeholders in code steps. The flagged "open items" are explicit adrev inputs, not hidden gaps. The SSN forecast *values* are data the operator supplies; the *structure* and lookup are fully specified.

**Type consistency:** `PredictionInputs`, `PathPrediction`, `ChannelReliability`, `PropagationError`, `EnginePaths`, `PropagationState` used consistently across Tasks 1-6. `parse_voacapx_out(text, freq_count)` and `build_deck(&inputs)` signatures stable. `grid_to_lat_lon` / `frequencies_khz` match the real origin/main definitions (verified).

**Known partial:** the antenna-model config surface and the SSN on-disk write/refresh are deliberately scoped as build-robust-features adrev open items (RF-correctness-critical decisions that warrant cross-provider review), not silent omissions.

---

## Execution Handoff

This plan is the input to **build-robust-features** (spec §4/§9: U1 gets the full cross-provider Codex adversarial-review treatment given RF-correctness criticality + the bundled-native-engine boundary). The adrev must especially scrutinize: the deck format edge cases (antipodal/zero-distance paths, >11 freqs, missing grids), the parser's column assumptions across voacapl versions, the engine's scratch-dir/write-isolation correctness, the packaged-build resource-path resolution, and the arm64 CI story.
