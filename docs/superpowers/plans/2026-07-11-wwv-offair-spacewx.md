# Off-air WWV/WWVH space-weather decode — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decode the NOAA SWPC space-weather bulletin off-air from the WWV/WWVH voice broadcast via the primary radio, feeding the existing propagation engine — an internet-free capability WLE lacks.

**Architecture:** A new `tuxlink-stt` path-dependency crate (whisper-rs) transcribes a ~70 s capture; a new `src-tauri/src/wwv_offair/` module orchestrates rig-tune → `arecord` capture → transcribe → normalize → the existing `parse_wwv` → the existing `apply_rf_solar_*` ingestion (which writes `ssn-forecast.json`, read fresh by the predict path). A new Tauri command drives it; a new frontend button in the station-finder actions row triggers it and shows provenance.

**Tech Stack:** Rust (MSRV 1.75), whisper-rs (whisper.cpp bindings), `hound` WAV, `arecord` shell-out, Tauri 2.x commands, React 18 + TS frontend, `tux-rig` crate for CAT.

**Spec:** `docs/superpowers/specs/2026-07-11-wwv-offair-spacewx-design.md` (read it first).

## Global Constraints

- **MSRV 1.75.** No `Result::inspect_err` (1.76+) or other post-1.75 APIs; clippy `incompatible_msrv` is denied.
- **This Pi cannot finish a cold `cargo` build/test locally.** Write Rust + tests; let CI compile/run them. `pnpm vitest run <file>` runs locally per-file. Rust manifest is `src-tauri/Cargo.toml` (NOT a workspace root) — always `--manifest-path src-tauri/Cargo.toml`.
- **RX-only.** This feature never keys the transmitter. No PTT path is touched. RADIO-1 (ADR 0018) does not gate it.
- **Internet-free at runtime.** Only the STT model download (setup time) touches the network; runtime is fully off-air.
- **Additive config only.** New config is `Config.wwv_offair: Option<WwvOffairConfig>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` — must not bump `CONFIG_SCHEMA_VERSION` or break existing `config.json`.
- **Commit discipline.** Every commit ends with `Agent: gorge-fern-cedar` + the `Co-Authored-By` trailer. Conventional-commit types. Commit inside the worktree with a standalone `cd` then bare git (the `cd &&` compound misfires the main-checkout hook).
- **Wire-walk gate** (`.claude/skills/wire-walk/`) before any "done" claim — the operator supplies the flows greenfield.

---

## File Structure

**New:**
- `src-tauri/tuxlink-stt/Cargo.toml` — path-dep crate (whisper-rs, hound, thiserror).
- `src-tauri/tuxlink-stt/src/lib.rs` — `DecodeMode`, `SttResult`, `SttConfidence`, `SttError`, `WhisperStt`, `transcribe`.
- `src-tauri/src/wwv_offair/mod.rs` — module root + `capture_cycle` orchestration.
- `src-tauri/src/wwv_offair/normalize.rs` — `normalize_spoken_numbers`.
- `src-tauri/src/wwv_offair/schedule.rs` — `next_window`, `WwvWindow`, `Station`.
- `src-tauri/src/wwv_offair/freq.rs` — `freq_for_time_of_day`.
- `src-tauri/src/wwv_offair/capture.rs` — `CaptureSource` trait + `PrimaryRigSource` + `ArecordCapture`.
- `src-tauri/src/wwv_offair/commands.rs` — `wwv_offair_refresh`, `wwv_offair_snapshot_read`.
- `src-tauri/src/wwv_offair/model.rs` — model path resolution + setup download + checksum.
- `src/wwv/wwvApi.ts` — frontend invoke bindings.
- `src/wwv/useWwvOffair.ts` — React hook (arm, status, result).

**Modified:**
- `src-tauri/src/propagation/solar_update.rs` — factor `apply_rf_solar_indices(indices, source, …)`; `apply_rf_solar_reply` delegates. Add `"rf-wwv-voice"` as a source value (no enum; `source: String`).
- `src-tauri/src/config.rs` — add `WwvOffairConfig` + `Config.wwv_offair` field.
- `src-tauri/src/lib.rs` — register the two commands in `generate_handler!` + `.manage` a `WwvOffairState` in `.setup()`.
- `src-tauri/Cargo.toml` — add `tuxlink-stt = { path = "tuxlink-stt" }`.
- `src-tauri/src/lib.rs` (or `main` module tree) — `mod wwv_offair;`.
- `src/catalog/StationFinderControls.tsx` — add "Refresh off-air" button in `station-finder__actions`; render off-air SFI/A/K + provenance.

---

## Phase 0 — Scaffolding & the ingestion seam

### Task 1: `tuxlink-stt` crate skeleton + types

**Files:**
- Create: `src-tauri/tuxlink-stt/Cargo.toml`
- Create: `src-tauri/tuxlink-stt/src/lib.rs`
- Modify: `src-tauri/Cargo.toml` (add path dep)

**Interfaces:**
- Produces: `DecodeMode` (`General | WwvBiased`), `SttConfidence { avg_logprob: f32, no_speech_prob: f32 }`, `SttResult { text: String, confidence: SttConfidence }`, `SttError`.

- [ ] **Step 1: Write the crate manifest**

```toml
# src-tauri/tuxlink-stt/Cargo.toml
[package]
name = "tuxlink-stt"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[dependencies]
whisper-rs = "0.14"
hound = "3"
thiserror = "1"
```

- [ ] **Step 2: Write the types + a failing test for `DecodeMode` prompt text**

```rust
// src-tauri/tuxlink-stt/src/lib.rs
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeMode { General, WwvBiased }

impl DecodeMode {
    /// The `set_initial_prompt` text used to bias decoding.
    pub fn initial_prompt(&self) -> Option<&'static str> {
        match self {
            DecodeMode::General => None,
            DecodeMode::WwvBiased => Some(
                "NOAA space weather bulletin. Solar flux, estimated planetary \
                 A-index, planetary K-index at UTC, geomagnetic storms, minor, \
                 moderate, strong.",
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SttConfidence { pub avg_logprob: f32, pub no_speech_prob: f32 }

#[derive(Debug, Clone, PartialEq)]
pub struct SttResult { pub text: String, pub confidence: SttConfidence }

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("model load failed: {0}")] ModelLoad(String),
    #[error("audio read failed: {0}")] Audio(String),
    #[error("transcription failed: {0}")] Transcribe(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wwv_mode_has_biasing_prompt() {
        assert!(DecodeMode::WwvBiased.initial_prompt().unwrap().contains("Solar flux"));
        assert_eq!(DecodeMode::General.initial_prompt(), None);
    }
}
```

- [ ] **Step 3: Add the path dependency**

In `src-tauri/Cargo.toml` `[dependencies]`, add:
```toml
tuxlink-stt = { path = "tuxlink-stt" }
```

- [ ] **Step 4: Run the pure test (CI compiles the crate; locally just the unit)**

Run (CI): `cargo test --manifest-path src-tauri/tuxlink-stt/Cargo.toml wwv_mode_has_biasing_prompt`
Expected: PASS. (If the Pi cannot build whisper-rs locally, rely on CI — note it in the commit.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tuxlink-stt src-tauri/Cargo.toml
git commit -F - <<'EOF'
feat(stt): scaffold tuxlink-stt crate — DecodeMode, SttResult, SttError

Agent: gorge-fern-cedar
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

### Task 2: Provenance seam — `apply_rf_solar_indices(indices, source, …)`

**Files:**
- Modify: `src-tauri/src/propagation/solar_update.rs`
- Test: same file `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `SolarIndices` (from `super::solar`), `SsnForecast`, `derive_ssn_from_sfi`.
- Produces: `pub fn apply_rf_solar_indices(indices: SolarIndices, source: &str, year: i32, month: u8, now_ms: u64, config_dir: &Path) -> Result<UpdateOutcome, PropagationError>`. `apply_rf_solar_reply` delegates to it with `"rf-wwv"`.

- [ ] **Step 1: Write the failing test** (the voice path passes pre-parsed indices + a custom source)

```rust
#[test]
fn rf_voice_source_tag_persists_and_updates_forecast() {
    let dir = tempfile::tempdir().unwrap();
    let indices = SolarIndices { sfi: 150.0, a_index: Some(8.0), k_index: Some(2.0) };
    let out = apply_rf_solar_indices(indices, "rf-wwv-voice", 2026, 7, 1_000, dir.path()).unwrap();
    assert!(out.forecast_updated);
    assert_eq!(out.source, "rf-wwv-voice");
    let snap = SolarSnapshot::load(dir.path()).unwrap();
    assert_eq!(snap.source, "rf-wwv-voice");
    // Forecast got the derived SSN for the current month.
    let f = SsnForecast::load_writable_then_bundled(dir.path());
    assert!(f.monthly.contains_key("2026-07"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run (CI): `cargo test --manifest-path src-tauri/Cargo.toml --locked rf_voice_source_tag_persists`
Expected: FAIL — `apply_rf_solar_indices` not found.

- [ ] **Step 3: Refactor `apply_rf_solar_reply` to delegate; add `apply_rf_solar_indices`**

```rust
/// Apply pre-parsed RF solar indices under an explicit `source` provenance tag.
/// Derives an SSN from the SFI (only daily SFI crosses the air — the documented
/// coarser fallback), writes it into the writable forecast as the current
/// `year`-`month` (preserving other months), and persists the live snapshot.
pub fn apply_rf_solar_indices(
    indices: SolarIndices,
    source: &str,
    year: i32,
    month: u8,
    now_ms: u64,
    config_dir: &Path,
) -> Result<UpdateOutcome, PropagationError> {
    let mut forecast = SsnForecast::load_writable_then_bundled(config_dir);
    let derived = solar::derive_ssn_from_sfi(indices.sfi);
    forecast.monthly.insert(format!("{year:04}-{month:02}"), derived);
    forecast.persist(config_dir)?;
    SolarSnapshot {
        indices: Some(indices),
        updated_at_ms: now_ms,
        source: source.to_string(),
        forecast_updated: true,
    }
    .persist(config_dir)?;
    Ok(UpdateOutcome { forecast_updated: true, indices: Some(indices), source: source.to_string() })
}

pub fn apply_rf_solar_reply(
    body: &str, year: i32, month: u8, now_ms: u64, config_dir: &Path,
) -> Result<UpdateOutcome, PropagationError> {
    let indices = solar::parse_wwv(body).ok_or_else(|| {
        PropagationError::Ssn("RF solar reply had no parsable solar flux".to_string())
    })?;
    apply_rf_solar_indices(indices, "rf-wwv", year, month, now_ms, config_dir)
}
```

- [ ] **Step 4: Run tests** — the new test + the existing `apply_rf_solar_reply` tests all pass.

Run (CI): `cargo test --manifest-path src-tauri/Cargo.toml --locked solar_update`
Expected: PASS (existing `rf_*` tests unchanged in behavior).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/propagation/solar_update.rs
git commit -F - <<'EOF'
refactor(propagation): add apply_rf_solar_indices(source) seam for off-air WWV

Factors the SSN-derive + snapshot-persist out of apply_rf_solar_reply so the
off-air voice path can pass pre-parsed indices with the "rf-wwv-voice" tag.

Agent: gorge-fern-cedar
Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
```

---

## Phase 1 — `tuxlink-stt` transcription

### Task 3: WAV → 16 kHz mono f32 loader

**Files:**
- Modify: `src-tauri/tuxlink-stt/src/lib.rs`

**Interfaces:**
- Produces: `pub fn load_wav_16k_mono_f32(path: &Path) -> Result<Vec<f32>, SttError>` — errors if sample rate != 16000.

- [ ] **Step 1: Write the failing test** (write a tiny 16 kHz mono WAV with `hound`, read it back)

```rust
#[test]
fn loads_16k_mono_wav() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.wav");
    let spec = hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for _ in 0..1600 { w.write_sample(0i16).unwrap(); }
    w.finalize().unwrap();
    let samples = load_wav_16k_mono_f32(&path).unwrap();
    assert_eq!(samples.len(), 1600);
}

#[test]
fn rejects_wrong_sample_rate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t8k.wav");
    let spec = hound::WavSpec { channels: 1, sample_rate: 8000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    w.write_sample(0i16).unwrap();
    w.finalize().unwrap();
    assert!(load_wav_16k_mono_f32(&path).is_err());
}
```

- [ ] **Step 2: Run to verify fail** — `cargo test … loads_16k_mono_wav` → FAIL (undefined).

- [ ] **Step 3: Implement**

```rust
pub fn load_wav_16k_mono_f32(path: &Path) -> Result<Vec<f32>, SttError> {
    let reader = hound::WavReader::open(path).map_err(|e| SttError::Audio(e.to_string()))?;
    let spec = reader.spec();
    if spec.sample_rate != 16000 {
        return Err(SttError::Audio(format!("expected 16kHz, got {}", spec.sample_rate)));
    }
    let ch = spec.channels.max(1) as usize;
    let ints: Vec<i16> = reader.into_samples::<i16>()
        .collect::<Result<_, _>>().map_err(|e| SttError::Audio(e.to_string()))?;
    // Downmix to mono by averaging channels; scale i16 -> f32 [-1,1].
    let mut out = Vec::with_capacity(ints.len() / ch);
    for frame in ints.chunks(ch) {
        let sum: i32 = frame.iter().map(|&s| s as i32).sum();
        out.push((sum as f32 / ch as f32) / 32768.0);
    }
    Ok(out)
}
```

- [ ] **Step 4: Run tests** → PASS.
- [ ] **Step 5: Commit** (`feat(stt): 16kHz mono f32 WAV loader`).

### Task 4: `WhisperStt` model wrapper + `transcribe`

**Files:**
- Modify: `src-tauri/tuxlink-stt/src/lib.rs`

**Interfaces:**
- Produces: `pub struct WhisperStt { ctx: whisper_rs::WhisperContext }`, `WhisperStt::load(model_path: &Path) -> Result<Self, SttError>`, `WhisperStt::transcribe(&self, wav: &Path, mode: DecodeMode) -> Result<SttResult, SttError>`.

- [ ] **Step 1: Write an `#[ignore]` integration test** (needs the real model + a fixture WAV — gated so CI without the model still passes; run explicitly where the model exists)

```rust
#[test]
#[ignore = "requires ggml base.en model + fixture WAV; run where present"]
fn transcribes_fixture() {
    let model = std::path::PathBuf::from(std::env::var("TUXLINK_STT_MODEL").unwrap());
    let stt = WhisperStt::load(&model).unwrap();
    let wav = std::path::PathBuf::from("tests/fixtures/wwv_clean_16k.wav");
    let r = stt.transcribe(&wav, DecodeMode::WwvBiased).unwrap();
    assert!(r.text.to_lowercase().contains("solar flux"));
}
```

- [ ] **Step 2: Implement using the whisper-rs API** (verified shape: `WhisperContext::new_with_params`, `create_state`, `FullParams`, `state.full`, `full_n_segments`, `full_get_segment_text`, per-segment `full_get_segment_no_speech_prob`/token probs)

```rust
use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};

pub struct WhisperStt { ctx: WhisperContext }

impl WhisperStt {
    pub fn load(model_path: &Path) -> Result<Self, SttError> {
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().ok_or_else(|| SttError::ModelLoad("non-utf8 path".into()))?,
            WhisperContextParameters::default(),
        ).map_err(|e| SttError::ModelLoad(e.to_string()))?;
        Ok(Self { ctx })
    }

    pub fn transcribe(&self, wav: &Path, mode: DecodeMode) -> Result<SttResult, SttError> {
        let audio = load_wav_16k_mono_f32(wav)?;
        let mut state = self.ctx.create_state().map_err(|e| SttError::Transcribe(e.to_string()))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        if let Some(p) = mode.initial_prompt() { params.set_initial_prompt(p); }
        state.full(params, &audio).map_err(|e| SttError::Transcribe(e.to_string()))?;
        let n = state.full_n_segments().map_err(|e| SttError::Transcribe(e.to_string()))?;
        let mut text = String::new();
        let mut min_logprob = 0.0f32;
        let mut max_no_speech = 0.0f32;
        for i in 0..n {
            text.push_str(&state.full_get_segment_text(i).map_err(|e| SttError::Transcribe(e.to_string()))?);
            text.push(' ');
            // Aggregate a worst-case confidence across segments.
            if let Ok(ns) = state.full_get_segment_no_speech_prob(i) { max_no_speech = max_no_speech.max(ns); }
            let toks = state.full_n_tokens(i).unwrap_or(0);
            if toks > 0 {
                let mut sum = 0.0f32; let mut cnt = 0.0f32;
                for j in 0..toks {
                    if let Ok(p) = state.full_get_token_prob(i, j) { sum += p.ln(); cnt += 1.0; }
                }
                if cnt > 0.0 { min_logprob = min_logprob.min(sum / cnt); }
            }
        }
        Ok(SttResult { text: text.trim().to_string(),
            confidence: SttConfidence { avg_logprob: min_logprob, no_speech_prob: max_no_speech } })
    }
}
```

> **API-verification step (not a placeholder):** the exact method names for
> per-segment no-speech probability differ across whisper-rs minor versions
> (`full_get_segment_no_speech_prob` vs a token-data field). Before running,
> `cargo doc --open -p whisper-rs` (or read `~/.cargo/registry/.../whisper-rs-*/src`)
> and adjust the two `full_get_segment_no_speech_prob` / `full_get_token_prob`
> calls to the installed version's names. This is a 2-line adjustment; the shape
> (aggregate worst-case confidence across segments) is fixed.

- [ ] **Step 3: Commit** (`feat(stt): WhisperStt model wrapper + transcribe with vocabulary biasing`).

### Task 5: Noise-rejection gate

**Files:**
- Modify: `src-tauri/tuxlink-stt/src/lib.rs`

**Interfaces:**
- Produces: `pub fn is_confident(c: &SttConfidence) -> bool` using ported Geographica thresholds (`no_speech_prob < 0.8`, `avg_logprob > -0.8`).

- [ ] **Step 1: Failing test**

```rust
#[test]
fn rejects_low_confidence() {
    assert!(!is_confident(&SttConfidence { avg_logprob: -1.2, no_speech_prob: 0.2 }));
    assert!(!is_confident(&SttConfidence { avg_logprob: -0.3, no_speech_prob: 0.9 }));
    assert!(is_confident(&SttConfidence { avg_logprob: -0.3, no_speech_prob: 0.2 }));
}
```

- [ ] **Step 2: Implement**

```rust
/// Ported from Geographica's tuned Whisper thresholds: reject hallucinated
/// transcripts from noise instead of emitting confident nonsense.
pub fn is_confident(c: &SttConfidence) -> bool {
    c.no_speech_prob < 0.8 && c.avg_logprob > -0.8
}
```

- [ ] **Step 3: Run → PASS. Step 4: Commit** (`feat(stt): noise-rejection confidence gate`).

---

## Phase 2 — `wwv_offair` pure core

### Task 6: `normalize_spoken_numbers`

**Files:**
- Create: `src-tauri/src/wwv_offair/normalize.rs`
- Create/Modify: `src-tauri/src/wwv_offair/mod.rs` (add `pub mod normalize;`)

**Interfaces:**
- Produces: `pub fn normalize_spoken_numbers(transcript: &str) -> String` — maps spoken number words to digits so `parse_wwv` matches.

- [ ] **Step 1: Failing tests** (drive the real WWV phrasings)

```rust
#[test]
fn word_numbers_to_digits() {
    let t = normalize_spoken_numbers("Solar flux one hundred seventeen and estimated planetary A index six");
    assert!(t.contains("solar flux 117"));
    assert!(t.contains("a index 6") || t.contains("a-index 6"));
}
#[test]
fn decimal_k_index() {
    let t = normalize_spoken_numbers("the estimated planetary k index at twelve hundred UTC was one point three three");
    assert!(t.contains("1.33"));
}
#[test]
fn passthrough_existing_digits() {
    assert!(normalize_spoken_numbers("Solar flux 142 reported").contains("142"));
}
```

- [ ] **Step 2: Run → FAIL. Step 3: Implement** a small English-number normalizer (units/teens/tens/hundred, "point" → ".", "twelve hundred" → "1200"). Full code:

```rust
/// Lowercases, then rewrites spoken English number words into digit strings so
/// the tolerant `parse_wwv` substring matcher works on STT output. Deliberately
/// small: only the closed WWV vocabulary needs to survive.
pub fn normalize_spoken_numbers(transcript: &str) -> String {
    let lower = transcript.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    let mut acc: Option<u64> = None;      // accumulating integer
    let mut frac: Option<String> = None;  // digits after "point"
    let mut in_point = false;

    let flush = |acc: &mut Option<u64>, frac: &mut Option<String>, out: &mut Vec<String>| {
        if let Some(n) = acc.take() {
            let mut s = n.to_string();
            if let Some(f) = frac.take() { s.push('.'); s.push_str(&f); }
            out.push(s);
        } else if let Some(f) = frac.take() {
            out.push(format!("0.{f}"));
        }
    };

    for w in words {
        let unit = word_to_unit(w);           // Some(0..=9) etc.
        match w {
            "point" => { in_point = true; if acc.is_none() { acc = Some(0); } }
            "hundred" => { if let Some(a) = acc { acc = Some(if a == 0 { 100 } else { a * 100 }); } }
            "thousand" => { if let Some(a) = acc { acc = Some(a * 1000); } }
            _ if unit.is_some() => {
                let d = unit.unwrap();
                if in_point {
                    frac.get_or_insert_with(String::new).push_str(&d.to_string());
                } else if let Some(ten) = word_to_ten(w) {
                    acc = Some(acc.unwrap_or(0) + ten);
                } else {
                    acc = Some(acc.unwrap_or(0) + d);
                }
            }
            _ => { flush(&mut acc, &mut frac, &mut out); in_point = false; out.push(w.to_string()); }
        }
    }
    flush(&mut acc, &mut frac, &mut out);
    out.join(" ")
}

fn word_to_unit(w: &str) -> Option<u64> {
    Some(match w {
        "zero"|"oh" => 0, "one" => 1, "two" => 2, "three" => 3, "four" => 4,
        "five" => 5, "six" => 6, "seven" => 7, "eight" => 8, "nine" => 9,
        "ten" => 10, "eleven" => 11, "twelve" => 12, "thirteen" => 13,
        "fourteen" => 14, "fifteen" => 15, "sixteen" => 16, "seventeen" => 17,
        "eighteen" => 18, "nineteen" => 19,
        "twenty"|"thirty"|"forty"|"fifty"|"sixty"|"seventy"|"eighty"|"ninety" => return word_to_ten(w),
        _ => return None,
    })
}
fn word_to_ten(w: &str) -> Option<u64> {
    Some(match w {
        "twenty" => 20, "thirty" => 30, "forty" => 40, "fifty" => 50,
        "sixty" => 60, "seventy" => 70, "eighty" => 80, "ninety" => 90,
        _ => return None,
    })
}
```

> Note "twelve hundred" → acc=12 then ×100 = 1200 (handled). "one hundred seventeen" → 100 then +17 = 117.

- [ ] **Step 4: Run → PASS** (`pnpm`-free; `cargo test … normalize` on CI, but logic is pure so a focused local `cargo test -p` may work — if not, CI). **Step 5: Commit** (`feat(wwv): spoken-number normalizer for STT output`).

### Task 7: Nearest-window scheduler

**Files:**
- Create: `src-tauri/src/wwv_offair/schedule.rs`

**Interfaces:**
- Produces: `pub enum Station { Wwv, Wwvh }`, `pub struct WwvWindow { pub station: Station, pub at_unix_ms: u64 }`, `pub fn next_window(now_unix_ms: u64) -> WwvWindow` (WWV :18, WWVH :45; nearest upcoming).

- [ ] **Step 1: Failing tests** (deterministic `now`)

```rust
// 2026-07-11T12:00:00Z = 1_783_512_000_000 ms. Next window is WWV :18 (12:18).
#[test]
fn picks_wwv_18_when_before_18() {
    let now = 1_783_512_000_000; // 12:00:00Z
    let w = next_window(now);
    assert_eq!(w.station, Station::Wwv);
    assert_eq!(w.at_unix_ms, now + 18 * 60_000);
}
#[test]
fn picks_wwvh_45_when_between_18_and_45() {
    let now = 1_783_512_000_000 + 20 * 60_000; // 12:20:00Z
    let w = next_window(now);
    assert_eq!(w.station, Station::Wwvh);
}
#[test]
fn rolls_to_next_hour_wwv_after_45() {
    let now = 1_783_512_000_000 + 50 * 60_000; // 12:50:00Z
    let w = next_window(now);
    assert_eq!(w.station, Station::Wwv);
    assert_eq!(w.at_unix_ms, 1_783_512_000_000 + 78 * 60_000); // 13:18
}
```

- [ ] **Step 2: FAIL. Step 3: Implement** (pure arithmetic on minute-of-hour; no chrono needed)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Station { Wwv, Wwvh }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct WwvWindow { pub station: Station, pub at_unix_ms: u64 }

const MIN: u64 = 60_000;
const HOUR: u64 = 3_600_000;

pub fn next_window(now_unix_ms: u64) -> WwvWindow {
    let into_hour = now_unix_ms % HOUR;
    let hour_start = now_unix_ms - into_hour;
    let wwv = 18 * MIN;
    let wwvh = 45 * MIN;
    if into_hour < wwv {
        WwvWindow { station: Station::Wwv, at_unix_ms: hour_start + wwv }
    } else if into_hour < wwvh {
        WwvWindow { station: Station::Wwvh, at_unix_ms: hour_start + wwvh }
    } else {
        WwvWindow { station: Station::Wwv, at_unix_ms: hour_start + HOUR + wwv }
    }
}
```

- [ ] **Step 4: PASS. Step 5: Commit** (`feat(wwv): nearest-window scheduler (WWV :18 / WWVH :45)`).

### Task 8: Frequency-by-time-of-day

**Files:**
- Create: `src-tauri/src/wwv_offair/freq.rs`

**Interfaces:**
- Produces: `pub fn freq_for_utc_hour(utc_hour: u8) -> u64` returning Hz (10 MHz all-rounder default; 5 MHz night 00–11; 15 MHz day 12–23 — simple, operator-overridable later via config).

- [ ] **Step 1: Failing test**

```rust
#[test]
fn day_night_frequency_selection() {
    assert_eq!(freq_for_utc_hour(3), 5_000_000);   // night
    assert_eq!(freq_for_utc_hour(18), 15_000_000); // day
    assert_eq!(freq_for_utc_hour(11), 5_000_000);
    assert_eq!(freq_for_utc_hour(12), 15_000_000);
}
```

- [ ] **Step 2: FAIL. Step 3: Implement**

```rust
/// Coarse WWV frequency choice by UTC hour. 10 MHz is the safe all-rounder; the
/// simple split below prefers 5 MHz overnight (better LF/MF propagation) and
/// 15 MHz midday. Operator override lands in WwvOffairConfig later.
pub fn freq_for_utc_hour(utc_hour: u8) -> u64 {
    match utc_hour { 0..=11 => 5_000_000, _ => 15_000_000 }
}
```

- [ ] **Step 4: PASS. Step 5: Commit** (`feat(wwv): time-of-day WWV frequency selection`).

---

## Phase 3 — Capture + rig orchestration

### Task 9: `CaptureSource` trait + `ArecordCapture`

**Files:**
- Create: `src-tauri/src/wwv_offair/capture.rs`

**Interfaces:**
- Produces: `pub trait CaptureSource { fn capture(&self, freq_hz: u64, dwell: std::time::Duration) -> Result<std::path::PathBuf, CaptureError>; }`, `pub struct ArecordCapture { pub device: String, pub out_dir: PathBuf }`, `pub enum CaptureError`.

- [ ] **Step 1: Write the arecord argv builder + a test for it** (the shell-out itself is integration-tested by the operator; the argv is unit-tested)

```rust
#[test]
fn arecord_argv_is_16k_mono_s16() {
    let args = arecord_args("plughw:1,0", 70, std::path::Path::new("/tmp/x.wav"));
    assert_eq!(args, vec!["-D","plughw:1,0","-f","S16_LE","-c","1","-r","16000","-d","70","/tmp/x.wav"]);
}
```

- [ ] **Step 2: FAIL. Step 3: Implement** (argv builder pure; `capture()` shells out)

```rust
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("arecord failed: {0}")] Arecord(String),
    #[error("capture device busy: {0}")] DeviceBusy(String),
    #[error("io: {0}")] Io(String),
}

pub trait CaptureSource {
    fn capture(&self, freq_hz: u64, dwell: Duration) -> Result<PathBuf, CaptureError>;
}

pub(crate) fn arecord_args(device: &str, secs: u64, out: &Path) -> Vec<String> {
    vec!["-D".into(), device.into(), "-f".into(), "S16_LE".into(),
         "-c".into(), "1".into(), "-r".into(), "16000".into(),
         "-d".into(), secs.to_string(), out.to_string_lossy().into_owned()]
}

pub struct ArecordCapture { pub device: String, pub out_dir: PathBuf }

impl CaptureSource for ArecordCapture {
    fn capture(&self, _freq_hz: u64, dwell: Duration) -> Result<PathBuf, CaptureError> {
        let out = self.out_dir.join(format!("wwv-{}.wav", dwell.as_secs()));
        let status = std::process::Command::new("arecord")
            .args(arecord_args(&self.device, dwell.as_secs().max(1), &out))
            .status().map_err(|e| CaptureError::Io(e.to_string()))?;
        if !status.success() {
            return Err(CaptureError::Arecord(format!("exit {status}")));
        }
        Ok(out)
    }
}
```

> `_freq_hz` is unused by `ArecordCapture` (the rig is tuned by the orchestrator, not the capture source) but is in the trait for the future `SdrSource`, which tunes its own front-end.

- [ ] **Step 4: PASS. Step 5: Commit** (`feat(wwv): CaptureSource trait + arecord capture`).

### Task 10: `capture_cycle` orchestration (rig save/tune/restore + serial sequencing)

**Files:**
- Create/Modify: `src-tauri/src/wwv_offair/mod.rs`

**Interfaces:**
- Consumes: `tux_rig::{ManagedRig, RigConfig, Mode, RigStatus}`, `CaptureSource`.
- Produces: `pub fn capture_cycle<C: CaptureSource>(rig_cfg: RigConfig, close_serial: bool, freq_hz: u64, dwell: Duration, capture: &C) -> Result<PathBuf, WwvError>` — save VFO → tune WWV/USB → (release_serial if close_serial) → capture → restore (re-spawn if released).

- [ ] **Step 1: Write a test with a mock rig** — extract a `trait TuneRig { fn status; fn tune; fn release_serial; }` so `capture_cycle` is generic over the rig, letting a mock assert call ordering without a real rigctld.

```rust
// Mock records the ordered calls; capture returns a fixed path.
#[test]
fn cycle_saves_tunes_captures_restores_no_release() {
    let mock = MockRig::new(RigStatus { freq_hz: 14_074_000, mode: Some(Mode::PktUsb), ptt: false });
    let cap = MockCapture::default();
    let out = run_cycle(&mock, false, 10_000_000, Duration::from_secs(70), &cap).unwrap();
    assert_eq!(out, std::path::PathBuf::from("/mock/wwv.wav"));
    assert_eq!(mock.calls(), vec![
        "status".into(),
        "tune 10000000 Usb".into(),
        "capture".into(),
        "tune 14074000 PktUsb".into(), // restore, no release/re-spawn
    ]);
}
#[test]
fn cycle_releases_serial_and_respawns_for_internal_codec() {
    let mock = MockRig::new(RigStatus { freq_hz: 14_074_000, mode: Some(Mode::PktUsb), ptt: false });
    let cap = MockCapture::default();
    run_cycle(&mock, true, 10_000_000, Duration::from_secs(70), &cap).unwrap();
    assert_eq!(mock.calls(), vec![
        "status".into(),
        "tune 10000000 Usb".into(),
        "release_serial".into(),
        "capture".into(),
        "respawn".into(),
        "tune 14074000 PktUsb".into(),
    ]);
}
```

- [ ] **Step 2: FAIL. Step 3: Implement** the generic `run_cycle` over a `TuneRig` trait + a thin `ManagedRig` adapter + the public `capture_cycle` that constructs the adapter from `RigConfig`.

```rust
use std::path::PathBuf;
use std::time::Duration;
use tux_rig::{ManagedRig, Mode, RigConfig, RigStatus, RigError};
use crate::wwv_offair::capture::CaptureSource;

#[derive(Debug, thiserror::Error)]
pub enum WwvError {
    #[error("rig: {0}")] Rig(String),
    #[error("capture: {0}")] Capture(String),
}

/// Minimal rig surface `run_cycle` needs — lets tests substitute a mock and lets
/// the internal-codec path re-spawn after release_serial.
pub(crate) trait TuneRig {
    fn status(&self) -> Result<RigStatus, RigError>;
    fn tune(&self, hz: u64, mode: Mode) -> Result<(), RigError>;
    fn release_serial(&self);
    fn respawn(&self) -> Result<(), RigError>;
}

pub(crate) fn run_cycle<R: TuneRig, C: CaptureSource>(
    rig: &R, close_serial: bool, freq_hz: u64, dwell: Duration, capture: &C,
) -> Result<PathBuf, WwvError> {
    let saved = rig.status().map_err(|e| WwvError::Rig(e.to_string()))?;
    rig.tune(freq_hz, Mode::Usb).map_err(|e| WwvError::Rig(e.to_string()))?;
    if close_serial { rig.release_serial(); }
    let out = capture.capture(freq_hz, dwell).map_err(|e| WwvError::Capture(e.to_string()))?;
    if close_serial { rig.respawn().map_err(|e| WwvError::Rig(e.to_string()))?; }
    // Restore original VFO+mode (mode may be unknown → leave as-is).
    if let Some(m) = saved.mode {
        rig.tune(saved.freq_hz, m).map_err(|e| WwvError::Rig(e.to_string()))?;
    }
    Ok(out)
}
```

The `ManagedRig` adapter (holds `RigConfig` for re-spawn) + public entry:

```rust
struct ManagedTuneRig { cfg: RigConfig, inner: std::cell::RefCell<Option<ManagedRig>> }
impl TuneRig for ManagedTuneRig {
    fn status(&self) -> Result<RigStatus, RigError> {
        self.inner.borrow_mut().as_mut().ok_or(RigError::Spawn("no rig".into()))?.status()
    }
    fn tune(&self, hz: u64, mode: Mode) -> Result<(), RigError> {
        self.inner.borrow_mut().as_mut().ok_or(RigError::Spawn("no rig".into()))?.tune(hz, mode)
    }
    fn release_serial(&self) {
        if let Some(r) = self.inner.borrow_mut().as_mut() { r.release_serial(); }
        *self.inner.borrow_mut() = None;
    }
    fn respawn(&self) -> Result<(), RigError> {
        let r = ManagedRig::spawn(self.cfg.clone())?;
        *self.inner.borrow_mut() = Some(r);
        Ok(())
    }
}

pub fn capture_cycle<C: CaptureSource>(
    rig_cfg: RigConfig, close_serial: bool, freq_hz: u64, dwell: Duration, capture: &C,
) -> Result<PathBuf, WwvError> {
    let rig = ManagedRig::spawn(rig_cfg.clone()).map_err(|e| WwvError::Rig(e.to_string()))?;
    let adapter = ManagedTuneRig { cfg: rig_cfg, inner: std::cell::RefCell::new(Some(rig)) };
    run_cycle(&adapter, close_serial, freq_hz, dwell, capture)
}
```

- [ ] **Step 4: PASS (mock tests). Step 5: Commit** (`feat(wwv): capture_cycle rig orchestration with serial-sequencing branch`).

---

## Phase 4 — Config, Tauri command, registration

### Task 11: `WwvOffairConfig`

**Files:**
- Modify: `src-tauri/src/config.rs`

**Interfaces:**
- Produces: `pub struct WwvOffairConfig { pub capture_device: String, pub model_path: Option<String>, pub auto_retry_next_window: bool }` + `Config.wwv_offair: Option<WwvOffairConfig>`.

- [ ] **Step 1: Failing test** — a config JSON without `wwv_offair` round-trips (None) and one with it parses.

```rust
#[test]
fn config_without_wwv_offair_is_none_and_roundtrips() {
    let c = read_config_from_str(MINIMAL_VALID_CONFIG_JSON).unwrap();
    assert!(c.wwv_offair.is_none());
}
#[test]
fn config_with_wwv_offair_parses() {
    let json = with_field(MINIMAL_VALID_CONFIG_JSON, r#""wwv_offair":{"capture_device":"plughw:1,0"}"#);
    let c = read_config_from_str(&json).unwrap();
    assert_eq!(c.wwv_offair.unwrap().capture_device, "plughw:1,0");
}
```

- [ ] **Step 2: FAIL. Step 3: Implement** — add the struct (mirror the `Ft8Config` shape, but `Option<>` on `Config` per `modem_ardop`):

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WwvOffairConfig {
    /// ALSA capture device string (e.g. "plughw:1,0"). Empty → use the rig's
    /// configured capture device / operator picker.
    pub capture_device: String,
    /// Override path to the ggml base.en model; None → resolved default data dir.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    /// Auto-retry the next window once on no-copy.
    pub auto_retry_next_window: bool,
}
impl Default for WwvOffairConfig {
    fn default() -> Self { Self { capture_device: String::new(), model_path: None, auto_retry_next_window: true } }
}
```

On `Config`, after `pub ft8: Ft8Config,`:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub wwv_offair: Option<WwvOffairConfig>,
```

- [ ] **Step 4: PASS (no schema-version bump — verify `CONFIG_SCHEMA_VERSION` unchanged). Step 5: Commit** (`feat(config): additive WwvOffairConfig`).

### Task 12: `wwv_offair_refresh` + `wwv_offair_snapshot_read` commands

**Files:**
- Create: `src-tauri/src/wwv_offair/commands.rs`
- Modify: `src-tauri/src/wwv_offair/mod.rs` (`pub mod commands;` + a `decode_and_ingest` helper)

**Interfaces:**
- Consumes: `capture_cycle`, `WhisperStt`, `normalize_spoken_numbers`, `solar::parse_wwv`, `solar_update::apply_rf_solar_indices`, `config::{read_config, config_path}`, `modem_commands::rig_config_from`, `tuxlink_stt::is_confident`.
- Produces: `#[tauri::command] pub async fn wwv_offair_refresh(now_ms: u64, state: State<'_, WwvOffairState>) -> Result<WwvRefreshOutcome, UiError>`; `#[tauri::command] pub async fn wwv_offair_snapshot_read() -> Result<Option<SolarSnapshot>, UiError>`.

- [ ] **Step 1: Unit-test the pure `decode_and_ingest`** (WAV path + model + config_dir → ingest), using the `#[ignore]` model gate; and a pure test that a low-confidence transcript yields `NoCopy` without writing the forecast. Extract:

```rust
pub(crate) enum DecodeOutcome { Ingested(UpdateOutcome), NoCopy }

pub(crate) fn decode_and_ingest(
    stt: &WhisperStt, wav: &std::path::Path, year: i32, month: u8, now_ms: u64, config_dir: &std::path::Path,
) -> Result<DecodeOutcome, WwvError> {
    let r = stt.transcribe(wav, tuxlink_stt::DecodeMode::WwvBiased)
        .map_err(|e| WwvError::Capture(e.to_string()))?;
    if !tuxlink_stt::is_confident(&r.confidence) { return Ok(DecodeOutcome::NoCopy); }
    let normalized = crate::wwv_offair::normalize::normalize_spoken_numbers(&r.text);
    let indices = match crate::propagation::solar::parse_wwv(&normalized) {
        Some(i) => i, None => return Ok(DecodeOutcome::NoCopy),
    };
    let out = crate::propagation::solar_update::apply_rf_solar_indices(
        indices, "rf-wwv-voice", year, month, now_ms, config_dir,
    ).map_err(|e| WwvError::Capture(e.to_string()))?;
    Ok(DecodeOutcome::Ingested(out))
}
```

- [ ] **Step 2: Implement the command** — resolve config, build `rig_config_from(&cfg.rig)` (None → `UiError::NotConfigured` + manual-tune hint), compute window/freq, run `capture_cycle` on `spawn_blocking`, `decode_and_ingest`, map to `WwvRefreshOutcome { updated: bool, indices: Option<SolarIndices>, source, no_copy: bool }`. `wwv_offair_snapshot_read` returns `SolarSnapshot::load(config_dir)`.

```rust
#[derive(Debug, Serialize)]
pub struct WwvRefreshOutcome { pub updated: bool, pub indices: Option<crate::propagation::solar::SolarIndices>, pub source: String, pub no_copy: bool }

#[tauri::command]
pub async fn wwv_offair_refresh(now_ms: u64, _state: tauri::State<'_, WwvOffairState>) -> Result<WwvRefreshOutcome, crate::ui_commands::UiError> {
    use crate::ui_commands::UiError;
    let cfg = crate::config::read_config().map_err(|e| UiError::Internal { detail: e.to_string() })?;
    let rig_cfg = crate::modem_commands::rig_config_from(&cfg.rig)
        .ok_or_else(|| UiError::NotConfigured("Configure CAT rig control, or tune WWV manually.".into()))?;
    let close_serial = cfg.rig.close_serial_sequencing;
    let device = cfg.wwv_offair.as_ref().map(|w| w.capture_device.clone()).unwrap_or_default();
    let config_dir = crate::config::config_path().parent().map(|p| p.to_path_buf())
        .ok_or_else(|| UiError::Internal { detail: "no config dir".into() })?;
    let model = crate::wwv_offair::model::resolve_model_path(&cfg)
        .map_err(|e| UiError::Unavailable { reason: e })?;
    // (year, month, freq) derived from now_ms via the schedule/freq helpers.
    let outcome = tokio::task::spawn_blocking(move || -> Result<WwvRefreshOutcome, String> {
        let stt = tuxlink_stt::WhisperStt::load(&model).map_err(|e| e.to_string())?;
        let cap = crate::wwv_offair::capture::ArecordCapture { device, out_dir: std::env::temp_dir() };
        let wav = crate::wwv_offair::capture_cycle(rig_cfg, close_serial, freq_hz(now_ms), std::time::Duration::from_secs(70), &cap)
            .map_err(|e| e.to_string())?;
        let (y, m) = year_month(now_ms);
        match crate::wwv_offair::commands::decode_and_ingest(&stt, &wav, y, m, now_ms, &config_dir).map_err(|e| e.to_string())? {
            crate::wwv_offair::commands::DecodeOutcome::Ingested(o) => Ok(WwvRefreshOutcome { updated: o.forecast_updated, indices: o.indices, source: o.source, no_copy: false }),
            crate::wwv_offair::commands::DecodeOutcome::NoCopy => Ok(WwvRefreshOutcome { updated: false, indices: None, source: "rf-wwv-voice".into(), no_copy: true }),
        }
    }).await.map_err(|e| UiError::Internal { detail: e.to_string() })?
      .map_err(|detail| UiError::Internal { detail })?;
    Ok(outcome)
}
```

> `freq_hz`, `year_month`, `resolve_model_path` are small helpers defined in Tasks 8/13 and a `chrono`-free UTC decomposition (the repo already computes `year`/`month` for solar updates — reuse `now_ms` → UTC via the same helper the predict path uses; if none, add a tiny `unix_ms_to_year_month`).

- [ ] **Step 3: Commit** (`feat(wwv): wwv_offair_refresh + snapshot_read Tauri commands`).

### Task 13: Model resolution + registration + managed state

**Files:**
- Create: `src-tauri/src/wwv_offair/model.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `pub fn resolve_model_path(cfg: &Config) -> Result<PathBuf, String>` (config override → `~/.local/share/tuxlink/models/ggml-base.en-q5_1.bin` → err with the download hint); `struct WwvOffairState;` managed unconditionally.

- [ ] **Step 1:** `resolve_model_path` unit test (override honored; missing default → Err containing "download"). **Step 2: Implement.**

```rust
pub fn resolve_model_path(cfg: &crate::config::Config) -> Result<std::path::PathBuf, String> {
    if let Some(p) = cfg.wwv_offair.as_ref().and_then(|w| w.model_path.clone()) {
        let pb = std::path::PathBuf::from(p);
        return if pb.is_file() { Ok(pb) } else { Err(format!("configured model not found: {}", pb.display())) };
    }
    let base = dirs_data_dir().join("tuxlink/models/ggml-base.en-q5_1.bin");
    if base.is_file() { Ok(base) } else {
        Err(format!("STT model not installed. Download to {} (see setup), or set wwv_offair.model_path.", base.display()))
    }
}
```

- [ ] **Step 3:** Register in `src-tauri/src/lib.rs` — add `mod wwv_offair;`, `.manage(crate::wwv_offair::WwvOffairState::default())` in the static `.manage` chain, and in `generate_handler![…]` add:
```rust
crate::wwv_offair::commands::wwv_offair_refresh,
crate::wwv_offair::commands::wwv_offair_snapshot_read,
```

- [ ] **Step 4:** CI clippy + test green. **Step 5: Commit** (`feat(wwv): model resolution + command registration`).

---

## Phase 5 — Frontend

### Task 14: `wwvApi.ts` + `useWwvOffair` hook

**Files:**
- Create: `src/wwv/wwvApi.ts`, `src/wwv/useWwvOffair.ts`
- Test: `src/wwv/wwvApi.test.ts` (vitest, mock `invoke`)

**Interfaces:**
- Produces: `refreshOffair(nowMs): Promise<WwvRefreshOutcome>`, `readSnapshot(): Promise<SolarSnapshot|null>`; hook state `{ arm(), status: 'idle'|'armed'|'capturing'|'done'|'nocopy'|'error', result }`.

- [ ] **Step 1: Failing vitest** (mock `@tauri-apps/api/core` `invoke`, assert `refreshOffair` calls `'wwv_offair_refresh'` with `{ nowMs }`).

```ts
import { vi, test, expect } from 'vitest';
const invoke = vi.fn().mockResolvedValue({ updated: true, indices: { sfi: 150 }, source: 'rf-wwv-voice', no_copy: false });
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
test('refreshOffair invokes the command', async () => {
  const { refreshOffair } = await import('./wwvApi');
  const r = await refreshOffair(1_783_512_000_000);
  expect(invoke).toHaveBeenCalledWith('wwv_offair_refresh', { nowMs: 1_783_512_000_000 });
  expect(r.updated).toBe(true);
});
```

- [ ] **Step 2:** `pnpm vitest run src/wwv/wwvApi.test.ts` → FAIL. **Step 3: Implement** `wwvApi.ts` (mirror `src/catalog/propagationApi.ts` invoke shape) + the hook.

```ts
// src/wwv/wwvApi.ts
import { invoke } from '@tauri-apps/api/core';
export interface WwvRefreshOutcome { updated: boolean; indices: { sfi: number; a_index?: number; k_index?: number } | null; source: string; no_copy: boolean; }
export interface SolarSnapshot { indices: { sfi: number; a_index?: number; k_index?: number } | null; updated_at_ms: number; source: string; forecast_updated: boolean; }
export async function refreshOffair(nowMs: number) { return invoke<WwvRefreshOutcome>('wwv_offair_refresh', { nowMs }); }
export async function readSnapshot() { return invoke<SolarSnapshot | null>('wwv_offair_snapshot_read'); }
```

- [ ] **Step 4:** vitest PASS. **Step 5: Commit** (`feat(wwv): frontend invoke bindings + hook`).

### Task 15: "Refresh off-air" button + provenance readout

**Files:**
- Modify: `src/catalog/StationFinderControls.tsx`

**Interfaces:**
- Consumes: `useWwvOffair`, `readSnapshot`.

- [ ] **Step 1: vitest** rendering test — the actions cluster shows a "Refresh off-air" button; clicking it calls the hook's `arm` (mock the hook).
- [ ] **Step 2: FAIL. Step 3: Implement** — add the button in `station-finder__actions` (the reserved row) and render off-air SFI/A/K + `source`/age from the snapshot next to the existing `station-finder__cond`:

```tsx
<button type="button" className="station-finder__refresh-offair"
  onClick={() => wwv.arm(Date.now())} disabled={wwv.status === 'capturing' || wwv.status === 'armed'}>
  {wwv.status === 'armed' ? `Armed ${wwv.windowLabel}` : wwv.status === 'capturing' ? 'Capturing…' : 'Refresh off-air'}
</button>
```
Provenance stamp (near the conditions span):
```tsx
{snapshot?.source === 'rf-wwv-voice' && snapshot.indices && (
  <span className="station-finder__offair" title={`off-air WWV ${new Date(snapshot.updated_at_ms).toISOString()}`}>
    off-air WWV · SFI <b>{snapshot.indices.sfi}</b>
    {snapshot.indices.k_index != null && <> · K <b>{snapshot.indices.k_index}</b></>}
  </span>
)}
```

- [ ] **Step 4:** vitest + `pnpm typecheck` PASS. **Step 5: Commit** (`feat(wwv): Refresh off-air button + off-air provenance readout`).

### Task 16: Low-SNR (no-copy) confirm UX

**Files:**
- Modify: `src/wwv/useWwvOffair.ts`, `src/catalog/StationFinderControls.tsx`

- [ ] **Step 1: vitest** — when `refreshOffair` returns `no_copy: true`, the hook enters `'nocopy'` and (if `auto_retry_next_window`) re-arms once; a second no-copy surfaces a "couldn't copy — retry next cycle / enter manually" affordance.
- [ ] **Step 2: FAIL. Step 3: Implement** the retry-once state machine + the inline affordance. **Step 4: PASS. Step 5: Commit** (`feat(wwv): no-copy retry + manual-entry affordance`).

---

## Phase 6 — Model acquisition

### Task 17: Setup-time model download + checksum + manual-place doc

**Files:**
- Create: `scripts/fetch-stt-model.sh` (setup helper; checksum-verified download to `~/.local/share/tuxlink/models/`)
- Modify: `docs/user-guide/` (a short "off-air space weather setup" page: run the fetch script, or manual-place for air-gapped installs)

- [ ] **Step 1:** Write `scripts/fetch-stt-model.sh` — downloads `ggml-base.en-q5_1.bin` to the data dir, verifies SHA-256 against a pinned constant, idempotent (skip if present + valid). No app-runtime network — this is an explicit operator setup step.
- [ ] **Step 2:** Add the user-guide page (fetch script usage + the manual-place path + checksum) so an air-gapped operator can provision offline.
- [ ] **Step 3:** `pnpm lint:docs` PASS (links valid). **Step 4: Commit** (`feat(wwv): setup-time STT model fetch + offline manual-place doc`).

---

## Phase 7 — Integration & wire-walk

### Task 18: Wire-walk gate (HARD — operator supplies flows)

- [ ] **Step 1:** Invoke the `wire-walk` skill. The operator supplies the key user flows greenfield (do NOT draft them). Trace each to code (`file:line`): e.g. "operator clicks Refresh off-air → armed → at :18 rig tunes/captures/restores → transcript ingested → conditions bar shows off-air SFI + provenance."
- [ ] **Step 2:** Any broken primary flow = NOT shipped; fix before claiming done.
- [ ] **Step 3:** Operator runs the real-radio RX capture (RADIO-1 does not gate RX, but on-air validation is operator-only per `rf_validation_onair_only`) against a live WWV :18 and confirms an end-to-end stamped update.

### Task 19: PR

- [ ] Open a draft PR `bd-tuxlink-xscum/wwv-offair-spacewx → main`, `[gorge-fern-cedar] feat: off-air WWV/WWVH space-weather decode`. Let CI compile/test both arches. Adversarial round via Codex (per `build-robust-features`). Wire-walk must pass before ready-for-review.

---

## Self-Review

**Spec coverage:** §5 pipeline → Tasks 9–13; §6.1 STT → Tasks 1,3,4,5; §6.2 capture → Task 9; §6.3 rig ordering (release_serial re-spawn branch) → Task 10; §6.4 normalize → Task 6; §6.5 parse+engine feed → Task 2,12; §6.6 scheduler → Task 7; §6.7 SDR seam → `CaptureSource` (Task 9); §7 config/provenance → Tasks 2,11; §8 failure modes → Tasks 5,10,12,16; §9 UI → Tasks 14–16; §10 model acquisition → Tasks 13,17; §11 testing → each task's TDD; §12 RX-only → Global Constraints; §14 wire-walk → Task 18. All covered.

**Placeholder scan:** the two "API-verification" notes (Task 4 whisper-rs method names; Task 12 UTC helper) are explicit, bounded verification actions against installed crate source, not open-ended TODOs — each states exactly what to check and the fixed shape around it. No "add error handling"/"TBD"/"similar to Task N" placeholders.

**Type consistency:** `SolarIndices`/`UpdateOutcome`/`SolarSnapshot` names match `solar_update.rs`; `Mode::Usb`/`RigStatus.freq_hz`/`RigStatus.mode: Option<Mode>` match `tux-rig`; `DecodeMode::WwvBiased`, `is_confident`, `SttConfidence` consistent across Tasks 1/4/5/12; `apply_rf_solar_indices(indices, source, year, month, now_ms, config_dir)` used identically in Tasks 2 and 12; command names `wwv_offair_refresh`/`wwv_offair_snapshot_read` consistent across Tasks 12/13/14.
