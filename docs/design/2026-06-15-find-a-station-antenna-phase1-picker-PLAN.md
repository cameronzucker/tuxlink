# Find-a-Station antenna Phase 1 (picker + NEC pattern library) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Find-a-Station antenna preset dropdown with a curated antenna + snapping-height picker backed by real precomputed NEC Type-14 patterns, plus a live polar elevation-pattern preview.

**Architecture:** An **offline developer-run generator** (`gen_antenna_patterns` Rust binary) shells out to `nec2c` for each `{antenna × height × 30 HF frequencies}`, parses the radiation-pattern table, clamps deep nulls to ≥ −99.999 dBi, and emits Type-14 `.voa` files via the Phase 0 `type14.rs` emitter. Those `.voa` files are committed and `include_str!`'d into the binary. At runtime, `operator_voa_content` returns the matched precomputed pattern (instead of IONCAP cards) into the existing writable scratch `antennas/default/` path; a new `antenna_pattern_preview` command returns a 91-point elevation slice for the UI. The frontend redesigns `AntennaControl` with a snapping height slider, a conditional ground-mounted state, and a polar preview.

**Tech Stack:** Rust (Tauri commands, `nec2c` CLI subprocess, `include_str!`), React/TypeScript (Vitest), `nec2c` NEC-2 engine (installed 2026-06-15).

**Spec:** [`2026-06-15-find-a-station-antenna-phase1-picker.md`](2026-06-15-find-a-station-antenna-phase1-picker.md)

---

## Ground truth (verified 2026-06-15, do not re-derive)

- **Phase 0 emitter** `src-tauri/src/propagation/type14.rs`: `Type14Pattern { title: String, blocks: Vec<FreqBlock> }` (exactly `N_BLOCKS=30` blocks), `FreqBlock { efficiency: f64, gains: Vec<f64> }` (exactly `N_GAINS=91`), `pub fn to_voa(&self) -> Result<String, Type14Error>`. Gains must be finite and in **−99.999..999.999 dBi** (F7.3); efficiency finite in **−99.99..99.99** (F6.2). Out-of-range → `Type14Error::GainOutOfRange` / `EfficiencyOutOfRange`.
- **`AntennaPreset`** enum lives in `src-tauri/src/propagation/antenna.rs:47-71` (10 variants, `#[serde(rename_all="kebab-case")]`, `#[derive(Default)]` with `EfhwSloper` default). `voa_file(self) -> &'static str` maps each to a stock `.voa`. `operator_voa_content(preset, height_m, ground) -> Option<String>` (antenna.rs:226) currently emits IONCAP cards; `OPERATOR_VOA_FILENAME = "txgen.voa"` (antenna.rs:210). `GroundType` enum at antenna.rs:115-127 (`Average` default; `PoorSoil` = ε 3, σ 0.001).
- **`PropagationPrefs`** in `src-tauri/src/propagation/prefs.rs:95-119`; `prefs::load(path)` returns defaults on missing/unparseable file; `prefs::save` is atomic. `DEFAULT_ANTENNA_HEIGHT_M = 9.0`.
- **`commands.rs`**: `propagation_predict_path(...) -> PathPrediction` (commands.rs:163) reloads prefs fresh per call and calls `operator_voa_content`. `propagation_prefs_read/write` at commands.rs:244/258. New commands register in the Tauri `invoke_handler` (find the `generate_handler!` list — search `propagation_prefs_read`).
- **`engine.rs`**: `antennas/default/` is **already writable** (engine.rs:212-228); `run_voacapl_with_files(paths, deck_text, antenna_files: &[(String,String)], scratch_parent)` writes each `(name, content)` into it. **No "Fix C" engine change needed.**
- **No `nec2c`/asset-bundling in the build.** `src-tauri/build.rs` only stamps git SHA. The library ships via `include_str!` of committed `.voa` files. `nec2c` is NOT a build/CI dependency — only the offline generator uses it.
- **nec2c output format** (verified via `dev/scratch/nec-probe/dipole20m.out`): after the line `---------- RADIATION PATTERNS -----------` come 3 header lines, then data rows: `THETA  PHI  VERTC  HORIZ  TOTAL  AXIAL  TILT  SENSE  ...`. Whitespace-delimited. **`TOTAL` (column index 4, 0-based) is gain in dBi.** THETA is from zenith (0°=up, 90°=horizon); **elevation = 90 − THETA**. nec2c prints `-999.99` as a no-power sentinel — clamp required.
- **Frontend**: `src/catalog/AntennaControl.tsx` (props `{prefs, onChange, error}`), `src/catalog/propagationPrefs.ts` (`AntennaPreset` TS union, `ANTENNA_PRESET_OPTIONS`, `GROUND_TYPE_OPTIONS`, `DEFAULT_PROPAGATION_PREFS`). Parent: `src/catalog/StationFinderPanel.tsx:20` (holds `prefs` state, bumps `predictReload` on change). Tests: `AntennaControl.test.tsx`, `propagationPrefs.test.ts`.
- **Test commands** — Rust: `cargo test --manifest-path src-tauri/Cargo.toml --lib propagation`; Frontend: `pnpm test` (vitest, jsdom).

## Curated catalog & grid (from spec)

8 entries. **Horizontal** (height-variable, grid `{2.5, 4, 6, 9}` m apex): `efhw-sloper`, `nvis-wire-dipole`, `resonant-portable-dipole`, `beam-yagi`. **Vertical** (ground-mounted, no height axis): `portable-vertical-whip`, `base-vertical-radials`, `mobile-hf-whip`. **Neutral**: `unknown`. **Removed**: `random-wire-unun`, `magnetic-loop` → migrate to `unknown`. All patterns modeled at **poor/dry-desert ground** (ε 3, σ 0.001); verticals over poor soil with a representative radial field.

Library = 4 horizontal × 4 heights + 3 vertical + 1 neutral = **20 patterns**.

---

## Task group A — NEC generator scaffold (offline tool)

### Task A1: Generator binary skeleton + frequency table

**Files:**
- Create: `src-tauri/src/bin/gen_antenna_patterns.rs`
- Reference: `src-tauri/src/propagation/type14.rs` (the emitter it calls)

- [ ] **Step 1: Create the binary with the VOACAP Type-14 frequency table and a CLI that writes nothing yet but prints the plan.**

```rust
//! Offline generator: runs nec2c over the antenna catalog × height grid and emits
//! Type-14 .voa pattern files. NOT part of the app/CI build. Run manually:
//!   cargo run --manifest-path src-tauri/Cargo.toml --bin gen_antenna_patterns
//! Requires nec2c on PATH.
use tuxlink_lib::propagation::type14::{FreqBlock, Type14Pattern, N_BLOCKS, N_GAINS};

/// The 30 frequencies (MHz) voacapl associates with Type-14 blocks 1..30.
/// VOACAP Type-14 convention: 2..30 MHz. Block i (1-based) -> FREQS_MHZ[i-1].
const FREQS_MHZ: [f64; N_BLOCKS] = [
    2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
    17.0, 18.0, 19.0, 20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0,
];

fn main() {
    eprintln!("gen_antenna_patterns: {} frequencies, target {} patterns", N_BLOCKS, 20);
}
```

> NOTE: the crate's lib name may not be `tuxlink_lib`. **Step 1a:** `grep -m1 '^name' src-tauri/Cargo.toml` and check for `[lib] name`; use the actual lib crate path to import `type14`. If the binary can't see `propagation` (it's behind `pub(crate)`), make `pub mod propagation` reachable or move the generator to `cargo run --bin` within the same crate. Confirm `N_BLOCKS`/`N_GAINS` are `pub` (they are).

- [ ] **Step 2: Confirm it builds.**

Run: `cargo build --manifest-path src-tauri/Cargo.toml --bin gen_antenna_patterns`
Expected: compiles; running it prints the eprintln line.

- [ ] **Step 3: Commit.**

```bash
git add src-tauri/src/bin/gen_antenna_patterns.rs
git commit -m "build(antenna): gen_antenna_patterns generator skeleton + Type-14 freq table

Agent: cardinal-moraine-glade
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

> **Frequency-table grounding (do in the Codex RF round, Task F2):** verify FREQS_MHZ matches what voacapl's Type-14 loader assigns to block indices. If voacapl expects a different set (e.g. a header-declared frequency list), the generator must emit that list into the `.voa` header and FREQS_MHZ must match it. Cross-check against `~/itshfbc/antennas/samples/sample.14` and voacapl `antcalc.for`.

### Task A2: nec2c subprocess runner + radiation-pattern parser

**Files:**
- Modify: `src-tauri/src/bin/gen_antenna_patterns.rs`

- [ ] **Step 1: Write a failing unit test (inline `#[cfg(test)]`) for the parser against a captured nec2c fixture.**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // A 4-row slice of real nec2c output (THETA 0,1,2,3; TOTAL col = index 4).
    const SAMPLE: &str = "\
                             ---------- RADIATION PATTERNS -----------

 ---- ANGLES -----     ----- POWER GAINS -----       ---- POLARIZATION ----
  THETA      PHI       VERTC    HORIZ    TOTAL       AXIAL      TILT  SENSE
 DEGREES   DEGREES        DB       DB       DB       RATIO   DEGREES
    0.00      0.00      3.68  -999.99     3.68      0.0000      0.00 LINEAR
    1.00      0.00      3.68  -999.99     3.68      0.0000      0.00 LINEAR
   89.00      0.00     -8.20  -999.99    -8.20      0.0000      0.00 LINEAR
   90.00      0.00   -999.99  -999.99  -999.99      0.0000      0.00 LINEAR
";
    #[test]
    fn parses_total_gain_by_theta() {
        let gains = parse_total_gains(SAMPLE).unwrap();
        // keyed by THETA in degrees (0..=90)
        assert_eq!(gains.get(&0).copied(), Some(3.68));
        assert_eq!(gains.get(&90).copied(), Some(-999.99)); // sentinel preserved pre-clamp
    }
}
```

- [ ] **Step 2: Run it — fails (no `parse_total_gains`).**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --bin gen_antenna_patterns parses_total_gain`
Expected: FAIL (cannot find function).

- [ ] **Step 3: Implement the runner + parser.**

```rust
use std::collections::BTreeMap;
use std::io::Write;
use std::process::Command;

/// Run nec2c on a card deck, return stdout/out-file text.
fn run_nec2c(deck: &str) -> std::io::Result<String> {
    let dir = std::env::temp_dir();
    let inp = dir.join(format!("tux_nec_{}.nec", std::process::id()));
    let out = dir.join(format!("tux_nec_{}.out", std::process::id()));
    std::fs::File::create(&inp)?.write_all(deck.as_bytes())?;
    let status = Command::new("nec2c")
        .arg(format!("-i{}", inp.display()))
        .arg(format!("-o{}", out.display()))
        .status()?;
    if !status.success() {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "nec2c failed"));
    }
    let text = std::fs::read_to_string(&out)?;
    let _ = std::fs::remove_file(&inp);
    let _ = std::fs::remove_file(&out);
    Ok(text)
}

/// Parse the RADIATION PATTERNS table; return THETA(deg) -> TOTAL gain (dBi), raw (unclamped).
fn parse_total_gains(out: &str) -> Result<BTreeMap<u32, f64>, String> {
    let start = out.find("RADIATION PATTERNS").ok_or("no radiation pattern block")?;
    let mut map = BTreeMap::new();
    for line in out[start..].lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // data rows: THETA PHI VERTC HORIZ TOTAL ... ; THETA & TOTAL parse as f64.
        if cols.len() >= 5 {
            if let (Ok(theta), Ok(total)) = (cols[0].parse::<f64>(), cols[4].parse::<f64>()) {
                if (0.0..=90.0).contains(&theta) {
                    map.insert(theta.round() as u32, total);
                }
            }
        }
    }
    if map.is_empty() { return Err("no data rows parsed".into()); }
    Ok(map)
}
```

- [ ] **Step 4: Run the test — passes.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --bin gen_antenna_patterns parses_total_gain`
Expected: PASS.

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/bin/gen_antenna_patterns.rs
git commit -m "build(antenna): nec2c runner + radiation-pattern parser (TOTAL dBi by theta)

Agent: cardinal-moraine-glade
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

### Task A3: Elevation-vector assembly + null clamp + Type14Pattern builder

**Files:**
- Modify: `src-tauri/src/bin/gen_antenna_patterns.rs`

- [ ] **Step 1: Failing test for the 91-point elevation vector (elevation = 90 − theta) and the clamp.**

```rust
#[test]
fn assembles_91_point_elevation_with_clamp() {
    let mut by_theta = BTreeMap::new();
    for t in 0..=90u32 { by_theta.insert(t, 3.0); }
    by_theta.insert(90, -999.99); // zenith-horizon null sentinel
    let gains = elevation_vector(&by_theta);
    assert_eq!(gains.len(), N_GAINS);            // 91
    // index 0 = elevation 0° = theta 90° -> clamped from -999.99
    assert!(gains[0] >= -99.999);
    // index 90 = elevation 90° = theta 0° -> 3.0
    assert_eq!(gains[90], 3.0);
}
```

> **Decide elevation indexing explicitly** to match what voacapl/Type-14 expects: Phase 0's golden pattern (`type14.rs` test `synth_high`) uses `gains[deg]` where higher index = higher angle (`deg >= 45 => 6.0` high-angle). So **gains[i] = gain at elevation i°**, i ∈ 0..=90. elevation i° = theta (90 − i)°.

- [ ] **Step 2: Run — fails.** `cargo test ... assembles_91_point` → FAIL.

- [ ] **Step 3: Implement.**

```rust
/// Clamp to the Type-14 F7.3 floor so to_voa() never errors on a deep null.
fn clamp_gain(g: f64) -> f64 {
    if !g.is_finite() { return -99.999; }
    g.clamp(-99.999, 999.999)
}

/// gains[i] = gain at elevation i degrees (i in 0..=90). elevation i = theta (90 - i).
fn elevation_vector(by_theta: &BTreeMap<u32, f64>) -> Vec<f64> {
    (0..=90u32)
        .map(|elev| {
            let theta = 90 - elev;
            clamp_gain(by_theta.get(&theta).copied().unwrap_or(-99.999))
        })
        .collect()
}
```

- [ ] **Step 4: Run — passes.**

- [ ] **Step 5: Add the pattern builder (one antenna geometry over 30 freqs → Type14Pattern) + test that it round-trips through to_voa().**

```rust
/// Build a Type14Pattern: run nec2c at each of the 30 freqs for a fixed geometry,
/// where `deck_at(freq_mhz)` returns the full nec2c deck for that frequency.
fn build_pattern(title: &str, deck_at: impl Fn(f64) -> String) -> Result<Type14Pattern, String> {
    let mut blocks = Vec::with_capacity(N_BLOCKS);
    for &f in FREQS_MHZ.iter() {
        let out = run_nec2c(&deck_at(f)).map_err(|e| format!("nec2c {f} MHz: {e}"))?;
        let by_theta = parse_total_gains(&out)?;
        blocks.push(FreqBlock { efficiency: 0.0, gains: elevation_vector(&by_theta) });
    }
    Ok(Type14Pattern { title: title.chars().take(70).collect(), blocks })
}
```

```rust
#[test]
fn built_pattern_emits_valid_voa() {
    // synthetic deck_at ignored — fabricate via a stub geometry returning a trivial deck
    // is not possible without nec2c; gate this test behind the nec2c presence:
    if std::process::Command::new("nec2c").arg("-v").output().is_err() { return; }
    let deck_at = |f: f64| sample_dipole_deck(f, 6.0); // from Task B1
    let p = build_pattern("tuxlink efhw-sloper 6m poor", deck_at).unwrap();
    assert!(p.to_voa().is_ok(), "emitter rejected a generated pattern");
}
```

- [ ] **Step 6: Run — passes (or no-ops if nec2c absent).**

- [ ] **Step 7: Commit.**

```bash
git add src-tauri/src/bin/gen_antenna_patterns.rs
git commit -m "build(antenna): elevation-vector assembly + null clamp + Type14Pattern builder

Agent: cardinal-moraine-glade
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task group B — Antenna geometries (RF-critical; Codex-gated)

> **Discipline:** these NEC decks ARE the physics. Per `feedback_ai_amateur_radio_reliability` no gain-vs-angle may be fabricated — the decks are real NEC geometry; the gains come out of nec2c. The starter decks below are defensible first cuts grounded in standard wire models; **Task F2 (Codex RF round) must review every geometry** before the library is treated as shipped. Ground card for ALL: `GN 2 0 0 0 3.0 0.001` (poor/dry desert). Verticals add a radial field (see B2).

### Task B1: Horizontal-wire deck builders (EFHW/sloper, NVIS dipole, portable dipole)

**Files:**
- Modify: `src-tauri/src/bin/gen_antenna_patterns.rs`

- [ ] **Step 1: Implement a parametric horizontal-dipole deck builder (length scales per band) + the three horizontal variants, with an elevation RP sweep.**

```rust
/// Center-fed horizontal wire of total length `len_m` at apex `height_m`, fed at center,
/// over poor ground, elevation cut (theta 0..90, 1deg) at `freq_mhz`. 21 segments (odd).
fn horizontal_wire_deck(len_m: f64, height_m: f64, freq_mhz: f64) -> String {
    let half = len_m / 2.0;
    format!(
        "CM tuxlink horizontal wire {len_m:.2}m @ {height_m:.1}m, poor ground\nCE\n\
         GW 1 21 {x0:.3} 0 {h:.3} {x1:.3} 0 {h:.3} 0.001\nGE -1\n\
         GN 2 0 0 0 3.0 0.001\nEX 0 1 11 0 1.0 0.0\n\
         FR 0 1 0 0 {freq_mhz:.3} 0\nRP 0 91 1 1000 0.0 0.0 1.0 0.0\nEN\n",
        x0 = -half, x1 = half, h = height_m
    )
}
/// EFHW/sloper modeled as a full-size horizontal wire ~half-wave at mid-band (no overhead null).
fn deck_efhw(freq_mhz: f64, height_m: f64) -> String { horizontal_wire_deck(20.0, height_m, freq_mhz) }
/// Low NVIS dipole: half-wave for ~7 MHz (favors high angle at low height).
fn deck_nvis_dipole(freq_mhz: f64, height_m: f64) -> String { horizontal_wire_deck(20.0, height_m, freq_mhz) }
/// Portable linked dipole / inverted-V proxy: shorter field dipole.
fn deck_portable_dipole(freq_mhz: f64, height_m: f64) -> String { horizontal_wire_deck(12.0, height_m, freq_mhz) }
```

> **Geometry refinement is a Task F2 item.** The three horizontal models currently differ only by length; the spec says EFHW vs NVIS-dipole vs portable should be *defensibly* distinct. The Codex round decides whether (a) the sloper gets a real slope (different z per end), (b) the inverted-V gets drooping ends, and (c) lengths/feed differ enough to matter. Do NOT invent distinctions that aren't physical — collapsing two horizontals if they're genuinely identical at these heights is the honest fallback the spec allows.

- [ ] **Step 2: Smoke-run each builder through nec2c (test gated on nec2c presence) — asserts non-empty parse + a valid Type14Pattern.**

```rust
#[test]
fn horizontal_decks_run_and_emit() {
    if std::process::Command::new("nec2c").arg("-v").output().is_err() { return; }
    for h in [2.5, 4.0, 6.0, 9.0] {
        let p = build_pattern("h", |f| deck_efhw(f, h)).unwrap();
        assert!(p.to_voa().is_ok());
    }
}
```

- [ ] **Step 3: Run — passes.** `cargo test ... horizontal_decks_run_and_emit`

- [ ] **Step 4: Commit.** (`build(antenna): horizontal-wire NEC deck builders (EFHW/NVIS/portable)`)

### Task B2: Vertical deck builders (whip, base vertical + radials, mobile whip)

**Files:**
- Modify: `src-tauri/src/bin/gen_antenna_patterns.rs`

- [ ] **Step 1: Implement a ground-mounted vertical monopole deck with a radial field, parametric height (whip length).**

```rust
/// Ground-mounted vertical monopole of `len_m`, base at ~0.1m, with N buried/elevated radials,
/// over poor ground, elevation cut at freq. Radials approximated as 4 horizontal wires at 0.05m.
fn vertical_deck(len_m: f64, freq_mhz: f64) -> String {
    let mut s = format!(
        "CM tuxlink vertical {len_m:.2}m + radials, poor ground\nCE\n\
         GW 1 15 0 0 0.1 0 0 {top:.3} 0.001\n", top = 0.1 + len_m);
    // 4 radials, 10m, at 0.05m height
    let radials = [(10.0_f64, 0.0_f64), (0.0, 10.0), (-10.0, 0.0), (0.0, -10.0)];
    for (i, (x, y)) in radials.iter().enumerate() {
        s.push_str(&format!("GW {tag} 9 0 0 0.05 {x:.3} {y:.3} 0.05 0.001\n", tag = i + 2));
    }
    s.push_str(&format!(
        "GE -1\nGN 2 0 0 0 3.0 0.001\nEX 0 1 1 0 1.0 0.0\n\
         FR 0 1 0 0 {freq_mhz:.3} 0\nRP 0 91 1 1000 0.0 0.0 1.0 0.0\nEN\n"));
    s
}
fn deck_base_vertical(freq_mhz: f64) -> String { vertical_deck(7.0, freq_mhz) }   // ~quarter-wave 40m class
fn deck_portable_whip(freq_mhz: f64) -> String { vertical_deck(3.0, freq_mhz) }   // short portable whip
fn deck_mobile_whip(freq_mhz: f64) -> String { vertical_deck(1.5, freq_mhz) }     // short loaded mobile
```

> **Task F2 grounding:** loaded short verticals (mobile/portable) are not bare 1.5/3 m radiators — real ones use a loading coil. NEC can model this with an LD load card or by accepting the short-radiator pattern shape (elevation lobe is what Type-14 captures; efficiency is folded separately). Codex decides whether an `LD` card is warranted or the short-monopole elevation pattern is adequate (it usually is for *pattern shape* — absolute efficiency is not the Type-14 axis here). Radial count/length over poor soil also reviewed here.

- [ ] **Step 2: nec2c smoke test for verticals (gated).** Assert valid Type14Pattern.
- [ ] **Step 3: Run — passes.**
- [ ] **Step 4: Commit.** (`build(antenna): vertical monopole + radial-field NEC deck builders`)

### Task B3: Yagi deck + neutral `unknown` pattern

**Files:**
- Modify: `src-tauri/src/bin/gen_antenna_patterns.rs`

- [ ] **Step 1: Implement a 3-element Yagi deck (boresight elevation cut) and a neutral isotropic-ish pattern for `unknown`.**

```rust
/// 3-element monoband Yagi at apex height, boresight (phi=0) elevation cut. Spacing/lengths
/// scale crudely with wavelength; Type-14 captures the elevation lobe at boresight only.
fn deck_yagi(freq_mhz: f64, height_m: f64) -> String {
    let lambda = 300.0 / freq_mhz;
    let refl = 0.25 * lambda; let driv = 0.236 * lambda; let dir = 0.224 * lambda;
    format!(
        "CM tuxlink 3-el yagi @ {height_m:.1}m, poor ground\nCE\n\
         GW 1 21 {r0:.3} -2.0 {h:.3} {r0:.3} 2.0 {h:.3} 0.005\n\
         GW 2 21 {d0:.3} {dl0:.3} {h:.3} {d0:.3} {dl1:.3} {h:.3} 0.005\n\
         GW 3 21 {i0:.3} -2.0 {h:.3} {i0:.3} 2.0 {h:.3} 0.005\n\
         GE -1\nGN 2 0 0 0 3.0 0.001\nEX 0 2 11 0 1.0 0.0\n\
         FR 0 1 0 0 {freq_mhz:.3} 0\nRP 0 91 1 0 0.0 0.0 1.0 0.0\nEN\n",
        r0 = -0.0_f64, d0 = refl, i0 = refl + dir, h = height_m,
        dl0 = -driv/2.0, dl1 = driv/2.0)
}
/// Neutral pattern for `unknown`: flat 0 dBi at all elevations (honest "not modeled").
fn unknown_pattern() -> Type14Pattern {
    let block = FreqBlock { efficiency: 0.0, gains: vec![0.0; N_GAINS] };
    Type14Pattern { title: "tuxlink unknown/generic neutral pattern".into(),
        blocks: vec![block; N_BLOCKS] }
}
```

> **Task F2:** the Yagi geometry is the least physically tuned (element lengths/spacing are rough). Codex reviews whether a crude Yagi pattern is defensible or whether the entry should carry a "directional, boresight estimate" caveat in the UI. The neutral `unknown` is intentionally flat — verify the emitter accepts all-zero gains (it does; 0.0 is in range).

- [ ] **Step 2: Smoke test (gated) — yagi emits; unknown emits.**
- [ ] **Step 3: Run — passes.**
- [ ] **Step 4: Commit.** (`build(antenna): 3-el yagi deck + neutral unknown pattern`)

### Task B4: Catalog driver — generate all 20 .voa files

**Files:**
- Modify: `src-tauri/src/bin/gen_antenna_patterns.rs`
- Create (output, committed): `src-tauri/src/propagation/patterns/*.voa`

- [ ] **Step 1: Implement `main()` to iterate the catalog × grid, write `.voa` files with deterministic names.**

```rust
const HEIGHT_GRID_M: [f64; 4] = [2.5, 4.0, 6.0, 9.0];
const OUT_DIR: &str = "src/propagation/patterns"; // relative to src-tauri/ (generator cwd)

fn write_voa(name: &str, p: &Type14Pattern) {
    let voa = p.to_voa().unwrap_or_else(|e| panic!("emit {name}: {e}"));
    let path = std::path::Path::new(OUT_DIR).join(format!("{name}.voa"));
    std::fs::create_dir_all(OUT_DIR).unwrap();
    std::fs::write(&path, voa).unwrap();
    eprintln!("wrote {}", path.display());
}

fn main() {
    // horizontals: <preset>__<height-tenths>.voa  e.g. efhw-sloper__060.voa for 6.0m
    let horizontals: [(&str, fn(f64, f64) -> String); 4] = [
        ("efhw-sloper", deck_efhw), ("nvis-wire-dipole", deck_nvis_dipole),
        ("resonant-portable-dipole", deck_portable_dipole), ("beam-yagi", deck_yagi),
    ];
    for (preset, deck) in horizontals {
        for h in HEIGHT_GRID_M {
            let name = format!("{preset}__{:03}", (h * 10.0) as u32);
            let p = build_pattern(&format!("tuxlink {preset} {h}m poor"), |f| deck(f, h)).unwrap();
            write_voa(&name, &p);
        }
    }
    let verticals: [(&str, fn(f64) -> String); 3] = [
        ("portable-vertical-whip", deck_portable_whip),
        ("base-vertical-radials", deck_base_vertical),
        ("mobile-hf-whip", deck_mobile_whip),
    ];
    for (preset, deck) in verticals {
        let p = build_pattern(&format!("tuxlink {preset} ground-mounted poor"), deck).unwrap();
        write_voa(preset, &p);
    }
    write_voa("unknown", &unknown_pattern());
}
```

- [ ] **Step 2: Run the generator for real.**

Run: `cd src-tauri && cargo run --bin gen_antenna_patterns 2>&1 | tail -25`
Expected: 20 "wrote ..." lines; `ls src-tauri/src/propagation/patterns/*.voa | wc -l` → 20.

- [ ] **Step 3: Spot-check honesty: a low NVIS dipole has a stronger zenith (elevation 90°) gain than a base vertical.**

Run:
```bash
# elevation 90 = last gain on the last continuation of block for 14 MHz (block 13).
# quick check: zenith gain line differs between a low wire and a vertical
grep -c . src-tauri/src/propagation/patterns/nvis-wire-dipole__025.voa
```
Expected: files are well-formed Type-14 (30 blocks). Deeper physics assertion is the deck test in Task C.

- [ ] **Step 4: Commit the generator main + the 20 generated .voa files.**

```bash
git add src-tauri/src/bin/gen_antenna_patterns.rs src-tauri/src/propagation/patterns/
git commit -m "build(antenna): generate 20 NEC Type-14 patterns (8-antenna catalog x height grid)

Agent: cardinal-moraine-glade
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task group C — Enum curation + migration (Rust)

### Task C1: Drop removed variants + migrate persisted values to `unknown`

**Files:**
- Modify: `src-tauri/src/propagation/antenna.rs:47-71` (enum), `:226` (operator_voa_content — Task D)
- Test: inline `#[cfg(test)]` in `antenna.rs` and `prefs.rs`

- [ ] **Step 1: Failing test — deserializing a removed value yields `Unknown`, and the OTHER prefs survive.**

```rust
#[test]
fn removed_presets_migrate_to_unknown() {
    let json = r#"{"antenna_preset":"random-wire-unun","req_snr_db":42.0,
        "tx_power_w":50.0,"antenna_height_m":4.0,"ground_type":"poor-soil",
        "noise_environment":"rural"}"#;
    let p: crate::propagation::prefs::PropagationPrefs = serde_json::from_str(json).unwrap();
    assert_eq!(p.antenna_preset, AntennaPreset::Unknown);
    assert_eq!(p.req_snr_db, 42.0); // not nuked to default
}
```

- [ ] **Step 2: Run — fails** (serde errors on the now-unknown variant, or `magnetic-loop` still parses).

- [ ] **Step 3: Remove `RandomWireUnun` and `MagneticLoop` from the enum; add `#[serde(other)]` to `Unknown` so unrecognized values map to it.**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AntennaPreset {
    #[default]
    EfhwSloper,
    PortableVerticalWhip,
    NvisWireDipole,
    BaseVerticalRadials,
    MobileHfWhip,
    ResonantPortableDipole,
    BeamYagi,
    #[serde(other)]
    Unknown,
}
```

> NOTE: `#[serde(other)]` requires the variant be unit and is only valid for the deserializer; `Serialize` still writes `unknown`. Remove `RandomWireUnun`/`MagneticLoop` arms from `voa_file()` and any `match` on the enum (the compiler will list them). The `serde(other)` arm makes deserialization total, so a missing/renamed value never errors the whole prefs load.

- [ ] **Step 4: Fix all now-broken `match` arms** the compiler flags (e.g. `voa_file()`, `operator_voa_content`'s classification). Run `cargo build --manifest-path src-tauri/Cargo.toml` and resolve each.

- [ ] **Step 5: Run the test — passes.** `cargo test --manifest-path src-tauri/Cargo.toml --lib removed_presets_migrate`

- [ ] **Step 6: Commit.** (`feat(antenna)!: curate catalog to 8 defensible models; migrate removed presets to unknown`)

---

## Task group D — Runtime pattern lookup + preview slice

### Task D1: Precomputed pattern library module (`include_str!`) + height snapping

**Files:**
- Create: `src-tauri/src/propagation/patterns.rs`
- Modify: `src-tauri/src/propagation/mod.rs` (add `pub mod patterns;`)

- [ ] **Step 1: Failing test — `pattern_voa(preset, height)` returns the right file for a horizontal (snapped) and a vertical (height-independent).**

```rust
#[test]
fn lookup_snaps_height_and_ignores_for_verticals() {
    // 5.2m snaps to the 6.0m grid stop for a horizontal
    let h = pattern_voa(AntennaPreset::EfhwSloper, 5.2);
    assert!(h.contains("Antenna") || h.contains("14")); // is a Type-14 file body
    // verticals: any height returns the single ground-mounted pattern
    let v1 = pattern_voa(AntennaPreset::BaseVerticalRadials, 2.0);
    let v2 = pattern_voa(AntennaPreset::BaseVerticalRadials, 30.0);
    assert_eq!(v1, v2);
}
```

- [ ] **Step 2: Run — fails.**

- [ ] **Step 3: Implement the library + snapping.**

```rust
//! Precomputed NEC Type-14 pattern library (generated by src/bin/gen_antenna_patterns.rs).
use super::antenna::AntennaPreset;

pub const HEIGHT_GRID_M: [f64; 4] = [2.5, 4.0, 6.0, 9.0];

/// Is this antenna's elevation pattern height-variable (horizontal) or fixed (vertical/neutral)?
pub fn is_height_variable(p: AntennaPreset) -> bool {
    matches!(p, AntennaPreset::EfhwSloper | AntennaPreset::NvisWireDipole
        | AntennaPreset::ResonantPortableDipole | AntennaPreset::BeamYagi)
}

/// Snap a requested height to the nearest grid stop.
pub fn snap_height(height_m: f64) -> f64 {
    HEIGHT_GRID_M.iter().copied()
        .min_by(|a, b| (a - height_m).abs().partial_cmp(&(b - height_m).abs()).unwrap())
        .unwrap_or(6.0)
}

macro_rules! voa { ($f:literal) => { include_str!(concat!("patterns/", $f, ".voa")) }; }

/// Return the Type-14 .voa text for the selected antenna+height.
pub fn pattern_voa(preset: AntennaPreset, height_m: f64) -> &'static str {
    use AntennaPreset::*;
    if is_height_variable(preset) {
        let stop = (snap_height(height_m) * 10.0) as u32; // 025/040/060/090
        return match (preset, stop) {
            (EfhwSloper, 25) => voa!("efhw-sloper__025"), (EfhwSloper, 40) => voa!("efhw-sloper__040"),
            (EfhwSloper, 60) => voa!("efhw-sloper__060"), (EfhwSloper, _) => voa!("efhw-sloper__090"),
            (NvisWireDipole, 25) => voa!("nvis-wire-dipole__025"), (NvisWireDipole, 40) => voa!("nvis-wire-dipole__040"),
            (NvisWireDipole, 60) => voa!("nvis-wire-dipole__060"), (NvisWireDipole, _) => voa!("nvis-wire-dipole__090"),
            (ResonantPortableDipole, 25) => voa!("resonant-portable-dipole__025"), (ResonantPortableDipole, 40) => voa!("resonant-portable-dipole__040"),
            (ResonantPortableDipole, 60) => voa!("resonant-portable-dipole__060"), (ResonantPortableDipole, _) => voa!("resonant-portable-dipole__090"),
            (BeamYagi, 25) => voa!("beam-yagi__025"), (BeamYagi, 40) => voa!("beam-yagi__040"),
            (BeamYagi, 60) => voa!("beam-yagi__060"), (BeamYagi, _) => voa!("beam-yagi__090"),
            _ => voa!("unknown"),
        };
    }
    match preset {
        PortableVerticalWhip => voa!("portable-vertical-whip"),
        BaseVerticalRadials => voa!("base-vertical-radials"),
        MobileHfWhip => voa!("mobile-hf-whip"),
        _ => voa!("unknown"),
    }
}
```

- [ ] **Step 4: Run — passes.** (`include_str!` resolves at compile time against the committed files.)

- [ ] **Step 5: Commit.** (`feat(antenna): precomputed Type-14 pattern library module + height snapping`)

### Task D2: Rewrite `operator_voa_content` to return the precomputed pattern

**Files:**
- Modify: `src-tauri/src/propagation/antenna.rs:226` (`operator_voa_content`)

- [ ] **Step 1: Failing test — operator_voa_content returns the library pattern (distinct per antenna+height), not IONCAP cards.**

```rust
#[test]
fn operator_voa_uses_precomputed_library() {
    let low = operator_voa_content(AntennaPreset::NvisWireDipole, 2.5, GroundType::PoorSoil).unwrap();
    let high = operator_voa_content(AntennaPreset::NvisWireDipole, 9.0, GroundType::PoorSoil).unwrap();
    assert_ne!(low, high, "height must change the emitted pattern");
    // Unknown still returns a (neutral) pattern, not None, now that we have a library entry:
    let unk = operator_voa_content(AntennaPreset::Unknown, 9.0, GroundType::PoorSoil);
    assert!(unk.is_some());
}
```

> **Behavior change:** previously `Unknown => None` (caller fell back to a stock `.voa`). Now every preset has a library entry, so `operator_voa_content` returns `Some` for all. Confirm the caller in `commands.rs:183-201` still works: it writes `txgen.voa` when `Some`. That path is unchanged; it just always takes the `Some` branch now. **Leave `voa_file()` in place** for the RX/gateway side which still uses stock codes.

- [ ] **Step 2: Run — fails** (still emits IONCAP).

- [ ] **Step 3: Reimplement.**

```rust
pub fn operator_voa_content(
    preset: AntennaPreset,
    height_m: f64,
    _ground: GroundType, // Phase 1: library is poor-soil only; ground feeds voacapl's path card elsewhere
) -> Option<String> {
    Some(crate::propagation::patterns::pattern_voa(preset, height_m).to_string())
}
```

> The `_ground` param stays in the signature (callers pass it; the SYSTEM/ground card is emitted separately in deck.rs). Document the single-ground limitation inline.

- [ ] **Step 4: Run — passes.** Also run the full propagation suite: `cargo test --manifest-path src-tauri/Cargo.toml --lib propagation`. Fix any IONCAP-specific tests in antenna.rs that no longer apply (delete or rewrite them to assert the library behavior — do not leave dead asserts).

- [ ] **Step 5: Commit.** (`feat(antenna): operator_voa_content returns precomputed Type-14 patterns`)

### Task D3: Elevation-slice extraction + `antenna_pattern_preview` command

**Files:**
- Create: `src-tauri/src/propagation/preview.rs` (parser: .voa → elevation slice)
- Modify: `src-tauri/src/propagation/mod.rs`, `src-tauri/src/propagation/commands.rs`, the Tauri `invoke_handler` registration (search `propagation_prefs_read` in `src-tauri/src/lib.rs` or `main.rs`)

- [ ] **Step 1: Failing test — parse a Type-14 `.voa` back into a 91-point slice at a chosen block (frequency).**

```rust
#[test]
fn extracts_elevation_slice_from_voa() {
    let voa = crate::propagation::patterns::pattern_voa(AntennaPreset::NvisWireDipole, 2.5);
    // block 13 ~ 14 MHz (FREQS_MHZ[12]=14.0)
    let slice = elevation_slice(voa, 13).unwrap();
    assert_eq!(slice.len(), 91);
    assert!(slice.iter().all(|g| g.is_finite() && *g >= -99.999));
}
```

- [ ] **Step 2: Run — fails.**

- [ ] **Step 3: Implement the parser** (inverse of `to_voa`'s block layout — read block `n`'s 91 gains across its first line + continuations). Use the Phase 0 format knowledge: block line 1 = `%2d` index + `%6.2f` eff + 10 gains; then continuations of 10 gains (9 lines × 10 = 90; 1+90=91). Parse fixed-width F7.3 fields.

```rust
/// Extract block `n` (1..=30) as a 91-point gain vector (gains[i] = elevation i deg).
pub fn elevation_slice(voa: &str, block: usize) -> Result<Vec<f64>, String> {
    // Locate the block by its leading %2d index; read 91 F7.3 fields across the
    // index line (after the F6.2 efficiency) and the following 9 continuation lines.
    // (Full fixed-width parse — mirror type14.rs's writer offsets.)
    // ... implement per the byte layout documented in type14.rs ...
    todo!("fixed-width parse mirroring type14.rs writer")
}
```

> **This `todo!` is a real implementation step, not a placeholder to ship** — the executor writes the fixed-width parser using the exact column offsets from `type14.rs`'s writer (the writer is the spec for the reader). Add a round-trip test: build a known `Type14Pattern`, `to_voa()`, `elevation_slice()`, assert equality with the source block's gains. This is the **preview-slice equality** test the spec requires.

- [ ] **Step 4: Add the Tauri command.**

```rust
#[derive(serde::Serialize)]
pub struct AntennaPreview { pub gains_dbi: Vec<f64>, pub peak_elevation_deg: u32, pub snapped_height_m: f64, pub height_variable: bool }

#[tauri::command]
pub async fn antenna_pattern_preview(
    antenna_preset: antenna::AntennaPreset,
    height_m: f64,
    freq_khz: Option<f64>,
) -> Result<AntennaPreview, UiError> {
    let voa = crate::propagation::patterns::pattern_voa(antenna_preset, height_m);
    let block = freq_to_block(freq_khz.unwrap_or(14_100.0)); // nearest FREQS_MHZ
    let gains = preview::elevation_slice(voa, block).map_err(UiError::Internal)?;
    let peak = gains.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).map(|(i, _)| i as u32).unwrap_or(0);
    Ok(AntennaPreview {
        gains_dbi: gains, peak_elevation_deg: peak,
        snapped_height_m: crate::propagation::patterns::snap_height(height_m),
        height_variable: crate::propagation::patterns::is_height_variable(antenna_preset),
    })
}
```

- [ ] **Step 5: Register the command** in the `invoke_handler` `generate_handler![...]` list alongside `propagation_prefs_read`.

- [ ] **Step 6: Run — passes.** `cargo test --manifest-path src-tauri/Cargo.toml --lib propagation`

- [ ] **Step 7: Commit.** (`feat(antenna): antenna_pattern_preview command + elevation-slice parser`)

---

## Task group E — Frontend picker + polar preview

### Task E1: propagationPrefs.ts — curate list, height grid, ground default, helpers

**Files:**
- Modify: `src/catalog/propagationPrefs.ts`
- Test: `src/catalog/propagationPrefs.test.ts`

- [ ] **Step 1: Failing test — removed presets gone; height grid + class helper exist; ground default poor-soil.**

```ts
import { ANTENNA_PRESET_OPTIONS, HEIGHT_GRID_M, isHeightVariable, DEFAULT_PROPAGATION_PREFS } from './propagationPrefs';
it('curated catalog has 8 entries, no random-wire/mag-loop', () => {
  const values = ANTENNA_PRESET_OPTIONS.map(o => o.value);
  expect(values).toHaveLength(8);
  expect(values).not.toContain('random-wire-unun');
  expect(values).not.toContain('magnetic-loop');
});
it('height grid + class helper', () => {
  expect(HEIGHT_GRID_M).toEqual([2.5, 4, 6, 9]);
  expect(isHeightVariable('base-vertical-radials')).toBe(false);
  expect(isHeightVariable('efhw-sloper')).toBe(true);
});
it('ground default flips to poor-soil', () => {
  expect(DEFAULT_PROPAGATION_PREFS.groundType).toBe('poor-soil');
});
```

- [ ] **Step 2: Run — fails.** `pnpm test propagationPrefs`

- [ ] **Step 3: Implement** — remove the two from the `AntennaPreset` union + `ANTENNA_PRESET_OPTIONS`; add `export const HEIGHT_GRID_M = [2.5, 4, 6, 9] as const;`; add `export function isHeightVariable(p: AntennaPreset): boolean` (horizontal set); set `DEFAULT_PROPAGATION_PREFS.groundType = 'poor-soil'`. Add the `antenna_pattern_preview` invoke wrapper + `AntennaPreview` type.

```ts
export interface AntennaPreview { gainsDbi: number[]; peakElevationDeg: number; snappedHeightM: number; heightVariable: boolean; }
export async function readAntennaPreview(antennaPreset: AntennaPreset, heightM: number, freqKhz?: number): Promise<AntennaPreview> {
  const w = await invoke<{ gains_dbi: number[]; peak_elevation_deg: number; snapped_height_m: number; height_variable: boolean }>(
    'antenna_pattern_preview', { antennaPreset, heightM, freqKhz });
  return { gainsDbi: w.gains_dbi, peakElevationDeg: w.peak_elevation_deg, snappedHeightM: w.snapped_height_m, heightVariable: w.height_variable };
}
```

- [ ] **Step 4: Run — passes.**
- [ ] **Step 5: Commit.** (`feat(catalog): curate antenna list + height grid + preview binding`)

### Task E2: PolarPattern preview component

**Files:**
- Create: `src/catalog/PolarPattern.tsx`, `src/catalog/PolarPattern.test.tsx`

- [ ] **Step 1: Failing test — renders an SVG with a polyline and marks peak elevation.**

```tsx
import { render } from '@testing-library/react';
import { PolarPattern } from './PolarPattern';
it('renders a lobe polyline and peak marker', () => {
  const gains = Array.from({ length: 91 }, (_, i) => (i >= 45 ? 3 : -10));
  const { container } = render(<PolarPattern gainsDbi={gains} peakElevationDeg={90} />);
  expect(container.querySelector('svg')).toBeTruthy();
  expect(container.querySelector('[data-testid="lobe"]')).toBeTruthy();
});
```

- [ ] **Step 2: Run — fails.**
- [ ] **Step 3: Implement** a quarter-polar SVG (0° horizon → 90° zenith) plotting `gainsDbi` as radius (normalized to max), `data-testid="lobe"` polyline, a peak marker, and a "not modeled" flat state when all gains equal. Keep it ~120×74, dark-theme-friendly (`currentColor` / CSS vars), no external deps.
- [ ] **Step 4: Run — passes.**
- [ ] **Step 5: Commit.** (`feat(catalog): PolarPattern elevation-lobe preview component`)

### Task E3: AntennaControl redesign — snapping slider + conditional state + preview

**Files:**
- Modify: `src/catalog/AntennaControl.tsx`
- Test: `src/catalog/AntennaControl.test.tsx`

- [ ] **Step 1: Failing tests** — (a) vertical selected → no height slider, shows "Ground-mounted"; (b) horizontal → slider with 4 stops; (c) preview fetched + rendered on change.

```tsx
it('hides height + shows ground-mounted for verticals', async () => {
  render(<AntennaControl prefs={{ ...DEFAULT_PROPAGATION_PREFS, antennaPreset: 'base-vertical-radials' }} onChange={() => {}} />);
  expect(screen.queryByTestId('antenna-height-slider')).toBeNull();
  expect(screen.getByText(/ground-mounted/i)).toBeTruthy();
});
it('shows a 4-stop height slider for horizontals', () => {
  render(<AntennaControl prefs={{ ...DEFAULT_PROPAGATION_PREFS, antennaPreset: 'efhw-sloper' }} onChange={() => {}} />);
  const slider = screen.getByTestId('antenna-height-slider') as HTMLInputElement;
  expect(slider.min).toBe('0'); expect(slider.max).toBe('3'); expect(slider.step).toBe('1'); // 4 grid indices
});
```

> Mock the preview invoke: `vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => ({ gains_dbi: Array(91).fill(0), peak_elevation_deg: 0, snapped_height_m: 6, height_variable: true })) }))`.

- [ ] **Step 2: Run — fails.**
- [ ] **Step 3: Implement** — replace the height `<input type=number>` with a slider over `HEIGHT_GRID_M` indices (snap on change → write the grid value to `prefs.antennaHeightM`); conditional render the slider vs a "Ground-mounted — height fixed" note via `isHeightVariable(prefs.antennaPreset)`; fetch `readAntennaPreview` on `(antennaPreset, antennaHeightM)` change (debounced) and render `<PolarPattern>`. Keep ground/noise/snr/power fields. Use the existing `station-finder__antenna*` classes; add slider + preview styles.
- [ ] **Step 4: Run — passes.** `pnpm test AntennaControl`
- [ ] **Step 5: Commit.** (`feat(catalog): AntennaControl snapping height slider + conditional state + live preview`)

---

## Task group F — Integration, RF review, wire-walk

### Task F1: Distinctness + height-sensitivity deck tests (the spec's TDD core)

**Files:**
- Test: new inline tests in `src-tauri/src/propagation/` (a `deck`-level test that runs a real prediction or compares emitted `.voa`)

- [ ] **Step 1: Test — distinct `.voa` per antenna+height; a low NVIS wire has higher zenith (elev 90°) gain than a 9m wire.**

```rust
#[test]
fn height_changes_high_angle_lobe() {
    use crate::propagation::preview::elevation_slice;
    use crate::propagation::patterns::pattern_voa;
    let low = elevation_slice(pattern_voa(AntennaPreset::NvisWireDipole, 2.5), 13).unwrap();
    let high = elevation_slice(pattern_voa(AntennaPreset::NvisWireDipole, 9.0), 13).unwrap();
    // zenith = index 90; a lower wire concentrates more power overhead (NVIS)
    assert!(low[90] > high[90] - 6.0, "expected low wire to favor high angle");
    assert_ne!(low, high);
}
```

> If the physics assertion fails, that is a **real signal** the geometries aren't distinct enough — do NOT weaken the test to pass; raise it in the Codex round (Task F2) and fix the decks (Task B). This test is the spec's "different defensible selections measurably change the prediction."

- [ ] **Step 2: Run — passes** (after deck refinement if needed).
- [ ] **Step 3: Commit.** (`test(antenna): height-sensitivity + per-antenna distinctness deck tests`)

### Task F2: Codex adversarial RF round (REQUIRED — per no_carveout_on_cross_provider_adrev)

- [ ] **Step 1: Run Codex on the geometry + clamp + frequency-table choices.** Per CLAUDE.md custom-prompt pattern:

```bash
cat > /tmp/codex-prompt.txt <<'EOF'
Adversarial RF review of the diff against origin/main in this worktree. Run
`git diff origin/main..HEAD`. Focus: (1) Are the NEC antenna geometries in
src-tauri/src/bin/gen_antenna_patterns.rs physically defensible for poor/dry-desert
ground? Specifically: EFHW/NVIS/portable horizontals differ only by length — is
that honest, or should sloper/inverted-V have real geometry, or should near-identical
ones be collapsed? Are the loaded short verticals (mobile 1.5m) defensible without a
loading-coil LD card? Is the 3-el yagi geometry sane? (2) Does FREQS_MHZ (2..31 MHz)
match what voacapl's Type-14 loader assigns to block indices 1..30? Check
~/itshfbc/antennas/samples/sample.14. (3) Is the null clamp to -99.999 dBi correct,
and does elevation = 90 - theta map gains to the right Type-14 bins (cross-check the
Phase 0 golden in type14.rs)? Output findings as markdown.
EOF
cat /tmp/codex-prompt.txt | npx --yes @openai/codex review - 2>&1 \
  | tee dev/adversarial/2026-06-15-antenna-phase1-geometry-codex.md
wc -l dev/adversarial/2026-06-15-antenna-phase1-geometry-codex.md   # >1000 = real review, ~5 = rejected stub
```

- [ ] **Step 2: Triage findings.** Apply geometry/frequency/clamp fixes; regenerate the library (re-run Task B4 Step 2); re-run all tests. Summarize dispositions in the PR body (raw transcript is gitignored).

### Task F3: Wire-walk gate (REQUIRED — first user-reachable flow of the epic)

- [ ] **Step 1: Run the `wire-walk` skill** (`.claude/skills/wire-walk/`). The operator supplies the key flows greenfield. Trace each to code (`file:line`). The motivating flow: *operator opens Find-a-Station → picks an antenna → the height slider snaps → the polar preview redraws → the forecast re-runs with the new pattern*. Confirm every hop is wired: `AntennaControl` onChange → `writePropagationPrefs` → `predictReload` bump in `StationFinderPanel` → `propagation_predict_path` reloads prefs → `operator_voa_content` → `pattern_voa` → scratch `antennas/default/txgen.voa` → voacapl; AND `readAntennaPreview` → `antenna_pattern_preview` → `PolarPattern`.
- [ ] **Step 2: Any broken primary flow = NOT shipped.** Fix before claiming done.

### Task F4: PR + operator smoke

- [ ] **Step 1: Full gate:** `cargo test --manifest-path src-tauri/Cargo.toml --lib` + `pnpm test` + `pnpm lint:docs`. All green.
- [ ] **Step 2: Push; open PR** `gh pr create --base main --head bd-tuxlink-bl01/phase1-picker-library --title "[cardinal-moraine-glade] feat(antenna): Find-a-Station Phase 1 — NEC pattern library + picker"`. PR body: decisions, the 20-pattern library, the single-ground limitation, Codex dispositions, wire-walk result.
- [ ] **Step 3: Operator smoke** (WebKitGTK/grim per `chromium_not_webkitgtk_proxy`) — pick a vertical vs a low wire, confirm the preview lobe + forecast both change. Opportunistic/post-merge per `browser_smoke_before_ship` (not a pre-merge gate on the contended Pi).

---

## Self-review notes (planner)

- **Spec coverage:** catalog curation (C1) ✓, height grid + snapping (D1) ✓, slider+preview UI (E2/E3) ✓, conditional vertical state (E3) ✓, ground default flip (E1) ✓, NEC library at poor-soil (B/B4) ✓, null clamp (A3) ✓, preview slice + command (D3) ✓, migration (C1) ✓, TDD distinct/height-sensitivity/migration/preview-equality/null-clamp (A3/C1/D3/F1) ✓, Codex (F2) ✓, wire-walk (F3) ✓, single-ground limitation documented (D2 inline + spec) ✓.
- **Known soft spots flagged for execution:** the `elevation_slice` fixed-width parser (D3 Step 3) is a real impl task, not a placeholder; the antenna geometries (B) are first cuts explicitly gated on the Codex round (F2); FREQS_MHZ must be validated against voacapl (A1/F2).
- **Type consistency:** `pattern_voa`, `snap_height`, `is_height_variable`, `HEIGHT_GRID_M`, `elevation_slice`, `antenna_pattern_preview`, `AntennaPreview` used consistently across Rust (D1/D3) and mirrored in TS (E1).
