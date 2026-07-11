# Station Intelligence L2 — Capture Service Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The persistent backend listening service for FT8 Station Intelligence: ALSA capture off the operator-selected USB codec, 48 kHz → 12 kHz decimation, wall-clock-true 15 s UTC slot assembly, slot WAV writeout to tmpfs, decode via the L1 `Jt9Runner`, the full service state machine with health counters, the decode ring, Tauri events + snapshot/control commands, modem yield/resume arbitration, and opt-in CAT band sweep. Also resolves tuxlink-gujnz (salvage-on-signal in the L1 runner).

**Architecture:** Two phases in the leaf crates (this file), one in the main crate (Phase C, appended by the second pass). Phase A: new std-only leaf workspace crate `src-tauri/tuxlink-capture` — pure logic (decimator, slot assembler, slot-WAV writer, state machine, band table) that compiles and TDDs on the dev Pi in seconds, with a dev-dependency on `tuxlink-jt9` so the writer↔preflight round-trip is a unit test. Phase B: the one-arm salvage-on-signal change inside `src-tauri/tuxlink-jt9` plus its three contract doc edits. Phase C: main-crate module `src/ft8/` (ALSA, threads, arbiter, sweep, clock probe, Tauri commands/events) — everything that cannot compile on the Pi and is verified via CI.

**Tech Stack:** Rust (std only in both leaf crates — no tokio, no external deps in `tuxlink-capture`), jt9 from the wsjtx package as an external binary (via the shipped L1 crate), the existing SDR WAV fixtures. Phase C adds the `alsa` crate to the MAIN crate only.

**Canonical design:** `docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md` (v4, REVIEWED — five adversarial rounds). Design authority above it: `docs/design/2026-07-10-station-intel-jt9-engine-delta.md`. Read the spec before starting any task. Epic: bd `tuxlink-b026z`, this plan = child `tuxlink-b026z.3`; resolves `tuxlink-gujnz`; dispositions `tuxlink-b026z.8`.

## Global Constraints

- **All commands run from inside the worktree, and paths are pinned absolute — subagent shell cwd resets between calls** (the project's documented `pin_paths_in_worktree_sessions` failure mode). Canonical forms (per-task Run lines abbreviate; this constraint governs):
  - `WT=/home/administrator/Code/tuxlink/worktrees/bd-tuxlink-b026z.3-station-intel-l2-capture`
  - `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked`
  - every `git`/script command block starts with `cd "$WT" && pwd` as its own step, re-issued after any command that reports "Shell cwd was reset".
- **Tasks execute strictly in order; each task starts from the previous task's commit.** Tasks 1–6 all touch `tuxlink-capture` (and Tasks 4–5 the same file); parallel dispatch collides.
- **Leaf crates are std-only.** `tuxlink-capture` has a ZERO-entry `[dependencies]` section; its only edge is `[dev-dependencies] tuxlink-jt9 = { path = "../tuxlink-jt9" }` (allowed — test-only, for the writer↔`preflight_slot_wav` round-trip). `tuxlink-jt9` stays dep-free.
- **The dev Pi CAN compile + test the leaf crates locally in seconds — real TDD; run every red-green cycle locally.** The Pi CANNOT compile the main src-tauri crate; Phase C (appended below the end marker by the second pass) is verified via CI only.
- **Pinned constants (from the spec; every number below appears verbatim in code and tests):** 180,000 output frames per slot @ 12 kHz; 720,000 input frames per slot @ 48 kHz; 51 decimator taps; N = 5 (jt9-degraded); k = 20 (band-dead); 12 s decode timeout (`tuxlink_jt9::types::SLOT_DECODE_TIMEOUT_SECS`); sweep dwell default 8 slots (valid 4–40); ring 240 slots; minimum gap-fill threshold 2,400 input frames (50 ms); lost-frames drop when > 48,000 (1 s); hold-latch TTL 30 s; supervisor tick 5 s.
- **Commits:** the per-task commit blocks show the SUBJECT ONLY. Every commit uses the heredoc form with both trailers (the repo's `.githooks/commit-msg` hard-refuses commits without the `Agent:` trailer):
  ```bash
  git commit -m "$(cat <<'EOF'
  <subject line from the task>

  Agent: esker-sorrel-redwood
  Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
  EOF
  )"
  ```
  Per project convention, if a subagent cannot commit in the worktree it stops after staging and reports; the PARENT commits. Branch: `bd-tuxlink-b026z.3/station-intel-l2-capture`.
- MSRV 1.75 (`rust-version = "1.75"`, and clippy's `incompatible_msrv` is denied — no `Result::inspect_err` etc.). New crate carries `license = "AGPL-3.0-or-later"`.
- `cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --all-targets --locked -- -D warnings` (and `-p tuxlink-jt9` for Task 7) must stay clean. CI runs `--workspace --all-targets --locked -- -D warnings` on amd64 + arm64 — do not "fix" CI toward the narrower local command.
- Task 1 adds a workspace member: regenerate `Cargo.lock` once WITHOUT `--locked` (the `rust_dep_requires_cargo_lock_update` project rule), commit the lock; every later command uses `--locked`.
- Test temp hygiene: suites create pid-suffixed dirs under `std::env::temp_dir()`; add best-effort `let _ = std::fs::remove_dir_all(..)`/`remove_file` cleanup at test end where practical.
- **BEFORE starting work on any task:** read `.claude/skills/test-driven-development` or invoke `/test-driven-development`; read `docs/pitfalls/testing-pitfalls.md`. Follow TDD: failing test → minimal code → green.
- **BEFORE marking any task complete:** review the task's tests against `docs/pitfalls/testing-pitfalls.md`; verify error-path and edge-case coverage; run the task's test commands and confirm green.
- After every review-gate marker below: review the batch from multiple perspectives, minimum three rounds; keep going past three if the third still finds substantive issues.
- **Review-gate protocol (Gates A–F).** Every finding is recorded in the gate's findings file with: a severity — **P1** (blocks merge), **P2** (fix before the next gate), **P3** (note) — plus `file:line`, a short verbatim quote of the offending code/text, why it is wrong, and the proposed fix. P1 and P2 findings are FIXED via normal commits before the next task starts; P3s are recorded in the findings file with their disposition. Each gate lists its **Files under review** (derived from the batch's tasks) so a round cannot silently skip a file. Each gate's outcome is also summarized — one line per finding + disposition — in the eventual PR body; the `dev/scratch/` findings files are gitignored and stay local.
- **Push cadence (Phase C).** The parent pushes the branch at the **Gate D, Gate E, and Gate F boundaries**, after that gate's P1/P2 fixes are committed. Each gate push's CI run is that batch's red-green: the per-task **[CI-side]** steps execute at the batch's gate push, not per task. Phases A–B need no interim push (they red-green locally on the Pi). After Gate F, the final push carries T19's docs commit. Fix-forward on CI findings from each push before starting the next batch.

---

## Phase A — `tuxlink-capture` leaf crate (Tasks 1–6)

### Task 1: Crate scaffold + band→dial table

**Files:**
- Create: `src-tauri/tuxlink-capture/Cargo.toml`
- Create: `src-tauri/tuxlink-capture/src/lib.rs`
- Create: `src-tauri/tuxlink-capture/src/bands.rs`
- Modify: `src-tauri/Cargo.toml` (workspace `members` array — add `"tuxlink-capture"` after `"tuxlink-jt9"`)

**Interfaces:**
- Produces (consumed by Phase C's start-labeling, band chip, sweep, and command validation):
```rust
pub const BANDS: [(&str, u64); 9];
pub fn dial_hz(band: &str) -> Option<u64>;
```

- [ ] **Step 1: Scaffold the crate**

`src-tauri/tuxlink-capture/Cargo.toml`:
```toml
[package]
name = "tuxlink-capture"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "AGPL-3.0-or-later"
description = "Station Intelligence L2 pure-logic leaf: 48k→12k decimator, wall-clock-true slot assembler, slot-WAV writer, listener state machine, FT8 band table."

[dependencies]

[dev-dependencies]
tuxlink-jt9 = { path = "../tuxlink-jt9" }
```

`src-tauri/tuxlink-capture/src/lib.rs`:
```rust
//! Station Intelligence L2 pure-logic leaf crate (tuxlink-b026z.3).
//!
//! std-only by design: everything here compiles and TDDs on the dev Pi in
//! seconds. The main crate's `src/ft8/` module (ALSA, threads, Tauri) wires
//! these pieces at Phase C. Design authority:
//! docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md.

pub mod bands;
```

Edit `src-tauri/Cargo.toml` — the workspace members line becomes:
```toml
members = [".", "tuxlink-security", "tuxlink-mcp-core", "tuxlink-mcp", "tuxlink-mcp-testserver", "tux-rig", "tuxlink-agent-runner", "tuxlink-agent-frontend", "d3zwe", "tuxlink-ft8", "tuxlink-jt9", "tuxlink-capture"]
```

Then regenerate the lock (Cargo.lock records every workspace member as a `[[package]]`, so `--locked` fails until the new member is recorded):
`cargo metadata --manifest-path "$WT/src-tauri/Cargo.toml" > /dev/null` once WITHOUT `--locked`. The updated `Cargo.lock` is committed in the final step.

- [ ] **Step 2: Write the failing band-table tests**

`src-tauri/tuxlink-capture/src/bands.rs` (table is data and must be complete for the file to compile; the lookup is the stub under test):
```rust
//! FT8 band → dial-frequency table (spec §Band table).
//!
//! Pinned FT8 dial frequencies (Hz), USB. The table is the single source for
//! Phase C's band chips, CAT start-labeling (nearest entry within ±3 kHz),
//! sweep QSY targets, and `ft8_set_band` validation.

/// Band label → dial Hz, low band to high. Order is part of the contract:
/// sweep round-robin walks the CONFIGURED band list, but display surfaces
/// sort by this table's order.
pub const BANDS: [(&str, u64); 9] = [
    ("160m", 1_840_000),
    ("80m", 3_573_000),
    ("40m", 7_074_000),
    ("30m", 10_136_000),
    ("20m", 14_074_000),
    ("17m", 18_100_000),
    ("15m", 21_074_000),
    ("12m", 24_915_000),
    ("10m", 28_074_000),
];

/// Exact-label lookup. Labels are case-sensitive lowercase ("20m", not
/// "20M") — the config layer owns normalization; this table does not guess.
pub fn dial_hz(band: &str) -> Option<u64> {
    let _ = band;
    None // stub — replaced in Step 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pinned_band_resolves_to_its_exact_dial() {
        let want: [(&str, u64); 9] = [
            ("160m", 1_840_000),
            ("80m", 3_573_000),
            ("40m", 7_074_000),
            ("30m", 10_136_000),
            ("20m", 14_074_000),
            ("17m", 18_100_000),
            ("15m", 21_074_000),
            ("12m", 24_915_000),
            ("10m", 28_074_000),
        ];
        for (band, hz) in want {
            assert_eq!(dial_hz(band), Some(hz), "band {band}");
        }
    }

    #[test]
    fn unknown_bands_are_none_never_panic() {
        for b in ["60m", "6m", "2m", "20M", " 20m", "20m ", "", "ft8", "14074"] {
            assert_eq!(dial_hz(b), None, "band {b:?}");
        }
    }

    #[test]
    fn table_shape_is_pinned() {
        assert_eq!(BANDS.len(), 9);
        // Strictly ascending in frequency — catches transposed entries.
        for w in BANDS.windows(2) {
            assert!(w[0].1 < w[1].1, "{} !< {}", w[0].0, w[1].0);
        }
    }
}
```

- [ ] **Step 3: Run tests, verify the lookup test FAILS**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked`
Expected: `every_pinned_band_resolves_to_its_exact_dial` FAILS (stub returns `None`); `unknown_bands_are_none_never_panic` passes by stub coincidence (acceptable — it exists to pin no-panic and the None contract); `table_shape_is_pinned` passes (data test). (If this errors on the LOCK rather than failing tests, Step 1's lock regeneration was skipped — do it now.)

- [ ] **Step 4: Implement the lookup**

Replace the stub in `bands.rs`:
```rust
pub fn dial_hz(band: &str) -> Option<u64> {
    BANDS.iter().find(|(b, _)| *b == band).map(|&(_, hz)| hz)
}
```

- [ ] **Step 5: Run tests green + clippy clean**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked`
Expected: 3 passed.
Run: `cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --all-targets --locked -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/tuxlink-capture src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(ft8): tuxlink-capture leaf crate + FT8 band→dial table (tuxlink-b026z.3 T1)"
```
(Use the heredoc trailer form from Global Constraints. If `Cargo.lock` did not change, omit it — a new member with a path dev-dep normally does change it.)

---

### Task 2: Canonical slot-WAV writer

**Files:**
- Create: `src-tauri/tuxlink-capture/src/wavwrite.rs`
- Modify: `src-tauri/tuxlink-capture/src/lib.rs` (add `pub mod wavwrite;`)

**Interfaces:**
- Consumes (dev-dep, tests only): `tuxlink_jt9::wav::preflight_slot_wav`.
- Produces (consumed by Phase C's capture thread on slot completion):
```rust
pub const OUT_SLOT_FRAMES: usize = 180_000;
pub const OUT_RATE_HZ: u32 = 12_000;
pub fn write_slot_wav(path: &Path, samples: &[i16]) -> std::io::Result<()>;
```

The writer emits EXACTLY the canonical layout `tuxlink_jt9::wav::preflight_slot_wav` demands (44-byte RIFF/WAVE header, fmt chunk at offset 12, data chunk at offset 36, PCM16 mono 12 kHz, exactly 180,000 frames) — the preflight validates the canonical layout ONLY, by design, because L1 only ever receives this writer's output. Any other input length is a caller bug and is rejected with an error before any file is created.

- [ ] **Step 1: Write the failing tests**

`src-tauri/tuxlink-capture/src/wavwrite.rs` (stub first):
```rust
//! Canonical slot-WAV writer (spec §WAV writeout).
//!
//! Emits the exact 44-byte-header RIFF/WAVE PCM16 mono 12 kHz layout that
//! `tuxlink_jt9::wav::preflight_slot_wav` validates — the round-trip is a
//! unit test in this module (dev-dependency). Exactly `OUT_SLOT_FRAMES`
//! frames; any other length errors with `ErrorKind::InvalidInput` BEFORE
//! creating the file.

use std::path::Path;

/// Frames per slot at the decimated output rate: 15.000 s × 12 kHz.
pub const OUT_SLOT_FRAMES: usize = 180_000;
/// Decimated output rate.
pub const OUT_RATE_HZ: u32 = 12_000;

pub fn write_slot_wav(path: &Path, samples: &[i16]) -> std::io::Result<()> {
    let _ = (path, samples);
    Err(std::io::Error::new(std::io::ErrorKind::Other, "stub")) // replaced in Step 3
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir()
            .join(format!("tuxlink-capture-wavwrite-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    fn ramp() -> Vec<i16> {
        (0..OUT_SLOT_FRAMES)
            .map(|i| (i as i32 % 32_768 - 16_384) as i16)
            .collect()
    }

    #[test]
    fn wrong_lengths_are_rejected_before_any_file_exists() {
        for n in [0usize, OUT_SLOT_FRAMES - 1, OUT_SLOT_FRAMES + 1] {
            let p = tmp(&format!("wrong-{n}.wav"));
            let err = write_slot_wav(&p, &vec![0i16; n]).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "len {n}");
            assert!(!p.exists(), "len {n}: no file may be created on rejection");
        }
    }

    #[test]
    fn header_fields_are_byte_exact_canonical() {
        let p = tmp("header.wav");
        write_slot_wav(&p, &ramp()).unwrap();
        let b = std::fs::read(&p).unwrap();
        assert_eq!(b.len(), 44 + OUT_SLOT_FRAMES * 2, "total file size");
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([b[4], b[5], b[6], b[7]]), 36 + 360_000);
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes([b[16], b[17], b[18], b[19]]), 16); // fmt size
        assert_eq!(u16::from_le_bytes([b[20], b[21]]), 1); // PCM
        assert_eq!(u16::from_le_bytes([b[22], b[23]]), 1); // mono
        assert_eq!(u32::from_le_bytes([b[24], b[25], b[26], b[27]]), 12_000); // rate
        assert_eq!(u32::from_le_bytes([b[28], b[29], b[30], b[31]]), 24_000); // byte rate
        assert_eq!(u16::from_le_bytes([b[32], b[33]]), 2); // block align
        assert_eq!(u16::from_le_bytes([b[34], b[35]]), 16); // bits
        assert_eq!(&b[36..40], b"data");
        assert_eq!(u32::from_le_bytes([b[40], b[41], b[42], b[43]]), 360_000);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn payload_round_trips_sample_exact() {
        let p = tmp("payload.wav");
        let samples = ramp();
        write_slot_wav(&p, &samples).unwrap();
        let b = std::fs::read(&p).unwrap();
        let got: Vec<i16> = b[44..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(got, samples);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn output_passes_the_l1_preflight_round_trip() {
        // THE contract test (spec §WAV): our writer's output must be
        // accepted by tuxlink_jt9::wav::preflight_slot_wav verbatim.
        let p = tmp("preflight.wav");
        write_slot_wav(&p, &ramp()).unwrap();
        assert_eq!(tuxlink_jt9::wav::preflight_slot_wav(&p), Ok(()));
        let _ = std::fs::remove_file(&p);
    }
}
```

Add `pub mod wavwrite;` to `src-tauri/tuxlink-capture/src/lib.rs`.

- [ ] **Step 2: Run tests, verify they FAIL**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked wavwrite`
Expected: `header_fields_are_byte_exact_canonical`, `payload_round_trips_sample_exact`, `output_passes_the_l1_preflight_round_trip` FAIL (stub errors); `wrong_lengths_are_rejected_before_any_file_exists` FAILS too (the stub's `ErrorKind::Other` is not `InvalidInput`).

- [ ] **Step 3: Implement**

Replace the stub:
```rust
pub fn write_slot_wav(path: &Path, samples: &[i16]) -> std::io::Result<()> {
    use std::io::Write;
    if samples.len() != OUT_SLOT_FRAMES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "slot WAV requires exactly {OUT_SLOT_FRAMES} frames, got {}",
                samples.len()
            ),
        ));
    }
    let data_len: u32 = (OUT_SLOT_FRAMES as u32) * 2;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?; // fmt chunk size
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&OUT_RATE_HZ.to_le_bytes())?;
    f.write_all(&(OUT_RATE_HZ * 2).to_le_bytes())?; // byte rate
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    let mut pcm = Vec::with_capacity(data_len as usize);
    for s in samples {
        pcm.extend_from_slice(&s.to_le_bytes());
    }
    f.write_all(&pcm)?;
    f.flush()
}
```

- [ ] **Step 4: Run tests green + clippy clean**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked`
Run: `cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --all-targets --locked -- -D warnings`
Expected: all pass, clippy clean.

- [ ] **Step 5: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/tuxlink-capture/src/wavwrite.rs src-tauri/tuxlink-capture/src/lib.rs
git commit -m "feat(ft8): canonical slot-WAV writer, tuxlink-jt9 preflight round-trip proven (tuxlink-b026z.3 T2)"
```

---

### Task 3: 48 k → 12 k decimator (51-tap Kaiser, 4:1, persistent state)

**Files:**
- Create: `src-tauri/tuxlink-capture/src/decimator.rs`
- Modify: `src-tauri/tuxlink-capture/src/lib.rs` (add `pub mod decimator;`)

**Interfaces:**
- Produces (consumed by Task 4's assembler, which owns a `Decimator` instance):
```rust
pub const TAPS: usize = 51;
pub const DECIM: usize = 4;
pub const COEFFS: [f32; TAPS];
pub struct Decimator; // + impl Default
impl Decimator {
    pub fn new() -> Decimator;
    pub fn process(&mut self, input: &[i16], out: &mut Vec<i16>);
}
```

Coefficient provenance: the committed const table below is a Kaiser windowed-sinc computed offline at plan-authoring time (fs = 48,000 Hz, fc = 6,000 Hz — centered in the 4–8 kHz transition band — beta = 5.65, the 60 dB Kaiser design `0.1102·(60−8.7)`, normalized to DC gain 1, rounded to f32, ideal sinc zeros committed as literal `0.0`). Its measured response (f64 DFT of the exact f32 values): passband ripple ±0.013 dB over 0–4.0 kHz; stopband ≥ 60.45 dB over 8–24 kHz with the worst point exactly at 8.0 kHz; 79.9 dB at 9 kHz. The test module carries a from-first-principles generator (`generate_coeffs()`, std-only Bessel-I0 power series) and `committed_table_matches_kaiser_generator` compares the table against it, so the table cannot rot.

- [ ] **Step 1: Write the module with the committed table, a stubbed `process`, and the full failing test suite**

`src-tauri/tuxlink-capture/src/decimator.rs`:
```rust
//! 48 kHz → 12 kHz decimating FIR, 4:1, computed at the output rate
//! (spec §Decimator).
//!
//! 51-tap Kaiser windowed-sinc lowpass. Delta pins: passband 0–4 kHz (jt9
//! decodes to 4 007 Hz), stopband ≥ 8 kHz at ≥ 60 dB, Kaiser window,
//! polyphase at the output rate — the dot product runs only at output
//! instants, ¼ of the naive MAC count (~0.66 M MAC/s — trivial). i16 in →
//! i16 out; accumulate in f32, round half away from zero, saturate.
//! Filter state persists across `process` calls AND across slot boundaries
//! (the pinned continuity model; 720 000 ≡ 0 mod 4 keeps output phase
//! aligned slot-to-slot). Group delay (25 input samples ≈ 520 µs) is a
//! constant shift three orders of magnitude inside jt9's ±2 s DT tolerance
//! — a verified non-issue (spec §Decimator).

pub const TAPS: usize = 51;
pub const DECIM: usize = 4;

/// Committed coefficient table, DC gain 1.
///
/// GENERATOR NOTE (the committed reference implementation is
/// `generate_coeffs()` in the test module; the
/// `committed_table_matches_kaiser_generator` test keeps this table honest):
/// Kaiser windowed-sinc, fs = 48 000 Hz, fc = 6 000 Hz (transition centered
/// between the 4 kHz passband edge and the 8 kHz stopband edge),
/// beta = 5.65 (60 dB design: 0.1102·(60 − 8.7)):
///   h[n] = sinc(2·fc/fs·(n − 25)) · I0(beta·√(1 − (2n/50 − 1)²)) / I0(beta),
/// normalized so Σ h = 1, rounded to f32. The ideal sinc zeros
/// (n − 25 ≡ 0 mod 4, n ≠ 25) are committed as literal 0.0. Literals are
/// the SHORTEST round-trip f32 forms (clippy `excessive_precision` denies
/// longer ones) — do not "restore" extra digits.
///
/// Verified response of THIS f32 table (f64 DFT, asserted by the response
/// tests below): passband ripple ±0.013 dB over 0–4.0 kHz; attenuation
/// ≥ 60.45 dB over 8–24 kHz (worst point exactly 8.0 kHz); 79.9 dB at 9 kHz.
pub const COEFFS: [f32; TAPS] = [
    0.00018418659,
    0.0,
    -0.0005318928,
    -0.0011225927,
    -0.0011288024,
    0.0,
    0.0020681017,
    0.0038194556,
    0.0034638832,
    0.0,
    -0.0054616802,
    -0.009540089,
    -0.00826268,
    0.0,
    0.0121794455,
    0.020809278,
    0.017771836,
    0.0,
    -0.026221976,
    -0.045708396,
    -0.040614147,
    0.0,
    0.072335385,
    0.15663773,
    0.22426403,
    0.25011787,
    0.22426403,
    0.15663773,
    0.072335385,
    0.0,
    -0.040614147,
    -0.045708396,
    -0.026221976,
    0.0,
    0.017771836,
    0.020809278,
    0.0121794455,
    0.0,
    -0.00826268,
    -0.009540089,
    -0.0054616802,
    0.0,
    0.0034638832,
    0.0038194556,
    0.0020681017,
    0.0,
    -0.0011288024,
    -0.0011225927,
    -0.0005318928,
    0.0,
    0.00018418659,
];

/// Streaming 4:1 decimator with persistent filter state and input-phase
/// tracking across arbitrary chunk lengths (including ≢ 0 mod 4 — gap fills
/// are clock-sized).
pub struct Decimator {
    ring: [f32; TAPS],
    pos: usize,
    phase: usize,
}

impl Default for Decimator {
    fn default() -> Self {
        Self::new()
    }
}

impl Decimator {
    pub fn new() -> Self {
        Self {
            ring: [0.0; TAPS],
            pos: 0,
            phase: 0,
        }
    }

    /// Consume `input` (48 kHz), append decimated 12 kHz samples to `out`.
    /// y[m] = Σₖ h[k]·x[4m − k] with x pre-history = 0; the stream's first
    /// input sample is x[0] and produces y[0].
    pub fn process(&mut self, input: &[i16], out: &mut Vec<i16>) {
        let _ = (input, &out);
        unimplemented!("stub — replaced in Step 3")
    }
}
```

Then append the test module to the same file:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ---- test-time coefficient generator (the committed reference) ----

    /// Modified Bessel function of the first kind, order 0 — power series,
    /// std-only (converges in ~20 terms for the beta range used here).
    fn kaiser_i0(x: f64) -> f64 {
        let mut sum = 1.0;
        let mut term = 1.0;
        let mut k = 1.0;
        loop {
            let half = x / (2.0 * k);
            term *= half * half;
            sum += term;
            if term < 1e-18 * sum {
                return sum;
            }
            k += 1.0;
        }
    }

    fn generate_coeffs() -> Vec<f64> {
        const FS: f64 = 48_000.0;
        const FC: f64 = 6_000.0;
        const BETA: f64 = 5.65;
        let m = (TAPS - 1) as f64;
        let mut h: Vec<f64> = (0..TAPS)
            .map(|n| {
                let x = n as f64 - m / 2.0;
                let sinc = if x == 0.0 {
                    2.0 * FC / FS
                } else {
                    (2.0 * std::f64::consts::PI * FC / FS * x).sin()
                        / (std::f64::consts::PI * x)
                };
                let t = 2.0 * n as f64 / m - 1.0;
                let w = kaiser_i0(BETA * (1.0 - t * t).sqrt()) / kaiser_i0(BETA);
                sinc * w
            })
            .collect();
        let sum: f64 = h.iter().sum();
        for c in &mut h {
            *c /= sum;
        }
        h
    }

    #[test]
    fn committed_table_matches_kaiser_generator() {
        let gen = generate_coeffs();
        for (k, (&c, &g)) in COEFFS.iter().zip(gen.iter()).enumerate() {
            assert!(
                (f64::from(c) - g).abs() < 1e-6,
                "tap {k}: committed {c} vs generated {g}"
            );
        }
    }

    // ---- frequency-response verification from the committed table ----

    fn response_db(freq_hz: f64) -> f64 {
        let mut re = 0.0f64;
        let mut im = 0.0f64;
        for (k, &c) in COEFFS.iter().enumerate() {
            let phi = 2.0 * std::f64::consts::PI * freq_hz * k as f64 / 48_000.0;
            re += f64::from(c) * phi.cos();
            im -= f64::from(c) * phi.sin();
        }
        20.0 * (re.hypot(im) + 1e-30).log10()
    }

    #[test]
    fn passband_ripple_within_spec() {
        // Spec §Decimator: ≤ ±0.5 dB across 0–3.8 kHz, ≤ ±1.0 dB across
        // 3.8–4.0 kHz (jt9's ceiling is 4 007 Hz — the edge verified loosely).
        let mut f = 0.0f64;
        while f <= 3_800.0 {
            let r = response_db(f);
            assert!(r.abs() <= 0.5, "{f} Hz: {r:.4} dB exceeds ±0.5 dB");
            f += 100.0;
        }
        let mut f = 3_800.0f64;
        while f <= 4_000.0 {
            let r = response_db(f);
            assert!(r.abs() <= 1.0, "{f} Hz: {r:.4} dB exceeds ±1.0 dB");
            f += 25.0;
        }
    }

    #[test]
    fn stopband_attenuation_at_least_60_db_including_exactly_8_khz() {
        // The explicit 8.0 kHz assertion is spec-pinned (the design's worst
        // point sits exactly there, at −60.45 dB).
        let at8k = response_db(8_000.0);
        assert!(at8k <= -60.0, "exactly 8.0 kHz: {at8k:.2} dB");
        let mut f = 8_000.0f64;
        while f <= 24_000.0 {
            let r = response_db(f);
            assert!(r <= -60.0, "{f} Hz: {r:.2} dB above −60 dB");
            f += 100.0;
        }
    }

    // ---- KATs through the streaming Decimator ----

    fn tone(freq_hz: f64, amp: f64, n: usize) -> Vec<i16> {
        (0..n)
            .map(|i| {
                (amp * (2.0 * std::f64::consts::PI * freq_hz * i as f64 / 48_000.0).sin())
                    .round() as i16
            })
            .collect()
    }

    fn rms(s: &[i16]) -> f64 {
        (s.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / s.len() as f64)
            .sqrt()
    }

    #[test]
    fn nine_khz_tone_is_at_least_60_db_down_post_decimation() {
        // The delta's named vector: 9 kHz aliases to 3 kHz after 4:1
        // decimation; the FIR must have killed it BEFORE the alias lands
        // in-band.
        let input = tone(9_000.0, 16_000.0, 96_000); // 2 s
        let mut d = Decimator::new();
        let mut out = Vec::new();
        d.process(&input, &mut out);
        assert_eq!(out.len(), 24_000);
        let steady = &out[1_000..];
        let in_rms = 16_000.0 / std::f64::consts::SQRT_2;
        let out_rms = rms(steady);
        assert!(
            out_rms <= in_rms * 1e-3,
            "9 kHz residue {out_rms:.2} vs input {in_rms:.2} — less than 60 dB down"
        );
    }

    #[test]
    fn one_khz_passband_level_within_half_db() {
        let input = tone(1_000.0, 16_000.0, 96_000);
        let mut d = Decimator::new();
        let mut out = Vec::new();
        d.process(&input, &mut out);
        // 1 kHz at 12 kHz out = 12 samples/period; 9 600 = 800 whole periods.
        let steady = &out[1_000..10_600];
        let in_rms = 16_000.0 / std::f64::consts::SQRT_2;
        let db = 20.0 * (rms(steady) / in_rms).log10();
        assert!(db.abs() <= 0.5, "1 kHz level error {db:.3} dB");
    }

    #[test]
    fn dc_passes_at_unity_gain() {
        let input = vec![8_000i16; 4_800];
        let mut d = Decimator::new();
        let mut out = Vec::new();
        d.process(&input, &mut out);
        assert_eq!(out.len(), 1_200);
        for (i, &v) in out[100..].iter().enumerate() {
            assert!((i32::from(v) - 8_000).abs() <= 1, "output {i}: {v}");
        }
    }

    #[test]
    fn impulse_response_is_the_phase0_taps() {
        // x = [32767, 0, 0, ...] ⇒ y[m] = h[4m]·32767 for 4m ≤ 50; pins the
        // y[m] = Σ h[k]·x[4m−k] alignment (first input sample produces y[0])
        // and the round-half-away quantizer.
        let mut input = vec![0i16; 200];
        input[0] = 32_767;
        let mut d = Decimator::new();
        let mut out = Vec::new();
        d.process(&input, &mut out);
        assert_eq!(out.len(), 50);
        for m in 0..=12 {
            let want = (f64::from(COEFFS[4 * m]) * 32_767.0).round();
            let got = f64::from(out[m]);
            assert!((got - want).abs() <= 1.0, "y[{m}]: got {got}, want {want}");
        }
    }

    fn lcg_noise(n: usize) -> Vec<i16> {
        let mut x: u32 = 0x1234_5678;
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 16) as i16
            })
            .collect()
    }

    #[test]
    fn streaming_equivalence_chunked_equals_oneshot_including_odd_chunks() {
        // Gap fills are clock-sized and arbitrary-length: input-phase
        // tracking across chunk lengths ≢ 0 (mod 4) is load-bearing (spec
        // §Decimator KATs).
        let input = lcg_noise(96_000);
        let mut one = Vec::new();
        Decimator::new().process(&input, &mut one);
        assert_eq!(one.len(), 24_000);

        let sizes = [1usize, 2, 3, 5, 7, 11, 13, 479, 4_800];
        let mut chunked = Vec::new();
        let mut d = Decimator::new();
        let mut off = 0;
        let mut i = 0;
        while off < input.len() {
            let n = sizes[i % sizes.len()].min(input.len() - off);
            d.process(&input[off..off + n], &mut chunked);
            off += n;
            i += 1;
        }
        assert_eq!(chunked, one, "chunked stream must equal one-shot exactly");
    }
}
```

Add `pub mod decimator;` to `src-tauri/tuxlink-capture/src/lib.rs`.

- [ ] **Step 2: Run tests, verify FAIL pattern**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked decimator`
Expected: `committed_table_matches_kaiser_generator`, `passband_ripple_within_spec`, `stopband_attenuation_at_least_60_db_including_exactly_8_khz` PASS immediately (they verify the committed data table — that is their job); every KAT that calls `process` FAILS with the `unimplemented!` panic. If a table-verification test fails instead, the table was mistyped — fix the TABLE against the values in this plan, never loosen the assertion.

- [ ] **Step 3: Implement `process`**

Replace the stub:
```rust
    /// Consume `input` (48 kHz), append decimated 12 kHz samples to `out`.
    /// y[m] = Σₖ h[k]·x[4m − k] with x pre-history = 0; the stream's first
    /// input sample is x[0] and produces y[0]. Phase and filter history
    /// persist across calls, so chunked calls of ANY lengths (including
    /// ≢ 0 mod 4) equal one-shot processing.
    pub fn process(&mut self, input: &[i16], out: &mut Vec<i16>) {
        for &s in input {
            let newest = self.pos;
            self.ring[newest] = f32::from(s);
            self.pos = (self.pos + 1) % TAPS;
            if self.phase == 0 {
                let mut acc = 0.0f32;
                for (k, &c) in COEFFS.iter().enumerate() {
                    acc += c * self.ring[(newest + TAPS - k) % TAPS];
                }
                out.push(saturate_round(acc));
            }
            self.phase = (self.phase + 1) % DECIM;
        }
    }
```

And add the quantizer below the impl block:
```rust
/// Round half away from zero, saturate to i16 (spec §Decimator: "accumulate
/// in f32, round-half-away, saturate").
fn saturate_round(x: f32) -> i16 {
    let r = if x >= 0.0 { (x + 0.5).floor() } else { (x - 0.5).ceil() };
    r.clamp(-32_768.0, 32_767.0) as i16
}
```

- [ ] **Step 4: Run tests green + clippy clean**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked`
Expected: all pass (Tasks 1–3 suites).
Run: `cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --all-targets --locked -- -D warnings`
Expected: clean (the `Default` impl exists precisely so `new_without_default` stays quiet — do not delete it).

- [ ] **Step 5: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/tuxlink-capture/src/decimator.rs src-tauri/tuxlink-capture/src/lib.rs
git commit -m "feat(ft8): 51-tap Kaiser 4:1 decimator with response-verified committed table (tuxlink-b026z.3 T3)"
```

---

**REVIEW GATE A (after Tasks 1–3):** review the DSP + writer batch from multiple perspectives: (1) numeric honesty — the response tests derive everything from `COEFFS` itself, no magic expected-values that could mask a mistyped table; (2) contract fidelity — writer output vs `tuxlink_jt9::wav::preflight_slot_wav` byte layout, `OUT_SLOT_FRAMES`/`OUT_RATE_HZ` vs the L1 crate's `SLOT_FRAMES`/`SLOT_RATE_HZ`; (3) state-machine honesty of the decimator — phase/history across odd chunks, saturation, clippy/MSRV. Minimum three rounds; persist findings to `dev/scratch/b026z.3-gate-A-findings.md` before proceeding. Files under review: `src-tauri/tuxlink-capture/Cargo.toml`, `src-tauri/tuxlink-capture/src/lib.rs`, `src-tauri/tuxlink-capture/src/bands.rs`, `src-tauri/tuxlink-capture/src/wavwrite.rs`, `src-tauri/tuxlink-capture/src/decimator.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`.

---

### Task 4: Slot assembler core — boundaries, gap fill, provenance

**Files:**
- Create: `src-tauri/tuxlink-capture/src/slot.rs`
- Modify: `src-tauri/tuxlink-capture/src/lib.rs` (add `pub mod slot;`)

**Interfaces:**
- Consumes: `decimator::Decimator` (owned internally; filter state persists across slot boundaries — the pinned continuity model).
- Produces (consumed by Task 5's anomaly extensions and Phase C's capture thread):
```rust
pub const IN_RATE_HZ: u64 = 48_000;
pub const IN_SLOT_FRAMES: usize = 720_000;
pub const OUT_SLOT_FRAMES: usize = 180_000;

pub struct BoundaryConfig {
    pub slot_ms: u64,                    // 15_000
    pub min_gap_fill_frames: u64,        // 2_400 (50 ms)
    pub max_single_gap_frames: u64,      // 48_000 (1 s) — per-gap clock-anomaly bound
    pub max_lost_frames: u64,            // 48_000 (1 s) — per-slot cumulative drop bound (enforced in Task 5)
    pub max_boundary_divergence_ms: u64, // 1_000 — UTC-vs-mono at a boundary
} // + impl Default with exactly those values

pub struct GapReport { pub kind: GapKind }
pub enum GapKind { Overrun, Suspended }
pub enum DiscardClass { FirstSlot, ClockAnomaly }
pub enum SlotEvent {
    Completed(CompletedSlot),
    Abandoned { class: DiscardClass },
    // Task 5 adds: Dropped { class: DropClass }
}
pub struct CompletedSlot {
    pub slot_utc_ms: u64,
    pub samples: Vec<i16>,          // exactly OUT_SLOT_FRAMES @ 12 kHz
    pub lost_frames: u64,           // input-rate zero-filled frames (gap fills + boundary shortfall)
    pub boundary_skew_frames: u64,  // input-rate surplus dropped at close (never carried)
    pub clip_fraction: f32,         // delivered frames only
    pub rms_dbfs: f32,              // delivered frames only; NEG_INFINITY when none
}
pub struct SlotAssembler;
impl SlotAssembler {
    pub fn new(cfg: BoundaryConfig) -> SlotAssembler;
    pub fn push(&mut self, samples: &[i16], utc_now_ms: u64, mono_now_us: u64,
                gap: Option<GapReport>) -> Vec<SlotEvent>;
}
```

**Pinned reading of the spec (§Slot assembly):** the expected-frame counter runs at the INPUT rate (48 kHz), slots are 720,000 input frames, and zero-fill happens at the input side before decimation. Therefore the assembler consumes the RAW post-extraction 48 kHz channel-0 stream, owns zero-fill at 48 k, holds the `Decimator` internally, and emits 180,000-frame `CompletedSlot`s. All frame math is at 48 k; `lost_frames` is recorded at the input rate. The assembler is PURE — wall and monotonic time arrive as values with every push (`utc_now_ms`, `mono_now_us` timestamp the END of the delivered batch), never read ambiently. Boundary handling is batch-granular (periods are 100 ms in production), well inside the ±0.5 s start tolerance jt9's ±2 s DT absorbs. The batch that crosses a boundary belongs wholly to the NEW slot; the old slot closes first (shortfall zero-fill / surplus truncation), so nothing is ever carried.

- [ ] **Step 1: Write the types, a stubbed `push`, and the failing core tests**

`src-tauri/tuxlink-capture/src/slot.rs`:
```rust
//! Wall-clock-true 15 s UTC slot assembler (spec §Slot assembly).
//!
//! PURE: time is data at this seam — `(utc_now_ms, mono_now_us)` arrive
//! with every push and are never read ambiently. The two clock domains
//! have disjoint jobs (pinned):
//!   - UTC labels slot identity only: sampled at boundary detection to
//!     stamp `slot_utc_ms` (0/15/30/45 s, start within ±0.5 s) and to
//!     choose the next boundary.
//!   - Monotonic drives everything inside a slot: the per-slot anchor is
//!     captured at the boundary and the expected-frame counter is
//!     (mono_now − anchor) × 48 000 — NTP steps and slews cannot
//!     manufacture in-slot gaps.
//!
//! Input is the RAW post-extraction 48 kHz channel-0 stream. The assembler
//! owns zero-fill at 48 k, holds the `Decimator` (filter state persists
//! across slot boundaries — continuity model), and emits exactly
//! 180 000-frame `CompletedSlot`s.

use crate::decimator::Decimator;

pub const IN_RATE_HZ: u64 = 48_000;
pub const IN_SLOT_FRAMES: usize = 720_000;
pub const OUT_SLOT_FRAMES: usize = 180_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryConfig {
    /// Slot length in UTC milliseconds.
    pub slot_ms: u64,
    /// Gap deficits below this are scheduling jitter, not loss — filling
    /// them is pure signal damage (spec: 2 400 frames = 50 ms; empirical
    /// basis: 0.25 s time-shift = 0 decodes; 0.25 s zero-filled = 13/14).
    pub min_gap_fill_frames: u64,
    /// A SINGLE intra-slot gap above this is a clock anomaly (48 000 = 1 s).
    pub max_single_gap_frames: u64,
    /// Cumulative filled frames above this drop the slot as a real failure
    /// (48 000 = 1 s). Enforced in Task 5.
    pub max_lost_frames: u64,
    /// UTC-vs-monotonic divergence above this observed at a boundary is a
    /// clock anomaly (an NTP step): 1 000 ms.
    pub max_boundary_divergence_ms: u64,
}

impl Default for BoundaryConfig {
    fn default() -> Self {
        Self {
            slot_ms: 15_000,
            min_gap_fill_frames: 2_400,
            max_single_gap_frames: 48_000,
            max_lost_frames: 48_000,
            max_boundary_divergence_ms: 1_000,
        }
    }
}

/// Reported by the capture loop alongside the first batch after a
/// gap-causing event. EPIPE tells us THAT an overrun occurred, never how
/// much was lost — the deficit always comes from the monotonic
/// expected-frame counter (spec §ALSA read loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GapReport {
    pub kind: GapKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapKind {
    /// Capture restarted after an ALSA overrun (`-EPIPE` recover).
    Overrun,
    /// `-ESTRPIPE`: the stream was suspended — uniformly a clock anomaly.
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardClass {
    /// The partial first slot after start/resume — a scheduled discard
    /// (policy, not failure; counts toward NEITHER counter).
    FirstSlot,
    /// Negative computed gap, a single gap longer than 1 s, UTC-vs-mono
    /// divergence over 1 s at a boundary, or suspend: the slot's timing
    /// cannot be trusted. Scheduled discard; re-anchor at the next UTC
    /// boundary. (Doc phrasing avoids a line-leading `>` — clippy's
    /// doc-quote lint.)
    ClockAnomaly,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlotEvent {
    Completed(CompletedSlot),
    Abandoned { class: DiscardClass },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletedSlot {
    /// UTC label: the slot's boundary, a multiple of `slot_ms`.
    pub slot_utc_ms: u64,
    /// Exactly `OUT_SLOT_FRAMES` decimated 12 kHz frames.
    pub samples: Vec<i16>,
    /// Zero-filled input frames inside the slot (gap fills + boundary
    /// shortfall), input rate.
    pub lost_frames: u64,
    /// Surplus input frames DROPPED at boundary close — never carried
    /// (carryover would accumulate card-vs-wall skew without bound; at
    /// ≤ 50 ppm the drop lands in FT8's inter-slot guard interval).
    pub boundary_skew_frames: u64,
    /// Fraction of DELIVERED frames at ±full scale (fills excluded).
    pub clip_fraction: f32,
    /// RMS of DELIVERED frames only, dBFS re 32768 (denominator
    /// 720 000 − lost, so degraded slots don't read as quiet).
    /// `f32::NEG_INFINITY` when nothing was delivered.
    pub rms_dbfs: f32,
}

pub struct SlotAssembler {
    cfg: BoundaryConfig,
    decimator: Decimator,
    phase: Phase,
    /// The scheduled first-slot discard record is emitted when the first
    /// boundary after construction opens (start/resume). Clock-anomaly
    /// re-anchors do NOT re-emit it — the anomaly emitted its own record.
    pending_first_slot_discard: bool,
}

enum Phase {
    /// Waiting for the next UTC boundary; the target is chosen from the
    /// first push seen while waiting.
    Waiting { next_boundary_utc_ms: Option<u64> },
    InSlot(Current),
}

struct Current {
    slot_utc_ms: u64,
    anchor_mono_us: u64,
    /// Delivered + zero-filled input frames, capped at IN_SLOT_FRAMES.
    buf: Vec<i16>,
    lost_frames: u64,
    surplus_frames: u64,
    clipped: u64,
    delivered_in_slot: u64,
    sum_sq: f64,
}

impl Current {
    fn open(slot_utc_ms: u64, anchor_mono_us: u64) -> Self {
        Self {
            slot_utc_ms,
            anchor_mono_us,
            buf: Vec::with_capacity(IN_SLOT_FRAMES),
            lost_frames: 0,
            surplus_frames: 0,
            clipped: 0,
            delivered_in_slot: 0,
            sum_sq: 0.0,
        }
    }

    fn append_delivered(&mut self, samples: &[i16]) {
        for &s in samples {
            if self.buf.len() < IN_SLOT_FRAMES {
                self.delivered_in_slot += 1;
                if s == i16::MAX || s == i16::MIN {
                    self.clipped += 1;
                }
                self.sum_sq += f64::from(s) * f64::from(s);
                self.buf.push(s);
            } else {
                // Surplus past the slot's 720 000: dropped at close, never
                // carried — recorded as boundary_skew_frames.
                self.surplus_frames += 1;
            }
        }
    }

    fn fill_zeros(&mut self, frames: u64) {
        let room = (IN_SLOT_FRAMES - self.buf.len()) as u64;
        let n = frames.min(room);
        self.buf.resize(self.buf.len() + n as usize, 0);
        self.lost_frames += n;
    }

    /// Input frames accounted to this slot so far (delivered-in-slot +
    /// fills + surplus) — the "have" side of the deficit computation.
    fn accounted_input_frames(&self) -> u64 {
        self.buf.len() as u64 + self.surplus_frames
    }
}

impl SlotAssembler {
    pub fn new(cfg: BoundaryConfig) -> Self {
        Self {
            cfg,
            decimator: Decimator::new(),
            phase: Phase::Waiting { next_boundary_utc_ms: None },
            pending_first_slot_discard: true,
        }
    }

    pub fn push(
        &mut self,
        samples: &[i16],
        utc_now_ms: u64,
        mono_now_us: u64,
        gap: Option<GapReport>,
    ) -> Vec<SlotEvent> {
        let _ = (samples, utc_now_ms, mono_now_us, gap);
        unimplemented!("stub — replaced in Step 3")
    }
}
```

Then append the test module (the `Sim` helper is shared by Task 5's tests; batch/gap frame counts are kept divisible by 48 so the µs advance `frames × 125 / 6` stays exact):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Drives the assembler with synthetic, exactly-tracked clocks.
    /// Frame counts used by tests are multiples of 48 so the µs conversion
    /// (× 125 / 6) is exact and UTC/mono never drift by rounding.
    struct Sim {
        asm: SlotAssembler,
        utc_us: u64,
        mono_us: u64,
        events: Vec<SlotEvent>,
    }

    impl Sim {
        fn new(start_utc_ms: u64) -> Self {
            Self {
                asm: SlotAssembler::new(BoundaryConfig::default()),
                utc_us: start_utc_ms * 1_000,
                mono_us: 5_000_000,
                events: Vec::new(),
            }
        }

        fn advance_frames(&mut self, frames: u64) {
            assert_eq!(frames % 48, 0, "test discipline: keep µs math exact");
            let us = frames * 125 / 6;
            self.utc_us += us;
            self.mono_us += us;
        }

        /// Real time passes for `frames` worth of audio AND the batch is
        /// delivered (the normal case).
        fn deliver(&mut self, frames: usize, value: i16) {
            self.deliver_gap(frames, value, None);
        }

        fn deliver_gap(&mut self, frames: usize, value: i16, gap: Option<GapReport>) {
            self.advance_frames(frames as u64);
            let batch = vec![value; frames];
            let ev = self.asm.push(&batch, self.utc_us / 1_000, self.mono_us, gap);
            self.events.extend(ev);
        }

        /// Time passes, nothing is delivered (a dropout).
        fn stall_frames(&mut self, frames: u64) {
            self.advance_frames(frames);
        }

        /// Deliver `frames` while only `wall_frames` of real time pass —
        /// a card catching up after jitter (wall < frames), or a fabricated
        /// negative-gap anomaly (wall = 0, Task 5).
        fn deliver_wall(
            &mut self,
            frames: usize,
            wall_frames: u64,
            value: i16,
            gap: Option<GapReport>,
        ) {
            self.advance_frames(wall_frames);
            let batch = vec![value; frames];
            let ev = self.asm.push(&batch, self.utc_us / 1_000, self.mono_us, gap);
            self.events.extend(ev);
        }

        fn completed(&self) -> Vec<&CompletedSlot> {
            self.events
                .iter()
                .filter_map(|e| match e {
                    SlotEvent::Completed(c) => Some(c),
                    _ => None,
                })
                .collect()
        }

        fn abandoned(&self) -> Vec<DiscardClass> {
            self.events
                .iter()
                .filter_map(|e| match e {
                    SlotEvent::Abandoned { class } => Some(*class),
                    _ => None,
                })
                .collect()
        }
    }

    /// 100 ms production-period batches.
    const BATCH: usize = 4_800;

    #[test]
    fn first_partial_slot_is_a_scheduled_discard_and_first_full_slot_completes() {
        // Start mid-slot at UTC 10.03 s (off-phase from the boundary, like
        // real capture): everything before the 15.0 s boundary is the
        // partial first slot (discarded, one FirstSlot record); the
        // 15.0–30.0 s slot completes.
        let mut sim = Sim::new(10_030);
        for _ in 0..250 {
            sim.deliver(BATCH, 1_000); // 25 s of audio
        }
        assert_eq!(sim.abandoned(), vec![DiscardClass::FirstSlot]);
        let done = sim.completed();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].slot_utc_ms, 15_000);
        assert_eq!(done[0].samples.len(), OUT_SLOT_FRAMES);
        assert_eq!(done[0].lost_frames, 0);
        assert_eq!(done[0].boundary_skew_frames, 0);
    }

    #[test]
    fn boundary_detection_is_within_half_a_second() {
        // Batches land at 100 ms cadence starting off-phase (start UTC
        // 10.030 s → pushes at ...14.930, 15.030...): the slot label is the
        // exact multiple of 15 000 and the opening batch arrived within
        // 0.5 s after it.
        let mut sim = Sim::new(10_030);
        let mut opened_at_utc_ms = None;
        for _ in 0..200 {
            sim.deliver(BATCH, 0);
            if opened_at_utc_ms.is_none() && !sim.abandoned().is_empty() {
                opened_at_utc_ms = Some(sim.utc_us / 1_000);
            }
        }
        let opened = opened_at_utc_ms.expect("slot must open");
        assert!((15_000..15_500).contains(&opened), "opened at {opened}");
        assert_eq!(sim.completed()[0].slot_utc_ms, 15_000);
    }

    #[test]
    fn boundary_shortfall_is_zero_filled_to_exact_length() {
        // A slow source: one 4 800-frame batch goes missing near the end of
        // the slot with NO gap report and no catch-up (frames simply never
        // existed — e.g. a slow card). The close must fill to exactly
        // 720 000 in / 180 000 out and account the fill in lost_frames.
        let mut sim = Sim::new(10_030);
        for _ in 0..50 {
            sim.deliver(BATCH, 1_000); // reach the 15 s boundary
        }
        for _ in 0..148 {
            sim.deliver(BATCH, 1_000); // with the opener: 149 of 150 batches
        }
        sim.stall_frames(BATCH as u64); // one batch of wall time, no data
        for _ in 0..30 {
            sim.deliver(BATCH, 1_000); // crosses the 30 s boundary
        }
        let done = sim.completed();
        assert!(!done.is_empty());
        assert_eq!(done[0].slot_utc_ms, 15_000);
        assert_eq!(done[0].samples.len(), OUT_SLOT_FRAMES);
        assert_eq!(done[0].lost_frames, BATCH as u64, "shortfall counted as filled");
    }

    #[test]
    fn gap_is_zero_filled_in_place_and_counted() {
        // 0.5 s dropout mid-slot with an Overrun report on the next batch:
        // 24 000 zeros land immediately after the last delivered frame.
        // Delivered content is DC 1000, so the decimated output shows ~0 in
        // the filled region and ~1000 away from it — placement is
        // observable, not just counted.
        let mut sim = Sim::new(10_030);
        for _ in 0..50 {
            sim.deliver(BATCH, 1_000);
        }
        for _ in 0..74 {
            sim.deliver(BATCH, 1_000); // with the opener: 360 000 frames in
        }
        sim.stall_frames(24_000); // 0.5 s dropout
        sim.deliver_gap(BATCH, 1_000, Some(GapReport { kind: GapKind::Overrun }));
        for _ in 0..70 {
            sim.deliver(BATCH, 1_000); // the 70th push crosses the boundary
        }
        let done = sim.completed();
        assert_eq!(done.len(), 1);
        let slot = done[0];
        assert_eq!(slot.lost_frames, 24_000);
        assert_eq!(slot.boundary_skew_frames, 0);
        assert_eq!(slot.samples.len(), OUT_SLOT_FRAMES);
        // Fill placement: input frames 360 000..384 000 are zeros → output
        // indices 90 000..96 000. Probe the middle of the fill and a point
        // far from it.
        assert!(
            slot.samples[91_800].abs() < 50,
            "fill region should be ~0, got {}",
            slot.samples[91_800]
        );
        assert!(
            (i32::from(slot.samples[50_000]) - 1_000).abs() <= 2,
            "delivered region should be ~1000, got {}",
            slot.samples[50_000]
        );
    }

    #[test]
    fn sub_threshold_deficit_is_jitter_not_loss() {
        // A 2 352-frame deficit (< 2 400) with an Overrun report: NO fill.
        // The late frames then arrive (catch-up, no time advance) and the
        // slot completes with lost_frames == 0.
        let mut sim = Sim::new(10_030);
        for _ in 0..50 {
            sim.deliver(BATCH, 1_000);
        }
        for _ in 0..100 {
            sim.deliver(BATCH, 1_000);
        }
        sim.stall_frames(2_352);
        sim.deliver_gap(BATCH, 1_000, Some(GapReport { kind: GapKind::Overrun }));
        // The card catches up: a full batch delivered in 2 448 frames of
        // wall time (49 + 51 ms = one whole 100 ms period — cadence
        // restored, the late frames were jitter, not loss).
        sim.deliver_wall(BATCH, 2_448, 1_000, None);
        for _ in 0..49 {
            sim.deliver(BATCH, 1_000); // the 48th push crosses the boundary
        }
        let done = sim.completed();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].lost_frames, 0, "sub-threshold deficits are never filled");
        assert_eq!(done[0].boundary_skew_frames, 0);
        assert_eq!(done[0].samples.len(), OUT_SLOT_FRAMES);
    }

    #[test]
    fn exact_divisibility_720000_in_180000_out() {
        // The clean case: exactly 150 batches per slot, zero fill, zero
        // skew, for three consecutive slots (proves per-slot re-anchoring
        // and decimator phase continuity: 720 000 ≡ 0 mod 4). Start 130 ms
        // before the boundary: the FIRST push (14 970) picks 15 000 as the
        // target — a start whose first push lands exactly ON a boundary
        // waits for the NEXT one (documented knife-edge; scheduled-discard
        // either way).
        let mut sim = Sim::new(15_000 - 130);
        for _ in 0..(1 + 150 * 3 + 1) {
            sim.deliver(BATCH, 200);
        }
        let done = sim.completed();
        assert!(done.len() >= 3, "got {} slots", done.len());
        for (i, slot) in done.iter().take(3).enumerate() {
            assert_eq!(slot.slot_utc_ms, 15_000 + 15_000 * i as u64);
            assert_eq!(slot.samples.len(), OUT_SLOT_FRAMES, "slot {i}");
            assert_eq!(slot.lost_frames, 0, "slot {i}");
            assert_eq!(slot.boundary_skew_frames, 0, "slot {i}");
        }
    }

    #[test]
    fn provenance_math_is_computed_on_delivered_frames_only() {
        // First 48 000 delivered frames are full-scale (clipped), the rest
        // are 16 384; a 24 000-frame filled gap sits in the middle. The
        // denominator is delivered frames (720 000 − 24 000 = 696 000);
        // fills are excluded from clip_fraction and rms_dbfs.
        let mut sim = Sim::new(10_030);
        for _ in 0..50 {
            // Pre-boundary fodder at 16 384 too: the 50th push OPENS the
            // slot and its batch is delivered slot content.
            sim.deliver(BATCH, 16_384);
        }
        for _ in 0..10 {
            sim.deliver(BATCH, i16::MAX); // 48 000 clipped frames
        }
        for _ in 0..64 {
            sim.deliver(BATCH, 16_384);
        }
        sim.stall_frames(24_000);
        sim.deliver_gap(BATCH, 16_384, Some(GapReport { kind: GapKind::Overrun }));
        for _ in 0..70 {
            sim.deliver(BATCH, 16_384); // the 70th push crosses the boundary
        }
        let done = sim.completed();
        assert_eq!(done.len(), 1);
        let slot = done[0];
        assert_eq!(slot.lost_frames, 24_000);
        let delivered = 720_000.0 - 24_000.0;
        let want_clip = 48_000.0 / delivered;
        assert!(
            (f64::from(slot.clip_fraction) - want_clip).abs() < 1e-6,
            "clip_fraction {} want {want_clip}",
            slot.clip_fraction
        );
        let sum_sq = 48_000.0 * f64::from(i16::MAX) * f64::from(i16::MAX)
            + (delivered - 48_000.0) * 16_384.0f64 * 16_384.0;
        let want_rms_dbfs = 20.0 * ((sum_sq / delivered).sqrt() / 32_768.0).log10();
        assert!(
            (f64::from(slot.rms_dbfs) - want_rms_dbfs).abs() < 0.01,
            "rms_dbfs {} want {want_rms_dbfs:.4}",
            slot.rms_dbfs
        );
    }
}
```

Add `pub mod slot;` to `src-tauri/tuxlink-capture/src/lib.rs`.

- [ ] **Step 2: Run tests, verify they FAIL on the stub panic**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked slot`
Expected: all 7 slot tests FAIL with the `unimplemented!` panic (none may pass — every one drives `push`).

- [ ] **Step 3: Implement `push` and its helpers**

Replace the stubbed `push` inside `impl SlotAssembler` (keep `new` as written) and add the two private methods:
```rust
    /// Feed one delivered batch. `utc_now_ms`/`mono_now_us` are sampled by
    /// the capture loop AFTER the batch was read (they timestamp the batch
    /// end). Returns zero or more slot events.
    pub fn push(
        &mut self,
        samples: &[i16],
        utc_now_ms: u64,
        mono_now_us: u64,
        gap: Option<GapReport>,
    ) -> Vec<SlotEvent> {
        let mut events = Vec::new();
        let slot_ms = self.cfg.slot_ms;

        if matches!(gap, Some(GapReport { kind: GapKind::Suspended })) {
            // -ESTRPIPE: uniformly a clock anomaly — abandon, re-anchor at
            // the next boundary. The suspended batch dies with the slot.
            self.abandon_clock_anomaly(&mut events);
            return events;
        }

        // Waiting: open at the first push at/after the chosen boundary.
        if let Phase::Waiting { next_boundary_utc_ms } = &mut self.phase {
            let next = *next_boundary_utc_ms
                .get_or_insert((utc_now_ms / slot_ms + 1) * slot_ms);
            if utc_now_ms < next {
                return events; // pre-boundary partial: scheduled discard
            }
            if self.pending_first_slot_discard {
                self.pending_first_slot_discard = false;
                events.push(SlotEvent::Abandoned {
                    class: DiscardClass::FirstSlot,
                });
            }
            let slot_utc = utc_now_ms - utc_now_ms % slot_ms;
            // The anchor is the opening batch's START (mono_now minus the
            // batch's duration): anchoring at the batch END would bias
            // every later deficit computation one batch low and misfire
            // the negative-gap anomaly on healthy streams.
            let anchor = mono_now_us.saturating_sub(frames_to_us(samples.len()));
            let mut cur = Current::open(slot_utc, anchor);
            cur.append_delivered(samples); // the crossing batch opens the slot
            self.phase = Phase::InSlot(cur);
            return events;
        }

        // In slot: the boundary close runs BEFORE the crossing batch is
        // appended — the batch arrived after the boundary and belongs
        // wholly to the new slot (batch granularity ≤ one 100 ms period,
        // inside the ±0.5 s start tolerance).
        let (slot_utc, anchor) = match &self.phase {
            Phase::InSlot(c) => (c.slot_utc_ms, c.anchor_mono_us),
            Phase::Waiting { .. } => unreachable!("handled above"),
        };
        if utc_now_ms >= slot_utc + slot_ms {
            // Clock-anomaly rule: UTC-vs-monotonic divergence observed at
            // the boundary (an NTP step) abandons the slot.
            let mono_elapsed_ms = mono_now_us.saturating_sub(anchor) / 1_000;
            let utc_elapsed_ms = utc_now_ms - slot_utc;
            if mono_elapsed_ms.abs_diff(utc_elapsed_ms)
                > self.cfg.max_boundary_divergence_ms
            {
                self.abandon_clock_anomaly(&mut events);
                return events; // the crossing batch dies with the anomaly
            }
            self.close_slot(&mut events);
            let slot_utc = utc_now_ms - utc_now_ms % slot_ms;
            // Same batch-START anchoring as the open path above.
            let anchor = mono_now_us.saturating_sub(frames_to_us(samples.len()));
            let mut cur = Current::open(slot_utc, anchor);
            cur.append_delivered(samples);
            self.phase = Phase::InSlot(cur);
            return events;
        }

        // Overrun gap: the deficit comes from the monotonic expected-frame
        // counter, never from ALSA (spec §ALSA read loop / §Slot assembly).
        if matches!(gap, Some(GapReport { kind: GapKind::Overrun })) {
            let Phase::InSlot(cur) = &mut self.phase else {
                unreachable!("handled above")
            };
            let mono_elapsed_us = mono_now_us.saturating_sub(cur.anchor_mono_us);
            let expected = (u128::from(mono_elapsed_us) * u128::from(IN_RATE_HZ)
                / 1_000_000) as u64;
            let have = cur.accounted_input_frames() + samples.len() as u64;
            if expected < have {
                // Negative computed gap: clock anomaly (spec rule).
                self.abandon_clock_anomaly(&mut events);
                return events;
            }
            let deficit = expected - have;
            if deficit > self.cfg.max_single_gap_frames {
                // A single intra-slot gap > 1 s: clock anomaly.
                self.abandon_clock_anomaly(&mut events);
                return events;
            }
            if deficit >= self.cfg.min_gap_fill_frames {
                // Zero-fill in place, immediately after the last delivered
                // frame (i.e. BEFORE this batch is appended).
                cur.fill_zeros(deficit);
            }
            // Below the threshold: scheduling jitter — never filled.
        }

        let Phase::InSlot(cur) = &mut self.phase else {
            unreachable!("handled above")
        };
        cur.append_delivered(samples);
        events
    }

    fn abandon_clock_anomaly(&mut self, events: &mut Vec<SlotEvent>) {
        if matches!(self.phase, Phase::InSlot(_)) {
            events.push(SlotEvent::Abandoned {
                class: DiscardClass::ClockAnomaly,
            });
        }
        // Re-anchor: the next boundary is chosen from the NEXT push's UTC
        // (time may have stepped arbitrarily). No FirstSlot record — the
        // anomaly is its own record.
        self.phase = Phase::Waiting { next_boundary_utc_ms: None };
    }

    fn close_slot(&mut self, events: &mut Vec<SlotEvent>) {
        let Phase::InSlot(cur) = &mut self.phase else { return };
        let shortfall = (IN_SLOT_FRAMES - cur.buf.len()) as u64;
        if shortfall > 0 {
            cur.fill_zeros(shortfall);
        }
        // Task 5 inserts the lost-frames drop check HERE.
        let mut samples = Vec::with_capacity(OUT_SLOT_FRAMES);
        self.decimator.process(&cur.buf, &mut samples);
        debug_assert_eq!(samples.len(), OUT_SLOT_FRAMES);
        let clip_fraction = if cur.delivered_in_slot == 0 {
            0.0
        } else {
            (cur.clipped as f64 / cur.delivered_in_slot as f64) as f32
        };
        let rms_dbfs = if cur.delivered_in_slot == 0 {
            f32::NEG_INFINITY
        } else {
            let rms = (cur.sum_sq / cur.delivered_in_slot as f64).sqrt();
            (20.0 * (rms / 32_768.0).log10()) as f32
        };
        events.push(SlotEvent::Completed(CompletedSlot {
            slot_utc_ms: cur.slot_utc_ms,
            samples,
            lost_frames: cur.lost_frames,
            boundary_skew_frames: cur.surplus_frames,
            clip_fraction,
            rms_dbfs,
        }));
    }
```

And add this free helper below the `impl SlotAssembler` block:
```rust
/// Duration of `frames` input frames, in µs (exact for the 100 ms
/// production period; truncates sub-µs remainders for odd lengths).
fn frames_to_us(frames: usize) -> u64 {
    frames as u64 * 1_000_000 / IN_RATE_HZ
}
```

- [ ] **Step 4: Run tests green + clippy clean**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked`
Expected: all pass (the slot suite takes a few seconds — each completed slot decimates 720,000 frames).
Run: `cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --all-targets --locked -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/tuxlink-capture/src/slot.rs src-tauri/tuxlink-capture/src/lib.rs
git commit -m "feat(ft8): wall-clock-true slot assembler — boundaries, gap fill, provenance (tuxlink-b026z.3 T4)"
```

---

### Task 5: Slot anomaly + skew rules — surplus drop, clock anomalies, lost-frames drop

**Files:**
- Modify: `src-tauri/tuxlink-capture/src/slot.rs`

**Interfaces:**
- Produces (extends Task 4's `SlotEvent`; consumed by Phase C's outcome folding):
```rust
pub enum DropClass { LostFrames }
// SlotEvent gains the variant:
//   Dropped { class: DropClass, slot_utc_ms: u64, lost_frames: u64 }
```
`Dropped` carries `slot_utc_ms` + `lost_frames` (beyond the bare `class`) because Phase C's ring records every slot boundary with provenance — a drop event without its slot label could not be ring-recorded honestly.

**Honest scoping note:** Task 4's `push` already implements the clock-anomaly abandon paths and surplus capping (they are inseparable from the push control flow). Task 5's NEW code is the `Dropped` variant + the lost-frames check in `close_slot`; the rest of this task is the spec-pinned NAMED tests for the anomaly/skew rules (spec §Testing strategy: surplus drop with the fast-clock 1000-slot bound, negative gap, > 1 s gap, UTC-vs-mono divergence, suspend, lost-frames drop, carry-nothing invariant). Those tests pin behavior permanently; several will pass on first run — that is their job here.

- [ ] **Step 1: Write the failing/pinning tests**

Append to the test module in `src-tauri/tuxlink-capture/src/slot.rs`:
```rust
    #[test]
    fn surplus_is_dropped_at_close_never_carried() {
        // A 1% fast card: 4 800 frames arrive in 99 ms of wall time. Every
        // slot sheds its own bounded surplus as boundary_skew_frames and
        // the NEXT slot starts clean — no inherited offset, no later
        // shortfall (the carry-nothing invariant).
        let mut sim = Sim::new(10_030);
        for _ in 0..1_000 {
            sim.deliver_wall(BATCH, 4_752, 0, None);
        }
        let done = sim.completed();
        assert!(done.len() >= 5, "got {} slots", done.len());
        for (i, slot) in done.iter().enumerate() {
            assert_eq!(
                slot.lost_frames, 0,
                "slot {i}: dropped surplus must never resurface as fill"
            );
            assert!(
                slot.boundary_skew_frames > 0,
                "slot {i}: a fast card must shed surplus every slot"
            );
            assert!(
                slot.boundary_skew_frames <= 2 * BATCH as u64,
                "slot {i}: skew {} not bounded",
                slot.boundary_skew_frames
            );
            assert_eq!(slot.samples.len(), OUT_SLOT_FRAMES, "slot {i}");
        }
    }

    #[test]
    fn fast_clock_1000_slots_keeps_skew_bounded_never_carried() {
        // +50 ppm soundcard: 4 800 frames delivered every 99 995 µs. Spec
        // §Testing strategy pins 1 000 slots: slot-content-vs-UTC skew
        // stays bounded (carryover would accumulate ~4.3 s/day — the
        // delta's time-shift kill mechanism, self-inflicted; zero decodes
        // after ~11 h). NOTE: this test decimates 180 M output samples —
        // ~25 s in release, ~5 MINUTES in a debug build on the dev Pi. It
        // is not hung; `cargo test --release -p tuxlink-capture` is a
        // legitimate iteration shortcut (CI runs the debug profile).
        let mut asm = SlotAssembler::new(BoundaryConfig::default());
        let mut utc_us: u64 = 10_030_000;
        let mut mono_us: u64 = 5_000_000;
        let batch = vec![0i16; BATCH];
        let mut completed = 0usize;
        let mut max_skew = 0u64;
        let mut total_skew = 0u64;
        while completed < 1_000 {
            utc_us += 99_995;
            mono_us += 99_995;
            for ev in asm.push(&batch, utc_us / 1_000, mono_us, None) {
                match ev {
                    SlotEvent::Completed(c) => {
                        completed += 1;
                        assert_eq!(c.samples.len(), OUT_SLOT_FRAMES);
                        assert_eq!(c.lost_frames, 0, "slot {completed}");
                        max_skew = max_skew.max(c.boundary_skew_frames);
                        total_skew += c.boundary_skew_frames;
                    }
                    SlotEvent::Abandoned { class } => {
                        assert_eq!(
                            class,
                            DiscardClass::FirstSlot,
                            "only the scheduled first-slot discard is allowed"
                        );
                    }
                    SlotEvent::Dropped { .. } => {
                        panic!("a healthy fast clock must never drop a slot")
                    }
                }
            }
        }
        // Bounded per slot: never more than one delivery batch of surplus.
        assert!(max_skew <= BATCH as u64, "max per-slot skew {max_skew}");
        // And the surplus is real (~36 frames/slot at +50 ppm): if closes
        // silently carried instead of dropping, this would read 0 while
        // slot content drifted ~0.75 s by slot 1 000.
        assert!(total_skew >= 20_000, "total dropped surplus {total_skew}");
    }

    #[test]
    fn negative_computed_gap_is_a_clock_anomaly() {
        let mut sim = Sim::new(10_030);
        for _ in 0..60 {
            sim.deliver(BATCH, 0); // opens at 15.03 s + 10 in-slot batches
        }
        // An Overrun report whose batch arrives with ZERO wall advance:
        // delivered exceeds the monotonic expectation → negative gap.
        sim.deliver_wall(BATCH, 0, 0, Some(GapReport { kind: GapKind::Overrun }));
        assert_eq!(
            sim.abandoned(),
            vec![DiscardClass::FirstSlot, DiscardClass::ClockAnomaly]
        );
        assert!(sim.completed().is_empty());
        // Re-anchor at the NEXT boundary (30 s); the following slot
        // completes — and no second FirstSlot record appears (the anomaly
        // was its own record).
        for _ in 0..320 {
            sim.deliver(BATCH, 0);
        }
        assert_eq!(sim.completed().len(), 1);
        assert_eq!(sim.completed()[0].slot_utc_ms, 30_000);
        assert_eq!(
            sim.abandoned(),
            vec![DiscardClass::FirstSlot, DiscardClass::ClockAnomaly]
        );
    }

    #[test]
    fn single_gap_over_one_second_is_a_clock_anomaly() {
        let mut sim = Sim::new(10_030);
        for _ in 0..60 {
            sim.deliver(BATCH, 0);
        }
        sim.stall_frames(50_400); // a single 1.05 s dropout
        sim.deliver_gap(BATCH, 0, Some(GapReport { kind: GapKind::Overrun }));
        assert_eq!(
            sim.abandoned(),
            vec![DiscardClass::FirstSlot, DiscardClass::ClockAnomaly]
        );
        assert!(sim.completed().is_empty());
    }

    #[test]
    fn utc_vs_mono_divergence_at_boundary_is_a_clock_anomaly() {
        let mut sim = Sim::new(10_030);
        for _ in 0..100 {
            sim.deliver(BATCH, 0); // utc 20.03 s, mid-slot
        }
        sim.utc_us += 2_000_000; // NTP step: UTC jumps +2 s; monotonic does not
        for _ in 0..80 {
            sim.deliver(BATCH, 0); // reaches the (stepped) boundary
        }
        assert!(
            sim.abandoned().contains(&DiscardClass::ClockAnomaly),
            "the step must be observed at the boundary"
        );
        assert!(sim.completed().is_empty());
        // Recovery: re-anchored at the next boundary, a full clean slot
        // completes (300 pushes cover waiting out the partial interval plus
        // one whole slot).
        for _ in 0..310 {
            sim.deliver(BATCH, 0);
        }
        assert_eq!(sim.completed().len(), 1);
    }

    #[test]
    fn cumulative_lost_frames_over_one_second_drops_the_slot() {
        // Two 0.6 s gaps: each under the 48 000-frame single-gap anomaly
        // bound, together 57 600 filled frames — over the 48 000
        // lost-frames bound. The slot is EMITTED AS A DROP (a real failure,
        // counts toward N upstream), not completed, not a scheduled
        // discard.
        let mut sim = Sim::new(10_030);
        for _ in 0..50 {
            sim.deliver(BATCH, 1_000); // opens at 15.03 s
        }
        for _ in 0..40 {
            sim.deliver(BATCH, 1_000);
        }
        sim.stall_frames(28_800); // gap 1: 0.6 s
        sim.deliver_gap(BATCH, 1_000, Some(GapReport { kind: GapKind::Overrun }));
        for _ in 0..40 {
            sim.deliver(BATCH, 1_000);
        }
        sim.stall_frames(28_800); // gap 2: 0.6 s
        sim.deliver_gap(BATCH, 1_000, Some(GapReport { kind: GapKind::Overrun }));
        for _ in 0..60 {
            sim.deliver(BATCH, 1_000); // crosses the 30 s boundary
        }
        let drops: Vec<_> = sim
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    SlotEvent::Dropped { class: DropClass::LostFrames, .. }
                )
            })
            .collect();
        assert_eq!(drops.len(), 1);
        match drops[0] {
            SlotEvent::Dropped { slot_utc_ms, lost_frames, .. } => {
                assert_eq!(*slot_utc_ms, 15_000);
                assert!(*lost_frames > 48_000, "lost {lost_frames}");
            }
            _ => unreachable!(),
        }
        assert!(sim.completed().is_empty());
        // The next slot completes normally — the drop did not poison the
        // stream.
        for _ in 0..160 {
            sim.deliver(BATCH, 1_000);
        }
        assert_eq!(sim.completed().len(), 1);
        assert_eq!(sim.completed()[0].slot_utc_ms, 30_000);
    }

    #[test]
    fn suspended_abandons_and_reanchors() {
        let mut sim = Sim::new(10_030);
        for _ in 0..60 {
            sim.deliver(BATCH, 0);
        }
        sim.deliver_gap(BATCH, 0, Some(GapReport { kind: GapKind::Suspended }));
        assert_eq!(
            sim.abandoned(),
            vec![DiscardClass::FirstSlot, DiscardClass::ClockAnomaly]
        );
        // Recovery into the next boundary; the following slot completes.
        for _ in 0..320 {
            sim.deliver(BATCH, 0);
        }
        assert_eq!(sim.completed().len(), 1);
        assert_eq!(sim.completed()[0].slot_utc_ms, 30_000);
    }
```

- [ ] **Step 2: Run — expect a COMPILE failure, then add the data types, then verify the behavior failures**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked slot`
Expected first: compile error (`SlotEvent::Dropped` / `DropClass` do not exist). Add the data-only pieces to `slot.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropClass {
    /// `lost_frames` exceeded the per-slot bound (48 000 = 1 s): too much
    /// of the slot is synthetic zeros to trust a decode.
    LostFrames,
}
```

and extend `SlotEvent`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SlotEvent {
    Completed(CompletedSlot),
    Abandoned { class: DiscardClass },
    /// A REAL failure (counts toward N upstream, unlike scheduled
    /// discards): the slot is discarded with provenance so the ring can
    /// record it honestly.
    Dropped {
        class: DropClass,
        slot_utc_ms: u64,
        lost_frames: u64,
    },
}
```

Re-run. Expected: `cumulative_lost_frames_over_one_second_drops_the_slot` FAILS (the slot is still emitted as `Completed`); the anomaly/skew tests pass (they pin Task 4's control flow — verify each failure message would be meaningful by reading them once against the implementation).

Iteration loop for the 1000-slot test (pinned — ~5 min in debug on the Pi, ~25 s in release; CI runs the debug profile, so the release run is a local shortcut only):
Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --release --locked fast_clock`

- [ ] **Step 3: Implement the lost-frames drop**

In `close_slot`, replace the marker comment line
```rust
        // Task 5 inserts the lost-frames drop check HERE.
```
with:
```rust
        if cur.lost_frames > self.cfg.max_lost_frames {
            // Spec §Slot assembly: drop the slot when lost_frames > 48 000
            // (1 s). A real failure — counted toward N upstream. The
            // decimator is NOT fed: its state continuity covers emitted
            // slots only (same as abandoned slots).
            events.push(SlotEvent::Dropped {
                class: DropClass::LostFrames,
                slot_utc_ms: cur.slot_utc_ms,
                lost_frames: cur.lost_frames,
            });
            return;
        }
```

- [ ] **Step 4: Run tests green + clippy clean**

Run (iteration loop, while red-greening — the 1000-slot test is the slow one):
`cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --release --locked fast_clock`
Run (final, ONCE, full suite in the CI profile):
`cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked`
Expected: all pass (the debug run takes ~5 min — dominated by the fast-clock test; it is not hung). Run: `cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --all-targets --locked -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/tuxlink-capture/src/slot.rs
git commit -m "feat(ft8): slot anomaly rules — surplus drop, clock-anomaly abandonment, lost-frames drop (tuxlink-b026z.3 T5)"
```

---

### Task 6: Listener state machine — axes, health flags, sweep element, N/k counters

**Files:**
- Create: `src-tauri/tuxlink-capture/src/state.rs`
- Modify: `src-tauri/tuxlink-capture/src/lib.rs` (add `pub mod state;`)

**Interfaces:**
- Produces (the pure machine Phase C's supervisor/commands drive; every transition method below is a Phase C consumption point):
```rust
pub const N_DEGRADED: u8 = 5;
pub const K_BAND_DEAD: u8 = 20;

pub enum BlockedReason { DeviceAbsent, NeedsDeviceSelection, WsjtxAbsent,
                         UnsupportedSampleRate, CaptureWedged }
pub enum ServiceAxis { Stopped, Starting, Listening, Yielded,
                       Blocked(BlockedReason), Stopping }
pub struct HealthFlags { pub clock_unsynced: bool, pub cat_fixed_band: bool,
                         pub jt9_degraded: bool }
pub enum Sweep { Inactive,
                 Active { band_idx: usize, dwell_progress: u8 },
                 FallbackHold { failures: u8 } }
pub enum SlotPhase { WaitingFirstSlot, Decoded, BandDead }
pub enum RingOutcomeKind { Decoded, BandDead, Failed, DroppedBackpressure,
                           DroppedLostFrames, DroppedStorageError, Discarded }

pub struct ListenerMachine; // + impl Default
impl ListenerMachine {
    pub fn new() -> ListenerMachine;
    // axis transitions (the supervisor writes all of these except on_pause)
    pub fn on_start_requested(&mut self) -> bool; // false = refused (wedged / already live)
    pub fn on_blocked(&mut self, reason: BlockedReason);
    pub fn on_listening(&mut self);
    pub fn on_pause(&mut self);   // the modem-yield writer; stopped/blocked untouched
    pub fn on_resume(&mut self);  // yielded → starting; k reset; sweep re-arm
    pub fn on_stopping(&mut self);
    pub fn on_stopped(&mut self);
    pub fn on_capture_wedged(&mut self);
    // counters / phase / sweep bookkeeping
    pub fn on_slot_outcome(&mut self, outcome: RingOutcomeKind);
    pub fn on_band_change(&mut self);           // manual chip QSY: k reset
    pub fn sweep_activate(&mut self);
    pub fn sweep_deactivate(&mut self);
    pub fn on_qsy_success(&mut self, next_band_idx: usize);
    pub fn on_qsy_failure(&mut self);
    pub fn dwell_complete(&self, dwell_slots: u8) -> bool;
    // flags
    pub fn set_clock_unsynced(&mut self, v: bool);
    pub fn set_cat_fixed_band(&mut self, v: bool);
    // accessors
    pub fn axis(&self) -> ServiceAxis;
    pub fn flags(&self) -> HealthFlags;
    pub fn sweep(&self) -> Sweep;
    pub fn slot_phase(&self) -> SlotPhase;
    pub fn n_consecutive(&self) -> u8;
    pub fn k_consecutive(&self) -> u8;
}
```

Counter semantics implemented here are EXACTLY spec §Counter semantics: N incremented by `Failed` and every `Dropped*`, cleared by `Decoded` (including salvaged/partial — data flowed) AND by `BandDead` (a clean zero-decode exit is a good slot); k incremented by `BandDead`, reset by `Decoded`, with `Failed` and `Dropped*` k-NEUTRAL; scheduled `Discarded` counts toward NEITHER; k resets on band change (QSY) and on resume; slot phase treats `Dropped*`/`Discarded` as neutral (holds its last value; never resets to `WaitingFirstSlot` on panel reopen — phase is recency, not session state); `jt9_degraded` sets at N ≥ 5 and clears on any good slot; dwell counts decoded-or-band-dead slots ONLY (a persistent failure streak freezes the dwell — rotating a broken pipeline samples nothing).

- [ ] **Step 1: Write the module with data types, stubbed methods, and the full failing test suite**

`src-tauri/tuxlink-capture/src/state.rs` — data types + stubs first:
```rust
//! Listener state machine: service axis, health flags, sweep element, slot
//! phase, and the N/k counters (spec §State machine + §Counter semantics).
//!
//! PURE: no time, no I/O. Phase C's supervisor is the single writer of
//! every axis transition except pause (`pause_for_modem` writes `Yielded`;
//! the yield/stop request flags never write the axis themselves — spec
//! §Lifecycle ownership).

/// jt9-degraded threshold: N consecutive non-Decoded/non-BandDead outcomes
/// (types.rs contract, pinned N = 5).
pub const N_DEGRADED: u8 = 5;
/// band-dead threshold: k consecutive zero-decode slots (pinned k = 20,
/// 5 minutes).
pub const K_BAND_DEAD: u8 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedReason {
    /// A persisted identity that no longer resolves (supervisor-retried
    /// every 5 s — self-healing on USB replug).
    DeviceAbsent,
    /// No persisted device identity at all (command-gated: `set_device`).
    NeedsDeviceSelection,
    /// jt9 discovery failed (command-gated).
    WsjtxAbsent,
    /// hw param negotiation rejected (command-gated).
    UnsupportedSampleRate,
    /// A force-detached thread may still hold the PCM; this process can no
    /// longer arbitrate the card. Recovery: app restart. `set_device` and
    /// start are REFUSED from here.
    CaptureWedged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAxis {
    Stopped,
    Starting,
    Listening,
    /// yielded(device-busy) — a modem holds (or is about to hold) the card.
    Yielded,
    Blocked(BlockedReason),
    Stopping,
}

/// Orthogonal health flags — they coexist with `Listening`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HealthFlags {
    pub clock_unsynced: bool,
    pub cat_fixed_band: bool,
    pub jt9_degraded: bool,
}

/// The sweep element is a NAMED part of the machine, not a flag (spec).
/// Runtime state only — `config.sweep.enabled` is never mutated by the
/// machine; `FallbackHold` re-arms to `Active` at the next start or resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sweep {
    Inactive,
    Active { band_idx: usize, dwell_progress: u8 },
    FallbackHold { failures: u8 },
}

/// Slot phase within `listening` (computed from recency; never resets on
/// panel reopen — delta pin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotPhase {
    WaitingFirstSlot,
    Decoded,
    BandDead,
}

/// The per-slot-boundary outcome kind the decode/capture side folds in.
/// Every ring record maps to exactly one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingOutcomeKind {
    /// Includes salvaged/partial decodes — data flowed.
    Decoded,
    BandDead,
    /// Any `SlotFailure` from the decode engine.
    Failed,
    DroppedBackpressure,
    DroppedLostFrames,
    DroppedStorageError,
    /// Scheduled discards: first-slot, QSY transition, clock anomaly.
    Discarded,
}

pub struct ListenerMachine {
    axis: ServiceAxis,
    flags: HealthFlags,
    sweep: Sweep,
    slot_phase: SlotPhase,
    n_consecutive: u8,
    k_consecutive: u8,
    qsy_failures: u8,
}

impl Default for ListenerMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl ListenerMachine {
    pub fn new() -> Self {
        Self {
            axis: ServiceAxis::Stopped,
            flags: HealthFlags::default(),
            sweep: Sweep::Inactive,
            slot_phase: SlotPhase::WaitingFirstSlot,
            n_consecutive: 0,
            k_consecutive: 0,
            qsy_failures: 0,
        }
    }

    // ---- accessors (real from the start; the tests need them) ----
    pub fn axis(&self) -> ServiceAxis { self.axis }
    pub fn flags(&self) -> HealthFlags { self.flags }
    pub fn sweep(&self) -> Sweep { self.sweep }
    pub fn slot_phase(&self) -> SlotPhase { self.slot_phase }
    pub fn n_consecutive(&self) -> u8 { self.n_consecutive }
    pub fn k_consecutive(&self) -> u8 { self.k_consecutive }

    // ---- transitions: STUBS, replaced in Step 3 ----
    pub fn on_start_requested(&mut self) -> bool { unimplemented!("T6 stub") }
    pub fn on_blocked(&mut self, _reason: BlockedReason) { unimplemented!("T6 stub") }
    pub fn on_listening(&mut self) { unimplemented!("T6 stub") }
    pub fn on_pause(&mut self) { unimplemented!("T6 stub") }
    pub fn on_resume(&mut self) { unimplemented!("T6 stub") }
    pub fn on_stopping(&mut self) { unimplemented!("T6 stub") }
    pub fn on_stopped(&mut self) { unimplemented!("T6 stub") }
    pub fn on_capture_wedged(&mut self) { unimplemented!("T6 stub") }
    pub fn on_slot_outcome(&mut self, _outcome: RingOutcomeKind) { unimplemented!("T6 stub") }
    pub fn on_band_change(&mut self) { unimplemented!("T6 stub") }
    pub fn sweep_activate(&mut self) { unimplemented!("T6 stub") }
    pub fn sweep_deactivate(&mut self) { unimplemented!("T6 stub") }
    pub fn on_qsy_success(&mut self, _next_band_idx: usize) { unimplemented!("T6 stub") }
    pub fn on_qsy_failure(&mut self) { unimplemented!("T6 stub") }
    pub fn dwell_complete(&self, _dwell_slots: u8) -> bool { unimplemented!("T6 stub") }
    pub fn set_clock_unsynced(&mut self, _v: bool) { unimplemented!("T6 stub") }
    pub fn set_cat_fixed_band(&mut self, _v: bool) { unimplemented!("T6 stub") }
}
```

Then the test module — every counter rule and axis transition from the spec gets a named test (this enumeration IS the spec's §Testing "state machine" bullet):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use BlockedReason::*;
    use RingOutcomeKind::*;

    /// A machine driven to `Listening` the way the supervisor does.
    fn listening() -> ListenerMachine {
        let mut m = ListenerMachine::new();
        assert!(m.on_start_requested());
        m.on_listening();
        m
    }

    // ================= counter rules (spec §Counter semantics) ==========

    #[test]
    fn n_increments_on_failed() {
        let mut m = listening();
        m.on_slot_outcome(Failed);
        assert_eq!(m.n_consecutive(), 1);
    }

    #[test]
    fn n_increments_on_every_dropped_kind() {
        let mut m = listening();
        m.on_slot_outcome(DroppedBackpressure);
        m.on_slot_outcome(DroppedLostFrames);
        m.on_slot_outcome(DroppedStorageError);
        assert_eq!(m.n_consecutive(), 3);
    }

    #[test]
    fn n_clears_on_decoded_including_salvaged() {
        let mut m = listening();
        for _ in 0..3 {
            m.on_slot_outcome(Failed);
        }
        m.on_slot_outcome(Decoded); // salvaged/partial folds as Decoded too
        assert_eq!(m.n_consecutive(), 0);
    }

    #[test]
    fn n_clears_on_band_dead_a_clean_zero_decode_exit_is_a_good_slot() {
        let mut m = listening();
        for _ in 0..4 {
            m.on_slot_outcome(Failed);
        }
        m.on_slot_outcome(BandDead);
        assert_eq!(m.n_consecutive(), 0);
    }

    #[test]
    fn jt9_degraded_sets_at_n5_and_clears_on_good_slot() {
        let mut m = listening();
        for i in 0..N_DEGRADED {
            assert!(!m.flags().jt9_degraded, "before slot {i}");
            m.on_slot_outcome(Failed);
        }
        assert!(m.flags().jt9_degraded, "N=5 must set the flag");
        m.on_slot_outcome(BandDead);
        assert!(!m.flags().jt9_degraded, "the first good slot clears");
        assert_eq!(m.n_consecutive(), 0);
    }

    #[test]
    fn k_increments_on_band_dead_and_resets_on_decoded() {
        let mut m = listening();
        m.on_slot_outcome(BandDead);
        m.on_slot_outcome(BandDead);
        assert_eq!(m.k_consecutive(), 2);
        m.on_slot_outcome(Decoded);
        assert_eq!(m.k_consecutive(), 0);
    }

    #[test]
    fn failed_and_dropped_are_k_neutral() {
        // Neither failure nor a dropped slot is evidence about band
        // quietness: k neither increments nor resets.
        let mut m = listening();
        for _ in 0..3 {
            m.on_slot_outcome(BandDead);
        }
        m.on_slot_outcome(Failed);
        m.on_slot_outcome(DroppedBackpressure);
        m.on_slot_outcome(DroppedLostFrames);
        m.on_slot_outcome(DroppedStorageError);
        assert_eq!(m.k_consecutive(), 3, "k must hold, not reset or grow");
    }

    #[test]
    fn scheduled_discards_count_toward_neither_counter() {
        let mut m = listening();
        m.on_slot_outcome(Failed);
        m.on_slot_outcome(BandDead);
        let (n, k) = (m.n_consecutive(), m.k_consecutive());
        for _ in 0..10 {
            m.on_slot_outcome(Discarded); // first-slot / QSY / clock-anomaly
        }
        assert_eq!((m.n_consecutive(), m.k_consecutive()), (n, k));
    }

    #[test]
    fn k_resets_on_band_change() {
        let mut m = listening();
        for _ in 0..7 {
            m.on_slot_outcome(BandDead);
        }
        m.on_band_change(); // manual chip QSY
        assert_eq!(m.k_consecutive(), 0);
    }

    #[test]
    fn k_resets_on_resume() {
        let mut m = listening();
        for _ in 0..7 {
            m.on_slot_outcome(BandDead);
        }
        m.on_pause();
        m.on_resume();
        assert_eq!(m.k_consecutive(), 0);
    }

    // ================= slot phase (recency; Dropped/Discarded neutral) ==

    #[test]
    fn phase_starts_waiting_and_moves_to_decoded() {
        let mut m = listening();
        assert_eq!(m.slot_phase(), SlotPhase::WaitingFirstSlot);
        m.on_slot_outcome(Decoded);
        assert_eq!(m.slot_phase(), SlotPhase::Decoded);
    }

    #[test]
    fn phase_moves_to_band_dead_only_at_k20() {
        let mut m = listening();
        for i in 0..(K_BAND_DEAD - 1) {
            m.on_slot_outcome(BandDead);
            assert_eq!(
                m.slot_phase(),
                SlotPhase::WaitingFirstSlot,
                "slot {i}: below k=20 the phase must not claim band-dead"
            );
        }
        m.on_slot_outcome(BandDead);
        assert_eq!(m.slot_phase(), SlotPhase::BandDead);
    }

    #[test]
    fn phase_holds_on_failed_dropped_and_discarded() {
        let mut m = listening();
        m.on_slot_outcome(Decoded);
        for o in [Failed, DroppedBackpressure, DroppedLostFrames,
                  DroppedStorageError, Discarded] {
            m.on_slot_outcome(o);
            assert_eq!(m.slot_phase(), SlotPhase::Decoded, "{o:?} must be phase-neutral");
        }
    }

    // ================= service axis =====================================

    #[test]
    fn start_from_stopped_enters_starting() {
        let mut m = ListenerMachine::new();
        assert_eq!(m.axis(), ServiceAxis::Stopped);
        assert!(m.on_start_requested());
        assert_eq!(m.axis(), ServiceAxis::Starting);
    }

    #[test]
    fn start_is_refused_from_capture_wedged() {
        // A detached thread may hold the PCM; starting a second capture
        // path in a process that can no longer arbitrate the card is worse
        // than refusing (spec §Device selection). Recovery: app restart.
        let mut m = listening();
        m.on_capture_wedged();
        assert!(!m.on_start_requested());
        assert_eq!(m.axis(), ServiceAxis::Blocked(CaptureWedged));
    }

    #[test]
    fn start_from_blocked_non_wedged_reenters_starting() {
        // set_device / config change / start-retry recover every
        // command-gated blocked state.
        for r in [DeviceAbsent, NeedsDeviceSelection, WsjtxAbsent, UnsupportedSampleRate] {
            let mut m = ListenerMachine::new();
            assert!(m.on_start_requested());
            m.on_blocked(r);
            assert!(m.on_start_requested(), "{r:?}");
            assert_eq!(m.axis(), ServiceAxis::Starting, "{r:?}");
        }
    }

    #[test]
    fn start_is_a_refused_noop_when_already_live() {
        // Idempotent start (spec §Lifecycle ownership): with a live
        // supervisor the handler signals a sequence re-run instead — the
        // machine refuses the transition and holds its axis.
        for (mk, axis) in [
            (ServiceAxis::Starting, ServiceAxis::Starting),
            (ServiceAxis::Listening, ServiceAxis::Listening),
            (ServiceAxis::Yielded, ServiceAxis::Yielded),
        ] {
            let mut m = ListenerMachine::new();
            assert!(m.on_start_requested());
            if mk != ServiceAxis::Starting {
                m.on_listening();
            }
            if mk == ServiceAxis::Yielded {
                m.on_pause();
            }
            assert!(!m.on_start_requested(), "{mk:?}");
            assert_eq!(m.axis(), axis, "{mk:?}");
        }
    }

    #[test]
    fn every_blocked_reason_is_reachable_from_starting() {
        for r in [DeviceAbsent, NeedsDeviceSelection, WsjtxAbsent,
                  UnsupportedSampleRate, CaptureWedged] {
            let mut m = ListenerMachine::new();
            assert!(m.on_start_requested());
            m.on_blocked(r);
            assert_eq!(m.axis(), ServiceAxis::Blocked(r));
        }
    }

    #[test]
    fn listening_from_starting() {
        let m = listening();
        assert_eq!(m.axis(), ServiceAxis::Listening);
    }

    #[test]
    fn pause_from_listening_yields() {
        let mut m = listening();
        m.on_pause();
        assert_eq!(m.axis(), ServiceAxis::Yielded);
    }

    #[test]
    fn pause_from_starting_yields() {
        // Pause during `starting` converts the sequence to yielded; the
        // supervisor's between-step flag check abandons the sequence
        // without re-writing the axis (spec §Arbitration).
        let mut m = ListenerMachine::new();
        assert!(m.on_start_requested());
        m.on_pause();
        assert_eq!(m.axis(), ServiceAxis::Yielded);
    }

    #[test]
    fn pause_from_stopped_is_a_stateless_noop() {
        // Pause fires on EVERY modem spawn, including systems that never
        // enabled FT8 — those must acquire no phantom listener state.
        let mut m = ListenerMachine::new();
        m.on_pause();
        assert_eq!(m.axis(), ServiceAxis::Stopped);
    }

    #[test]
    fn pause_from_blocked_leaves_the_axis_untouched() {
        // The arbiter latches the hold; the blocked axis and reason stay
        // (spec §Arbitration, blocked arm).
        let mut m = ListenerMachine::new();
        assert!(m.on_start_requested());
        m.on_blocked(WsjtxAbsent);
        m.on_pause();
        assert_eq!(m.axis(), ServiceAxis::Blocked(WsjtxAbsent));
    }

    #[test]
    fn resume_from_yielded_reenters_starting() {
        // Resume re-runs start steps 1–7 (spec §Lifecycle ownership) — the
        // machine re-enters Starting; the supervisor walks it forward.
        let mut m = listening();
        m.on_pause();
        m.on_resume();
        assert_eq!(m.axis(), ServiceAxis::Starting);
    }

    #[test]
    fn resume_is_a_noop_outside_yielded() {
        let mut m = listening();
        m.on_resume();
        assert_eq!(m.axis(), ServiceAxis::Listening);
    }

    #[test]
    fn capture_wedged_is_reachable_from_any_live_state() {
        // Stop-path join-bound overrun and pause-path join timeout both
        // force-detach into capture-wedged, whatever the axis was.
        let mut a = listening();
        a.on_capture_wedged();
        assert_eq!(a.axis(), ServiceAxis::Blocked(CaptureWedged));
        let mut b = listening();
        b.on_pause();
        b.on_capture_wedged();
        assert_eq!(b.axis(), ServiceAxis::Blocked(CaptureWedged));
        let mut c = listening();
        c.on_stopping();
        c.on_capture_wedged();
        assert_eq!(c.axis(), ServiceAxis::Blocked(CaptureWedged));
    }

    #[test]
    fn stop_sequence_stopping_then_stopped_resets_counters() {
        let mut m = listening();
        for _ in 0..3 {
            m.on_slot_outcome(Failed);
        }
        m.on_stopping();
        assert_eq!(m.axis(), ServiceAxis::Stopping);
        m.on_stopped();
        assert_eq!(m.axis(), ServiceAxis::Stopped);
        assert_eq!(m.n_consecutive(), 0);
        assert_eq!(m.k_consecutive(), 0);
        assert!(!m.flags().jt9_degraded);
    }

    // ================= health flags =====================================

    #[test]
    fn clock_and_cat_flags_are_orthogonal_setters() {
        let mut m = listening();
        m.set_clock_unsynced(true);
        m.set_cat_fixed_band(true);
        assert!(m.flags().clock_unsynced);
        assert!(m.flags().cat_fixed_band);
        assert_eq!(m.axis(), ServiceAxis::Listening, "flags coexist with listening");
        m.set_clock_unsynced(false);
        assert!(!m.flags().clock_unsynced);
        assert!(m.flags().cat_fixed_band);
    }

    // ================= sweep element ====================================

    #[test]
    fn sweep_activates_at_band_zero_and_dwell_counts_good_slots_only() {
        let mut m = listening();
        m.sweep_activate();
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 0 });
        m.on_slot_outcome(Decoded);
        m.on_slot_outcome(BandDead);
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 2 });
        assert!(!m.dwell_complete(8));
        for _ in 0..6 {
            m.on_slot_outcome(BandDead);
        }
        assert!(m.dwell_complete(8), "8 decoded-or-band-dead slots = dwell done");
    }

    #[test]
    fn dwell_freezes_under_a_failure_streak() {
        // Rotating a broken decode pipeline samples nothing — intended
        // freeze; jt9-degraded is the operator's signal (spec §Sweep).
        let mut m = listening();
        m.sweep_activate();
        m.on_slot_outcome(Decoded);
        for _ in 0..10 {
            m.on_slot_outcome(Failed);
            m.on_slot_outcome(DroppedBackpressure);
            m.on_slot_outcome(Discarded);
        }
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 1 });
    }

    #[test]
    fn qsy_success_advances_band_resets_dwell_and_k() {
        let mut m = listening();
        m.sweep_activate();
        for _ in 0..8 {
            m.on_slot_outcome(BandDead);
        }
        assert_eq!(m.k_consecutive(), 8);
        m.on_qsy_success(1);
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 1, dwell_progress: 0 });
        assert_eq!(m.k_consecutive(), 0, "k resets on band change");
    }

    #[test]
    fn two_consecutive_qsy_failures_enter_fallback_hold() {
        let mut m = listening();
        m.sweep_activate();
        m.on_qsy_failure();
        assert!(matches!(m.sweep(), Sweep::Active { .. }), "one failure retries");
        m.on_qsy_failure();
        assert_eq!(m.sweep(), Sweep::FallbackHold { failures: 2 });
    }

    #[test]
    fn a_qsy_success_between_failures_clears_the_streak() {
        let mut m = listening();
        m.sweep_activate();
        m.on_qsy_failure();
        m.on_qsy_success(1);
        m.on_qsy_failure();
        assert!(
            matches!(m.sweep(), Sweep::Active { .. }),
            "non-consecutive failures must not enter FallbackHold"
        );
    }

    #[test]
    fn fallback_hold_rearms_on_resume() {
        let mut m = listening();
        m.sweep_activate();
        m.on_qsy_failure();
        m.on_qsy_failure();
        assert_eq!(m.sweep(), Sweep::FallbackHold { failures: 2 });
        m.on_pause();
        m.on_resume();
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 0 });
    }

    #[test]
    fn fallback_hold_rearms_on_start() {
        let mut m = listening();
        m.sweep_activate();
        m.on_qsy_failure();
        m.on_qsy_failure();
        m.on_stopping();
        m.on_stopped();
        assert!(m.on_start_requested());
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 0 });
    }

    #[test]
    fn dwell_reanchors_on_resume() {
        let mut m = listening();
        m.sweep_activate();
        for _ in 0..5 {
            m.on_slot_outcome(BandDead);
        }
        assert_eq!(m.sweep(), Sweep::Active { band_idx: 0, dwell_progress: 5 });
        m.on_pause();
        m.on_resume();
        assert_eq!(
            m.sweep(),
            Sweep::Active { band_idx: 0, dwell_progress: 0 },
            "dwell re-anchors on resume; band position is kept"
        );
    }

    #[test]
    fn sweep_deactivate_returns_to_inactive() {
        let mut m = listening();
        m.sweep_activate();
        m.on_qsy_failure();
        m.sweep_deactivate();
        assert_eq!(m.sweep(), Sweep::Inactive);
        m.on_qsy_failure();
        assert_eq!(m.sweep(), Sweep::Inactive, "failures while inactive are inert");
    }
}
```

Add `pub mod state;` to `src-tauri/tuxlink-capture/src/lib.rs`.

- [ ] **Step 2: Run tests, verify they FAIL on the stub panics**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked state`
Expected: every state test FAILS with an `unimplemented!` panic (all of them drive at least one transition method).

- [ ] **Step 3: Implement the machine**

Replace the stubbed methods inside `impl ListenerMachine` (keep `new` and the accessors):
```rust
    // ---- axis transitions ----

    /// `stopped` / non-wedged `blocked` → `starting` (true). Refused
    /// (false) from `capture-wedged` (restart-required) and from any live
    /// axis (idempotent start: the live supervisor re-runs its sequence).
    pub fn on_start_requested(&mut self) -> bool {
        match self.axis {
            ServiceAxis::Blocked(BlockedReason::CaptureWedged) => false,
            ServiceAxis::Stopped | ServiceAxis::Blocked(_) => {
                self.axis = ServiceAxis::Starting;
                self.rearm_sweep();
                true
            }
            _ => false,
        }
    }

    pub fn on_blocked(&mut self, reason: BlockedReason) {
        self.axis = ServiceAxis::Blocked(reason);
    }

    pub fn on_listening(&mut self) {
        self.axis = ServiceAxis::Listening;
    }

    /// Pause is the ONE transition not written by the supervisor (spec
    /// §Lifecycle ownership). From `stopped`: stateless no-op — no phantom
    /// listener state. From `blocked`: the hold latch is the arbiter's
    /// job; the axis and reason stay untouched.
    pub fn on_pause(&mut self) {
        if matches!(self.axis, ServiceAxis::Listening | ServiceAxis::Starting) {
            self.axis = ServiceAxis::Yielded;
        }
    }

    /// `yielded` → `starting` (the supervisor re-runs steps 1–7). k resets
    /// (spec §Counter semantics); FallbackHold re-arms and the dwell
    /// re-anchors (spec §Sweep).
    pub fn on_resume(&mut self) {
        if self.axis == ServiceAxis::Yielded {
            self.axis = ServiceAxis::Starting;
            self.k_consecutive = 0;
            self.rearm_sweep();
        }
    }

    pub fn on_stopping(&mut self) {
        self.axis = ServiceAxis::Stopping;
    }

    pub fn on_stopped(&mut self) {
        self.axis = ServiceAxis::Stopped;
        self.n_consecutive = 0;
        self.k_consecutive = 0;
        self.flags.jt9_degraded = false;
        // slot_phase intentionally KEPT: phase is ring recency, not
        // session state (delta pin: never resets on reopen).
    }

    pub fn on_capture_wedged(&mut self) {
        self.axis = ServiceAxis::Blocked(BlockedReason::CaptureWedged);
    }

    // ---- counters / phase / dwell ----

    pub fn on_slot_outcome(&mut self, outcome: RingOutcomeKind) {
        match outcome {
            RingOutcomeKind::Decoded => {
                self.n_consecutive = 0;
                self.k_consecutive = 0;
                self.flags.jt9_degraded = false;
                self.slot_phase = SlotPhase::Decoded;
                self.bump_dwell();
            }
            RingOutcomeKind::BandDead => {
                self.n_consecutive = 0;
                self.flags.jt9_degraded = false;
                self.k_consecutive = self.k_consecutive.saturating_add(1);
                if self.k_consecutive >= K_BAND_DEAD {
                    self.slot_phase = SlotPhase::BandDead;
                }
                self.bump_dwell();
            }
            RingOutcomeKind::Failed
            | RingOutcomeKind::DroppedBackpressure
            | RingOutcomeKind::DroppedLostFrames
            | RingOutcomeKind::DroppedStorageError => {
                self.n_consecutive = self.n_consecutive.saturating_add(1);
                if self.n_consecutive >= N_DEGRADED {
                    self.flags.jt9_degraded = true;
                }
                // k-neutral; phase holds; dwell frozen (a failing pipeline
                // samples nothing — rotating it is pointless).
            }
            RingOutcomeKind::Discarded => {
                // Scheduled discards: neither counter, phase holds, dwell
                // unchanged (spec §Counter semantics).
            }
        }
    }

    pub fn on_band_change(&mut self) {
        self.k_consecutive = 0;
    }

    // ---- sweep element ----

    pub fn sweep_activate(&mut self) {
        self.sweep = Sweep::Active { band_idx: 0, dwell_progress: 0 };
        self.qsy_failures = 0;
    }

    pub fn sweep_deactivate(&mut self) {
        self.sweep = Sweep::Inactive;
        self.qsy_failures = 0;
    }

    pub fn on_qsy_success(&mut self, next_band_idx: usize) {
        if matches!(self.sweep, Sweep::Inactive) {
            return;
        }
        self.sweep = Sweep::Active { band_idx: next_band_idx, dwell_progress: 0 };
        self.qsy_failures = 0;
        self.k_consecutive = 0; // k resets on band change
    }

    /// Two CONSECUTIVE failures → FallbackHold (config untouched; re-arms
    /// at the next start/resume — spec §Sweep).
    pub fn on_qsy_failure(&mut self) {
        if matches!(self.sweep, Sweep::Inactive) {
            return;
        }
        self.qsy_failures = self.qsy_failures.saturating_add(1);
        if self.qsy_failures >= 2 {
            self.sweep = Sweep::FallbackHold { failures: self.qsy_failures };
        }
    }

    pub fn dwell_complete(&self, dwell_slots: u8) -> bool {
        matches!(self.sweep, Sweep::Active { dwell_progress, .. }
                 if dwell_progress >= dwell_slots)
    }

    fn bump_dwell(&mut self) {
        if let Sweep::Active { dwell_progress, .. } = &mut self.sweep {
            *dwell_progress = dwell_progress.saturating_add(1);
        }
    }

    /// Start/resume re-arm: FallbackHold → Active (rotation restarts at
    /// band 0); an Active dwell re-anchors; Inactive stays inactive.
    fn rearm_sweep(&mut self) {
        match self.sweep {
            Sweep::FallbackHold { .. } => {
                self.sweep = Sweep::Active { band_idx: 0, dwell_progress: 0 };
                self.qsy_failures = 0;
            }
            Sweep::Active { band_idx, .. } => {
                self.sweep = Sweep::Active { band_idx, dwell_progress: 0 };
            }
            Sweep::Inactive => {}
        }
    }

    // ---- flags ----

    pub fn set_clock_unsynced(&mut self, v: bool) {
        self.flags.clock_unsynced = v;
    }

    pub fn set_cat_fixed_band(&mut self, v: bool) {
        self.flags.cat_fixed_band = v;
    }
```

- [ ] **Step 4: Run tests green + clippy clean**

Run (iteration loop while red-greening the state tests — skips the ~5 min debug fast-clock test):
`cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked state`
Run (final, ONCE, full suite): `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked`
Expected: the full crate suite passes (bands + wavwrite + decimator + slot + state; the slot suite's fast-clock test dominates the wall time — `cargo test -p tuxlink-capture --release --locked fast_clock` remains the legitimate release shortcut for THAT test only).
Run: `cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --all-targets --locked -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/tuxlink-capture/src/state.rs src-tauri/tuxlink-capture/src/lib.rs
git commit -m "feat(ft8): listener state machine — axes, health flags, sweep element, N/k counters (tuxlink-b026z.3 T6)"
```

---

**REVIEW GATE B (after Tasks 4–6):** review the assembler + machine batch. Perspectives: (1) spec-rule tracing — walk every rule in spec §Slot assembly and §Counter semantics to its named test (a rule without a test is a finding); (2) clock-domain honesty — no ambient time reads, UTC only labels, monotonic only measures, the anchor convention consistent between open sites; (3) event taxonomy — Completed vs Abandoned (counter-neutral) vs Dropped (counts toward N) map 1:1 onto the spec's outcome classes and Phase C's `RingOutcomeKind`; (4) saturating arithmetic + no panics on adversarial inputs (empty batches, huge mono jumps, u64 edges). Minimum three rounds; persist findings to `dev/scratch/b026z.3-gate-B-findings.md` before proceeding. Files under review: `src-tauri/tuxlink-capture/src/slot.rs`, `src-tauri/tuxlink-capture/src/state.rs`, `src-tauri/tuxlink-capture/src/lib.rs`.

---

## Phase B — `tuxlink-jt9` salvage-on-signal (Task 7, resolves tuxlink-gujnz)

### Task 7: Salvage-on-signal parity + contract doc edits

**Files:**
- Modify: `src-tauri/tuxlink-jt9/src/runner.rs` (the non-timeout classification arms)
- Modify: `src-tauri/tuxlink-jt9/tests/fake_jt9.rs` (one test inverted, four added)
- Modify: `src-tauri/tuxlink-jt9/src/types.rs` (three doc edits — the cross-crate contract surface)

**Interfaces:**
- No signature changes. Behavior change to `Jt9Runner::decode_slot` (consumed by Phase C's `DecodeEngine`): a signal-death or nonzero clean exit with ≥ 1 parsed decode line now returns `SlotOutcome::Decoded` with `partial = !saw_sentinel` — identical to the timeout arm; zero parsed lines keeps `Failed(Signal)`. **Arm ordering pinned on ALL paths: the `StderrEof` check runs BEFORE salvage** — a capture bug must never masquerade as decodes.

Rationale (spec §gujnz, recorded for the delta v3 note): jt9's dominant real failure mode IS decode-stream-then-SIGSEGV (a kill at t=1 s had delivered 10/14 lines); lines print only after jt9's internal CRC-14 accepts a candidate; the strict parser guards corruption; the timeout path already trusts the identical stream; discarding biases band intelligence against exactly the slots proving the band alive. Downstream does NOT distinguish timeout-salvage from crash-salvage.

- [ ] **Step 1: Invert the taxonomy test and add the four new tests (failing first)**

In `src-tauri/tuxlink-jt9/tests/fake_jt9.rs`, the CURRENT test (quoted verbatim — this is what gets replaced):
```rust
#[test]
fn signal_death_discards_prior_decodes_by_taxonomy() {
    // Signal beats decodes in the failure taxonomy (delta §failure
    // taxonomy): even when jt9 emitted a decode line before dying, a
    // signal death still reports Failed(Signal) — the decodes are
    // discarded, not salvaged. (Salvage-on-decodes is a timeout-path-only
    // behavior; a signal death is never partial-salvaged.)
    let (runner, wav, tmp) = setup("segv-with-decodes", &format!(
        "#!/bin/sh\necho '{DECODE_LINE}'\necho 'dying now' 1>&2\nkill -SEGV $$\n"));
    match runner.decode_slot(&wav, &tmp, 0) {
        SlotOutcome::Failed(SlotFailure::Signal { .. }) => {}
        other => panic!("want Signal (decodes discarded), got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}
```

Replace it in full with (name flipped, doc prose flipped):
```rust
#[test]
fn signal_death_salvages_parsed_decodes() {
    // Salvage-on-signal (tuxlink-gujnz; L2 spec §gujnz): jt9's dominant
    // real failure mode IS decode-stream-then-SIGSEGV, and decode lines
    // print only after jt9's internal CRC-14 accepts a candidate — parsed
    // lines from a signal death are trustworthy. ≥ 1 parsed line →
    // Decoded with partial = !saw_sentinel, identical to the timeout arm;
    // zero lines keeps Failed(Signal).
    let (runner, wav, tmp) = setup("segv-with-decodes", &format!(
        "#!/bin/sh\necho '{DECODE_LINE}'\necho 'dying now' 1>&2\nkill -SEGV $$\n"));
    match runner.decode_slot(&wav, &tmp, 0) {
        SlotOutcome::Decoded(recs) => {
            assert_eq!(recs.len(), 1);
            assert!(recs[0].partial, "no sentinel before the signal => partial");
        }
        other => panic!("want salvaged Decoded, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}
```

Then ADD these four tests at the end of the file:
```rust
#[test]
fn zero_line_signal_death_is_still_failed_signal() {
    // The salvage arm requires ≥ 1 PARSED decode line; a bare signal death
    // stays Failed(Signal) (spec §gujnz: "zero parsed lines keeps
    // Failed(Signal)").
    let (runner, wav, tmp) = setup("segv-zero-lines", "#!/bin/sh\nkill -SEGV $$\n");
    assert!(matches!(
        runner.decode_slot(&wav, &tmp, 0),
        SlotOutcome::Failed(SlotFailure::Signal { .. })
    ));
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn signal_death_after_sentinel_salvages_complete_records() {
    // A crash AFTER <DecodeFinished> yields complete records:
    // partial = !saw_sentinel = false (spec §gujnz — identical semantics
    // to the timeout-after-sentinel arm).
    let (runner, wav, tmp) = setup("segv-post-sentinel", &format!(
        "#!/bin/sh\necho '{DECODE_LINE}'\necho '{SENTINEL}'\nkill -SEGV $$\n"));
    match runner.decode_slot(&wav, &tmp, 0) {
        SlotOutcome::Decoded(recs) => {
            assert_eq!(recs.len(), 1);
            assert!(!recs[0].partial, "sentinel seen => complete records");
        }
        other => panic!("want salvaged Decoded, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn stderr_eof_beats_salvage_on_the_signal_path() {
    // Arm ordering pinned on ALL paths (spec §gujnz): StderrEof BEFORE
    // salvage — signal death + "EOF on input file" + parsed lines is
    // still Failed(StderrEof). A capture bug must never masquerade as
    // decodes (theoretical on the signal path, pinned not assumed).
    let (runner, wav, tmp) = setup("segv-eof-with-decodes", &format!(
        "#!/bin/sh\necho '{DECODE_LINE}'\necho 'EOF on input file' 1>&2\nkill -SEGV $$\n"));
    assert_eq!(
        runner.decode_slot(&wav, &tmp, 0),
        SlotOutcome::Failed(SlotFailure::StderrEof)
    );
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}

#[test]
fn nonzero_exit_with_decodes_salvages_too() {
    // The salvage arm covers nonzero clean exits as well as signals
    // (spec §gujnz: "signal-death (or nonzero clean exit)"). With the
    // sentinel present the records are complete.
    let (runner, wav, tmp) = setup("exit3-with-decodes", &format!(
        "#!/bin/sh\necho '{DECODE_LINE}'\necho '{SENTINEL}'\nexit 3\n"));
    match runner.decode_slot(&wav, &tmp, 0) {
        SlotOutcome::Decoded(recs) => {
            assert_eq!(recs.len(), 1);
            assert!(!recs[0].partial, "sentinel seen => complete records");
        }
        other => panic!("want salvaged Decoded, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(wav.parent().unwrap());
}
```

(Existing tests that stay valid and must remain untouched: `signal_death_is_classified_with_stderr_tail` and `non_utf8_stderr_is_captured_lossily` emit ZERO decode lines — they still classify `Failed(Signal)`; `nonzero_exit_without_signal_is_exit_code_signal` emits only the sentinel — zero decode lines, still `Failed(Signal { signal: "exit 3" })`; `stderr_eof_wins_over_decodes_on_clean_exit` already pins EOF-beats-salvage on the exit-0 path.)

- [ ] **Step 2: Run, verify the new/inverted tests FAIL against today's runner**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-jt9 --locked --test fake_jt9`
Expected: `signal_death_salvages_parsed_decodes`, `signal_death_after_sentinel_salvages_complete_records`, `nonzero_exit_with_decodes_salvages_too` FAIL (today's runner returns `Failed(Signal)` unconditionally); `zero_line_signal_death_is_still_failed_signal` and `stderr_eof_beats_salvage_on_the_signal_path` pass against today's code (they pin what must NOT change); everything else stays green.

- [ ] **Step 3: Implement the salvage arms in the runner**

In `src-tauri/tuxlink-jt9/src/runner.rs`, the CURRENT non-timeout classification (quoted verbatim from `runner.rs:196-214` — this exact block is replaced):
```rust
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(sig) = status.signal() {
                return SlotOutcome::Failed(SlotFailure::Signal {
                    signal: format!("signal {sig}"),
                    stderr_tail: tail(&stderr_text, 300),
                });
            }
        }
        if stderr_text.contains("EOF on input file") {
            return SlotOutcome::Failed(SlotFailure::StderrEof);
        }
        if !status.success() {
            return SlotOutcome::Failed(SlotFailure::Signal {
                signal: format!("exit {}", status.code().unwrap_or(-1)),
                stderr_tail: tail(&stderr_text, 300),
            });
        }
```

Replacement:
```rust
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(sig) = status.signal() {
                // Arm ordering pinned by the L2 spec (§gujnz): StderrEof
                // BEFORE salvage on ALL abnormal-termination arms — a
                // capture bug must never masquerade as decodes.
                if stderr_text.contains("EOF on input file") {
                    return SlotOutcome::Failed(SlotFailure::StderrEof);
                }
                if !decodes.is_empty() {
                    // Salvage-on-signal (tuxlink-gujnz): jt9's dominant
                    // real failure mode is decode-stream-then-SIGSEGV, and
                    // lines print only after jt9's internal CRC-14 accepts
                    // a candidate. partial mirrors the timeout arm: true
                    // iff the completeness sentinel was never seen.
                    for d in &mut decodes {
                        d.partial = !saw_sentinel;
                    }
                    return SlotOutcome::Decoded(decodes);
                }
                return SlotOutcome::Failed(SlotFailure::Signal {
                    signal: format!("signal {sig}"),
                    stderr_tail: tail(&stderr_text, 300),
                });
            }
        }
        if stderr_text.contains("EOF on input file") {
            return SlotOutcome::Failed(SlotFailure::StderrEof);
        }
        if !status.success() {
            if !decodes.is_empty() {
                // Nonzero-exit salvage: same rationale as the signal arm
                // (jt9 has no documented nonzero exits; a crash after
                // decodes is evidence the band is alive, not that the
                // parsed data is bad). The StderrEof check above already
                // ran — ordering pinned.
                for d in &mut decodes {
                    d.partial = !saw_sentinel;
                }
                return SlotOutcome::Decoded(decodes);
            }
            return SlotOutcome::Failed(SlotFailure::Signal {
                signal: format!("exit {}", status.code().unwrap_or(-1)),
                stderr_tail: tail(&stderr_text, 300),
            });
        }
```

(Timeout-vs-signal tiebreak: verified safe by the spec — the timeout path returns at `runner.rs:181` before signal classification at `:194` is reachable, and post-salvage both arms return `Decoded` for ≥ 1 line while zero-line outcomes stay distinct, `Timeout` vs `Signal`.)

- [ ] **Step 4: Apply the three types.rs contract doc edits**

In `src-tauri/tuxlink-jt9/src/types.rs`:

**(1) Counter-scoping sentence.** The CURRENT doc paragraph on `SlotFailure` (quoted verbatim, `types.rs:33-39`):
```rust
/// Degraded-flag thresholds (consumed by the L2 plan's slot scheduler; the
/// delta requires them pinned here): jt9-degraded after N = 5 consecutive
/// non-Decoded/non-BandDead outcomes, clearing on the first good slot;
/// band-dead after k = 20 consecutive zero-decode slots (5 minutes). The N=5
/// degraded counter also folds L2 backpressure drops — a slot L2 drops
/// without ever calling `decode_slot` still counts as a non-Decoded outcome
/// toward N.
```
Replace those lines with (the spec's exact amendment — this is the CANONICAL cross-crate statement; the delta v3 note points here):
```rust
/// Degraded-flag thresholds (consumed by the L2 plan's slot scheduler; the
/// delta requires them pinned here): jt9-degraded after N = 5 consecutive
/// non-Decoded/non-BandDead outcomes, clearing on the first good slot;
/// band-dead after k = 20 consecutive zero-decode slots (5 minutes). The N=5
/// degraded counter also folds L2 backpressure, lost-frames, and
/// storage-error drops — a slot L2 drops for one of those reasons without
/// ever calling `decode_slot` still counts as a non-Decoded outcome toward
/// N. Scheduled discards (the partial first slot after start/resume, the
/// QSY transition slot, clock-anomaly abandonment) count toward neither N
/// nor k.
```

**(2) `Ft8Decode::partial` doc.** CURRENT (quoted verbatim):
```rust
    /// True when this record was salvaged from a timed-out run's partial
    /// stdout (no `<DecodeFinished>` sentinel seen).
    pub partial: bool,
```
Replace with:
```rust
    /// True when this record was salvaged from an abnormally-terminated
    /// run (timeout or signal/nonzero exit); false when the completeness
    /// sentinel was seen.
    pub partial: bool,
```

**(3) `SlotFailure::Signal` doc.** CURRENT (quoted verbatim):
```rust
    /// jt9 died by signal (its common failure mode: Fortran error + SIGSEGV).
    Signal { signal: String, stderr_tail: String },
```
Replace with:
```rust
    /// jt9 died by signal or nonzero exit (its common failure mode:
    /// Fortran error + SIGSEGV) with ZERO parsed decode lines.
    /// Salvage-on-signal (tuxlink-gujnz): ≥ 1 parsed line returns
    /// `Decoded` (partial = no sentinel) instead — this variant is the
    /// zero-line case only.
    Signal { signal: String, stderr_tail: String },
```

- [ ] **Step 5: Run the full tuxlink-jt9 suite green + clippy clean**

Run: `cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-jt9 --locked`
Expected: all green, including the real-jt9 e2e (jt9 is installed on the dev Pi; those tests print SKIP if it is somehow absent — CI always runs them).
Run: `cargo clippy --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-jt9 --all-targets --locked -- -D warnings`
Expected: clean.
Also run the capture crate once more (its dev-dep just changed semantics, not API — must still be green):
`cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked`

- [ ] **Step 6: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/tuxlink-jt9/src/runner.rs src-tauri/tuxlink-jt9/src/types.rs src-tauri/tuxlink-jt9/tests/fake_jt9.rs
git commit -m "feat(ft8): salvage-on-signal parity in the jt9 runner + contract doc edits (tuxlink-b026z.3 T7, resolves tuxlink-gujnz)"
```

---

**REVIEW GATE C (after Task 7 — Phase A+B complete):** perspectives: (1) classification-order walk — trace every `SlotOutcome` arm of the modified runner against the fake-jt9 suite and the spec's pinned ordering (StderrEof → salvage → Signal, on BOTH the signal and nonzero-exit paths); (2) contract-surface audit — the three types.rs doc edits match the spec §Contract edits verbatim in substance, and no OTHER doc in tuxlink-jt9 still claims timeout-only salvage (grep `timed-out` across the crate); (3) leaf-crate purity — `tuxlink-capture/Cargo.toml` `[dependencies]` is still empty, no `std::time`/`SystemTime` reads anywhere in `tuxlink-capture` (time is injected data); (4) cross-crate constant agreement — 180,000/12,000 in `wavwrite.rs` vs `tuxlink_jt9::wav`, N/k in `state.rs` vs the types.rs doc. Minimum three rounds; persist findings to `dev/scratch/b026z.3-gate-C-findings.md` before proceeding to Phase C. Files under review: `src-tauri/tuxlink-jt9/src/runner.rs`, `src-tauri/tuxlink-jt9/src/types.rs`, `src-tauri/tuxlink-jt9/tests/fake_jt9.rs` (plus the `tuxlink-capture` purity re-check named in perspective 3).

---

## Phase C — main-crate `src/ft8/` service (Tasks 8–19)

**Phase C compile discipline (governs every task below — read before Task 8):**
the dev Pi CANNOT compile the main src-tauri crate. For every Phase C task the
subagent:

1. WRITES the code + unit tests exactly as specified — full TDD authorship
   (test first, then implementation), but the red-green RUN is deferred to CI.
2. Does NOT attempt `cargo build` / `cargo test` / `cargo check` /
   `cargo clippy` against the main crate locally — not even "a quick check"
   (a cold main-crate build wedges this contended Pi; project rule
   `no_cold_cargo_on_contended_pi`). Commands marked **[CI-side]** below are
   what the PR's CI run executes; the subagent does not run them.
3. Runs locally ONLY leaf-crate commands where a task touches
   `tuxlink-capture`/`tuxlink-jt9` (none of Tasks 8–19 do, except Task 18's
   fixture-prep sanity step which is pure Python/shell).
4. Stages + commits per the task's commit step (heredoc trailer form from
   Global Constraints); the PARENT pushes **at the Gate D / Gate E / Gate F
   boundaries** (Global Constraints §Push cadence) and CI (amd64 + arm64,
   `cargo clippy/test --workspace --all-targets --locked`) is the compile +
   test verdict for each pushed batch. If a subagent cannot commit in the
   worktree, it stops after staging and reports — the parent commits.

**Clippy traps (CI runs `-D warnings` on the whole workspace; every Phase C
task must write around these up front — there is no local red-green to catch
them):**

- `redundant_clone` — never `.clone()` a value you then move or drop; clone
  only across a real ownership boundary (thread spawn, struct storage,
  `Arc` fan-out).
- `needless_collect` — never `.collect::<Vec<_>>()` just to iterate again;
  chain the iterators.
- `result_large_err` — clippy denies `Result<_, E>` where `E` exceeds
  128 bytes. Diagnostics travel as `String` (stringify at the boundary); do
  NOT thread big enums or context structs through `Err` arms.
  `SourceError::Io(String)` / `SourceError::UnsupportedFormat(String)` and
  `Ft8Platform`'s stringly errors below follow this deliberately.
- `incompatible_msrv` (MSRV 1.75, lint denied) — no `Result::inspect_err`
  (1.76), no `slice::trim_ascii` (1.80), no `Option::take_if` (1.80).
  `u32::div_ceil` (1.73) and `JoinHandle::is_finished` (1.61) are fine.
- `clippy::mutex_atomic` — a lone bool/u64 behind a Mutex is denied; use
  `AtomicBool`/`AtomicU64` (the service structs below already do).
- `clippy::type_complexity` — alias long trait-object types
  (`type SourceFactory = ...`) instead of inlining them in signatures.
- `clippy::too_many_arguments` — the service constructors below take a
  bundled `Ft8Deps` struct, not 8 loose args; keep it that way.
- Dead-code: `src/lib.rs` declares `pub mod ft8;` in Task 10, and Phase C
  items are `pub` items of a `pub mod` in a lib crate, so `dead_code` does
  not fire on the not-yet-wired-to-Tauri surface. Do NOT sprinkle
  `#[allow(dead_code)]`.
- Float comparisons in tests: use explicit tolerance
  (`(a - b).abs() < 1e-6`), never `assert_eq!` on `f32`/`f64`
  (`clippy::float_cmp` fires in `--all-targets`).

**Cross-cutting interface note (binding for Tasks 10–18):** the leaf-crate
`SlotAssembler` consumes RAW 48 kHz channel-0 samples and owns its `Decimator`
internally (Tasks 4–5 manifest). The waterfall tap (spec §Waterfall tap) needs
the continuous 12 kHz stream, which the assembler does not expose mid-slot.
Task 12 therefore runs a SECOND `Decimator` instance in the capture thread for
the tap. Both instances use the same committed `COEFFS`, so their outputs are
bit-identical — the delta's "one path, three consumers" pin is preserved
semantically (one filter DESIGN, one signal path shape); the duplicate
instance exists only because the assembler's filter state must stay private
for gap-fill continuity. This resolution is recorded here so no reviewer
"fixes" it by exposing assembler internals.

**Module-placement deviation (recorded):** the spec's module sketch places
`Ft8ListenerState` in `src/ft8/mod.rs`; the implementation puts it in
`src/ft8/service.rs` (mod.rs stays a declaration-only façade). Deliberate —
recorded here so no reviewer "restores" the sketch's location.

---

### Task 8: devices.rs — `hw:` resolver handle + capture-capable enumeration

**Files:**
- Modify: `src-tauri/src/winlink/ax25/devices.rs`
- Modify: `src-tauri/src/mcp_ports.rs` (two `SnapshotCard` fixture literals gain the new field)
- Modify: `src-tauri/src/winlink/ax25/mod.rs` (re-export `enumerate_capture_devices`)

**Interfaces:**
- Consumes: existing `SnapshotCard`, `SysSnapshot`, `AudioDevice`,
  `StableAudioId`, `enumerate_audio_devices`, `resolve_managed_device`
  (`devices.rs`).
- Produces (consumed by Tasks 10–11's `Ft8Platform` and the snapshot picker):
```rust
// devices.rs
pub struct SnapshotCard { /* existing fields */ pub has_capture: bool }   // NEW field
pub struct ResolvedManagedDevice {
    pub alsa_plughw: String,
    pub alsa_hw: String,        // NEW: "hw:<card_index>,0" from the FRESHLY resolved index
    pub card_index: u32,
}
pub fn alsa_hw_name(card_index: u32) -> String;                            // "hw:<index>,0"
pub fn enumerate_capture_devices(snapshot: &SysSnapshot) -> Vec<AudioDevice>;
```

**TDD note:** write Steps 1–2's tests BEFORE the Step 3 implementation edits.
They cannot RUN locally (main crate); authorship order still holds so the
tests document intent independent of the code.

**Why `hw:` and not `plughw:`/`CARD=<id>` (spec §ALSA open, restated for the
subagent):** the plug layer silently converts any rate/format request, making
`blocked(unsupported-sample-rate)` unreachable; and `CARD=<id>` collides when
two same-model USB codecs share a `card_id` — the existing duplicate-card
fixture (`cardid_hash_disambiguates_same_card_id_via_device_node`,
`devices.rs:~1154`) proves the resolver picks a specific card INDEX yet
returns only an id-based name. `alsa_hw` closes that hole with the live index.

- [ ] **Step 1: Write the failing resolver test extension**

In `devices.rs` tests, the existing duplicate-card test ends with (quoted
verbatim — the two lines to extend follow them):
```rust
        let resolved =
            resolve_managed_device(&id_b, &snap).expect("id_b must resolve to card b");
        // Resolved to b's live index (9), not a's (4) — no collision, right card.
        assert_eq!(resolved.card_index, 9);
        assert_eq!(resolved.alsa_plughw, "plughw:CARD=Device,DEV=0");
```
APPEND directly after those asserts, inside the same test:
```rust
        // L2 (tuxlink-b026z.3): the hw: handle is derived from the FRESHLY
        // resolved index — the id-collision case is exactly where an id-based
        // open ("hw:CARD=Device") would grab the wrong card. The plughw name
        // is ambiguous here (both cards share the id); alsa_hw is not.
        assert_eq!(resolved.alsa_hw, "hw:9,0");
```

- [ ] **Step 2: Write the failing capture-filter tests**

Add to the `devices.rs` test module (uses the module's existing `device_node`
/ `CMEDIA_VID` helpers — read the neighboring fixtures first and reuse their
style):
```rust
    /// L2 capture enumeration (spec §Device selection): the FT8 picker lists
    /// only cards with a capture substream; the PACKET picker keeps its
    /// USB-presence-only filter. A playback-only USB card (has_capture:
    /// false) appears in enumerate_audio_devices but NOT in
    /// enumerate_capture_devices.
    #[test]
    fn capture_enumeration_excludes_playback_only_cards() {
        let capture_card = SnapshotCard {
            card_index: 1,
            card_id: "DRA".into(),
            card_name: "DRA-100 USB Audio".into(),
            by_id_basename: Some("usb-DRA-100-00".into()),
            usb: Some(UsbIdentity {
                vid: CMEDIA_VID.into(),
                pid: "013a".into(),
                serial: None,
            }),
            usb_parent: Some(device_node("2-1")),
            has_capture: true,
        };
        let playback_only = SnapshotCard {
            card_index: 2,
            card_id: "Headset".into(),
            card_name: "USB Playback-Only Headset".into(),
            by_id_basename: Some("usb-Headset-00".into()),
            usb: Some(UsbIdentity {
                vid: "1234".into(),
                pid: "5678".into(),
                serial: Some("HS1".into()),
            }),
            usb_parent: Some(device_node("2-2")),
            has_capture: false,
        };
        let snap = SysSnapshot {
            cards: vec![capture_card, playback_only],
            ..Default::default()
        };

        // Packet picker: unchanged — both USB cards listed.
        assert_eq!(enumerate_audio_devices(&snap).len(), 2);

        // FT8 capture picker: playback-only card filtered out.
        let cap = enumerate_capture_devices(&snap);
        assert_eq!(cap.len(), 1);
        assert_eq!(cap[0].human_name, "DRA-100 USB Audio");
    }

    /// Onboard (non-USB) cards stay excluded from capture enumeration even
    /// when they report a capture substream (the bcm2835 class).
    #[test]
    fn capture_enumeration_still_excludes_onboard_cards() {
        let onboard = SnapshotCard {
            card_index: 0,
            card_id: "vc4hdmi".into(),
            card_name: "vc4-hdmi".into(),
            by_id_basename: None,
            usb: None,
            usb_parent: None,
            has_capture: true,
        };
        let snap = SysSnapshot { cards: vec![onboard], ..Default::default() };
        assert!(enumerate_capture_devices(&snap).is_empty());
        assert!(enumerate_audio_devices(&snap).is_empty());
    }
```
NB: these fixtures use `..Default::default()` on `SysSnapshot` — verify
`SysSnapshot` derives (or manually implements) `Default`; if it does not,
construct every field explicitly instead of adding a derive to released
code.

- [ ] **Step 3: Implement — new field, new handle, new function**

**(3a)** Add the field to `SnapshotCard` (after `usb_parent`):
```rust
    /// True when the card exposes at least one CAPTURE substream
    /// (`/proc/asound/card<N>/pcm*c` exists). The packet picker ignores this
    /// (playback-only cards were never a packet hazard in practice); the FT8
    /// capture picker (`enumerate_capture_devices`) filters on it so the
    /// operator is never offered a card that cannot record (tuxlink-b026z.3).
    pub has_capture: bool,
```

**(3b) Mechanical fixture sweep.** Every existing `SnapshotCard { ... }`
literal must gain `has_capture: true` (preserves prior behavior for all
packet-path tests). Find them all:
```bash
cd "$WT" && pwd
grep -rn "SnapshotCard {" src-tauri/src --include='*.rs'
```
Expected: ~13 in `devices.rs` (tests + `read_sys_snapshot`) and 2 in
`mcp_ports.rs` (test fixtures). Add `has_capture: true,` to every TEST
literal. Literals using struct-update (`..b.clone()`) need no edit.

**(3c)** In `read_sys_snapshot` (the impure shim), populate the field from
`/proc`. The card-construction closure currently reads (quoted verbatim,
`devices.rs` ~line 456):
```rust
            .map(|(card_index, card_id, card_name)| SnapshotCard {
                card_index,
                card_id,
                card_name,
```
The literal continues with the remaining fields set to `None` defaults (read
the actual continuation before editing). Add to that literal:
```rust
                has_capture: card_has_capture_substream(card_index),
```
and add the helper next to the shim (same soft-failure posture as the rest of
the shim — unreadable dir ⇒ `false`, never a panic):
```rust
/// IMPURE: true when `/proc/asound/card<N>` contains a `pcm*` entry whose
/// name ends in `c` (a capture substream directory, e.g. `pcm0c`). Part of
/// the read_sys_snapshot shim — untested by design; the pure filter over the
/// resulting flag IS tested (`enumerate_capture_devices`).
fn card_has_capture_substream(card_index: u32) -> bool {
    let dir = format!("/proc/asound/card{card_index}");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let name = e.file_name();
        let name = name.to_string_lossy();
        name.starts_with("pcm") && name.ends_with('c')
    })
}
```

**(3d)** `alsa_hw` on `ResolvedManagedDevice`. The struct + resolver currently
read (quoted verbatim, `devices.rs:392-421`, doc comments elided here — keep
them):
```rust
pub struct ResolvedManagedDevice {
    /// The live ALSA `plughw:CARD=<id>,DEV=0` name for `ADEVICE`.
    pub alsa_plughw: String,
    /// The live boot-order `card<N>` index backing `alsa_plughw`.
    pub card_index: u32,
}
```
becomes:
```rust
pub struct ResolvedManagedDevice {
    /// The live ALSA `plughw:CARD=<id>,DEV=0` name for `ADEVICE`.
    pub alsa_plughw: String,
    /// The live ALSA `hw:<card_index>,0` name, derived from the FRESHLY
    /// resolved boot-order index (tuxlink-b026z.3). The FT8 capture path
    /// opens THIS name: `plughw:` would silently resample (masking
    /// `blocked(unsupported-sample-rate)`), and `CARD=<id>` collides when
    /// two same-model codecs share a card id (see
    /// `cardid_hash_disambiguates_same_card_id_via_device_node`).
    pub alsa_hw: String,
    /// The live boot-order `card<N>` index backing both names.
    pub card_index: u32,
}

/// The ALSA `hw:<index>,0` device name for a live card index. Numeric-index
/// form on purpose — see [`ResolvedManagedDevice::alsa_hw`].
pub fn alsa_hw_name(card_index: u32) -> String {
    format!("hw:{card_index},0")
}
```
and the resolver's `.map` closure gains the field:
```rust
        .map(|d| ResolvedManagedDevice {
            alsa_plughw: d.alsa_plughw,
            alsa_hw: alsa_hw_name(d.card_index),
            card_index: d.card_index,
        })
```
Sweep any other `ResolvedManagedDevice { ... }` literal
(`grep -rn "ResolvedManagedDevice {" src-tauri/src --include='*.rs'` — the
existing resolver tests construct expected values) and add the matching
`alsa_hw: alsa_hw_name(<index>)` / literal string.

**(3e)** `enumerate_capture_devices`, directly below
`enumerate_audio_devices` — read `enumerate_audio_devices` in devices.rs
and mirror its body (the new function reuses its shape, adding only the
capture filter):
```rust
/// Enumerate the CAPTURE-capable audio devices from a snapshot — the FT8
/// listener's picker source (spec §Device selection). Same stable-identity
/// resolution as [`enumerate_audio_devices`], plus the capture-substream
/// filter (`SnapshotCard::has_capture`). The packet picker's function is
/// deliberately untouched: its USB-presence-only filter is released
/// behavior.
///
/// Pure over `snapshot` — no `/dev`, no ALSA, no I/O.
pub fn enumerate_capture_devices(snapshot: &SysSnapshot) -> Vec<AudioDevice> {
    snapshot
        .cards
        .iter()
        .filter(|c| is_usable_packet_card(c) && c.has_capture)
        .map(|card| AudioDevice {
            human_name: card.card_name.clone(),
            alsa_plughw: plughw_name(card),
            stable_id: derive_stable_id(card),
            usb_parent: card.usb_parent.clone(),
            card_index: card.card_index,
        })
        .collect()
}
```

**(3f)** Re-export from `src/winlink/ax25/mod.rs` — the existing re-export
line (quoted verbatim, `mod.rs:33` region):
```rust
    PttChoice, ResolvedManagedDevice, StableAudioId, SysSnapshot,
```
gains `enumerate_capture_devices` in the same `pub use` list (alphabetical
position; read the full `pub use` statement first).

- [ ] **Step 4: [CI-side] verification**

`cargo clippy --workspace --all-targets --locked -- -D warnings` and
`cargo test --workspace --locked` run on the PR. Locally: do NOT build. Run
only a grep self-check that no `SnapshotCard {` literal is missing the new
field:
```bash
cd "$WT" && pwd
grep -rn "SnapshotCard {" src-tauri/src --include='*.rs' | wc -l
grep -rn "has_capture" src-tauri/src --include='*.rs' | wc -l
```
Expected: the second count ≥ the first count minus struct-update literals
(each full literal has the field; plus the struct definition, helper, and
filter references).

- [ ] **Step 5: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/winlink/ax25/devices.rs src-tauri/src/winlink/ax25/mod.rs src-tauri/src/mcp_ports.rs
git commit -m "feat(ft8): capture-capable device enumeration + hw:<index> resolver handle (tuxlink-b026z.3 T8)"
```

**Completion check:** tests authored before code; every `SnapshotCard`
literal carries `has_capture`; packet-picker behavior untouched
(`enumerate_audio_devices` body unmodified); no local main-crate build was
attempted.

---
### Task 9: config.rs — `Ft8Config` + `Ft8SweepConfig` + validation + deps

**Files:**
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/Cargo.toml` (`[dependencies]` gains `tuxlink-capture` + `alsa`)
- Modify: `src-tauri/Cargo.lock` (regenerated — see Step 5)

**Interfaces:**
- Consumes: `tuxlink_capture::bands::dial_hz` (Task 1), `StableAudioId`
  (`devices.rs`), the `ElmerConfig` `skip_serializing_if` precedent
  (`config.rs:~1493`), `ConfigValidationError` (`config.rs:~783`).
- Produces (consumed by Tasks 11 + 17):
```rust
#[serde(default)] pub struct Ft8Config {
    pub enabled: bool,                                          // false
    pub device: Option<crate::winlink::ax25::devices::StableAudioId>, // None
    pub band: String,                                           // "20m"
    pub sweep: Ft8SweepConfig,
}
#[serde(default)] pub struct Ft8SweepConfig {
    pub enabled: bool,                                          // false
    pub bands: Vec<String>,                 // ["80m","40m","20m","15m","10m"]
    pub dwell_slots: u8,                                        // 8
}
impl Ft8Config { pub fn is_default(&self) -> bool; }
impl RigUiConfig { pub fn is_configured(&self) -> bool; }
// Config gains: #[serde(default, skip_serializing_if = "Ft8Config::is_default")] pub ft8: Ft8Config,
// ConfigValidationError gains: Ft8UnknownBand, Ft8SweepUnknownBand, Ft8DwellOutOfRange, Ft8SweepRequiresRig
```

**TDD note:** author Step 1's tests first; they run in CI only.

- [ ] **Step 1: Write the failing config tests**

Add to the `config.rs` test module (mirror the elmer round-trip tests at
`config.rs:~3109` for style — read them first):
```rust
    // ---- tuxlink-b026z.3: Ft8Config ------------------------------------

    /// A default Config serializes WITHOUT an "ft8" key (skip_serializing_if
    /// — the ElmerConfig precedent): pre-FT8 config files stay byte-identical
    /// after a load→save cycle.
    #[test]
    fn ft8_config_default_is_skipped_on_serialize() {
        let cfg: Config = serde_json::from_str(&config_json(CONFIG_SCHEMA_VERSION, "")).unwrap();
        assert!(cfg.ft8.is_default());
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("\"ft8\""), "default ft8 section must be omitted: {json}");
    }

    /// A non-default Ft8Config round-trips every field, including the
    /// persisted StableAudioId and the sweep block.
    #[test]
    fn ft8_config_round_trips_when_customized() {
        use crate::winlink::ax25::devices::{StableAudioId, StableIdKind};
        let mut cfg: Config = serde_json::from_str(&config_json(CONFIG_SCHEMA_VERSION, "")).unwrap();
        cfg.ft8.enabled = true;
        cfg.ft8.device = Some(StableAudioId {
            kind: StableIdKind::ByIdSymlink,
            value: "usb-DRA-100-00".into(),
        });
        cfg.ft8.band = "40m".into();
        cfg.ft8.sweep.enabled = false;
        cfg.ft8.sweep.bands = vec!["40m".into(), "20m".into()];
        cfg.ft8.sweep.dwell_slots = 12;
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"ft8\""));
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ft8, cfg.ft8);
    }

    /// Absent-from-disk ft8 section deserializes to the full default (serde
    /// migration — no schema bump; matches the AprsConfig pattern).
    #[test]
    fn ft8_config_defaults_when_absent() {
        let cfg: Config = serde_json::from_str(&config_json(CONFIG_SCHEMA_VERSION, "")).unwrap();
        assert!(!cfg.ft8.enabled);
        assert_eq!(cfg.ft8.device, None);
        assert_eq!(cfg.ft8.band, "20m");
        assert!(!cfg.ft8.sweep.enabled);
        assert_eq!(cfg.ft8.sweep.bands, ["80m", "40m", "20m", "15m", "10m"]);
        assert_eq!(cfg.ft8.sweep.dwell_slots, 8);
    }

    /// validate() rules (spec §Config): band ∈ table, sweep.bands ∈ table,
    /// dwell_slots ∈ 4..=40, sweep.enabled ⇒ rig configured. Every rule has
    /// a rejecting case AND the default config passes.
    #[test]
    fn ft8_config_validation_rules() {
        let base = || -> Config {
            serde_json::from_str(&config_json(CONFIG_SCHEMA_VERSION, "")).unwrap()
        };
        assert!(base().validate().is_ok(), "the default ft8 config must validate");

        let mut c = base();
        c.ft8.band = "6m".into(); // not in the FT8 table
        assert!(matches!(c.validate(), Err(ConfigValidationError::Ft8UnknownBand { .. })));

        let mut c = base();
        c.ft8.sweep.bands = vec!["20m".into(), "23cm".into()];
        assert!(matches!(c.validate(), Err(ConfigValidationError::Ft8SweepUnknownBand { .. })));

        let mut c = base();
        c.ft8.sweep.dwell_slots = 3;
        assert!(matches!(c.validate(), Err(ConfigValidationError::Ft8DwellOutOfRange { .. })));
        let mut c = base();
        c.ft8.sweep.dwell_slots = 41;
        assert!(matches!(c.validate(), Err(ConfigValidationError::Ft8DwellOutOfRange { .. })));

        // sweep.enabled with NO rig configured → rejected.
        let mut c = base();
        c.ft8.sweep.enabled = true;
        assert!(!c.rig.is_configured());
        assert!(matches!(c.validate(), Err(ConfigValidationError::Ft8SweepRequiresRig)));

        // sweep.enabled WITH a rig → accepted.
        let mut c = base();
        c.rig.rig_hamlib_model = Some(1043);
        c.rig.cat_serial_path = Some("/dev/ttyUSB0".into());
        c.ft8.sweep.enabled = true;
        assert!(c.validate().is_ok());
    }

    /// is_configured mirrors modem_commands::rig_config_from's Some-conditions
    /// (model present AND a non-blank CAT serial path) without importing the
    /// modem layer into config.
    #[test]
    fn rig_is_configured_predicate() {
        let mut rig = RigUiConfig::default();
        assert!(!rig.is_configured());
        rig.rig_hamlib_model = Some(1043);
        assert!(!rig.is_configured(), "model alone is not a usable CAT link");
        rig.cat_serial_path = Some("   ".into());
        assert!(!rig.is_configured(), "blank serial path is not configured");
        rig.cat_serial_path = Some("/dev/ttyUSB0".into());
        assert!(rig.is_configured());
    }
```

- [ ] **Step 2: Implement the config structs**

Add after the `AprsConfig` block (`config.rs:~1481`), before the ElmerConfig
section:
```rust
// ============================================================================
// FT8 Station Intelligence listener config (tuxlink-b026z.3)
// ============================================================================

/// FT8 listener settings, persisted under `ft8` in config.json.
///
/// Spec: docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md
/// §Config. Additive section (`#[serde(default)]` migrates configs that
/// predate it; no schema bump). `deny_unknown_fields` is intentionally
/// absent, matching the other additive UI-config sections.
///
/// - `enabled`: autostart flag. `ft8_listener_start` sets it true;
///   `ft8_listener_stop` sets it false. Autostart fires on `enabled` ALONE —
///   NOT gated on `device.is_some()` (a first-contact operator interrupted
///   mid-pick must find `blocked(needs-device-selection)` after restart, not
///   a silent `stopped`).
/// - `device`: the operator-picked capture card. `None` →
///   `blocked(needs-device-selection)`. No auto-selection, ever (operator
///   decision 2 in the spec header).
/// - `band`: the selected band chip. The serde default `"20m"` is a
///   PRESELECTED CHIP, not an assertion — until CAT confirms or the operator
///   clicks, records carry `band_source = default-unconfirmed`
///   (spec §Band provenance).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ft8Config {
    pub enabled: bool,
    pub device: Option<crate::winlink::ax25::devices::StableAudioId>,
    pub band: String,
    pub sweep: Ft8SweepConfig,
}

impl Default for Ft8Config {
    fn default() -> Self {
        Self {
            enabled: false,
            device: None,
            band: "20m".into(),
            sweep: Ft8SweepConfig::default(),
        }
    }
}

impl Ft8Config {
    /// True when byte-for-byte equivalent to the default — the
    /// `skip_serializing_if` predicate (ElmerConfig precedent): a never-
    /// touched ft8 section is omitted so pre-FT8 configs stay byte-identical.
    pub fn is_default(&self) -> bool {
        *self == Ft8Config::default()
    }
}

/// Opt-in CAT band sweep (spec §Sweep). Requires a configured rig:
/// `sweep.enabled` with `Config.rig` unset is a validation error. Dwell
/// default 8 slots = 2 min/band (5-band default rotation = 10 min);
/// valid 4–40.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ft8SweepConfig {
    pub enabled: bool,
    pub bands: Vec<String>,
    pub dwell_slots: u8,
}

impl Default for Ft8SweepConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bands: vec!["80m".into(), "40m".into(), "20m".into(), "15m".into(), "10m".into()],
            dwell_slots: 8,
        }
    }
}
```

Add the field to `Config` (after the `elmer` field — quoted verbatim, the
current struct tail):
```rust
    #[serde(default, skip_serializing_if = "ElmerConfig::is_default")]
    pub elmer: ElmerConfig,
}
```
becomes:
```rust
    #[serde(default, skip_serializing_if = "ElmerConfig::is_default")]
    pub elmer: ElmerConfig,
    /// FT8 Station Intelligence listener settings (tuxlink-b026z.3).
    /// `#[serde(default)]` migrates configs that predate this field (absent →
    /// `Ft8Config::default()`); the field is now KNOWN, satisfying
    /// `deny_unknown_fields`. `skip_serializing_if` keeps a never-touched
    /// config byte-identical to its pre-FT8 shape.
    #[serde(default, skip_serializing_if = "Ft8Config::is_default")]
    pub ft8: Ft8Config,
}
```

Add the rig predicate to `impl RigUiConfig` (create the impl block next to
the struct if none exists):
```rust
impl RigUiConfig {
    /// True when a usable CAT link is configured: hamlib model present AND a
    /// non-blank serial path. Mirrors the Some-conditions of
    /// `modem_commands::rig_config_from` WITHOUT importing the modem layer —
    /// config-level validation (Ft8SweepRequiresRig) needs the predicate
    /// here. If rig_config_from's conditions ever change, change this too
    /// (this side cites rig_config_from; the citation is one-way).
    pub fn is_configured(&self) -> bool {
        self.rig_hamlib_model.is_some()
            && self.cat_serial_path.as_deref().is_some_and(|p| !p.trim().is_empty())
    }
}
```
(MSRV: `Option::is_some_and` is stable since 1.70 — fine.)

- [ ] **Step 3: Implement the validation rules**

Extend `ConfigValidationError` (quoted verbatim current enum, then the
addition):
```rust
    #[error("packet.ssid {ssid} is out of the 0–15 AX.25 range")]
    PacketSsidOutOfRange { ssid: u8 },
```
gains, after it:
```rust
    #[error("ft8.band {band:?} is not an FT8 band (see the band table)")]
    Ft8UnknownBand { band: String },
    #[error("ft8.sweep.bands entry {band:?} is not an FT8 band")]
    Ft8SweepUnknownBand { band: String },
    #[error("ft8.sweep.dwell_slots {dwell_slots} is outside the valid 4–40 range")]
    Ft8DwellOutOfRange { dwell_slots: u8 },
    #[error("ft8.sweep.enabled requires a configured rig (CAT) — set the rig model + serial first")]
    Ft8SweepRequiresRig,
```

Extend `Config::validate()` — append before the final `Ok(())` (quoted
verbatim anchor):
```rust
        if self.packet.ssid > 15 {
            return Err(ConfigValidationError::PacketSsidOutOfRange { ssid: self.packet.ssid });
        }
        Ok(())
```
becomes:
```rust
        if self.packet.ssid > 15 {
            return Err(ConfigValidationError::PacketSsidOutOfRange { ssid: self.packet.ssid });
        }
        // FT8 listener rules (tuxlink-b026z.3, spec §Config). The band table
        // is the leaf crate's — one source for chips, QSY targets, and this.
        if tuxlink_capture::bands::dial_hz(&self.ft8.band).is_none() {
            return Err(ConfigValidationError::Ft8UnknownBand { band: self.ft8.band.clone() });
        }
        for band in &self.ft8.sweep.bands {
            if tuxlink_capture::bands::dial_hz(band).is_none() {
                return Err(ConfigValidationError::Ft8SweepUnknownBand { band: band.clone() });
            }
        }
        if !(4..=40).contains(&self.ft8.sweep.dwell_slots) {
            return Err(ConfigValidationError::Ft8DwellOutOfRange {
                dwell_slots: self.ft8.sweep.dwell_slots,
            });
        }
        if self.ft8.sweep.enabled && !self.rig.is_configured() {
            return Err(ConfigValidationError::Ft8SweepRequiresRig);
        }
        Ok(())
```

- [ ] **Step 4: Add the dependencies**

In `src-tauri/Cargo.toml` `[dependencies]` (append near the other path/leaf
entries — read the section tail first):
```toml
# FT8 Station Intelligence L2 (tuxlink-b026z.3): pure capture logic (leaf,
# std-only) + the ALSA binding for the one hardware touchpoint
# (src/ft8/alsa_source.rs). `alsa` links system libasound: CI/apt gains
# libasound2-dev (Task 18) and the .deb a libasound2 runtime Depends.
tuxlink-capture = { path = "tuxlink-capture" }
alsa = "0.9"
```

- [ ] **Step 5: Regenerate Cargo.lock (no compile needed — runs on the Pi)**

```bash
cd "$WT" && pwd
cargo metadata --manifest-path "$WT/src-tauri/Cargo.toml" > /dev/null
git -C "$WT" diff --stat src-tauri/Cargo.lock
```
Expected: `Cargo.lock` changes (new `[[package]]` entries: `alsa`,
`alsa-sys`, plus the `tuxlink-capture` path member's new `alsa`-free dep
edge). `cargo metadata` resolves and rewrites the lock WITHOUT compiling —
this is the one lock-touching moment; every later command stays `--locked`.
NEVER pass `--locked` to this step (`rust_dep_requires_cargo_lock_update`:
`--locked` here would mask the very update being made).

- [ ] **Step 6: [CI-side] verification** — workspace clippy + tests on the
PR run. Locally: none (leaf crates unaffected by this task; running
`cargo test -p tuxlink-capture --locked` once to confirm the lock still
resolves is permitted and cheap).

- [ ] **Step 7: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/config.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(ft8): Ft8Config + sweep config, validation rules, alsa + tuxlink-capture deps (tuxlink-b026z.3 T9)"
```

**Completion check:** default config serializes without an `ft8` key; every
validation rule has a rejecting test; `Cargo.lock` committed; no `--locked`
was passed to the metadata regen; no local main-crate build attempted.

---
### Task 10: `src/ft8/` skeleton — traits, wire records, fakes, ALSA source, clock probe

**Files:**
- Create: `src-tauri/src/ft8/mod.rs`
- Create: `src-tauri/src/ft8/traits.rs`
- Create: `src-tauri/src/ft8/records.rs`
- Create: `src-tauri/src/ft8/clock.rs`
- Create: `src-tauri/src/ft8/events.rs`
- Create: `src-tauri/src/ft8/alsa_source.rs`
- Create: `src-tauri/src/ft8/testutil.rs`
- Modify: `src-tauri/src/lib.rs` (module declaration)

**Interfaces:**
- Consumes: `tuxlink_capture::slot::{GapReport, GapKind}`,
  `tuxlink_jt9::types::{SlotOutcome, SlotFailure, Ft8Decode, SLOT_DECODE_TIMEOUT_SECS}`,
  `tuxlink_jt9::runner::Jt9Runner`, `StableAudioId` (T8),
  `tuxlink_capture::state::{ServiceAxis, BlockedReason, HealthFlags, Sweep, SlotPhase, RingOutcomeKind}`.
  Note `DiscardClass` lives in `tuxlink_capture::slot`, not `state` (Tasks
  4–5 manifest).
- Produces (consumed by Tasks 11–18):
```rust
// traits.rs
pub struct ReadBatch { pub frames: usize, pub mono_ts_us: u64, pub gap: Option<GapReport> }
pub enum SourceError { Busy, Absent, UnsupportedFormat(String), Suspended, Wedged, Io(String) }
pub trait SampleSource: Send { fn read(&mut self, buf: &mut [i16]) -> Result<ReadBatch, SourceError>; }
pub trait DecodeEngine: Send + Sync {
    fn prewarm(&self) -> Result<(), String>;
    fn decode_slot(&self, wav: &Path, slot_tmp: &Path, slot_utc_ms: u64) -> SlotOutcome;
}
pub struct Jt9Engine { /* wraps Jt9Runner 1:1 */ }
// clock.rs
pub enum ClockSync { Synced, Unsynced, Unknown }
pub trait ClockProbe: Send + Sync { fn ntp_synchronized(&self) -> ClockSync; }
pub struct TimedatectlProbe;
// events.rs
pub trait EventSink: Send + Sync {
    fn emit_listening_change(&self, change: &Ft8ListeningChange);
    fn emit_slot(&self, record: &SlotRecord);
}
// records.rs — the serialized L3/L4 wire shapes (SlotRecord, RingOutcome,
// BandSource, DecodeDto, AudioDeviceChoice, ServiceAxisDto, BlockedReasonDto,
// HealthFlagsDto, SlotPhaseDto, SweepStatusDto, Ft8ListeningChange)
// alsa_source.rs
pub struct AlsaSource; impl AlsaSource { pub fn open(alsa_hw: &str) -> Result<Self, SourceError>; }
```

Design notes carried from the spec, restated so the subagent does not
re-derive them:
- **`DecodeEngine::prewarm` returns `Err(String)`, not `Err(SlotFailure)`** —
  the clippy `result_large_err` trap plus the start sequence only needs the
  spawn-class discrimination, which travels in the string (the production
  impl formats the `SlotFailure` with `{:?}` and the sequence matches on the
  `"SpawnFailed"` / `"not found"` substrings — see Task 11 step 4).
- **`UnsupportedFormat(String)` carries the ALSA diagnostic** (spec: the axis
  name is delta-pinned; the diagnostic distinguishes rate vs channel vs
  format). The bare-variant shorthand in earlier notes is superseded here.
- **Time is data:** `ReadBatch.mono_ts_us` comes from the source;
  `AlsaSource` derives it from a process-epoch `Instant`. Nothing in the
  service reads ambient time except through `Ft8Platform` (Task 11).
- **`ReadBatch.mono_ts_us: u64` flattens the spec's `MonoTs` newtype** — a
  deliberate contract-shape deviation, recorded: the leaf-crate assembler
  already consumes bare `u64` micros (Tasks 4–5 manifest), and a one-field
  newtype crossing the crate boundary bought no safety the tests do not
  already pin. Do not "restore" MonoTs for spec conformance.

**TDD note:** records.rs serde-shape tests and the fakes' self-tests are
authored before their impls. `alsa_source.rs` has NO unit tests (hardware) —
CI-compile-checked only; keep its logic minimal.

- [ ] **Step 1: Module skeleton + lib.rs declaration**

`src-tauri/src/ft8/mod.rs`:
```rust
//! FT8 Station Intelligence L2 — the persistent listening service
//! (tuxlink-b026z.3). ALSA capture → 48k→12k decimation → wall-clock-true
//! 15 s slot assembly → tmpfs WAV → jt9 decode, with the full service state
//! machine, modem yield/resume arbitration, and opt-in CAT sweep.
//!
//! Layering: pure logic lives in the `tuxlink-capture` leaf crate; this
//! module is everything that touches ALSA, threads, Tauri, tux-rig, or
//! process lifecycle. Spec:
//! docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md.

pub mod alsa_source;
pub mod clock;
pub mod events;
pub mod records;
pub mod traits;

#[cfg(test)]
pub mod testutil;
```
(`arbiter`, `commands`, `service`, `sweep` are appended to this list by their
own tasks.)

In `src-tauri/src/lib.rs`, the module list (quoted verbatim anchor):
```rust
pub mod mcp_ports;
pub mod modem_commands;
```
gains a declaration between them:
```rust
pub mod mcp_ports;
pub mod ft8;
pub mod modem_commands;
```
(Place `pub mod ft8;` after `pub mod forms;` instead if the surrounding list
is strictly alphabetical — read the real list and match its ordering
convention; the load-bearing part is that the declaration exists.)

- [ ] **Step 2: traits.rs**

```rust
//! Testability seams (spec §Testability traits). All four production impls
//! are thin; everything above them is driven by fakes in unit tests.

use std::path::Path;

use tuxlink_capture::slot::GapReport;
use tuxlink_jt9::runner::Jt9Runner;
use tuxlink_jt9::types::SlotOutcome;

/// One capture read's result. **Time is data at this seam**: the monotonic
/// timestamp arrives as a value so the slot assembler stays pure and tests
/// drive synthetic clocks.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadBatch {
    /// Valid frames written into the caller's buffer (channel-0, 48 kHz).
    pub frames: usize,
    /// Monotonic timestamp (µs) at which this batch was read.
    pub mono_ts_us: u64,
    /// A gap the source detected BEFORE these frames (xrun recovery /
    /// suspend). Size is never trusted from ALSA — the assembler computes it
    /// from the monotonic expected-frame counter.
    pub gap: Option<GapReport>,
}

/// Capture-source failure classes (spec §ALSA read loop errno mapping).
/// Diagnostics are `String`s (clippy result_large_err discipline).
#[derive(Debug, Clone, PartialEq)]
pub enum SourceError {
    /// Device held by another process (EBUSY at open).
    Busy,
    /// Device gone (ENODEV/EBADFD-class, or open ENOENT).
    Absent,
    /// Parameter negotiation failed on the hw device (rate/format/channels);
    /// carries the ALSA diagnostic for `blocked(unsupported-sample-rate)`.
    UnsupportedFormat(String),
    /// -ESTRPIPE: stream suspended (system sleep). The source recovers the
    /// PCM internally; the capture loop abandons the slot (clock anomaly).
    Suspended,
    /// 10 consecutive wait-timeouts on a silent, non-erroring stream — the
    /// C-Media wedge class. Treated as device loss.
    Wedged,
    /// Any other errno, stringified.
    Io(String),
}

/// The one audio seam. Production: [`crate::ft8::alsa_source::AlsaSource`].
pub trait SampleSource: Send {
    /// Blocking-bounded read: waits at most ~200 ms before returning either
    /// frames, an empty batch, or an error. Never parks unboundedly — the
    /// capture loop checks its abort flag between calls.
    fn read(&mut self, buf: &mut [i16]) -> Result<ReadBatch, SourceError>;
}

/// The decode seam. Production wraps [`Jt9Runner`] 1:1.
pub trait DecodeEngine: Send + Sync {
    /// One-time FFTW wisdom warm (spec §WAV writeout: once per runner
    /// construction, during `starting`, BEFORE any PCM is held). Errors are
    /// stringified `SlotFailure`s; the start sequence matches the
    /// spawn/not-found class by substring (Task 11).
    fn prewarm(&self) -> Result<(), String>;
    fn decode_slot(&self, wav: &Path, slot_tmp: &Path, slot_utc_ms: u64) -> SlotOutcome;
}

/// Production [`DecodeEngine`]: delegates to the L1 runner.
pub struct Jt9Engine {
    runner: Jt9Runner,
}

impl Jt9Engine {
    pub fn new(runner: Jt9Runner) -> Self {
        Self { runner }
    }
}

impl DecodeEngine for Jt9Engine {
    fn prewarm(&self) -> Result<(), String> {
        self.runner.prewarm().map_err(|f| format!("{f:?}"))
    }
    fn decode_slot(&self, wav: &Path, slot_tmp: &Path, slot_utc_ms: u64) -> SlotOutcome {
        self.runner.decode_slot(wav, slot_tmp, slot_utc_ms)
    }
}

/// THE process-lifetime monotonic epoch (µs). ONE epoch for the whole
/// process, by contract: **assembler mono values MUST come from one epoch**
/// — the slot assembler DIFFERENCES monotonic stamps across producers
/// (`AlsaSource` read batches and `Ft8Platform::mono_now_us` during gap
/// handling), so a second epoch would read as a giant clock anomaly on the
/// first mixed push. Every production monotonic stamp in `src/ft8/` calls
/// this; no other `OnceLock<Instant>` epoch may exist in the module.
pub(crate) fn process_mono_us() -> u64 {
    static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let epoch = EPOCH.get_or_init(std::time::Instant::now);
    u64::try_from(epoch.elapsed().as_micros()).unwrap_or(u64::MAX)
}
```

- [ ] **Step 3: records.rs — the wire shapes, with serde-shape tests FIRST**

The serde-shape tests pin the JSON the L3 frontend will parse (project rule
`serde_rename_all_enum_fields`: `rename_all` on an enum renames VARIANT TAGS
only; field names inside variants need their own attribute — hence the shape
test). Write the tests, then the types:

```rust
//! Serialized wire shapes for the FT8 events + snapshot (the L3/L4
//! contract, spec §Ring + §Snapshot). Pure DTOs: `From` impls mirror the
//! std-only leaf-crate state types into serde-derived shapes.

use serde::Serialize;

use crate::winlink::ax25::devices::StableAudioId;
use tuxlink_capture::state::{
    BlockedReason, HealthFlags, RingOutcomeKind, ServiceAxis, SlotPhase, Sweep,
};
use tuxlink_jt9::types::Ft8Decode;

/// Band-label provenance (spec §Band provenance): the service never claims a
/// band nobody asserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BandSource {
    CatConfirmed,
    OperatorAsserted,
    DefaultUnconfirmed,
}

/// One decoded FT8 message on the wire (mirrors `tuxlink_jt9::Ft8Decode`,
/// which cannot derive serde in a dep-free leaf crate).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodeDto {
    pub slot_utc_ms: u64,
    pub snr_db: i32,
    pub dt_s: f64,
    pub freq_hz: u32,
    pub message: String,
    pub from_call: Option<String>,
    pub to_call: Option<String>,
    pub grid: Option<String>,
    pub partial: bool,
}

impl From<&Ft8Decode> for DecodeDto {
    fn from(d: &Ft8Decode) -> Self {
        Self {
            slot_utc_ms: d.slot_utc_ms,
            snr_db: d.snr_db,
            dt_s: d.dt_s,
            freq_hz: d.freq_hz,
            message: d.message.clone(),
            from_call: d.from_call.clone(),
            to_call: d.to_call.clone(),
            grid: d.grid.clone(),
            partial: d.partial,
        }
    }
}

/// Scheduled-discard classes on the wire (spec §Counter semantics: these
/// count toward NEITHER counter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiscardClassDto {
    FirstSlot,
    QsyTransition,
    ClockAnomaly,
}

/// Per-slot outcome on the wire (spec §Ring). Internally tagged; variant
/// tags kebab-case; payload fields explicitly camelCase-named.
///
/// Deviation from the spec's `Failed(kind)` sketch, recorded: `Failed`
/// carries the Debug-formatted failure STRING so L3/L4 receive the full
/// diagnostic; kind-level matching happens via [`RingOutcome::kind`] /
/// `RingOutcomeKind`, so nothing the spec's kind enum carried is lost.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RingOutcome {
    Decoded,
    BandDead,
    Failed {
        #[serde(rename = "failure")]
        failure: String,
    },
    DroppedBackpressure,
    DroppedLostFrames,
    DroppedStorageError {
        #[serde(rename = "diagnostic")]
        diagnostic: String,
    },
    Discarded {
        #[serde(rename = "class")]
        class: DiscardClassDto,
    },
}

impl RingOutcome {
    /// The counter-classification the leaf-crate machine consumes
    /// (`ListenerMachine::on_slot_outcome`). 1:1 by construction.
    pub fn kind(&self) -> RingOutcomeKind {
        match self {
            RingOutcome::Decoded => RingOutcomeKind::Decoded,
            RingOutcome::BandDead => RingOutcomeKind::BandDead,
            RingOutcome::Failed { .. } => RingOutcomeKind::Failed,
            RingOutcome::DroppedBackpressure => RingOutcomeKind::DroppedBackpressure,
            RingOutcome::DroppedLostFrames => RingOutcomeKind::DroppedLostFrames,
            RingOutcome::DroppedStorageError { .. } => RingOutcomeKind::DroppedStorageError,
            RingOutcome::Discarded { .. } => RingOutcomeKind::Discarded,
        }
    }
}

/// One ring entry (spec §Ring, field-for-field). Every slot boundary yields
/// one — including drops and discards (L4's failure counters and honest
/// recency need them).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotRecord {
    pub slot_utc_ms: u64,
    pub band: String,
    pub dial_hz: u64,
    pub band_source: BandSource,
    pub band_label_confirmed_utc_ms: Option<u64>,
    pub outcome: RingOutcome,
    /// Empty except for `Decoded`.
    pub decodes: Vec<DecodeDto>,
    /// `any(decode.partial)` — salvage provenance.
    pub partial_salvage: bool,
    pub lost_frames: u64,
    pub boundary_skew_frames: u64,
    pub clip_fraction: f32,
    pub rms_dbfs: f32,
    /// Position within the current sweep dwell, when sweeping.
    pub dwell_slot_index: Option<u8>,
}

/// A pickable capture device (spec §Device selection:
/// `available_devices: Vec<{human_name, stable_id}>`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceChoice {
    pub human_name: String,
    pub stable_id: StableAudioId,
}

// ---- state-machine mirrors (leaf-crate types cannot derive serde) --------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlockedReasonDto {
    DeviceAbsent,
    NeedsDeviceSelection,
    WsjtxAbsent,
    UnsupportedSampleRate,
    CaptureWedged,
}

impl From<BlockedReason> for BlockedReasonDto {
    fn from(r: BlockedReason) -> Self {
        match r {
            BlockedReason::DeviceAbsent => Self::DeviceAbsent,
            BlockedReason::NeedsDeviceSelection => Self::NeedsDeviceSelection,
            BlockedReason::WsjtxAbsent => Self::WsjtxAbsent,
            BlockedReason::UnsupportedSampleRate => Self::UnsupportedSampleRate,
            BlockedReason::CaptureWedged => Self::CaptureWedged,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "axis", rename_all = "kebab-case")]
pub enum ServiceAxisDto {
    Stopped,
    Starting,
    Listening,
    Yielded,
    Blocked {
        #[serde(rename = "reason")]
        reason: BlockedReasonDto,
    },
    Stopping,
}

impl From<ServiceAxis> for ServiceAxisDto {
    fn from(a: ServiceAxis) -> Self {
        match a {
            ServiceAxis::Stopped => Self::Stopped,
            ServiceAxis::Starting => Self::Starting,
            ServiceAxis::Listening => Self::Listening,
            ServiceAxis::Yielded => Self::Yielded,
            ServiceAxis::Blocked(r) => Self::Blocked { reason: r.into() },
            ServiceAxis::Stopping => Self::Stopping,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthFlagsDto {
    pub clock_unsynced: bool,
    pub cat_fixed_band: bool,
    pub jt9_degraded: bool,
}

impl From<HealthFlags> for HealthFlagsDto {
    fn from(f: HealthFlags) -> Self {
        Self {
            clock_unsynced: f.clock_unsynced,
            cat_fixed_band: f.cat_fixed_band,
            jt9_degraded: f.jt9_degraded,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SlotPhaseDto {
    WaitingFirstSlot,
    Decoded,
    BandDead,
}

impl From<SlotPhase> for SlotPhaseDto {
    fn from(p: SlotPhase) -> Self {
        match p {
            SlotPhase::WaitingFirstSlot => Self::WaitingFirstSlot,
            SlotPhase::Decoded => Self::Decoded,
            SlotPhase::BandDead => Self::BandDead,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SweepModeDto {
    Inactive,
    Active,
    FallbackHold,
}

/// Sweep status on the wire (spec §Snapshot:
/// `SweepStatus { mode, band_idx, dwell_progress }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SweepStatusDto {
    pub mode: SweepModeDto,
    pub band_idx: Option<usize>,
    pub dwell_progress: Option<u8>,
}

impl From<Sweep> for SweepStatusDto {
    fn from(s: Sweep) -> Self {
        match s {
            Sweep::Inactive => Self { mode: SweepModeDto::Inactive, band_idx: None, dwell_progress: None },
            Sweep::Active { band_idx, dwell_progress } => Self {
                mode: SweepModeDto::Active,
                band_idx: Some(band_idx),
                dwell_progress: Some(dwell_progress),
            },
            Sweep::FallbackHold { .. } => {
                Self { mode: SweepModeDto::FallbackHold, band_idx: None, dwell_progress: None }
            }
        }
    }
}

/// The `ft8-listening:change` payload (spec §Events: axis + flags + phase +
/// band + sweep summary).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ft8ListeningChange {
    pub service: ServiceAxisDto,
    pub flags: HealthFlagsDto,
    pub slot_phase: SlotPhaseDto,
    pub band: String,
    pub dial_hz: u64,
    pub sweep: SweepStatusDto,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the exact JSON tag/field shapes the L3 frontend will parse.
    /// serde `rename_all` on an ENUM renames variant TAGS only — this test
    /// is the project-mandated shape pin (serde_rename_all_enum_fields).
    #[test]
    fn ring_outcome_serde_shape_is_pinned() {
        let j = |o: &RingOutcome| serde_json::to_string(o).unwrap();
        assert_eq!(j(&RingOutcome::Decoded), r#"{"kind":"decoded"}"#);
        assert_eq!(j(&RingOutcome::BandDead), r#"{"kind":"band-dead"}"#);
        assert_eq!(
            j(&RingOutcome::Failed { failure: "Timeout".into() }),
            r#"{"kind":"failed","failure":"Timeout"}"#
        );
        assert_eq!(j(&RingOutcome::DroppedBackpressure), r#"{"kind":"dropped-backpressure"}"#);
        assert_eq!(j(&RingOutcome::DroppedLostFrames), r#"{"kind":"dropped-lost-frames"}"#);
        assert_eq!(
            j(&RingOutcome::DroppedStorageError { diagnostic: "ENOSPC".into() }),
            r#"{"kind":"dropped-storage-error","diagnostic":"ENOSPC"}"#
        );
        assert_eq!(
            j(&RingOutcome::Discarded { class: DiscardClassDto::QsyTransition }),
            r#"{"kind":"discarded","class":"qsy-transition"}"#
        );
    }

    #[test]
    fn service_axis_serde_shape_is_pinned() {
        let j = |a: &ServiceAxisDto| serde_json::to_string(a).unwrap();
        assert_eq!(j(&ServiceAxisDto::Listening), r#"{"axis":"listening"}"#);
        assert_eq!(
            j(&ServiceAxisDto::Blocked { reason: BlockedReasonDto::NeedsDeviceSelection }),
            r#"{"axis":"blocked","reason":"needs-device-selection"}"#
        );
        assert_eq!(
            j(&ServiceAxisDto::Blocked { reason: BlockedReasonDto::CaptureWedged }),
            r#"{"axis":"blocked","reason":"capture-wedged"}"#
        );
    }

    #[test]
    fn every_ring_outcome_maps_to_its_counter_kind() {
        use tuxlink_capture::state::RingOutcomeKind as K;
        assert_eq!(RingOutcome::Decoded.kind(), K::Decoded);
        assert_eq!(RingOutcome::BandDead.kind(), K::BandDead);
        assert_eq!(RingOutcome::Failed { failure: String::new() }.kind(), K::Failed);
        assert_eq!(RingOutcome::DroppedBackpressure.kind(), K::DroppedBackpressure);
        assert_eq!(RingOutcome::DroppedLostFrames.kind(), K::DroppedLostFrames);
        assert_eq!(
            RingOutcome::DroppedStorageError { diagnostic: String::new() }.kind(),
            K::DroppedStorageError
        );
        assert_eq!(
            RingOutcome::Discarded { class: DiscardClassDto::FirstSlot }.kind(),
            K::Discarded
        );
    }

    #[test]
    fn band_source_and_sweep_shapes_are_pinned() {
        assert_eq!(
            serde_json::to_string(&BandSource::DefaultUnconfirmed).unwrap(),
            r#""default-unconfirmed""#
        );
        let s: SweepStatusDto =
            tuxlink_capture::state::Sweep::Active { band_idx: 2, dwell_progress: 5 }.into();
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"mode":"active","bandIdx":2,"dwellProgress":5}"#
        );
    }
}
```
(NOTE for the subagent: Phase C consumes the leaf state types by value and
in assertions — `ServiceAxis`, `BlockedReason`, `SlotPhase`, `Sweep`,
`HealthFlags`, `RingOutcomeKind` need `Debug + Clone + Copy + PartialEq`
derives. If any is missing in the Phase-A crate, add the derives to
`tuxlink-capture/src/state.rs` in this task; that leaf edit CAN be
red-greened locally with `cargo test -p tuxlink-capture --locked`. Same for
`tuxlink-capture/src/slot.rs`'s `GapReport`/`GapKind`/`DiscardClass`/
`DropClass` — `Debug + Clone + PartialEq` — consumed by `ReadBatch` and the
capture loop.)

- [ ] **Step 4: clock.rs**

```rust
//! NTP-sync probe (spec §Clock probe): `timedatectl show -p NTPSynchronized
//! --value`, bounded 2 s, kill on overrun. Daemon-agnostic (chrony and
//! timesyncd both drive the property); no D-Bus crate dependency.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockSync {
    Synced,
    Unsynced,
    /// Binary missing / timeout / unparseable. The flag is NOT set on
    /// Unknown — a false "decode unreliable" warning on every non-systemd
    /// system is worse than a missing warning on an exotic one.
    Unknown,
}

pub trait ClockProbe: Send + Sync {
    fn ntp_synchronized(&self) -> ClockSync;
}

pub struct TimedatectlProbe;

const PROBE_DEADLINE: Duration = Duration::from_secs(2);
const PROBE_POLL: Duration = Duration::from_millis(50);

impl ClockProbe for TimedatectlProbe {
    fn ntp_synchronized(&self) -> ClockSync {
        let mut child = match Command::new("timedatectl")
            .args(["show", "-p", "NTPSynchronized", "--value"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return ClockSync::Unknown,
        };
        let deadline = Instant::now() + PROBE_DEADLINE;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(PROBE_POLL),
                _ => {
                    // Overrun or wait error: kill + reap, report Unknown.
                    let _ = child.kill();
                    let _ = child.wait();
                    return ClockSync::Unknown;
                }
            }
        }
        let mut out = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            let _ = stdout.read_to_string(&mut out);
        }
        parse_ntp_value(&out)
    }
}

/// The parse contract, extracted as PRODUCTION code so the unit test drives
/// the real mapping rather than a test-body re-implementation.
pub(crate) fn parse_ntp_value(raw: &str) -> ClockSync {
    match raw.trim() {
        "yes" => ClockSync::Synced,
        "no" => ClockSync::Unsynced,
        _ => ClockSync::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drives the extracted production parse fn (the subprocess itself is
    /// environment-dependent and not unit-tested; parse + fallback are).
    #[test]
    fn parse_values_map_to_sync_states() {
        for (raw, want) in [
            ("yes", ClockSync::Synced),
            ("no", ClockSync::Unsynced),
            ("maybe", ClockSync::Unknown),
            ("", ClockSync::Unknown),
            (" yes\n", ClockSync::Synced),
        ] {
            assert_eq!(parse_ntp_value(raw), want, "raw {raw:?}");
        }
    }
}
```

- [ ] **Step 5: events.rs (trait only — names + Tauri sink land in Task 17)**

```rust
//! The event seam (spec §Events). The trait is defined here; the production
//! `TauriEventSink` + the event-name constants are wired in the commands
//! task, keeping this file test-consumable without a Tauri dependency in
//! unit tests.

use crate::ft8::records::{Ft8ListeningChange, SlotRecord};

/// Side-effect sink the service emits into (AprsState `EventSink`
/// precedent). Production: Tauri `AppHandle::emit`, fire-and-forget; tests:
/// a recording sink.
pub trait EventSink: Send + Sync {
    /// `ft8-listening:change` — axis/flags/phase/band/sweep summary, emitted
    /// on every change to any of them.
    fn emit_listening_change(&self, change: &Ft8ListeningChange);
    /// `ft8-decodes:slot` — one per slot boundary (including drops/discards).
    fn emit_slot(&self, record: &SlotRecord);
}
```

- [ ] **Step 6: alsa_source.rs (production; NO unit tests — hardware)**

```rust
//! The ONE ALSA touchpoint (spec §ALSA open + §ALSA read loop). Opens
//! `hw:<card_index>,0` — numeric live index, NOT `plughw:`, NOT `CARD=<id>`
//! (plug silently resamples, masking `blocked(unsupported-sample-rate)`;
//! id-based names collide on same-model codecs — see
//! `devices::ResolvedManagedDevice::alsa_hw`).
//!
//! CI-COMPILE-CHECKED ONLY: this file has no unit tests by design (it needs
//! real ALSA hardware); logic is kept minimal and every decision above the
//! errno mapping lives in the testable capture loop (service.rs).

use alsa::pcm::{Access, Format, HwParams, PCM};
use alsa::{Direction, ValueOr};

use super::traits::{process_mono_us, ReadBatch, SampleSource, SourceError};
use tuxlink_capture::slot::{GapKind, GapReport};

/// Open parameters (spec §ALSA open): S16_LE, exactly 48 000 Hz, mono
/// preferred / stereo-ch0 fallback, period 4 800 frames (100 ms), buffer 4
/// periods.
const RATE_HZ: u32 = 48_000;
const PERIOD_FRAMES: i64 = 4_800;
const BUFFER_FRAMES: i64 = PERIOD_FRAMES * 4;
/// 10 consecutive wait-timeouts (2 s of silent, non-erroring stream) — the
/// C-Media wedge class (spec §ALSA read loop).
const WEDGE_TIMEOUTS: u32 = 10;
const WAIT_MS: i32 = 200;

// Monotonic stamps come from `traits::process_mono_us` — the ONE process
// epoch (its doc pins why; no second OnceLock epoch may exist here).

pub struct AlsaSource {
    pcm: PCM,
    channels: u32,
    /// Interleaved scratch for the stereo-ch0 fallback path.
    stereo_buf: Vec<i16>,
    consecutive_wait_timeouts: u32,
    /// Set when an EPIPE recovery happened and the NEXT successful read must
    /// report the gap.
    pending_gap: Option<GapReport>,
}

/// errno → SourceError for the OPEN path.
fn map_open_err(e: &alsa::Error) -> SourceError {
    match e.errno() {
        libc::EBUSY => SourceError::Busy,
        libc::ENOENT | libc::ENODEV | libc::ENXIO => SourceError::Absent,
        _ => SourceError::Io(e.to_string()),
    }
}

impl AlsaSource {
    /// Open + negotiate on the hw device. Any parameter rejection →
    /// `UnsupportedFormat` carrying the ALSA diagnostic (the axis name is
    /// delta-pinned; the diagnostic distinguishes rate vs channel vs format).
    pub fn open(alsa_hw: &str) -> Result<Self, SourceError> {
        let pcm = PCM::new(alsa_hw, Direction::Capture, true /* nonblock */)
            .map_err(|e| map_open_err(&e))?;
        let channels = {
            let hwp = HwParams::any(&pcm).map_err(|e| SourceError::Io(e.to_string()))?;
            hwp.set_access(Access::RWInterleaved)
                .map_err(|e| SourceError::UnsupportedFormat(format!("access: {e}")))?;
            hwp.set_format(Format::s16())
                .map_err(|e| SourceError::UnsupportedFormat(format!("format S16_LE: {e}")))?;
            hwp.set_rate(RATE_HZ, ValueOr::Nearest)
                .map_err(|e| SourceError::UnsupportedFormat(format!("rate 48000: {e}")))?;
            // hw (no plug) may still land a neighbor rate via Nearest —
            // verify EXACT 48 000 (native only; no resampler path).
            let got = hwp.get_rate().map_err(|e| SourceError::Io(e.to_string()))?;
            if got != RATE_HZ {
                return Err(SourceError::UnsupportedFormat(format!(
                    "device native rate {got} != required 48000"
                )));
            }
            // Channels: 1 preferred; 2 with channel-0 extraction as fallback.
            let channels = if hwp.set_channels(1).is_ok() {
                1
            } else {
                hwp.set_channels(2)
                    .map_err(|e| SourceError::UnsupportedFormat(format!("channels 1|2: {e}")))?;
                2
            };
            hwp.set_period_size_near(PERIOD_FRAMES, ValueOr::Nearest)
                .map_err(|e| SourceError::UnsupportedFormat(format!("period: {e}")))?;
            hwp.set_buffer_size_near(BUFFER_FRAMES)
                .map_err(|e| SourceError::UnsupportedFormat(format!("buffer: {e}")))?;
            pcm.hw_params(&hwp).map_err(|e| SourceError::UnsupportedFormat(e.to_string()))?;
            channels
        };
        pcm.prepare().map_err(|e| SourceError::Io(e.to_string()))?;
        pcm.start().map_err(|e| SourceError::Io(e.to_string()))?;
        Ok(Self {
            pcm,
            channels,
            stereo_buf: Vec::new(),
            consecutive_wait_timeouts: 0,
            pending_gap: None,
        })
    }

    /// errno → SourceError for the READ path; EPIPE handled by the caller.
    fn map_read_err(&mut self, e: &alsa::Error) -> Option<SourceError> {
        match e.errno() {
            libc::EAGAIN => None, // nonblocking no-data: not an error
            libc::ESTRPIPE => {
                // Suspend: recover the PCM so the next read works, surface
                // Suspended ONCE (capture loop abandons the slot).
                let _ = self.pcm.prepare();
                Some(SourceError::Suspended)
            }
            libc::ENODEV | libc::EBADFD | libc::ENOENT => Some(SourceError::Absent),
            _ => Some(SourceError::Io(e.to_string())),
        }
    }
}

impl SampleSource for AlsaSource {
    fn read(&mut self, buf: &mut [i16]) -> Result<ReadBatch, SourceError> {
        // PCM::wait bounds the park (abort latency ≈ one timeout).
        match self.pcm.wait(Some(WAIT_MS)) {
            Ok(true) => self.consecutive_wait_timeouts = 0,
            Ok(false) => {
                self.consecutive_wait_timeouts += 1;
                if self.consecutive_wait_timeouts >= WEDGE_TIMEOUTS {
                    return Err(SourceError::Wedged);
                }
                return Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: self.pending_gap.take() });
            }
            Err(e) => {
                if e.errno() == libc::EPIPE {
                    // Overrun signaled via wait: recover; gap size comes from
                    // the assembler's monotonic counter, never from ALSA.
                    let _ = self.pcm.prepare();
                    let _ = self.pcm.start();
                    self.pending_gap = Some(GapReport { kind: GapKind::Overrun });
                    return Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: None });
                }
                if let Some(err) = self.map_read_err(&e) {
                    return Err(err);
                }
                return Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: self.pending_gap.take() });
            }
        }

        let io = self.pcm.io_i16().map_err(|e| SourceError::Io(e.to_string()))?;
        if self.channels == 1 {
            match io.readi(buf) {
                Ok(frames) => Ok(ReadBatch { frames, mono_ts_us: process_mono_us(), gap: self.pending_gap.take() }),
                Err(e) if e.errno() == libc::EPIPE => {
                    drop(io);
                    let _ = self.pcm.prepare();
                    let _ = self.pcm.start();
                    self.pending_gap = Some(GapReport { kind: GapKind::Overrun });
                    Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: None })
                }
                Err(e) => match self.map_read_err(&e) {
                    Some(err) => Err(err),
                    None => Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: self.pending_gap.take() }),
                },
            }
        } else {
            // Stereo: read interleaved, keep channel 0 (left).
            self.stereo_buf.resize(buf.len() * 2, 0);
            match io.readi(&mut self.stereo_buf) {
                Ok(frames) => {
                    for i in 0..frames.min(buf.len()) {
                        buf[i] = self.stereo_buf[i * 2];
                    }
                    Ok(ReadBatch { frames: frames.min(buf.len()), mono_ts_us: process_mono_us(), gap: self.pending_gap.take() })
                }
                Err(e) if e.errno() == libc::EPIPE => {
                    drop(io);
                    let _ = self.pcm.prepare();
                    let _ = self.pcm.start();
                    self.pending_gap = Some(GapReport { kind: GapKind::Overrun });
                    Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: None })
                }
                Err(e) => match self.map_read_err(&e) {
                    Some(err) => Err(err),
                    None => Ok(ReadBatch { frames: 0, mono_ts_us: process_mono_us(), gap: self.pending_gap.take() }),
                },
            }
        }
    }
}
```
**Subagent latitude on the alsa 0.9 API:** exact method names
(`io_i16`, `set_buffer_size_near`, `Error::errno` returning `i32` vs
`Errno`) must be checked against the crate docs at implementation time — CI
is the arbiter. The STRUCTURE above (hw-only open, exact-rate verify,
mono-then-stereo, wait+nonblocking readi, the errno table, wedge counter,
pending-gap handoff) is pinned by the spec and not negotiable. `libc` is
already a transitive dependency; if `libc::` constants are not directly
usable, add `libc = "0.2"` to `[dependencies]` (lock regen via
`cargo metadata`, same rule as Task 9).

- [ ] **Step 7: testutil.rs — fakes for the four traits**

```rust
//! Test fakes for the four testability seams (spec §Testing strategy:
//! "fakes for all four traits") plus the synthetic clock they share.
//! `#[cfg(test)]`-gated via the mod declaration in mod.rs.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use super::clock::{ClockProbe, ClockSync};
use super::events::EventSink;
use super::records::{Ft8ListeningChange, SlotRecord};
use super::traits::{DecodeEngine, ReadBatch, SampleSource, SourceError};
use tuxlink_capture::slot::GapReport;
use tuxlink_jt9::types::SlotOutcome;

/// Shared synthetic time. UTC and monotonic advance in lockstep; tests (and
/// the ScriptedSource) drive it — nothing reads the ambient clock.
#[derive(Default)]
pub struct SyntheticClock {
    utc_ms: AtomicU64,
    mono_us: AtomicU64,
}

impl SyntheticClock {
    pub fn new(start_utc_ms: u64) -> Arc<Self> {
        let c = Self::default();
        c.utc_ms.store(start_utc_ms, Ordering::SeqCst);
        Arc::new(c)
    }
    pub fn advance_ms(&self, ms: u64) {
        self.utc_ms.fetch_add(ms, Ordering::SeqCst);
        self.mono_us.fetch_add(ms * 1_000, Ordering::SeqCst);
    }
    /// An NTP step: UTC moves, monotonic does not.
    pub fn step_utc_ms(&self, delta_ms: i64) {
        if delta_ms >= 0 {
            self.utc_ms.fetch_add(delta_ms as u64, Ordering::SeqCst);
        } else {
            self.utc_ms.fetch_sub(delta_ms.unsigned_abs(), Ordering::SeqCst);
        }
    }
    pub fn utc_ms(&self) -> u64 {
        self.utc_ms.load(Ordering::SeqCst)
    }
    pub fn mono_us(&self) -> u64 {
        self.mono_us.load(Ordering::SeqCst)
    }
}

/// One scripted step for the ScriptedSource.
pub enum SourceStep {
    /// Deliver `frames` frames of `value`, advancing synthetic time by
    /// frames/48 ms.
    Frames { frames: usize, value: i16, gap: Option<GapReport> },
    /// Return this error once.
    Fail(SourceError),
    /// Returns an EMPTY batch after a 1 ms sleep — models a wait timeout
    /// WITHOUT wedging: the read always RETURNS, so the capture loop keeps
    /// polling its abort flag (contrast `Park`, added in T14, whose read
    /// blocks — the hung-USB class the abort flag cannot reach).
    Idle,
}

/// Scripted [`SampleSource`]: replays a step queue against the shared
/// synthetic clock.
pub struct ScriptedSource {
    pub steps: Arc<Mutex<VecDeque<SourceStep>>>,
    pub clock: Arc<SyntheticClock>,
}

impl SampleSource for ScriptedSource {
    fn read(&mut self, buf: &mut [i16]) -> Result<ReadBatch, SourceError> {
        let step = self.steps.lock().unwrap().pop_front();
        match step {
            Some(SourceStep::Frames { frames, value, gap }) => {
                let n = frames.min(buf.len());
                for s in buf.iter_mut().take(n) {
                    *s = value;
                }
                // 48 frames per ms at 48 kHz; scripts use multiples of 48.
                self.clock.advance_ms((n as u64) / 48);
                Ok(ReadBatch { frames: n, mono_ts_us: self.clock.mono_us(), gap })
            }
            Some(SourceStep::Fail(e)) => Err(e),
            Some(SourceStep::Idle) | None => {
                // Bounded park like PCM::wait's timeout arm.
                std::thread::sleep(Duration::from_millis(1));
                Ok(ReadBatch { frames: 0, mono_ts_us: self.clock.mono_us(), gap: None })
            }
        }
    }
}

pub struct FakeClock {
    pub sync: Mutex<ClockSync>,
    /// Probe-call counter — the supervisor-cadence test (T11) asserts one
    /// probe per 20-boundary window through this.
    pub probe_calls: AtomicU64,
}

impl FakeClock {
    pub fn new(sync: ClockSync) -> Arc<Self> {
        Arc::new(Self { sync: Mutex::new(sync), probe_calls: AtomicU64::new(0) })
    }
}

impl ClockProbe for FakeClock {
    fn ntp_synchronized(&self) -> ClockSync {
        self.probe_calls.fetch_add(1, Ordering::SeqCst);
        *self.sync.lock().unwrap()
    }
}

#[derive(Default)]
pub struct RecordingSink {
    pub listening_changes: Mutex<Vec<Ft8ListeningChange>>,
    pub slots: Mutex<Vec<SlotRecord>>,
}

impl EventSink for RecordingSink {
    fn emit_listening_change(&self, change: &Ft8ListeningChange) {
        self.listening_changes.lock().unwrap().push(change.clone());
    }
    fn emit_slot(&self, record: &SlotRecord) {
        self.slots.lock().unwrap().push(record.clone());
    }
}

/// Programmable [`DecodeEngine`]: a queue of outcomes (last one repeats), an
/// optional per-decode delay gate (for backpressure tests), and a prewarm
/// gate (for stop-during-starting tests).
pub struct FakeEngine {
    pub outcomes: Mutex<VecDeque<SlotOutcome>>,
    pub default_outcome: SlotOutcome,
    pub prewarm_result: Mutex<Result<(), String>>,
    /// (blocked?, condvar): while the bool is true, decode_slot (and prewarm
    /// when `gate_prewarm`) parks — tests flip it to release.
    pub gate: Arc<(Mutex<bool>, Condvar)>,
    pub gate_prewarm: bool,
    pub decodes_started: AtomicU64,
    pub decodes_finished: AtomicU64,
}

impl FakeEngine {
    pub fn band_dead() -> Arc<Self> {
        Arc::new(Self {
            outcomes: Mutex::new(VecDeque::new()),
            default_outcome: SlotOutcome::BandDead,
            prewarm_result: Mutex::new(Ok(())),
            gate: Arc::new((Mutex::new(false), Condvar::new())),
            gate_prewarm: false,
            decodes_started: AtomicU64::new(0),
            decodes_finished: AtomicU64::new(0),
        })
    }
    pub fn hold_gate(&self) {
        *self.gate.0.lock().unwrap() = true;
    }
    pub fn release_gate(&self) {
        *self.gate.0.lock().unwrap() = false;
        self.gate.1.notify_all();
    }
    fn wait_gate(&self) {
        let (lock, cv) = (&self.gate.0, &self.gate.1);
        let mut blocked = lock.lock().unwrap();
        while *blocked {
            let (g, _t) = cv.wait_timeout(blocked, Duration::from_millis(50)).unwrap();
            blocked = g;
        }
    }
}

impl DecodeEngine for FakeEngine {
    fn prewarm(&self) -> Result<(), String> {
        if self.gate_prewarm {
            self.wait_gate();
        }
        self.prewarm_result.lock().unwrap().clone()
    }
    fn decode_slot(&self, _wav: &Path, _slot_tmp: &Path, _slot_utc_ms: u64) -> SlotOutcome {
        self.decodes_started.fetch_add(1, Ordering::SeqCst);
        self.wait_gate();
        let out = self
            .outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| self.default_outcome.clone());
        self.decodes_finished.fetch_add(1, Ordering::SeqCst);
        out
    }
}

/// Self-tests: each fake is exercised minimally HERE so the T8–T10 batch
/// carries its own consumers (dead-code discipline at the Gate D push) and
/// a broken fake fails fast instead of surfacing as a confusing service-test
/// failure in T11+.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ft8::records::{BandSource, Ft8ListeningChange, RingOutcome, SlotRecord};

    #[test]
    fn synthetic_clock_advances_domains_in_lockstep_and_steps_utc_alone() {
        let c = SyntheticClock::new(30_000);
        c.advance_ms(1_500);
        assert_eq!(c.utc_ms(), 31_500);
        assert_eq!(c.mono_us(), 1_500_000);
        c.step_utc_ms(-2_000); // NTP step: UTC moves, monotonic does not
        assert_eq!(c.utc_ms(), 29_500);
        assert_eq!(c.mono_us(), 1_500_000);
    }

    #[test]
    fn scripted_source_replays_frames_fails_then_idles() {
        let clock = SyntheticClock::new(0);
        let steps = Arc::new(Mutex::new(VecDeque::from([
            SourceStep::Frames { frames: 48, value: 7, gap: None },
            SourceStep::Fail(SourceError::Busy),
        ])));
        let mut src = ScriptedSource { steps, clock: clock.clone() };
        let mut buf = vec![0i16; 96];
        let batch = src.read(&mut buf).unwrap();
        assert_eq!(batch.frames, 48);
        assert_eq!(buf[0], 7);
        assert_eq!(batch.mono_ts_us, 1_000, "48 frames = 1 ms at 48 kHz");
        assert_eq!(src.read(&mut buf), Err(SourceError::Busy));
        // An exhausted queue behaves as Idle: empty batch, clock untouched.
        let idle = src.read(&mut buf).unwrap();
        assert_eq!(idle.frames, 0);
        assert_eq!(clock.mono_us(), 1_000);
    }

    #[test]
    fn fake_clock_reports_its_sync_and_counts_probes() {
        let c = FakeClock::new(ClockSync::Unsynced);
        assert_eq!(c.ntp_synchronized(), ClockSync::Unsynced);
        *c.sync.lock().unwrap() = ClockSync::Synced;
        assert_eq!(c.ntp_synchronized(), ClockSync::Synced);
        assert_eq!(c.probe_calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn recording_sink_records_both_event_kinds() {
        let sink = RecordingSink::default();
        sink.emit_listening_change(&Ft8ListeningChange {
            service: tuxlink_capture::state::ServiceAxis::Stopped.into(),
            flags: tuxlink_capture::state::HealthFlags::default().into(),
            slot_phase: tuxlink_capture::state::SlotPhase::WaitingFirstSlot.into(),
            band: "20m".into(),
            dial_hz: 14_074_000,
            sweep: tuxlink_capture::state::Sweep::Inactive.into(),
        });
        sink.emit_slot(&SlotRecord {
            slot_utc_ms: 15_000,
            band: "20m".into(),
            dial_hz: 14_074_000,
            band_source: BandSource::DefaultUnconfirmed,
            band_label_confirmed_utc_ms: None,
            outcome: RingOutcome::BandDead,
            decodes: Vec::new(),
            partial_salvage: false,
            lost_frames: 0,
            boundary_skew_frames: 0,
            clip_fraction: 0.0,
            rms_dbfs: -60.0,
            dwell_slot_index: None,
        });
        assert_eq!(sink.listening_changes.lock().unwrap().len(), 1);
        assert_eq!(sink.slots.lock().unwrap().len(), 1);
    }

    #[test]
    fn fake_engine_pops_queued_outcomes_then_repeats_the_default() {
        let eng = FakeEngine::band_dead();
        assert_eq!(eng.prewarm(), Ok(()));
        eng.outcomes
            .lock()
            .unwrap()
            .push_back(SlotOutcome::Decoded(Vec::new()));
        let p = Path::new("unused.wav");
        assert!(matches!(eng.decode_slot(p, p, 0), SlotOutcome::Decoded(_)));
        assert!(matches!(eng.decode_slot(p, p, 1), SlotOutcome::BandDead));
        assert_eq!(eng.decodes_started.load(Ordering::SeqCst), 2);
        assert_eq!(eng.decodes_finished.load(Ordering::SeqCst), 2);
    }
}
```
(`FakePlatform` — the composite impure-probe fake — is added by Task 11
alongside the `Ft8Platform` trait it fakes.)

- [ ] **Step 8: [CI-side] verification** — workspace clippy + tests on the PR
run (the records.rs shape tests + clock parse test run there). Locally: if
Step 3's NOTE required leaf derives, run
`cargo test --manifest-path "$WT/src-tauri/Cargo.toml" -p tuxlink-capture --locked`
(expected: green).

- [ ] **Step 9: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/ft8 src-tauri/src/lib.rs src-tauri/tuxlink-capture
git commit -m "feat(ft8): src/ft8 skeleton — traits, wire records, fakes, ALSA source, clock probe (tuxlink-b026z.3 T10)"
```
(Drop `src-tauri/tuxlink-capture` from the add if Step 3's NOTE was a no-op.)

**Completion check:** all seven files exist; `pub mod ft8;` declared;
serde-shape tests pin every enum tag the frontend will see; alsa_source.rs
has zero `#[test]`s and zero decision logic beyond the errno table; fakes
cover all four traits.

---

**REVIEW GATE D (after Tasks 8–10):** review the foundation batch. Perspectives:
(1) **contract fidelity** — every type in the Phase C interface manifest
(ReadBatch, SourceError, ClockSync, the records DTOs) matches what Tasks
11–18 consume, and the `From` mirrors cover every leaf-crate variant (a new
`BlockedReason` variant must not be silently unmapped — the `match`es are
exhaustive, verify no `_` arm); (2) **serde-shape honesty** — the pinned JSON
strings in records.rs tests are what L3 will actually parse; kebab-case tags,
camelCase fields, no `rename_all` field-leak assumption
(serde_rename_all_enum_fields); (3) **ALSA-source minimalism** — no logic in
alsa_source.rs that could have lived in the testable capture loop; the errno
table matches spec §ALSA read loop line by line; `hw:` (never `plughw:`) is
the only open path; (4) **clippy-trap sweep** — grep the new files for
`.clone()` on subsequently-moved values, `collect()` feeding a loop,
`Result<_, SlotFailure>`-shaped large-err surfaces. Minimum three rounds;
persist findings to `dev/scratch/b026z.3-gate-D-findings.md` before
proceeding. Files under review: `src-tauri/src/winlink/ax25/devices.rs`,
`src-tauri/src/winlink/ax25/mod.rs`, `src-tauri/src/mcp_ports.rs`,
`src-tauri/src/config.rs`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`,
`src-tauri/src/lib.rs`, and all of `src-tauri/src/ft8/` as of T10 (`mod.rs`,
`traits.rs`, `records.rs`, `clock.rs`, `events.rs`, `alsa_source.rs`,
`testutil.rs`).

**Gate D push:** after this gate's P1/P2 fixes are committed, the parent
pushes the branch (Global Constraints §Push cadence). This push's CI run
executes T8–T10's **[CI-side]** steps; fix-forward on its findings before
starting Task 11.

---
### Task 11: service.rs part 1 — managed state, platform seam, supervisor + start sequence, snapshot, ring

**Files:**
- Create: `src-tauri/src/ft8/service.rs`
- Modify: `src-tauri/src/ft8/mod.rs` (`pub mod service;`)
- Modify: `src-tauri/src/ft8/traits.rs` (adds `Ft8Platform` + production impl)
- Modify: `src-tauri/src/ft8/testutil.rs` (adds `FakePlatform`)

**Interfaces:**
- Consumes: everything Task 10 produced; `ListenerMachine` + accessors
  (Task 6 manifest); `resolve_managed_device`, `enumerate_capture_devices`,
  `read_sys_snapshot`, `alsa_hw_name` (T8); `probe_device_busy`
  (`direwolf_probe.rs:345`); `ManagedModem::confirm_audio_device_released`
  (`process.rs:286`); `tuxlink_jt9::discover::{discover_jt9, Jt9Binary}`;
  `tuxlink_capture::bands`; `ManagedRig` (`tux-rig/src/managed.rs`);
  `ModemSession` + `ModemState` (`modem_status.rs`).
- Produces (consumed by Tasks 12–18):
```rust
// traits.rs additions
pub trait Ft8Platform: Send + Sync {
    fn discover_jt9(&self) -> Result<Jt9Binary, String>;
    fn resolve_device(&self, id: &StableAudioId) -> Option<ResolvedManagedDevice>;
    fn enumerate_capture(&self) -> Vec<AudioDeviceChoice>;
    fn probe_busy(&self, plughw: &str, card_index: u32) -> Result<(), String>;
    fn open_source(&self, alsa_hw: &str) -> Result<Box<dyn SampleSource>, SourceError>;
    fn confirm_released(&self, card_index: u32) -> bool;
    fn write_slot_wav(&self, path: &Path, samples: &[i16]) -> std::io::Result<()>;
    fn make_engine(&self, bin: &Jt9Binary, wisdom_dir: &Path) -> Arc<dyn DecodeEngine>;
    fn rig_configured(&self) -> bool;
    fn rig_read_dial(&self) -> Result<u64, String>;
    fn rig_tune(&self, dial_hz: u64) -> Result<(), String>;
    fn modem_resume_eligible(&self) -> bool;   // ModemState ∈ {Stopped, Error, SocketLost}
    fn wisdom_dir(&self) -> PathBuf;           // machine-wide (spec: NOT per-device)
    fn slot_dir_root(&self) -> PathBuf;        // tmpfs (spec §WAV writeout)
    fn utc_now_ms(&self) -> u64;
    fn mono_now_us(&self) -> u64;
    fn count_pipe_fds(&self) -> Option<usize>; // /proc/self/fd pipe: entries (b026z.8)
}
pub struct ProdPlatform { /* wisdom_dir, slot_root, modem: Arc<ModemSession> */ }
// service.rs
pub struct Ft8Deps { pub platform: Arc<dyn Ft8Platform>, pub clock: Arc<dyn ClockProbe>, pub sink: Arc<dyn EventSink> }
pub struct SharedHold; // latch_now(), clear(), is_latched() — 30 s lazy TTL
pub struct Ft8ListenerState;
impl Ft8ListenerState {
    pub fn new(deps: Ft8Deps, ft8_cfg: Ft8Config) -> Arc<Self>;
    pub fn start(self: &Arc<Self>) -> Result<(), String>;      // spawns/reruns the supervisor
    pub fn snapshot(&self) -> Ft8Snapshot;
    pub fn set_ft8_config(&self, cfg: Ft8Config);              // commands push updates
    pub fn hold(&self) -> Arc<SharedHold>;                     // arbiter shares it (T14)
    pub fn rig_lock(&self) -> Arc<Mutex<()>>;                  // arbiter serializes rig sessions
    pub(crate) fn execute_start_sequence(self: &Arc<Self>, resume: bool); // test-drivable
}
pub struct Ft8Snapshot; // spec §Snapshot, field-for-field
pub(crate) const RING_CAP: usize = 240;
```

**Pinned constants (verbatim in code):** supervisor tick 5 s; ring 240;
clock re-probe every 20 slot boundaries; pipe watermark every 100
boundaries, excess threshold > 16; hold-latch TTL 30 s.

**TDD note:** Step 4's tests (one per numbered start arrow) are authored
BEFORE Step 3's sequence implementation.

- [ ] **Step 1: `Ft8Platform` + `ProdPlatform` in traits.rs**

Append to `traits.rs` (imports consolidated at the top of the file):
```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::ft8::records::AudioDeviceChoice;
use crate::modem_status::{ModemSession, ModemState};
use crate::winlink::ax25::devices::{
    enumerate_capture_devices, read_sys_snapshot, resolve_managed_device, ResolvedManagedDevice,
    StableAudioId,
};
use tuxlink_jt9::discover::Jt9Binary;
use tuxlink_jt9::types::SLOT_DECODE_TIMEOUT_SECS;

/// The impure-probe seam: every filesystem/process/CAT touchpoint the
/// service needs, bundled so tests fake ONE object. Errors are `String`s
/// (result_large_err discipline). Production is [`ProdPlatform`]; the test
/// double is `testutil::FakePlatform`.
pub trait Ft8Platform: Send + Sync {
    fn discover_jt9(&self) -> Result<Jt9Binary, String>;
    /// Re-resolve the persisted identity against a FRESH snapshot — the card
    /// index can change on re-enumeration; never reuse a cached name
    /// (spec §Device loss).
    fn resolve_device(&self, id: &StableAudioId) -> Option<ResolvedManagedDevice>;
    fn enumerate_capture(&self) -> Vec<AudioDeviceChoice>;
    fn probe_busy(&self, plughw: &str, card_index: u32) -> Result<(), String>;
    fn open_source(&self, alsa_hw: &str) -> Result<Box<dyn SampleSource>, SourceError>;
    /// ADR-0015 release confirm against `/dev/snd/pcmC<card>D0c`.
    fn confirm_released(&self, card_index: u32) -> bool;
    fn write_slot_wav(&self, path: &std::path::Path, samples: &[i16]) -> std::io::Result<()>;
    fn make_engine(&self, bin: &Jt9Binary, wisdom_dir: &std::path::Path) -> Arc<dyn DecodeEngine>;
    fn rig_configured(&self) -> bool;
    /// One spawn-read-drop `ManagedRig` session (serial NEVER held while
    /// capturing). Caller serializes via the service's rig lock.
    fn rig_read_dial(&self) -> Result<u64, String>;
    /// One spawn-tune-drop session. Same serialization contract.
    fn rig_tune(&self, dial_hz: u64) -> Result<(), String>;
    /// Positive resume eligibility over ModemState: `Stopped | Error |
    /// SocketLost` (spec §Resume — `Idle` means ardopcf holds the card).
    fn modem_resume_eligible(&self) -> bool;
    fn wisdom_dir(&self) -> PathBuf;
    fn slot_dir_root(&self) -> PathBuf;
    fn utc_now_ms(&self) -> u64;
    fn mono_now_us(&self) -> u64;
    /// Pipe-type entries in /proc/self/fd (readlink → "pipe:[...]"), or None
    /// when /proc is unreadable. tuxlink-b026z.8 watermark.
    fn count_pipe_fds(&self) -> Option<usize>;
}

/// Production platform. Paths are injected at construction (lib.rs setup
/// resolves them from Tauri's path API) so this struct stays Tauri-free and
/// the setup wiring stays trivial.
pub struct ProdPlatform {
    pub wisdom_dir: PathBuf,
    pub slot_root: PathBuf,
    pub modem: Arc<ModemSession>,
}

// Monotonic stamps: `process_mono_us` (defined above in this file, T10) is
// THE process epoch — ProdPlatform and AlsaSource share it, because the
// assembler DIFFERENCES monotonic values across both producers.

impl Ft8Platform for ProdPlatform {
    fn discover_jt9(&self) -> Result<Jt9Binary, String> {
        tuxlink_jt9::discover::discover_jt9(None).map_err(|e| format!("{e:?}"))
    }
    fn resolve_device(&self, id: &StableAudioId) -> Option<ResolvedManagedDevice> {
        resolve_managed_device(id, &read_sys_snapshot())
    }
    fn enumerate_capture(&self) -> Vec<AudioDeviceChoice> {
        enumerate_capture_devices(&read_sys_snapshot())
            .into_iter()
            .map(|d| AudioDeviceChoice { human_name: d.human_name, stable_id: d.stable_id })
            .collect()
    }
    fn probe_busy(&self, plughw: &str, card_index: u32) -> Result<(), String> {
        crate::winlink::ax25::direwolf_probe::probe_device_busy(plughw, card_index)
    }
    fn open_source(&self, alsa_hw: &str) -> Result<Box<dyn SampleSource>, SourceError> {
        crate::ft8::alsa_source::AlsaSource::open(alsa_hw).map(|s| Box::new(s) as Box<dyn SampleSource>)
    }
    fn confirm_released(&self, card_index: u32) -> bool {
        crate::winlink::modem::process::ManagedModem::confirm_audio_device_released(
            std::path::Path::new(&format!("/dev/snd/pcmC{card_index}D0c")),
            Duration::from_secs(2),
        )
    }
    fn write_slot_wav(&self, path: &std::path::Path, samples: &[i16]) -> std::io::Result<()> {
        tuxlink_capture::wavwrite::write_slot_wav(path, samples)
    }
    fn make_engine(&self, bin: &Jt9Binary, wisdom_dir: &std::path::Path) -> Arc<dyn DecodeEngine> {
        Arc::new(Jt9Engine::new(Jt9Runner::new(
            bin.clone(),
            wisdom_dir.to_path_buf(),
            Duration::from_secs(SLOT_DECODE_TIMEOUT_SECS),
        )))
    }
    fn rig_configured(&self) -> bool {
        crate::config::read_config().map(|c| c.rig.is_configured()).unwrap_or(false)
    }
    fn rig_read_dial(&self) -> Result<u64, String> {
        let cfg = crate::config::read_config().map_err(|e| e.to_string())?;
        let rc = crate::modem_commands::rig_config_from(&cfg.rig)
            .ok_or_else(|| "rig not configured".to_string())?;
        let mut rig = tux_rig::ManagedRig::spawn(rc).map_err(|e| e.to_string())?;
        let status = rig.status().map_err(|e| e.to_string())?;
        Ok(status.freq_hz)
        // rig drops here → rigctld killed → serial released.
    }
    fn rig_tune(&self, dial_hz: u64) -> Result<(), String> {
        let cfg = crate::config::read_config().map_err(|e| e.to_string())?;
        let rc = crate::modem_commands::rig_config_from(&cfg.rig)
            .ok_or_else(|| "rig not configured".to_string())?;
        let mode = crate::modem_commands::rig_data_mode(&cfg.rig);
        let mut rig = tux_rig::ManagedRig::spawn(rc).map_err(|e| e.to_string())?;
        rig.tune(dial_hz, mode).map_err(|e| e.to_string())
        // rig drops here → serial released.
    }
    fn modem_resume_eligible(&self) -> bool {
        matches!(
            self.modem.status_snapshot().state,
            ModemState::Stopped | ModemState::Error | ModemState::SocketLost
        )
    }
    fn wisdom_dir(&self) -> PathBuf {
        self.wisdom_dir.clone()
    }
    fn slot_dir_root(&self) -> PathBuf {
        self.slot_root.clone()
    }
    fn utc_now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
    fn mono_now_us(&self) -> u64 {
        process_mono_us()
    }
    fn count_pipe_fds(&self) -> Option<usize> {
        let entries = std::fs::read_dir("/proc/self/fd").ok()?;
        Some(
            entries
                .flatten()
                .filter(|e| {
                    std::fs::read_link(e.path())
                        .map(|t| t.to_string_lossy().starts_with("pipe:["))
                        .unwrap_or(false)
                })
                .count(),
        )
    }
}
```
(NOTE: `rig_config_from` and `rig_data_mode` in `modem_commands.rs` are
`pub(crate)` — verify, and keep them `pub(crate)`; this is intra-crate use.)

- [ ] **Step 2: `FakePlatform` in testutil.rs**

```rust
use std::path::PathBuf;

use super::records::AudioDeviceChoice;
use super::traits::{DecodeEngine, Ft8Platform};
use crate::winlink::ax25::devices::{ResolvedManagedDevice, StableAudioId};
use tuxlink_jt9::discover::Jt9Binary;

/// Composite fake for the impure-probe seam. Every knob is a Mutex/Atomic so
/// tests reconfigure it mid-scenario (device replug, card busy, ENOSPC).
pub struct FakePlatform {
    pub jt9: Mutex<Result<Jt9Binary, String>>,
    pub resolved: Mutex<Option<ResolvedManagedDevice>>,
    pub capture_devices: Mutex<Vec<AudioDeviceChoice>>,
    pub busy: Mutex<Result<(), String>>,
    /// Factory: each open_source call builds a fresh ScriptedSource over the
    /// shared step queue + clock. `Err` steps here model open failures.
    pub open_results: Mutex<VecDeque<Result<(), super::traits::SourceError>>>,
    pub source_steps: Arc<Mutex<VecDeque<SourceStep>>>,
    pub wav_result: Mutex<Result<(), String>>, // Err("ENOSPC...") → io::Error
    pub engine: Mutex<Arc<dyn DecodeEngine>>,
    pub rig_configured: Mutex<bool>,
    pub rig_dial: Mutex<Result<u64, String>>,
    pub rig_tune_results: Mutex<VecDeque<Result<(), String>>>,
    pub tuned_to: Mutex<Vec<u64>>,
    pub modem_eligible: Mutex<bool>,
    pub released: Mutex<bool>,
    pub pipe_fds: Mutex<Option<usize>>,
    /// count_pipe_fds call counter — the supervisor-cadence test asserts
    /// one watermark read per 100-boundary window through this.
    pub pipe_fd_calls: AtomicU64,
    pub clock: Arc<SyntheticClock>,
    pub tmp: PathBuf, // pid-suffixed test root; wisdom + slot dirs under it
}

impl FakePlatform {
    /// Happy-path default: jt9 present, device resolves, card free, rig
    /// absent, band-dead engine, synthetic clock at a slot boundary.
    pub fn happy() -> Arc<Self> {
        let tmp = std::env::temp_dir().join(format!(
            "tuxlink-ft8-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        Arc::new(Self {
            jt9: Mutex::new(Ok(Jt9Binary {
                jt9_path: PathBuf::from("/usr/bin/jt9"),
                engine_version: "WSJT-X test 0.0".into(),
            })),
            resolved: Mutex::new(Some(ResolvedManagedDevice {
                alsa_plughw: "plughw:CARD=DRA,DEV=0".into(),
                alsa_hw: "hw:1,0".into(),
                card_index: 1,
            })),
            capture_devices: Mutex::new(vec![AudioDeviceChoice {
                human_name: "DRA-100 USB Audio".into(),
                stable_id: StableAudioId {
                    kind: crate::winlink::ax25::devices::StableIdKind::ByIdSymlink,
                    value: "usb-DRA-100-00".into(),
                },
            }]),
            busy: Mutex::new(Ok(())),
            open_results: Mutex::new(VecDeque::new()), // empty = always Ok
            source_steps: Arc::new(Mutex::new(VecDeque::new())),
            wav_result: Mutex::new(Ok(())),
            engine: Mutex::new(FakeEngine::band_dead() as Arc<dyn DecodeEngine>),
            rig_configured: Mutex::new(false),
            rig_dial: Mutex::new(Ok(14_074_000)),
            rig_tune_results: Mutex::new(VecDeque::new()), // empty = always Ok
            tuned_to: Mutex::new(Vec::new()),
            modem_eligible: Mutex::new(true),
            released: Mutex::new(true),
            pipe_fds: Mutex::new(Some(8)),
            pipe_fd_calls: AtomicU64::new(0),
            clock: SyntheticClock::new(1_760_000_000_000), // an arbitrary UTC ms epoch
            tmp,
        })
    }
}

impl Ft8Platform for FakePlatform {
    fn discover_jt9(&self) -> Result<Jt9Binary, String> {
        self.jt9.lock().unwrap().clone()
    }
    fn resolve_device(&self, _id: &StableAudioId) -> Option<ResolvedManagedDevice> {
        self.resolved.lock().unwrap().clone()
    }
    fn enumerate_capture(&self) -> Vec<AudioDeviceChoice> {
        self.capture_devices.lock().unwrap().clone()
    }
    fn probe_busy(&self, _plughw: &str, _card_index: u32) -> Result<(), String> {
        self.busy.lock().unwrap().clone()
    }
    fn open_source(
        &self,
        _alsa_hw: &str,
    ) -> Result<Box<dyn super::traits::SampleSource>, super::traits::SourceError> {
        if let Some(Err(e)) = self.open_results.lock().unwrap().pop_front() {
            return Err(e);
        }
        Ok(Box::new(ScriptedSource { steps: self.source_steps.clone(), clock: self.clock.clone() }))
    }
    fn confirm_released(&self, _card_index: u32) -> bool {
        *self.released.lock().unwrap()
    }
    fn write_slot_wav(&self, path: &std::path::Path, samples: &[i16]) -> std::io::Result<()> {
        match &*self.wav_result.lock().unwrap() {
            Ok(()) => tuxlink_capture::wavwrite::write_slot_wav(path, samples),
            // Error::other, NOT ErrorKind::StorageFull — see the MSRV note
            // below this block.
            Err(msg) => Err(std::io::Error::other(msg.clone())),
        }
    }
    fn make_engine(&self, _bin: &Jt9Binary, _wisdom: &std::path::Path) -> Arc<dyn DecodeEngine> {
        self.engine.lock().unwrap().clone()
    }
    fn rig_configured(&self) -> bool {
        *self.rig_configured.lock().unwrap()
    }
    fn rig_read_dial(&self) -> Result<u64, String> {
        self.rig_dial.lock().unwrap().clone()
    }
    fn rig_tune(&self, dial_hz: u64) -> Result<(), String> {
        let r = self.rig_tune_results.lock().unwrap().pop_front().unwrap_or(Ok(()));
        if r.is_ok() {
            self.tuned_to.lock().unwrap().push(dial_hz);
        }
        r
    }
    fn modem_resume_eligible(&self) -> bool {
        *self.modem_eligible.lock().unwrap()
    }
    fn wisdom_dir(&self) -> PathBuf {
        self.tmp.join("wisdom")
    }
    fn slot_dir_root(&self) -> PathBuf {
        self.tmp.join("slots")
    }
    fn utc_now_ms(&self) -> u64 {
        self.clock.utc_ms()
    }
    fn mono_now_us(&self) -> u64 {
        self.clock.mono_us()
    }
    fn count_pipe_fds(&self) -> Option<usize> {
        self.pipe_fd_calls.fetch_add(1, Ordering::SeqCst);
        *self.pipe_fds.lock().unwrap()
    }
}
```
(MSRV note — already applied in the block above: `io::ErrorKind::StorageFull`
is stable since 1.83, ABOVE MSRV 1.75, so the fake uses
`std::io::Error::other` — stable 1.74 — and the ENOSPC test matches on the
MESSAGE, not the kind.)

- [ ] **Step 3: service.rs — state struct, SharedHold, supervisor, start sequence, snapshot**

```rust
//! The FT8 listener service: managed state + the supervisor thread + the
//! start sequence (spec §Service structure, §Start sequence, §Lifecycle
//! ownership). Capture/decode thread bodies land in part 2 (Task 12); stop +
//! resume protocols in part 3 (Task 13).
//!
//! Lock discipline (spec §Lock discipline, pinned): thread handles live
//! OUTSIDE the state mutex and are take()n before any join; the state mutex
//! is leaf-level — never held across a join, an ALSA call, a rig session, or
//! an event emit; lock order arbiter > rig > state everywhere, each acquired
//! AT MOST ONCE per thread (the arbiter's rig_session takes only the arbiter
//! lock; rig-touching helpers own the rig lock themselves — T14).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::config::Ft8Config;
use crate::ft8::clock::{ClockProbe, ClockSync};
use crate::ft8::events::EventSink;
// Dead-code discipline (Global Constraints §Push cadence): imports, fields,
// and constants land in the task with their FIRST reader, so every commit in
// the batch is clippy-clean. T12 adds `DiscardClassDto` + `RingOutcome`
// here; T13 adds `devices::ResolvedManagedDevice` with the `resolved` field.
use crate::ft8::records::{
    AudioDeviceChoice, BandSource, Ft8ListeningChange, ServiceAxisDto, SlotRecord,
};
use crate::ft8::traits::{DecodeEngine, Ft8Platform, SourceError};
use serde::Serialize;
use tuxlink_capture::state::{BlockedReason, ListenerMachine, ServiceAxis};

pub(crate) const SUPERVISOR_TICK: Duration = Duration::from_secs(5);
pub(crate) const RING_CAP: usize = 240;
pub(crate) const CLOCK_REPROBE_BOUNDARIES: u64 = 20;
pub(crate) const PIPE_WATERMARK_BOUNDARIES: u64 = 100;
pub(crate) const PIPE_WATERMARK_EXCESS: usize = 16;
pub(crate) const HOLD_LATCH_TTL: Duration = Duration::from_secs(30);

/// The positive hold token (spec §Hold latch). A lazily-evaluated timestamp:
/// it needs no supervisor to expire. Shared between the service (start-
/// sequence step 6 consults it) and the arbiter (every pause latches it).
#[derive(Default)]
pub struct SharedHold {
    latched_at: Mutex<Option<Instant>>,
}

impl SharedHold {
    pub fn latch_now(&self) {
        *self.latched_at.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
    }
    pub fn clear(&self) {
        *self.latched_at.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
    /// TTL-aware: a latch older than 30 s reads clear (an aborted modem
    /// spawn must not wedge FT8) and is dropped on observation.
    pub fn is_latched(&self) -> bool {
        let mut g = self.latched_at.lock().unwrap_or_else(|p| p.into_inner());
        match *g {
            Some(t) if t.elapsed() < HOLD_LATCH_TTL => true,
            Some(_) => {
                *g = None;
                false
            }
            None => false,
        }
    }
}

/// Injected seams, bundled (clippy too_many_arguments discipline).
pub struct Ft8Deps {
    pub platform: Arc<dyn Ft8Platform>,
    pub clock: Arc<dyn ClockProbe>,
    pub sink: Arc<dyn EventSink>,
}

/// A completed slot handed from capture to decode (rendezvous channel).
pub(crate) struct SlotJob {
    pub slot_utc_ms: u64,
    pub dir: PathBuf,
    pub wav: PathBuf,
    pub lost_frames: u64,
    pub boundary_skew_frames: u64,
    pub clip_fraction: f32,
    pub rms_dbfs: f32,
}

/// Everything behind the leaf-level state mutex.
struct Inner {
    machine: ListenerMachine,
    ft8_cfg: Ft8Config,
    band: String,
    dial_hz: u64,
    band_source: BandSource,
    band_label_confirmed_utc_ms: Option<u64>,
    engine_version: Option<String>,
    last_slot_utc_ms: Option<u64>,
    last_failure: Option<String>,
    ring: VecDeque<SlotRecord>,
    // Fields land with their first READER (dead-code discipline):
    // `discard_next_slot` arrives in T12 (handle_completed_slot takes it);
    // `resolved` arrives in T13 (resume_conditions_met reads it).
}

/// Thread handles — OUTSIDE the state mutex (lock discipline).
#[derive(Default)]
struct Handles {
    supervisor: Option<JoinHandle<()>>,
    capture: Option<JoinHandle<()>>,
    decode: Option<JoinHandle<()>>,
}

pub struct Ft8ListenerState {
    inner: Mutex<Inner>,
    handles: Mutex<Handles>,
    /// The master SyncSender (spec §Lifecycle: lives here, cloned into each
    /// capture thread; only stop() drops it — decode's recv sees
    /// Disconnected, race-free).
    master_tx: Mutex<Option<SyncSender<SlotJob>>>,
    engine: Mutex<Option<Arc<dyn DecodeEngine>>>,
    pub(crate) platform: Arc<dyn Ft8Platform>,
    pub(crate) clock: Arc<dyn ClockProbe>,
    pub(crate) sink: Arc<dyn EventSink>,
    hold: Arc<SharedHold>,
    /// Serializes ALL FT8 rig sessions (start-labeling, band chip, sweep).
    /// The arbiter (T14) holds a clone: "the arbiter owns all rig sessions"
    /// is true by construction — one mutex, arbiter-visible.
    rig_lock: Arc<Mutex<()>>,
    stop_request: AtomicBool,
    yield_request: AtomicBool,
    start_rerun_request: AtomicBool,
    // `capture_abort` + `slot_seq` land in T12 with their first readers
    // (capture_loop / handle_completed_slot) — dead-code discipline.
    /// Capture-side slot-boundary counter (spec: cadences count BOUNDARIES,
    /// not decoded slots).
    slot_boundaries: AtomicU64,
    /// The supervisor's Thread handle for park_timeout interruption.
    supervisor_thread: Mutex<Option<std::thread::Thread>>,
    pipe_fd_baseline: Mutex<Option<usize>>,
}

impl Ft8ListenerState {
    pub fn new(deps: Ft8Deps, ft8_cfg: Ft8Config) -> Arc<Self> {
        let dial = tuxlink_capture::bands::dial_hz(&ft8_cfg.band).unwrap_or(14_074_000);
        let band = ft8_cfg.band.clone();
        Arc::new(Self {
            inner: Mutex::new(Inner {
                machine: ListenerMachine::new(),
                ft8_cfg,
                band,
                dial_hz: dial,
                band_source: BandSource::DefaultUnconfirmed,
                band_label_confirmed_utc_ms: None,
                engine_version: None,
                last_slot_utc_ms: None,
                last_failure: None,
                ring: VecDeque::with_capacity(RING_CAP),
            }),
            handles: Mutex::new(Handles::default()),
            master_tx: Mutex::new(None),
            engine: Mutex::new(None),
            platform: deps.platform,
            clock: deps.clock,
            sink: deps.sink,
            hold: Arc::new(SharedHold::default()),
            rig_lock: Arc::new(Mutex::new(())),
            stop_request: AtomicBool::new(false),
            yield_request: AtomicBool::new(false),
            start_rerun_request: AtomicBool::new(false),
            slot_boundaries: AtomicU64::new(0),
            supervisor_thread: Mutex::new(None),
            pipe_fd_baseline: Mutex::new(None),
        })
    }

    pub fn hold(&self) -> Arc<SharedHold> {
        self.hold.clone()
    }
    pub fn rig_lock(&self) -> Arc<Mutex<()>> {
        self.rig_lock.clone()
    }
    pub fn set_ft8_config(&self, cfg: Ft8Config) {
        let mut g = self.lock_inner();
        // A device change invalidates the constructed runner (spec: "no
        // runner reconstruction unless the device changed").
        if g.ft8_cfg.device != cfg.device {
            drop(g);
            *self.engine.lock().unwrap_or_else(|p| p.into_inner()) = None;
            g = self.lock_inner();
        }
        g.ft8_cfg = cfg;
    }

    fn lock_inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) fn axis(&self) -> ServiceAxis {
        self.lock_inner().machine.axis()
    }

    fn interrupted(&self) -> bool {
        self.stop_request.load(Ordering::SeqCst) || self.yield_request.load(Ordering::SeqCst)
    }

    /// Emit the current listening-change summary (call OUTSIDE the state
    /// lock — build the payload under the lock, emit after).
    pub(crate) fn emit_listening_change(&self) {
        let change = {
            let g = self.lock_inner();
            Ft8ListeningChange {
                service: ServiceAxisDto::from(g.machine.axis()),
                flags: g.machine.flags().into(),
                slot_phase: g.machine.slot_phase().into(),
                band: g.band.clone(),
                dial_hz: g.dial_hz,
                sweep: g.machine.sweep().into(),
            }
        };
        self.sink.emit_listening_change(&change);
    }

    fn set_blocked(&self, reason: BlockedReason, diagnostic: Option<String>) {
        {
            let mut g = self.lock_inner();
            g.machine.on_blocked(reason);
            if let Some(d) = diagnostic {
                g.last_failure = Some(d);
            }
        }
        self.emit_listening_change();
    }

    /// start / autostart entry (spec §Lifecycle table): spawns the
    /// supervisor from `stopped` ONLY; with a live supervisor it signals a
    /// sequence re-run instead. Callers run under spawn_blocking (T17).
    pub fn start(self: &Arc<Self>) -> Result<(), String> {
        if matches!(self.axis(), ServiceAxis::Blocked(BlockedReason::CaptureWedged)) {
            return Err(
                "the FT8 capture thread is wedged and may still hold the sound card; \
                 restart Tuxlink to recover"
                    .into(),
            );
        }
        let mut h = self.handles.lock().unwrap_or_else(|p| p.into_inner());
        let live = h.supervisor.as_ref().map(|s| !s.is_finished()).unwrap_or(false);
        if live {
            // Idempotent start: signal a sequence re-run.
            self.start_rerun_request.store(true, Ordering::SeqCst);
            self.unpark_supervisor();
            return Ok(());
        }
        // Reap a finished supervisor handle before respawn.
        if let Some(old) = h.supervisor.take() {
            let _ = old.join();
        }
        self.stop_request.store(false, Ordering::SeqCst);
        self.yield_request.store(false, Ordering::SeqCst);
        {
            let mut g = self.lock_inner();
            if !g.machine.on_start_requested() {
                return Err(format!("cannot start from {:?}", g.machine.axis()));
            }
        }
        let state = self.clone();
        let handle = std::thread::Builder::new()
            .name("ft8-supervisor".into())
            .spawn(move || supervisor_loop(state))
            .map_err(|e| format!("spawn ft8-supervisor: {e}"))?;
        *self.supervisor_thread.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(handle.thread().clone());
        h.supervisor = Some(handle);
        drop(h);
        self.emit_listening_change();
        Ok(())
    }

    pub(crate) fn unpark_supervisor(&self) {
        if let Some(t) = self
            .supervisor_thread
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
        {
            t.unpark();
        }
    }

    // ---- start sequence (spec §Start sequence; executed BY the supervisor;
    // pub(crate) so unit tests drive it synchronously) -----------------------

    /// Steps 1–8 (fresh start) or 1–7 + 8′ capture-only (resume /
    /// device-absent recovery). A yield/stop request is checked between
    /// every step; the flags never write the axis (pause already did).
    pub(crate) fn execute_start_sequence(self: &Arc<Self>, resume: bool) {
        if !resume {
            // Stale slot-dir sweep from crashed runs (spec §WAV writeout).
            self.sweep_stale_slot_dirs();
        }
        {
            let mut g = self.lock_inner();
            match g.machine.axis() {
                ServiceAxis::Starting => {}
                // Resume re-enters via on_resume (yielded → starting; k
                // reset; sweep re-arm).
                ServiceAxis::Yielded => g.machine.on_resume(),
                // Blocked re-entry (set_device retrigger, device-absent
                // retry) MUST go through on_start_requested — T6 permits
                // Blocked→Starting (+ sweep re-arm) there, while on_resume
                // is a no-op outside Yielded. Without this, the sequence
                // runs on a STALE blocked axis and a mid-sequence pause
                // (step 6 busy) is silently swallowed (on_pause from
                // blocked leaves the axis untouched).
                ServiceAxis::Blocked(r) if r != BlockedReason::CaptureWedged => {
                    let _ = g.machine.on_start_requested();
                }
                // Wedged / other axes: leave the machine alone (start()
                // already refuses wedged; defensive no-op here).
                _ => {}
            }
        }
        self.emit_listening_change();

        // Step 1: discover jt9 (start + resume — the delta's pinned probe
        // timing keeps discovery at exactly these moments).
        let bin = match self.platform.discover_jt9() {
            Ok(b) => b,
            Err(e) => return self.set_blocked(BlockedReason::WsjtxAbsent, Some(e)),
        };
        {
            let mut g = self.lock_inner();
            g.engine_version = Some(bin.engine_version.clone());
        }
        // No stored Jt9Binary: resume re-discovers (step 1 runs on both
        // paths) and make_engine consumes the fresh local — a cached copy
        // would be an unread field.
        if self.interrupted() {
            return;
        }

        // Step 2: resolve device.
        let stable_id = { self.lock_inner().ft8_cfg.device.clone() };
        let Some(stable_id) = stable_id else {
            return self.set_blocked(BlockedReason::NeedsDeviceSelection, None);
        };
        let Some(resolved) = self.platform.resolve_device(&stable_id) else {
            return self.set_blocked(
                BlockedReason::DeviceAbsent,
                Some(format!("configured device {:?} not found", stable_id.value)),
            );
        };
        // T13 adds `Inner.resolved` (+ the store here) with its first
        // reader, resume_conditions_met — dead-code discipline.
        if self.interrupted() {
            return;
        }

        // Step 3: clock probe → flag.
        let sync = self.clock.ntp_synchronized();
        {
            let mut g = self.lock_inner();
            g.machine.set_clock_unsynced(matches!(sync, ClockSync::Unsynced));
        }
        if matches!(sync, ClockSync::Unknown) {
            tracing::info!(target: "tuxlink::ft8", "clock sync unverifiable (timedatectl absent/unparseable)");
        }
        if self.interrupted() {
            return;
        }

        // Step 4: wisdom dir + prewarm — once per runner construction,
        // BEFORE any PCM is held. Skipped on resume (runner survives).
        let need_engine = self.engine.lock().unwrap_or_else(|p| p.into_inner()).is_none();
        if need_engine {
            let wisdom = self.platform.wisdom_dir();
            if let Err(e) = std::fs::create_dir_all(&wisdom) {
                tracing::warn!(target: "tuxlink::ft8", "wisdom dir create failed: {e} — proceeding (costs first-slot planning time)");
            }
            let engine = self.platform.make_engine(&bin, &wisdom);
            match engine.prewarm() {
                Ok(()) => {}
                Err(e) if e.contains("SpawnFailed") || e.contains("not found") => {
                    return self.set_blocked(BlockedReason::WsjtxAbsent, Some(e));
                }
                Err(e) => {
                    // A failed prewarm costs ~1.7 s planning on the first
                    // slots; it does not block listening (spec step 4).
                    tracing::warn!(target: "tuxlink::ft8", "jt9 prewarm failed (non-fatal): {e}");
                }
            }
            *self.engine.lock().unwrap_or_else(|p| p.into_inner()) = Some(engine);
        }
        if self.interrupted() {
            return;
        }

        // Step 5: CAT presence → flag / start-labeling rig session.
        if self.platform.rig_configured() {
            {
                self.lock_inner().machine.set_cat_fixed_band(false);
            }
            self.start_rig_labeling();
        } else {
            let mut g = self.lock_inner();
            g.machine.set_cat_fixed_band(true);
            // cat-absent: the snapshot instructs the dial for the chip band.
            g.dial_hz =
                tuxlink_capture::bands::dial_hz(&g.ft8_cfg.band).unwrap_or(g.dial_hz);
            g.band = g.ft8_cfg.band.clone();
        }
        if self.interrupted() {
            return;
        }

        // Step 6: busy probe — the hold latch is consulted here too (a fresh
        // start inside a pause-to-modem-open window must not steal the card).
        let busy = self.hold.is_latched()
            || self
                .platform
                .probe_busy(&resolved.alsa_plughw, resolved.card_index)
                .is_err();
        if busy {
            {
                self.lock_inner().machine.on_pause();
            }
            self.emit_listening_change();
            return;
        }
        if self.interrupted() {
            return;
        }

        // Step 7: ALSA open (hw:).
        let source = match self.platform.open_source(&resolved.alsa_hw) {
            Ok(s) => s,
            Err(SourceError::Busy) => {
                {
                    self.lock_inner().machine.on_pause();
                }
                self.emit_listening_change();
                return;
            }
            Err(SourceError::Absent) | Err(SourceError::Wedged) => {
                return self.set_blocked(BlockedReason::DeviceAbsent, None);
            }
            Err(SourceError::UnsupportedFormat(d)) => {
                return self.set_blocked(BlockedReason::UnsupportedSampleRate, Some(d));
            }
            Err(e) => {
                return self.set_blocked(BlockedReason::DeviceAbsent, Some(format!("{e:?}")));
            }
        };
        if self.interrupted() {
            // Past step 7 the supervisor holds the PCM: drop it BEFORE
            // abandoning the sequence (spec §Arbitration, starting case).
            drop(source);
            return;
        }

        // Step 8 / 8′: spawn workers → listening.
        self.spawn_workers(source, resume);
        {
            let mut g = self.lock_inner();
            g.machine.on_listening();
            // Sweep (re-)arms at start/resume when enabled + CAT (T16 wires
            // the dwell scheduler; arming is part of entering listening).
            if g.ft8_cfg.sweep.enabled && self.platform.rig_configured() {
                g.machine.sweep_activate();
            }
        }
        self.emit_listening_change();
    }

    /// Step-5 helper: one rig session — read dial, label band
    /// (nearest table entry within ±3 kHz, else "unknown"), tune to the
    /// configured band's dial if it differs, drop the session.
    ///
    /// Lock architecture (pinned): this helper OWNS the rig-lock
    /// acquisition; `Ft8Arbiter::rig_session` (T14) takes ONLY the arbiter
    /// lock and never the rig lock — lock order arbiter > rig > state, each
    /// acquired at most once per thread. T14 routes the step-5 call through
    /// `rig_session` (the arbiter cannot exist at this task's commit, so
    /// that one-match routing edit lands in T14); until then the rig lock
    /// alone serializes FT8 rig sessions against each other.
    fn start_rig_labeling(self: &Arc<Self>) {
        let _rig = self.rig_lock.lock().unwrap_or_else(|p| p.into_inner());
        let configured_dial = {
            let g = self.lock_inner();
            tuxlink_capture::bands::dial_hz(&g.ft8_cfg.band)
        };
        match self.platform.rig_read_dial() {
            Ok(dial) => {
                let label = nearest_band(dial);
                let now = self.platform.utc_now_ms();
                let mut tune_target = None;
                {
                    let mut g = self.lock_inner();
                    match label {
                        Some((band, table_dial)) => {
                            g.band = band.to_string();
                            g.dial_hz = table_dial;
                        }
                        None => {
                            g.band = "unknown".into();
                            g.dial_hz = dial;
                        }
                    }
                    g.band_source = BandSource::CatConfirmed;
                    g.band_label_confirmed_utc_ms = Some(now);
                    if let Some(cfg_dial) = configured_dial {
                        if g.dial_hz != cfg_dial {
                            tune_target = Some((g.ft8_cfg.band.clone(), cfg_dial));
                        }
                    }
                }
                if let Some((band, cfg_dial)) = tune_target {
                    // Starting the listener is the consenting action (RX-only).
                    match self.platform.rig_tune(cfg_dial) {
                        Ok(()) => {
                            let mut g = self.lock_inner();
                            g.band = band;
                            g.dial_hz = cfg_dial;
                            g.band_source = BandSource::CatConfirmed;
                            g.band_label_confirmed_utc_ms = Some(self.platform.utc_now_ms());
                        }
                        Err(e) => {
                            // Partial tune ⇒ dial position unknown (T16 test
                            // pins this downgrade).
                            let mut g = self.lock_inner();
                            g.band_source = BandSource::DefaultUnconfirmed;
                            g.band_label_confirmed_utc_ms = None;
                            g.last_failure = Some(format!("start QSY failed: {e}"));
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(target: "tuxlink::ft8", "start-labeling dial read failed: {e}");
                let mut g = self.lock_inner();
                g.band_source = BandSource::DefaultUnconfirmed;
                g.band_label_confirmed_utc_ms = None;
            }
        }
    }

    fn sweep_stale_slot_dirs(&self) {
        let root = self.platform.slot_dir_root();
        let Ok(entries) = std::fs::read_dir(&root) else { return };
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with("slot-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }

    /// Placeholder in part 1; Task 12 replaces the body with the real
    /// capture + decode spawns. Kept compiling so this task's tests (which
    /// stop at the axis transition) run in CI.
    fn spawn_workers(self: &Arc<Self>, source: Box<dyn crate::ft8::traits::SampleSource>, resume: bool) {
        let _ = (source, resume); // Task 12 wires the threads.
    }

    // ---- snapshot -----------------------------------------------------------

    pub fn snapshot(&self) -> Ft8Snapshot {
        let g = self.lock_inner();
        let axis = g.machine.axis();
        // §Device selection is the ONE rule: devices embedded when device is
        // unset OR blocked on device-absent/needs-device-selection.
        let wants_devices = g.ft8_cfg.device.is_none()
            || matches!(
                axis,
                ServiceAxis::Blocked(BlockedReason::DeviceAbsent)
                    | ServiceAxis::Blocked(BlockedReason::NeedsDeviceSelection)
            );
        let ring_tail: Vec<SlotRecord> = g.ring.iter().rev().take(40).rev().cloned().collect();
        let snap = Ft8Snapshot {
            service: axis.into(),
            flags: g.machine.flags().into(),
            slot_phase: g.machine.slot_phase().into(),
            band: g.band.clone(),
            dial_hz: g.dial_hz,
            band_source: g.band_source,
            band_label_confirmed_utc_ms: g.band_label_confirmed_utc_ms,
            sweep: g.machine.sweep().into(),
            engine_version: g.engine_version.clone(),
            n_consecutive: g.machine.n_consecutive(),
            k_consecutive: g.machine.k_consecutive(),
            last_slot_utc_ms: g.last_slot_utc_ms,
            last_failure: g.last_failure.clone(),
            available_devices: None,
            ring_tail,
        };
        drop(g); // never hold the state lock across the enumeration I/O
        Ft8Snapshot {
            available_devices: wants_devices.then(|| self.platform.enumerate_capture()),
            ..snap
        }
    }
}

/// Nearest FT8 band within ±3 kHz of a dial reading (spec §Hold-band).
fn nearest_band(dial_hz: u64) -> Option<(&'static str, u64)> {
    tuxlink_capture::bands::BANDS
        .iter()
        .find(|(_, hz)| dial_hz.abs_diff(*hz) <= 3_000)
        .map(|&(b, hz)| (b, hz))
}

/// spec §Snapshot, field-for-field — the L3/L4 contract.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ft8Snapshot {
    pub service: ServiceAxisDto,
    pub flags: crate::ft8::records::HealthFlagsDto,
    pub slot_phase: crate::ft8::records::SlotPhaseDto,
    pub band: String,
    pub dial_hz: u64,
    pub band_source: BandSource,
    pub band_label_confirmed_utc_ms: Option<u64>,
    pub sweep: crate::ft8::records::SweepStatusDto,
    pub engine_version: Option<String>,
    pub n_consecutive: u8,
    pub k_consecutive: u8,
    pub last_slot_utc_ms: Option<u64>,
    pub last_failure: Option<String>,
    pub available_devices: Option<Vec<AudioDeviceChoice>>,
    pub ring_tail: Vec<SlotRecord>,
}

// ---- supervisor -------------------------------------------------------------

/// The service's owner and the FIRST thread spawned (spec §Threads). It
/// executes the start sequence, then ticks every 5 s: yielded-resume poll +
/// device-absent retry (T13), clock re-probe every 20 slot boundaries,
/// pipe-fd watermark every 100 (b026z.8), sweep dwell bookkeeping (T16),
/// hold-latch TTL (lazy — nothing to do here). It outlives every blocked
/// state; only stop() ends it.
fn supervisor_loop(state: Arc<Ft8ListenerState>) {
    {
        let mut base = state.pipe_fd_baseline.lock().unwrap_or_else(|p| p.into_inner());
        if base.is_none() {
            *base = state.platform.count_pipe_fds();
        }
    }
    state.execute_start_sequence(false);
    let mut last_clock_probe = 0u64;
    let mut last_watermark = 0u64;
    loop {
        std::thread::park_timeout(SUPERVISOR_TICK);
        if state.stop_request.load(Ordering::SeqCst) {
            return;
        }
        if state.start_rerun_request.swap(false, Ordering::SeqCst) {
            state.execute_start_sequence(false);
            continue;
        }
        match state.axis() {
            ServiceAxis::Yielded => state.tick_yielded(),           // T13
            ServiceAxis::Blocked(BlockedReason::DeviceAbsent) => state.tick_device_absent(), // T13
            ServiceAxis::Listening => {
                state.tick_listening(&mut last_clock_probe, &mut last_watermark)
            }
            _ => {}
        }
    }
}

impl Ft8ListenerState {
    /// T13 fills these in; part-1 stubs keep the supervisor compiling.
    pub(crate) fn tick_yielded(self: &Arc<Self>) {}
    pub(crate) fn tick_device_absent(self: &Arc<Self>) {}

    /// The `listening`-axis supervisor tick body, extracted so the cadence
    /// test drives it directly (no 5 s parks): clock re-probe every 20 slot
    /// boundaries, pipe-fd watermark every 100 — cadences count BOUNDARIES
    /// via the capture-side atomic, never decoded slots.
    pub(crate) fn tick_listening(
        self: &Arc<Self>,
        last_clock_probe: &mut u64,
        last_watermark: &mut u64,
    ) {
        let boundaries = self.slot_boundaries.load(Ordering::SeqCst);
        if boundaries.saturating_sub(*last_clock_probe) >= CLOCK_REPROBE_BOUNDARIES {
            *last_clock_probe = boundaries;
            let sync = self.clock.ntp_synchronized();
            let changed = {
                let mut g = self.lock_inner();
                let before = g.machine.flags().clock_unsynced;
                g.machine
                    .set_clock_unsynced(matches!(sync, ClockSync::Unsynced));
                before != g.machine.flags().clock_unsynced
            };
            if changed {
                self.emit_listening_change();
            }
        }
        if boundaries.saturating_sub(*last_watermark) >= PIPE_WATERMARK_BOUNDARIES {
            *last_watermark = boundaries;
            self.check_pipe_watermark();
        }
        self.sweep_tick_stub(); // T16 swaps this for crate::ft8::sweep::tick(self)
    }

    /// Replaced by sweep::tick in Task 16 (which deletes this stub).
    pub(crate) fn sweep_tick_stub(self: &Arc<Self>) {}

    /// Returns whether the watermark tripped (testable seam — the spec's
    /// named "pipe-fd watermark trip (fake /proc reader)" test drives this
    /// return value; the caller logs).
    pub(crate) fn check_pipe_watermark(&self) -> bool {
        let (Some(base), Some(now)) = (
            *self.pipe_fd_baseline.lock().unwrap_or_else(|p| p.into_inner()),
            self.platform.count_pipe_fds(),
        ) else {
            return false;
        };
        let tripped = now > base + PIPE_WATERMARK_EXCESS;
        if tripped {
            tracing::warn!(
                target: "tuxlink::ft8",
                baseline = base,
                current = now,
                "pipe-fd watermark exceeded — possible jt9 grandchild pipe-holder leak (tuxlink-b026z.8)"
            );
        }
        tripped
    }
}
```
Also add `pub mod service;` to `mod.rs`. Do NOT create an empty sweep.rs in
this task: `sweep_tick_stub` (in the block above) is the placeholder; Task
16 swaps the `tick_listening` call site to `crate::ft8::sweep::tick(self)`
and deletes the stub method.

- [ ] **Step 4: Tests — one per numbered start arrow + autostart + snapshot completeness**

Append to service.rs:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Ft8Config;
    use crate::ft8::records::{BlockedReasonDto, ServiceAxisDto};
    use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink};
    use crate::winlink::ax25::devices::{StableAudioId, StableIdKind};
    use tuxlink_capture::state::{BlockedReason, ServiceAxis};

    fn test_state(platform: Arc<FakePlatform>, cfg: Ft8Config) -> Arc<Ft8ListenerState> {
        Ft8ListenerState::new(
            Ft8Deps {
                platform,
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: Arc::new(RecordingSink::default()),
            },
            cfg,
        )
    }

    fn cfg_with_device() -> Ft8Config {
        let mut c = Ft8Config::default();
        c.enabled = true;
        c.device = Some(StableAudioId {
            kind: StableIdKind::ByIdSymlink,
            value: "usb-DRA-100-00".into(),
        });
        c
    }

    fn run_sequence(state: &Arc<Ft8ListenerState>) {
        {
            state.lock_inner().machine.on_start_requested();
        }
        state.execute_start_sequence(false);
    }

    // Arrow 1: jt9 absent → blocked(wsjtx-absent); the snapshot still
    // carries available_devices when device is ALSO unset (§Device
    // selection's one rule — both first-contact blockers in one visit).
    #[test]
    fn arrow1_jt9_absent_blocks_wsjtx_absent_and_still_offers_devices() {
        let p = FakePlatform::happy();
        *p.jt9.lock().unwrap() = Err("NotOnPath".into());
        let state = test_state(p, Ft8Config::default()); // device: None
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::WsjtxAbsent));
        let snap = state.snapshot();
        let devs = snap.available_devices.expect("picker must render while blocked on wsjtx");
        assert_eq!(devs.len(), 1);
        let _ = std::fs::remove_dir_all(&state.platform_tmp_for_test());
    }

    // Arrow 2a: device None → needs-device-selection.
    #[test]
    fn arrow2_no_device_blocks_needs_device_selection() {
        let p = FakePlatform::happy();
        let state = test_state(p, Ft8Config::default());
        run_sequence(&state);
        assert_eq!(
            state.axis(),
            ServiceAxis::Blocked(BlockedReason::NeedsDeviceSelection)
        );
    }

    // Arrow 2b: persisted-but-unresolvable → device-absent (supervisor-
    // retried; the retry itself is T13's test).
    #[test]
    fn arrow2_unresolvable_device_blocks_device_absent() {
        let p = FakePlatform::happy();
        *p.resolved.lock().unwrap() = None;
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::DeviceAbsent));
        // Stale-device snapshot ALSO offers the picker (§Device selection).
        assert!(state.snapshot().available_devices.is_some());
    }

    // Arrow 3: clock probe sets the flag; Unknown does NOT.
    #[test]
    fn arrow3_clock_probe_drives_the_flag() {
        for (sync, want) in [
            (crate::ft8::clock::ClockSync::Unsynced, true),
            (crate::ft8::clock::ClockSync::Synced, false),
            (crate::ft8::clock::ClockSync::Unknown, false),
        ] {
            let p = FakePlatform::happy();
            let state = Ft8ListenerState::new(
                Ft8Deps {
                    platform: p,
                    clock: FakeClock::new(sync),
                    sink: Arc::new(RecordingSink::default()),
                },
                cfg_with_device(),
            );
            run_sequence(&state);
            assert_eq!(state.snapshot().flags.clock_unsynced, want, "{sync:?}");
        }
    }

    // Arrow 4: prewarm spawn-class failure → wsjtx-absent; any other
    // prewarm failure proceeds to listening.
    #[test]
    fn arrow4_prewarm_failure_classes() {
        use crate::ft8::testutil::FakeEngine;
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead();
        *eng.prewarm_result.lock().unwrap() =
            Err("SpawnFailed(\"No such file\")".into());
        *p.engine.lock().unwrap() = eng;
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::WsjtxAbsent));

        let p2 = FakePlatform::happy();
        let eng2 = FakeEngine::band_dead();
        *eng2.prewarm_result.lock().unwrap() = Err("Timeout".into());
        *p2.engine.lock().unwrap() = eng2;
        let state2 = test_state(p2, cfg_with_device());
        run_sequence(&state2);
        assert_eq!(state2.axis(), ServiceAxis::Listening, "non-spawn prewarm failure proceeds");
    }

    // Arrow 5: CAT absent → cat-fixed-band + instructed dial; CAT present →
    // start-labeling (cat-confirmed) + tune-if-differs.
    #[test]
    fn arrow5_cat_presence_labels_or_flags() {
        // Absent.
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        let snap = state.snapshot();
        assert!(snap.flags.cat_fixed_band);
        assert_eq!(snap.band, "20m");
        assert_eq!(snap.dial_hz, 14_074_000, "instructed dial for the chip band");
        assert_eq!(snap.band_source, crate::ft8::records::BandSource::DefaultUnconfirmed);

        // Present, radio on 40m, configured chip 20m → labeled then retuned.
        let p2 = FakePlatform::happy();
        *p2.rig_configured.lock().unwrap() = true;
        *p2.rig_dial.lock().unwrap() = Ok(7_074_000);
        let state2 = test_state(p2.clone(), cfg_with_device());
        run_sequence(&state2);
        let snap2 = state2.snapshot();
        assert!(!snap2.flags.cat_fixed_band);
        assert_eq!(snap2.band_source, crate::ft8::records::BandSource::CatConfirmed);
        assert!(snap2.band_label_confirmed_utc_ms.is_some());
        assert_eq!(*p2.tuned_to.lock().unwrap(), vec![14_074_000], "tuned to the configured band");
        assert_eq!(snap2.band, "20m");
    }

    // Arrow 6: busy probe busy → yielded; hold latch latched → treated as
    // busy even when the probe reads free.
    #[test]
    fn arrow6_busy_or_latched_yields() {
        let p = FakePlatform::happy();
        *p.busy.lock().unwrap() = Err("plughw:CARD=DRA,DEV=0 is in use".into());
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Yielded);

        let p2 = FakePlatform::happy();
        let state2 = test_state(p2, cfg_with_device());
        state2.hold().latch_now();
        run_sequence(&state2);
        assert_eq!(state2.axis(), ServiceAxis::Yielded, "latched hold is treated as busy");
    }

    // Arrow 7: open errors map EBUSY→yielded, absent→device-absent,
    // param→unsupported-sample-rate (with diagnostic).
    #[test]
    fn arrow7_open_error_mapping() {
        use crate::ft8::traits::SourceError;
        let cases = [
            (SourceError::Busy, None),
            (SourceError::Absent, Some(BlockedReason::DeviceAbsent)),
            (
                SourceError::UnsupportedFormat("rate 44100 only".into()),
                Some(BlockedReason::UnsupportedSampleRate),
            ),
        ];
        for (err, want_block) in cases {
            let p = FakePlatform::happy();
            p.open_results.lock().unwrap().push_back(Err(err.clone()));
            let state = test_state(p, cfg_with_device());
            run_sequence(&state);
            match want_block {
                None => assert_eq!(state.axis(), ServiceAxis::Yielded, "{err:?}"),
                Some(b) => assert_eq!(state.axis(), ServiceAxis::Blocked(b), "{err:?}"),
            }
        }
        // The diagnostic surfaces.
        let p = FakePlatform::happy();
        p.open_results
            .lock()
            .unwrap()
            .push_back(Err(SourceError::UnsupportedFormat("rate 44100 only".into())));
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.snapshot().last_failure.as_deref(), Some("rate 44100 only"));
    }

    // Arrow 8: happy path lands listening / waiting-first-slot.
    #[test]
    fn arrow8_happy_path_reaches_listening() {
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert_eq!(
            state.snapshot().slot_phase,
            crate::ft8::records::SlotPhaseDto::WaitingFirstSlot
        );
    }

    // Autostart contract: enabled=true, device=None → the supervisor lands
    // blocked(needs-device-selection) — the state that RESUMES the
    // interrupted first-contact flow (never silently stopped).
    #[test]
    fn autostart_with_no_device_lands_needs_device_selection() {
        let p = FakePlatform::happy();
        let state = test_state(p, Ft8Config { enabled: true, ..Ft8Config::default() });
        state.start().expect("start spawns the supervisor");
        // The supervisor runs the sequence async; poll briefly.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if state.axis() == ServiceAxis::Blocked(BlockedReason::NeedsDeviceSelection) {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "axis: {:?}", state.axis());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Idempotent start: a second start() with a live supervisor is Ok
        // and does not spawn a second supervisor.
        state.start().expect("idempotent start");
        state.stop_request.store(true, Ordering::SeqCst);
        state.unpark_supervisor();
        let h = state.handles.lock().unwrap().supervisor.take().unwrap();
        let _ = h.join();
    }

    /// Pipe-fd watermark trip via the fake /proc reader (spec §Testing:
    /// named test; tuxlink-b026z.8). Baseline is captured at supervisor
    /// spawn; here we seed it directly and drive the counter.
    #[test]
    fn pipe_fd_watermark_trips_only_past_the_excess_threshold() {
        let p = FakePlatform::happy();
        let state = test_state(p.clone(), cfg_with_device());
        *state.pipe_fd_baseline.lock().unwrap() = Some(8);
        *p.pipe_fds.lock().unwrap() = Some(8 + PIPE_WATERMARK_EXCESS); // == threshold: no trip
        assert!(!state.check_pipe_watermark());
        *p.pipe_fds.lock().unwrap() = Some(8 + PIPE_WATERMARK_EXCESS + 1); // > threshold: trip
        assert!(state.check_pipe_watermark());
        *p.pipe_fds.lock().unwrap() = None; // /proc unreadable: never trips
        assert!(!state.check_pipe_watermark());
    }

    /// Supervisor cadence wiring (spec: cadences count BOUNDARIES): driving
    /// the capture-side boundary atomic through tick_listening, the clock is
    /// probed once per 20-boundary window and the pipe watermark read once
    /// per 100-boundary window — pinned via the fakes' call counters.
    #[test]
    fn supervisor_cadences_fire_per_boundary_window() {
        let p = FakePlatform::happy();
        let clock = FakeClock::new(crate::ft8::clock::ClockSync::Synced);
        let state = Ft8ListenerState::new(
            Ft8Deps {
                platform: p.clone(),
                clock: clock.clone(),
                sink: Arc::new(RecordingSink::default()),
            },
            cfg_with_device(),
        );
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Listening);
        *state.pipe_fd_baseline.lock().unwrap() = Some(8);
        // Deltas from here: run_sequence already probed the clock once
        // (step 3) and never read the pipe count.
        let probes_before = clock.probe_calls.load(Ordering::SeqCst);
        let fd_reads_before = p.pipe_fd_calls.load(Ordering::SeqCst);
        let (mut last_probe, mut last_watermark) = (0u64, 0u64);
        for boundary in 1..=200u64 {
            state.slot_boundaries.store(boundary, Ordering::SeqCst);
            state.tick_listening(&mut last_probe, &mut last_watermark);
        }
        assert_eq!(
            clock.probe_calls.load(Ordering::SeqCst) - probes_before,
            10, // boundaries 20, 40, …, 200: one probe per 20-boundary window
            "clock re-probe cadence"
        );
        assert_eq!(
            p.pipe_fd_calls.load(Ordering::SeqCst) - fd_reads_before,
            2, // boundaries 100 and 200: one read per 100-boundary window
            "pipe watermark cadence"
        );
        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }

    // Snapshot completeness: every §Snapshot field is present + serializes.
    #[test]
    fn snapshot_carries_every_contract_field() {
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        let snap = state.snapshot();
        let v = serde_json::to_value(&snap).unwrap();
        for field in [
            "service", "flags", "slotPhase", "band", "dialHz", "bandSource",
            "bandLabelConfirmedUtcMs", "sweep", "engineVersion", "nConsecutive",
            "kConsecutive", "lastSlotUtcMs", "lastFailure", "availableDevices",
            "ringTail",
        ] {
            assert!(v.get(field).is_some(), "snapshot missing {field}: {v}");
        }
        assert_eq!(v["engineVersion"], "WSJT-X test 0.0");
        assert_eq!(v["service"]["axis"], "listening");
    }
}
```
Add the small test accessor used above (next to `snapshot()`):
```rust
    #[cfg(test)]
    pub(crate) fn platform_tmp_for_test(&self) -> PathBuf {
        self.platform.slot_dir_root().parent().map(|p| p.to_path_buf()).unwrap_or_default()
    }
```
(and where a test locks `state.handles` / `state.lock_inner()` directly,
those fields/methods are already crate-visible — tests live in the same
module.)

- [ ] **Step 5: [CI-side] verification** — workspace clippy + tests. Locally:
none.

- [ ] **Step 6: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/ft8
git commit -m "feat(ft8): listener service part 1 — supervisor, start sequence, snapshot, platform seam (tuxlink-b026z.3 T11)"
```

**Completion check:** every numbered arrow (1, 2a, 2b, 3, 4, 5, 6, 7, 8) has
a named test; the yield/stop check exists between EVERY pair of steps (count
the `if self.interrupted()` sites: 7 — after steps 1–7 — plus the
PCM-dropping variant after step 7); prewarm precedes the open; the state
lock is never held across a platform call that does I/O in production
(`enumerate_capture` in `snapshot()` is outside the guard, rig calls drop
the guard first); constants match the pinned table.

---
### Task 12: service.rs part 2 — capture thread, decode thread, waterfall tap, slot dirs

**Files:**
- Modify: `src-tauri/src/ft8/service.rs` (replaces the `spawn_workers` stub;
  adds the two thread bodies, the tap, the record path)

**Interfaces:**
- Consumes: `SlotAssembler` + `BoundaryConfig` + `SlotEvent`/`CompletedSlot`
  /`DiscardClass`/`DropClass` (Tasks 4–5 manifest), `Decimator` (Task 3),
  `RingOutcome`/`SlotRecord`/`DecodeDto` (T10), `SlotJob`/`Ft8ListenerState`
  (T11), `SlotOutcome`/`SlotFailure` (L1).
- Produces:
```rust
pub struct WaterfallTap;                 // bounded 32×1200-frame lossy ring
impl WaterfallTap {
    pub fn push_samples(&self, samples_12k: &[i16]);   // capture-side; never blocks
    pub fn subscribe(&self);                            // L3 attach
    pub fn unsubscribe(&self);
    pub fn take_blocks(&self) -> Vec<Vec<i16>>;         // subscriber drain
}
impl Ft8ListenerState {
    pub(crate) fn record_slot(&self, rec: SlotRecord);  // machine + ring + emit
    pub fn tap(&self) -> &WaterfallTap;
}
// thread bodies: fn capture_loop(..); fn decode_loop(..)
```

**Pinned semantics (spec, restated):** `sync_channel(0)` rendezvous — NOT 1
(slot N+1 must be the drop, never a queued N+1 with N+2 dropped); a dropped
slot's dir is deleted immediately; storage failure is `DroppedStorageError`,
counted toward N, capture continues (no stall); the tap drops OLDEST and
never backpressures capture; slot dirs are `slot-<utc_ms>-<seq>` with a
process-monotonic `<seq>` (collision-proof under backward clock steps);
capture-side boundary atomic counts EVERY slot event.

**TDD note:** Step 3's tests are authored before Steps 1–2's bodies.

- [ ] **Step 0: Land the fields + imports deferred from T11 (dead-code discipline)**

T11 deferred these to the task with their first READER so every commit in
the batch stays clippy-clean (Global Constraints §Push cadence). In
service.rs:

- The `crate::ft8::records::{...}` import list gains `DiscardClassDto` and
  `RingOutcome` (readers: `fold_slot_events`, `handle_completed_slot`).
- `Ft8ListenerState` gains, after `start_rerun_request` (replacing the T11
  placeholder comment there):
```rust
    capture_abort: Arc<AtomicBool>,
    slot_seq: AtomicU64,
```
  initialized in `new()`:
```rust
            capture_abort: Arc::new(AtomicBool::new(false)),
            slot_seq: AtomicU64::new(0),
```
- `Inner` gains, after `ring` (replacing the T11 placeholder comment's
  `discard_next_slot` line; the `resolved` line stays a comment until T13):
```rust
    /// Set by QSY (T16): the next completed slot is a scheduled discard.
    discard_next_slot: Option<DiscardClassDto>,
```
  initialized `discard_next_slot: None,` in `new()`.

- [ ] **Step 1: WaterfallTap + the second decimator note**

Add to service.rs:
```rust
/// The waterfall tap (spec §Waterfall tap): a bounded lossy ring of
/// decimated 12 kHz i16 blocks, 1200 frames (100 ms) per block, capacity 32
/// (3.2 s). Drop-OLDEST under a stalled/absent consumer; pushes never block
/// and never backpressure capture. L2's whole contract is "the 12 kHz
/// stream is subscribable, bounded, and never backpressures capture" — FFT,
/// column cadence, and events are L3's.
pub struct WaterfallTap {
    inner: Mutex<TapInner>,
}

struct TapInner {
    blocks: VecDeque<Vec<i16>>,
    /// Partial block being accumulated to the 1200-frame boundary.
    pending: Vec<i16>,
    subscribed: bool,
}

pub(crate) const TAP_BLOCK_FRAMES: usize = 1_200;
pub(crate) const TAP_CAPACITY_BLOCKS: usize = 32;

impl Default for WaterfallTap {
    fn default() -> Self {
        Self {
            inner: Mutex::new(TapInner {
                blocks: VecDeque::with_capacity(TAP_CAPACITY_BLOCKS),
                pending: Vec::with_capacity(TAP_BLOCK_FRAMES),
                subscribed: false,
            }),
        }
    }
}

impl WaterfallTap {
    pub fn push_samples(&self, samples_12k: &[i16]) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if !g.subscribed {
            // No subscriber (the common state): keep the ring warm at zero
            // cost — reset pending and skip block assembly entirely.
            g.pending.clear();
            g.blocks.clear();
            return;
        }
        let mut rest = samples_12k;
        while !rest.is_empty() {
            let need = TAP_BLOCK_FRAMES - g.pending.len();
            let take = need.min(rest.len());
            g.pending.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if g.pending.len() == TAP_BLOCK_FRAMES {
                if g.blocks.len() == TAP_CAPACITY_BLOCKS {
                    g.blocks.pop_front(); // drop-oldest
                }
                let full = std::mem::replace(
                    &mut g.pending,
                    Vec::with_capacity(TAP_BLOCK_FRAMES),
                );
                g.blocks.push_back(full);
            }
        }
    }
    pub fn subscribe(&self) {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).subscribed = true;
    }
    pub fn unsubscribe(&self) {
        let mut g = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        g.subscribed = false;
        g.blocks.clear();
        g.pending.clear();
    }
    pub fn take_blocks(&self) -> Vec<Vec<i16>> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .blocks
            .drain(..)
            .collect()
    }
}
```
Add the field to `Ft8ListenerState` (`tap: WaterfallTap` initialized
`WaterfallTap::default()` in `new`) and the accessor:
```rust
    pub fn tap(&self) -> &WaterfallTap {
        &self.tap
    }
```

- [ ] **Step 2: the thread bodies + record path — replace the `spawn_workers` stub**

```rust
impl Ft8ListenerState {
    /// Step 8 / 8′ (spec §Lifecycle): capture is spawned on every entry to
    /// listening; decode + the channel are spawned ONCE and survive yield
    /// and device loss (only stop() drops the master sender).
    fn spawn_workers(
        self: &Arc<Self>,
        source: Box<dyn crate::ft8::traits::SampleSource>,
        resume: bool,
    ) {
        let mut h = self.handles.lock().unwrap_or_else(|p| p.into_inner());
        // Reap a finished capture handle (post-yield / post-device-loss).
        if let Some(old) = h.capture.take() {
            if old.is_finished() {
                let _ = old.join();
            } else {
                // Should be unreachable: pause/stop join before respawn.
                tracing::warn!(target: "tuxlink::ft8", "capture respawn with live predecessor — detaching old");
            }
        }
        let decode_alive = h.decode.as_ref().map(|d| !d.is_finished()).unwrap_or(false);
        if !resume || !decode_alive {
            // Fresh channel + decode thread. sync_channel(0) = rendezvous:
            // try_send succeeds ONLY when decode is parked in recv (spec
            // §Backpressure — a 1-slot queue would drop N+2, not N+1).
            let (tx, rx) = std::sync::mpsc::sync_channel::<SlotJob>(0);
            *self.master_tx.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
            let engine = self
                .engine
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone()
                .expect("engine constructed at step 4 before step 8");
            let state = self.clone();
            if let Some(old) = h.decode.take() {
                let _ = old.join(); // finished (checked above)
            }
            h.decode = Some(
                std::thread::Builder::new()
                    .name("ft8-decode".into())
                    .spawn(move || decode_loop(state, engine, rx))
                    .expect("spawn ft8-decode"),
            );
        }
        self.capture_abort.store(false, Ordering::SeqCst);
        let tx = self
            .master_tx
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .expect("master sender lives until stop()");
        let state = self.clone();
        let abort = self.capture_abort.clone();
        h.capture = Some(
            std::thread::Builder::new()
                .name("ft8-capture".into())
                .spawn(move || capture_loop(state, source, tx, abort))
                .expect("spawn ft8-capture"),
        );
    }

    /// One completed slot: tmpfs dir + WAV + rendezvous handoff, with the
    /// three drop paths (storage / backpressure / scheduled QSY discard).
    fn handle_completed_slot(
        &self,
        slot: tuxlink_capture::slot::CompletedSlot,
        tx: &SyncSender<SlotJob>,
    ) {
        // Scheduled QSY-transition discard (T16 sets the flag).
        let discard = { self.lock_inner().discard_next_slot.take() };
        if let Some(class) = discard {
            self.record_slot(self.base_record(
                slot.slot_utc_ms,
                RingOutcome::Discarded { class },
                Vec::new(),
                slot.lost_frames,
                slot.boundary_skew_frames,
                slot.clip_fraction,
                slot.rms_dbfs,
            ));
            return;
        }
        let seq = self.slot_seq.fetch_add(1, Ordering::SeqCst);
        let dir = self
            .platform
            .slot_dir_root()
            .join(format!("slot-{}-{}", slot.slot_utc_ms, seq));
        let wav = dir.join("slot.wav");
        let write = std::fs::create_dir_all(&dir)
            .and_then(|()| self.platform.write_slot_wav(&wav, &slot.samples));
        if let Err(e) = write {
            // Storage failure is a DEFINED outcome (spec §WAV writeout):
            // counted toward N, best-effort cleanup, capture continues.
            let _ = std::fs::remove_dir_all(&dir);
            let diag = format!("slot WAV write failed: {e}");
            let mut rec = self.base_record(
                slot.slot_utc_ms,
                RingOutcome::DroppedStorageError { diagnostic: diag.clone() },
                Vec::new(),
                slot.lost_frames,
                slot.boundary_skew_frames,
                slot.clip_fraction,
                slot.rms_dbfs,
            );
            rec.partial_salvage = false;
            {
                self.lock_inner().last_failure = Some(diag);
            }
            self.record_slot(rec);
            return;
        }
        let job = SlotJob {
            slot_utc_ms: slot.slot_utc_ms,
            dir: dir.clone(),
            wav,
            lost_frames: slot.lost_frames,
            boundary_skew_frames: slot.boundary_skew_frames,
            clip_fraction: slot.clip_fraction,
            rms_dbfs: slot.rms_dbfs,
        };
        match tx.try_send(job) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(job)) => {
                // Decode busy → THIS slot (N+1) drops; never queue.
                let _ = std::fs::remove_dir_all(&job.dir);
                tracing::info!(
                    target: "tuxlink::ft8",
                    slot_utc_ms = job.slot_utc_ms,
                    "slot dropped: decode still busy (backpressure)"
                );
                self.record_slot(self.base_record(
                    job.slot_utc_ms,
                    RingOutcome::DroppedBackpressure,
                    Vec::new(),
                    job.lost_frames,
                    job.boundary_skew_frames,
                    job.clip_fraction,
                    job.rms_dbfs,
                ));
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(job)) => {
                // stop() dropped the master sender mid-flight: clean up.
                let _ = std::fs::remove_dir_all(&job.dir);
            }
        }
    }

    /// Ring-record constructor stamped with the current band identity.
    fn base_record(
        &self,
        slot_utc_ms: u64,
        outcome: RingOutcome,
        decodes: Vec<crate::ft8::records::DecodeDto>,
        lost_frames: u64,
        boundary_skew_frames: u64,
        clip_fraction: f32,
        rms_dbfs: f32,
    ) -> SlotRecord {
        let g = self.lock_inner();
        let partial_salvage = decodes.iter().any(|d| d.partial);
        SlotRecord {
            slot_utc_ms,
            band: g.band.clone(),
            dial_hz: g.dial_hz,
            band_source: g.band_source,
            band_label_confirmed_utc_ms: g.band_label_confirmed_utc_ms,
            outcome,
            decodes,
            partial_salvage,
            lost_frames,
            boundary_skew_frames,
            clip_fraction,
            rms_dbfs,
            dwell_slot_index: match g.machine.sweep() {
                tuxlink_capture::state::Sweep::Active { dwell_progress, .. } => {
                    Some(dwell_progress)
                }
                _ => None,
            },
        }
    }

    /// Counter fold + ring push + emits. EVERY slot boundary lands here
    /// (spec §Ring: drops and discards included).
    pub(crate) fn record_slot(&self, rec: SlotRecord) {
        let flags_changed = {
            let mut g = self.lock_inner();
            let before = (g.machine.flags(), g.machine.slot_phase());
            g.machine.on_slot_outcome(rec.outcome.kind());
            if let RingOutcome::Failed { failure } = &rec.outcome {
                g.last_failure = Some(failure.clone());
            }
            g.last_slot_utc_ms = Some(rec.slot_utc_ms);
            if g.ring.len() == RING_CAP {
                g.ring.pop_front();
            }
            g.ring.push_back(rec.clone());
            before != (g.machine.flags(), g.machine.slot_phase())
        };
        self.sink.emit_slot(&rec);
        if flags_changed {
            self.emit_listening_change();
        }
    }

    /// Mid-run device loss (spec §Device loss): the capture thread calls
    /// this and returns; the PCM closes on drop; the supervisor retries
    /// every 5 s.
    pub(crate) fn on_device_lost(&self, diagnostic: Option<String>) {
        self.set_blocked(BlockedReason::DeviceAbsent, diagnostic);
    }
}

/// The ALSA read loop → gap accounting → tap → slot assembler (spec
/// §Threads). The assembler owns the DECODE-path decimator; the tap runs a
/// second identical `Decimator` (same COEFFS, bit-identical output — see
/// the Phase C preamble's cross-cutting interface note).
fn capture_loop(
    state: Arc<Ft8ListenerState>,
    mut source: Box<dyn crate::ft8::traits::SampleSource>,
    tx: SyncSender<SlotJob>,
    abort: Arc<AtomicBool>,
) {
    use tuxlink_capture::decimator::Decimator;
    use tuxlink_capture::slot::{BoundaryConfig, SlotAssembler, SlotEvent};

    let mut asm = SlotAssembler::new(BoundaryConfig::default());
    let mut tap_decim = Decimator::new();
    let mut tap_out: Vec<i16> = Vec::new();
    let mut buf = vec![0i16; 4_800]; // one 100 ms period

    loop {
        if abort.load(Ordering::SeqCst) {
            return; // PCM closes on source drop
        }
        let batch = match source.read(&mut buf) {
            Ok(b) => b,
            Err(SourceError::Suspended) => {
                // Clock-anomaly path: an empty push carrying the Suspended
                // gap makes the assembler abandon the slot; the source
                // already recovered its PCM.
                let events = asm.push(
                    &[],
                    state.platform.utc_now_ms(),
                    state.platform.mono_now_us(),
                    Some(tuxlink_capture::slot::GapReport {
                        kind: tuxlink_capture::slot::GapKind::Suspended,
                    }),
                );
                state.fold_slot_events(events, &tx);
                continue;
            }
            Err(SourceError::Absent) | Err(SourceError::Wedged) => {
                state.on_device_lost(None);
                return;
            }
            Err(e) => {
                state.on_device_lost(Some(format!("{e:?}")));
                return;
            }
        };
        let samples = &buf[..batch.frames];
        if !samples.is_empty() {
            tap_out.clear();
            tap_decim.process(samples, &mut tap_out);
            state.tap.push_samples(&tap_out);
        }
        let events = asm.push(samples, state.platform.utc_now_ms(), batch.mono_ts_us, batch.gap);
        state.fold_slot_events(events, &tx);
    }
}

impl Ft8ListenerState {
    pub(crate) fn fold_slot_events(
        &self,
        events: Vec<tuxlink_capture::slot::SlotEvent>,
        tx: &SyncSender<SlotJob>,
    ) {
        use tuxlink_capture::slot::{DiscardClass, DropClass, SlotEvent};
        for ev in events {
            self.slot_boundaries.fetch_add(1, Ordering::SeqCst);
            match ev {
                SlotEvent::Completed(slot) => self.handle_completed_slot(slot, tx),
                SlotEvent::Abandoned { class } => {
                    let dto = match class {
                        DiscardClass::FirstSlot => DiscardClassDto::FirstSlot,
                        DiscardClass::ClockAnomaly => DiscardClassDto::ClockAnomaly,
                    };
                    let utc = self.platform.utc_now_ms();
                    self.record_slot(self.base_record(
                        utc,
                        RingOutcome::Discarded { class: dto },
                        Vec::new(),
                        0,
                        0,
                        0.0,
                        f32::NEG_INFINITY,
                    ));
                }
                SlotEvent::Dropped { class: DropClass::LostFrames, slot_utc_ms, lost_frames } => {
                    self.record_slot(self.base_record(
                        slot_utc_ms,
                        RingOutcome::DroppedLostFrames,
                        Vec::new(),
                        lost_frames,
                        0,
                        0.0,
                        f32::NEG_INFINITY,
                    ));
                }
            }
        }
    }
}

/// recv → decode → outcome fold → ring/event → slot-dir delete (spec
/// §Threads). Exits when the master sender drops (stop) — Disconnected is
/// the ONLY exit; no stop sentinel exists in this design.
fn decode_loop(
    state: Arc<Ft8ListenerState>,
    engine: Arc<dyn DecodeEngine>,
    rx: std::sync::mpsc::Receiver<SlotJob>,
) {
    use tuxlink_jt9::types::SlotOutcome;
    while let Ok(job) = rx.recv() {
        let outcome = engine.decode_slot(&job.wav, &job.dir, job.slot_utc_ms);
        let rec = match outcome {
            SlotOutcome::Decoded(decodes) => {
                let dtos: Vec<crate::ft8::records::DecodeDto> =
                    decodes.iter().map(Into::into).collect();
                state.base_record(
                    job.slot_utc_ms,
                    RingOutcome::Decoded,
                    dtos,
                    job.lost_frames,
                    job.boundary_skew_frames,
                    job.clip_fraction,
                    job.rms_dbfs,
                )
            }
            SlotOutcome::BandDead => state.base_record(
                job.slot_utc_ms,
                RingOutcome::BandDead,
                Vec::new(),
                job.lost_frames,
                job.boundary_skew_frames,
                job.clip_fraction,
                job.rms_dbfs,
            ),
            SlotOutcome::Failed(f) => state.base_record(
                job.slot_utc_ms,
                RingOutcome::Failed { failure: format!("{f:?}") },
                Vec::new(),
                job.lost_frames,
                job.boundary_skew_frames,
                job.clip_fraction,
                job.rms_dbfs,
            ),
        };
        state.record_slot(rec);
        let _ = std::fs::remove_dir_all(&job.dir);
    }
}
```
(`fold_slot_events`'s `f32::NEG_INFINITY` rms for non-completed records: the
assembler never computed levels for an abandoned/dropped slot; `-inf` dBFS
serializes as `null` under serde_json — acceptable and honest. If clippy or
serde behavior differs in CI, substitute `-120.0` and note it in the code.)

- [ ] **Step 3: Tests**

Append to the service.rs test module. These drive `fold_slot_events` /
`handle_completed_slot` / the spawned threads directly with fakes — no
sleeps longer than poll loops with deadlines.

```rust
    use crate::ft8::records::RingOutcome;
    use crate::ft8::testutil::{FakeEngine, SourceStep};
    use tuxlink_capture::slot::{CompletedSlot, SlotEvent};

    fn completed(slot_utc_ms: u64) -> CompletedSlot {
        CompletedSlot {
            slot_utc_ms,
            samples: vec![0i16; tuxlink_capture::slot::OUT_SLOT_FRAMES],
            lost_frames: 0,
            boundary_skew_frames: 0,
            clip_fraction: 0.0,
            rms_dbfs: -60.0,
        }
    }

    /// Backpressure (spec §Backpressure): with decode parked busy, slot N
    /// decodes, slot N+1 SPECIFICALLY drops (dir deleted, N incremented,
    /// ring-recorded), N+2 decodes after release.
    #[test]
    fn backpressure_drops_slot_n_plus_1_specifically() {
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead();
        *p.engine.lock().unwrap() = eng.clone();
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state); // spawns decode via spawn_workers
        assert_eq!(state.axis(), ServiceAxis::Listening);
        let tx = state.master_tx.lock().unwrap().clone().unwrap();

        // Slot 1: decode accepts it, then we gate the engine busy.
        eng.hold_gate();
        state.handle_completed_slot(completed(1_000), &tx);
        // Wait until decode has STARTED slot 1 (parked on the gate).
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while eng.decodes_started.load(Ordering::SeqCst) < 1 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Slot 2 (N+1): rendezvous refuses — dropped, dir deleted.
        state.handle_completed_slot(completed(2_000), &tx);
        {
            let g = state.lock_inner();
            let dropped: Vec<_> = g
                .ring
                .iter()
                .filter(|r| r.outcome == RingOutcome::DroppedBackpressure)
                .collect();
            assert_eq!(dropped.len(), 1);
            assert_eq!(dropped[0].slot_utc_ms, 2_000, "slot N+1 specifically");
            assert_eq!(g.machine.n_consecutive(), 1, "backpressure drop counts toward N");
        }
        // No orphan dir for the dropped slot.
        let root = state.platform.slot_dir_root();
        let leftovers: Vec<_> = std::fs::read_dir(&root)
            .map(|it| it.flatten().collect())
            .unwrap_or_default();
        assert!(
            leftovers.iter().all(|e: &std::fs::DirEntry| {
                !e.file_name().to_string_lossy().starts_with("slot-2000-")
            }),
            "dropped slot dir must be deleted immediately"
        );
        // Release: slot 1 finishes (BandDead clears nothing here — BandDead
        // clears N per types.rs), slot 3 flows.
        eng.release_gate();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while eng.decodes_finished.load(Ordering::SeqCst) < 1 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        state.handle_completed_slot(completed(3_000), &tx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while eng.decodes_finished.load(Ordering::SeqCst) < 2 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        teardown(&state);
    }

    /// Storage failure (spec §WAV writeout): ENOSPC-class write error →
    /// DroppedStorageError recorded, N incremented, last_failure set, no
    /// panic, capture path continues (the next slot writes fine).
    #[test]
    fn storage_failure_is_a_defined_outcome_and_no_stall() {
        let p = FakePlatform::happy();
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state);
        let tx = state.master_tx.lock().unwrap().clone().unwrap();
        *p.wav_result.lock().unwrap() = Err("No space left on device (os error 28)".into());
        state.handle_completed_slot(completed(1_000), &tx);
        {
            let g = state.lock_inner();
            assert!(matches!(
                g.ring.back().unwrap().outcome,
                RingOutcome::DroppedStorageError { .. }
            ));
            assert_eq!(g.machine.n_consecutive(), 1);
            assert!(g.last_failure.as_deref().unwrap().contains("No space left"));
        }
        // Recovery: the next slot flows to decode.
        *p.wav_result.lock().unwrap() = Ok(());
        state.handle_completed_slot(completed(2_000), &tx);
        let snap_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let done = {
                let g = state.lock_inner();
                g.ring.iter().any(|r| r.slot_utc_ms == 2_000 && r.outcome == RingOutcome::BandDead)
            };
            if done {
                break;
            }
            assert!(std::time::Instant::now() < snap_deadline, "capture stalled after ENOSPC");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        teardown(&state);
    }

    /// Tap (spec §Waterfall tap): drop-oldest under a stalled consumer; and
    /// pushing through the tap never slips a slot boundary (the assembler
    /// sees the identical sample stream).
    #[test]
    fn tap_drops_oldest_and_never_blocks() {
        let tap = WaterfallTap::default();
        tap.subscribe();
        // 40 blocks' worth: 8 oldest must be gone, newest 32 retained.
        for i in 0..40 {
            let block = vec![i as i16; TAP_BLOCK_FRAMES];
            tap.push_samples(&block);
        }
        let blocks = tap.take_blocks();
        assert_eq!(blocks.len(), TAP_CAPACITY_BLOCKS);
        assert_eq!(blocks[0][0], 8, "oldest 8 dropped");
        assert_eq!(blocks[31][0], 39);
        // Unsubscribed: pushes are free and retain nothing.
        tap.unsubscribe();
        tap.push_samples(&vec![1i16; TAP_BLOCK_FRAMES * 2]);
        assert!(tap.take_blocks().is_empty());
    }

    /// No boundary slip: a full scripted slot through capture_loop with a
    /// stalled tap consumer still emits exactly its slots on the synthetic
    /// boundaries.
    #[test]
    fn tap_pressure_does_not_slip_slot_boundaries() {
        let p = FakePlatform::happy();
        let state = test_state(p.clone(), cfg_with_device());
        state.tap().subscribe(); // subscribed but never drained = max pressure
        // Script: 30 s of audio (2 slots' worth) + idle. happy()'s clock
        // epoch is 1_760_000_000_000 ms; mod 15_000 = 5_000, so synthetic
        // time starts 5 s PAST a boundary — 10 s BEFORE the next one. The
        // assembler anchors at the NEXT boundary (T4 pinned next-boundary
        // semantics): boundary 1 lands at +10 s and emits the scheduled
        // FirstSlot discard; boundary 2 lands at +25 s and emits one
        // Completed slot. 30 s of audio therefore produces exactly 2
        // boundary events — the `>= 2` assertion below.
        for _ in 0..(2 * 720_000 / 4_800) {
            p.source_steps
                .lock()
                .unwrap()
                .push_back(SourceStep::Frames { frames: 4_800, value: 100, gap: None });
        }
        run_sequence(&state);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.slot_boundaries.load(Ordering::SeqCst) < 2 {
            assert!(std::time::Instant::now() < deadline, "boundary slipped under tap pressure");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        teardown(&state);
    }

    /// The ring records ALL outcome kinds (spec §Ring: every boundary yields
    /// a record — drops and discards included) and the counters fold per
    /// §Counter semantics.
    #[test]
    fn ring_records_every_outcome_kind() {
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        let recs = [
            RingOutcome::Decoded,
            RingOutcome::BandDead,
            RingOutcome::Failed { failure: "Timeout".into() },
            RingOutcome::DroppedBackpressure,
            RingOutcome::DroppedLostFrames,
            RingOutcome::DroppedStorageError { diagnostic: "ENOSPC".into() },
            RingOutcome::Discarded { class: DiscardClassDto::ClockAnomaly },
        ];
        for (i, o) in recs.iter().enumerate() {
            state.record_slot(state.base_record(
                i as u64,
                o.clone(),
                Vec::new(),
                0,
                0,
                0.0,
                -60.0,
            ));
        }
        let g = state.lock_inner();
        assert_eq!(g.ring.len(), recs.len());
        // Counter spot-checks: the trailing Failed+drops streak after the
        // last BandDead: Failed, DroppedBackpressure, DroppedLostFrames,
        // DroppedStorageError count toward N; the final Discarded does NOT.
        assert_eq!(g.machine.n_consecutive(), 4, "scheduled discard is counter-neutral");
    }

    /// Ring eviction: capacity 240 — the 241st record evicts the OLDEST.
    #[test]
    fn ring_evicts_oldest_at_capacity() {
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        for i in 0..241u64 {
            state.record_slot(state.base_record(
                i,
                RingOutcome::BandDead,
                Vec::new(),
                0,
                0,
                0.0,
                -60.0,
            ));
        }
        let g = state.lock_inner();
        assert_eq!(g.ring.len(), RING_CAP);
        assert_eq!(g.ring.front().unwrap().slot_utc_ms, 1, "slot 0 evicted");
        assert_eq!(g.ring.back().unwrap().slot_utc_ms, 240);
    }

    fn teardown(state: &Arc<Ft8ListenerState>) {
        state.stop_request.store(true, Ordering::SeqCst);
        state.capture_abort.store(true, Ordering::SeqCst);
        *state.master_tx.lock().unwrap() = None; // decode exits on Disconnected
        state.unpark_supervisor();
        let mut h = state.handles.lock().unwrap();
        for handle in [h.supervisor.take(), h.capture.take(), h.decode.take()]
            .into_iter()
            .flatten()
        {
            let _ = handle.join();
        }
        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }
```
(`teardown` is the raw pre-Task-13 shutdown; Task 13 replaces its body with
`state.stop()` once the real protocol exists. `run_sequence` in these tests
drives the sequence synchronously — no supervisor is spawned except in the
autostart test, so `h.supervisor` is usually `None` and the loop skips it.)

- [ ] **Step 3b: Retrofit `teardown(&state)` into every T11 test that reaches Listening**

Until this task, `spawn_workers` was a stub, so T11's Listening-reaching
tests leaked nothing. Now they spawn REAL capture + decode threads and must
tear down, or the test binary accumulates parked threads and temp dirs.
Append `teardown(&state);` (or `teardown(&state2);` for the second state)
as the final line of EACH of these T11 tests — this is a checklist, verify
every one:

- `arrow4_prewarm_failure_classes` — the SECOND case only (`state2` reaches
  Listening; the first case ends Blocked).
- `arrow5_cat_presence_labels_or_flags` — BOTH cases (`state` and `state2`
  both reach Listening).
- `arrow8_happy_path_reaches_listening`.
- `snapshot_carries_every_contract_field`.
- `supervisor_cadences_fire_per_boundary_window` (replace its trailing
  `remove_dir_all` line with `teardown(&state);`).
- `autostart_with_no_device_lands_needs_device_selection` — replace its
  manual shutdown block (the `stop_request.store` / `unpark_supervisor` /
  handle-join lines) with `teardown(&state);` (equivalent, and it now also
  covers the temp dir).

- [ ] **Step 4: [CI-side] verification** — workspace clippy + tests on the PR.

- [ ] **Step 5: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/ft8/service.rs
git commit -m "feat(ft8): listener service part 2 — capture/decode threads, waterfall tap, ring records (tuxlink-b026z.3 T12)"
```

**Completion check:** `sync_channel(0)` — grep confirms no `sync_channel(1)`
anywhere; the backpressure test asserts the DROPPED slot's utc (N+1), not
just a drop count; every `SlotEvent` arm increments the boundary atomic;
dropped/discarded slots never reach the decode thread; slot dirs are
deleted on every path (decode return, backpressure, storage error,
disconnected).

---
### Task 13: service.rs part 3 — stop protocol, yield mechanics, resume + device-absent recovery

**Files:**
- Modify: `src-tauri/src/ft8/service.rs`
- Modify: `src-tauri/src/ft8/testutil.rs` (adds `FakeEngine::band_dead_with_prewarm_gate()` — T14's tests reuse it)

**Interfaces:**
- Consumes: T11/T12's service internals; `SharedHold`; `Ft8Platform`.
- Produces (consumed by T14's arbiter + T17's commands):
```rust
impl Ft8ListenerState {
    pub fn stop(&self);                                     // full protocol; spawn_blocking context
    pub(crate) fn pause_capture_for_yield(&self) -> Result<(), YieldJoinTimeout>; // listening-case mechanics
    pub(crate) fn resume_conditions_met(&self) -> bool;     // latch + probe + modem eligibility
    // tick_yielded / tick_device_absent get real bodies
}
pub(crate) struct YieldJoinTimeout;                          // capture join overran 2 s
pub(crate) const CAPTURE_JOIN: Duration = Duration::from_secs(2);
pub(crate) const DECODE_JOIN: Duration = Duration::from_secs(16);
pub(crate) const SUPERVISOR_JOIN: Duration = Duration::from_secs(16);
fn join_bounded(handle: JoinHandle<()>, timeout: Duration) -> Result<(), JoinHandle<()>>;
```

**Pinned protocol (spec §Lifecycle ownership, restated):** stop = set
stop-request (checked between every sequence step, same sites as yield) +
abort → join capture ≤ 2 s via `is_finished()` poll (PCM closed on drop) →
drop master `Sender` → join decode ≤ 16 s (14 s worst-case decode) → join
supervisor ≤ 16 s (park_timeout is abort-interruptible; prewarm is a
blocking decode_slot). Absent handles are SKIPPED, not errors. A join-bound
overrun force-detaches with a warning AND transitions
`blocked(capture-wedged)` — a detached thread may still hold the PCM;
recovery is app restart. Resume + device-absent recovery re-run steps 1–7
then spawn CAPTURE ONLY (8′) — never a second decode thread or supervisor;
prewarm skipped (once per runner construction).

**TDD note:** Step 3's tests first.

- [ ] **Step 0: Land the `resolved` field deferred from T11 (dead-code discipline) + the prewarm-gate constructor**

**(0a)** service.rs — `Inner` gains, replacing T11's placeholder comment
line for it (first readers land in this task: `resume_conditions_met`,
`tick_yielded`):
```rust
    /// The device the last successful resolution produced — live handles for
    /// probes + release-confirm. Refreshed by every sequence run.
    resolved: Option<ResolvedManagedDevice>,
```
initialized `resolved: None,` in `new()`; the import block gains
`use crate::winlink::ax25::devices::ResolvedManagedDevice;`; and in
`execute_start_sequence` step 2, replace the T11 placeholder comment
("T13 adds `Inner.resolved` …") with the store:
```rust
        {
            self.lock_inner().resolved = Some(resolved.clone());
        }
```

**(0b)** testutil.rs — `FakeEngine` gains the prewarm-gated constructor
(this task's mid-prewarm test and T14's pause-during-starting test both use
it):
```rust
    /// `band_dead()` with the prewarm gate armed: `prewarm()` parks on the
    /// gate until the test releases it — the stop/pause-during-starting
    /// scenarios need a sequence deterministically parked at step 4.
    pub fn band_dead_with_prewarm_gate() -> Arc<Self> {
        Arc::new(Self {
            outcomes: Mutex::new(VecDeque::new()),
            default_outcome: SlotOutcome::BandDead,
            prewarm_result: Mutex::new(Ok(())),
            gate: Arc::new((Mutex::new(false), Condvar::new())),
            gate_prewarm: true,
            decodes_started: AtomicU64::new(0),
            decodes_finished: AtomicU64::new(0),
        })
    }
```

- [ ] **Step 1: bounded join + stop()**

```rust
pub(crate) const CAPTURE_JOIN: Duration = Duration::from_secs(2);
pub(crate) const DECODE_JOIN: Duration = Duration::from_secs(16);
pub(crate) const SUPERVISOR_JOIN: Duration = Duration::from_secs(16);

/// Poll `is_finished()` to the bound, then join. Returns Err(handle) on
/// overrun so the caller can force-detach with provenance.
fn join_bounded(
    handle: std::thread::JoinHandle<()>,
    timeout: Duration,
) -> Result<(), std::thread::JoinHandle<()>> {
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return Err(handle);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = handle.join();
    Ok(())
}

impl Ft8ListenerState {
    /// Full stop protocol (spec §Lifecycle ownership). Blocking-context
    /// only — the Tauri command wraps it in spawn_blocking (T17).
    pub fn stop(&self) {
        {
            let mut g = self.lock_inner();
            if matches!(g.machine.axis(), ServiceAxis::Stopped) {
                return;
            }
            g.machine.on_stopping();
        }
        self.emit_listening_change();
        self.stop_request.store(true, Ordering::SeqCst);
        self.capture_abort.store(true, Ordering::SeqCst);
        self.unpark_supervisor();

        let mut wedged = false;

        // 1. capture (if present): ≤ 2 s; PCM closes on drop.
        let capture = self.handles.lock().unwrap_or_else(|p| p.into_inner()).capture.take();
        if let Some(h) = capture {
            if let Err(detached) = join_bounded(h, CAPTURE_JOIN) {
                tracing::warn!(
                    target: "tuxlink::ft8",
                    "capture join overran {CAPTURE_JOIN:?} at stop — force-detaching; \
                     the detached thread may still hold the PCM (capture-wedged)"
                );
                drop(detached);
                wedged = true;
            }
        }

        // 2. drop the master Sender → decode's recv returns Disconnected.
        *self.master_tx.lock().unwrap_or_else(|p| p.into_inner()) = None;

        // 3. decode (if present): ≤ 16 s (covers the 14 s worst-case decode).
        let decode = self.handles.lock().unwrap_or_else(|p| p.into_inner()).decode.take();
        if let Some(h) = decode {
            if let Err(detached) = join_bounded(h, DECODE_JOIN) {
                tracing::warn!(target: "tuxlink::ft8", "decode join overran at stop — force-detaching");
                drop(detached);
                wedged = true;
            }
        }

        // 4. supervisor: ≤ 16 s (it may be inside an unabortable prewarm).
        let supervisor = self.handles.lock().unwrap_or_else(|p| p.into_inner()).supervisor.take();
        if let Some(h) = supervisor {
            self.unpark_supervisor();
            if let Err(detached) = join_bounded(h, SUPERVISOR_JOIN) {
                tracing::warn!(target: "tuxlink::ft8", "supervisor join overran at stop — force-detaching");
                drop(detached);
                wedged = true;
            }
        }
        *self.supervisor_thread.lock().unwrap_or_else(|p| p.into_inner()) = None;

        // 5. the runner is reconstructed on the next start.
        *self.engine.lock().unwrap_or_else(|p| p.into_inner()) = None;

        {
            let mut g = self.lock_inner();
            if wedged {
                g.machine.on_capture_wedged();
            } else {
                g.machine.on_stopped();
            }
        }
        self.emit_listening_change();
    }
}
```

- [ ] **Step 2: yield mechanics + resume/retry ticks**

```rust
/// Capture join overran at pause: the arbiter maps this to
/// PauseError::CaptureWedged (T14).
#[derive(Debug)]
pub(crate) struct YieldJoinTimeout;

impl Ft8ListenerState {
    /// The `listening`-case pause mechanics (spec §Arbitration): abort →
    /// join capture ≤ 2 s → (PCM closed by the capture thread's drop) →
    /// write yielded. Latch + release-confirm + rig cancellation live in the
    /// arbiter (T14) — pause is that transition's single writer and calls
    /// this. On join overrun: blocked(capture-wedged) + Err.
    pub(crate) fn pause_capture_for_yield(&self) -> Result<(), YieldJoinTimeout> {
        self.capture_abort.store(true, Ordering::SeqCst);
        let capture = self.handles.lock().unwrap_or_else(|p| p.into_inner()).capture.take();
        if let Some(h) = capture {
            if let Err(detached) = join_bounded(h, CAPTURE_JOIN) {
                drop(detached);
                {
                    self.lock_inner().machine.on_capture_wedged();
                }
                self.emit_listening_change();
                return Err(YieldJoinTimeout);
            }
        }
        {
            self.lock_inner().machine.on_pause();
        }
        self.emit_listening_change();
        Ok(())
    }

    /// Pause from `stopped` is a stateless no-op guard the arbiter uses
    /// (spec: a system that never enabled FT8 must never acquire phantom
    /// listener state). Exposed for the T13 test; T14 routes through it.
    pub(crate) fn is_stopped(&self) -> bool {
        matches!(self.axis(), ServiceAxis::Stopped)
    }

    /// Resume conditions (spec §Resume — ALL must hold): latch clear, card
    /// probe free, modem session positively resume-eligible.
    pub(crate) fn resume_conditions_met(&self) -> bool {
        if self.hold.is_latched() {
            return false;
        }
        let resolved = { self.lock_inner().resolved.clone() };
        let probe_free = match resolved {
            Some(r) => self.platform.probe_busy(&r.alsa_plughw, r.card_index).is_ok(),
            // No resolution yet (yielded out of `starting` before step 2):
            // let the sequence re-run resolve it — treat as free.
            None => true,
        };
        probe_free && self.platform.modem_resume_eligible()
    }

    /// Supervisor tick, `yielded` axis: resume when all conditions hold.
    /// Positive latch clearing on observed card-busy also lives here (the
    /// modem actually acquired the card — the latch's job is done).
    pub(crate) fn tick_yielded(self: &Arc<Self>) {
        // Positive-evidence latch clear: card observed busy while latched.
        if self.hold.is_latched() {
            if let Some(r) = { self.lock_inner().resolved.clone() } {
                if self.platform.probe_busy(&r.alsa_plughw, r.card_index).is_err() {
                    self.hold.clear();
                }
            }
            return; // still latched or just cleared — resume next tick
        }
        if self.resume_conditions_met() {
            self.yield_request.store(false, Ordering::SeqCst);
            // Resume = steps 1–7 + 8′ capture-only (prewarm skipped: the
            // runner survives; jt9 discovery re-runs by design).
            self.execute_start_sequence(true);
        }
    }

    /// Supervisor tick, `blocked(device-absent)`: retry every tick (5 s).
    /// Identical path to resume — fresh re-resolution, capture-only respawn
    /// when the decode thread survives.
    pub(crate) fn tick_device_absent(self: &Arc<Self>) {
        self.execute_start_sequence(true);
    }
}
```
Delete the T11 stub bodies for `tick_yielded`/`tick_device_absent` (they are
replaced above). Also replace the T12 test helper `teardown` body with:
```rust
    fn teardown(state: &Arc<Ft8ListenerState>) {
        state.stop();
        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }
```

**Per-state thread-liveness assertion** (spec §Lifecycle "threads per
state") — add as a test helper + use it in the tests below:
```rust
    /// Asserts the §Lifecycle threads-per-state table for the current axis.
    fn assert_thread_liveness(state: &Arc<Ft8ListenerState>) {
        let h = state.handles.lock().unwrap();
        let alive = |o: &Option<std::thread::JoinHandle<()>>| {
            o.as_ref().map(|j| !j.is_finished()).unwrap_or(false)
        };
        let (sup, cap, dec) = (alive(&h.supervisor), alive(&h.capture), alive(&h.decode));
        drop(h);
        match state.axis() {
            ServiceAxis::Stopped => {
                assert!(!sup && !cap && !dec, "stopped: no threads");
            }
            ServiceAxis::Blocked(BlockedReason::CaptureWedged) => {} // detached: unknowable
            ServiceAxis::Blocked(_) | ServiceAxis::Starting => {
                assert!(!cap, "blocked/starting: no capture thread");
            }
            ServiceAxis::Yielded => {
                assert!(!cap, "yielded: capture joined");
                assert!(dec, "yielded: decode survives");
            }
            ServiceAxis::Listening => {
                assert!(cap && dec, "listening: capture + decode alive");
            }
            ServiceAxis::Stopping => {}
        }
    }
```
(In sequence-driven tests without a spawned supervisor, `sup` is legitimately
false for non-stopped states — the helper only pins the capture/decode
columns there; tests that spawn via `start()` get the full check.)

- [ ] **Step 3: Tests**

```rust
    /// Stop during an in-flight decode (slow fake engine): completes WITHOUT
    /// the force-detach path — the 16 s decode bound absorbs the 14 s
    /// worst case (spec §Lock discipline names this exact test).
    #[test]
    fn stop_during_inflight_decode_completes_without_force_detach() {
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead();
        *p.engine.lock().unwrap() = eng.clone();
        let state = test_state(p, cfg_with_device());
        run_sequence(&state);
        let tx = state.master_tx.lock().unwrap().clone().unwrap();
        eng.hold_gate();
        state.handle_completed_slot(completed(1_000), &tx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while eng.decodes_started.load(Ordering::SeqCst) < 1 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Release the gate from a helper thread ~200 ms into the stop, well
        // inside the 16 s decode bound.
        let eng2 = eng.clone();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            eng2.release_gate();
        });
        state.stop();
        releaser.join().unwrap();
        assert_eq!(state.axis(), ServiceAxis::Stopped, "no capture-wedged");
        assert_thread_liveness(&state);
        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }

    /// Stop during `starting`, mid-prewarm: the stop-request is honored at
    /// the next between-step check; the supervisor join bound covers the
    /// blocking prewarm; NO capture-wedged (no capture thread ever existed).
    /// Deterministic by construction: the gate is held BEFORE start(), so
    /// the sequence parks INSIDE prewarm at step 4.
    #[test]
    fn stop_during_starting_mid_prewarm_completes_clean() {
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead_with_prewarm_gate(); // Step 0b
        eng.hold_gate(); // BEFORE start(): the sequence parks at step 4
        *p.engine.lock().unwrap() = eng.clone();
        let state = test_state(p, cfg_with_device());
        state.start().expect("supervisor spawns");
        // Parked inside prewarm: axis holds Starting and no decode ever
        // starts while a short window elapses.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while state.axis() != ServiceAxis::Starting {
            assert!(std::time::Instant::now() < deadline, "never reached Starting");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(state.axis(), ServiceAxis::Starting, "parked in prewarm");
        assert_eq!(eng.decodes_started.load(Ordering::SeqCst), 0);
        // Release the gate ~200 ms into the stop, well inside the 16 s
        // supervisor join bound.
        let eng2 = eng.clone();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            eng2.release_gate();
        });
        state.stop();
        releaser.join().unwrap();
        assert_eq!(
            state.axis(),
            ServiceAxis::Stopped,
            "mid-prewarm stop is clean — never capture-wedged"
        );
        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }

    /// Pause from stopped = stateless no-op (spec §Arbitration first arm):
    /// no latch, no state change, no thread interaction.
    #[test]
    fn pause_from_stopped_is_a_stateless_noop() {
        let p = FakePlatform::happy();
        let state = test_state(p, cfg_with_device());
        assert!(state.is_stopped());
        // T14's arbiter checks is_stopped() and returns Ok(()) WITHOUT
        // latching; pin the primitive here: the hold stays clear and the
        // axis stays stopped even if pause mechanics are (wrongly) invoked.
        assert!(!state.hold().is_latched());
        assert_eq!(state.axis(), ServiceAxis::Stopped);
    }

    /// Resume re-spawn: after a yield, the decode thread SURVIVES and the
    /// resume spawns capture only (8′); prewarm is not re-run.
    #[test]
    fn resume_respawns_capture_only_and_decode_survives() {
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead();
        *p.engine.lock().unwrap() = eng.clone();
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert_thread_liveness(&state);

        // Yield (the T14 arbiter's listening arm, mechanics only).
        state.hold().latch_now();
        state.pause_capture_for_yield().expect("clean join");
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        assert_thread_liveness(&state); // decode alive, capture joined

        let prewarm_count_before = {
            // FakeEngine has no prewarm counter; pin via engine identity —
            // the SAME Arc must still be installed after resume.
            Arc::as_ptr(&(state.engine.lock().unwrap().clone().unwrap())) as usize
        };

        // Clear the latch + free card + eligible modem → tick resumes.
        state.hold().clear();
        *p.modem_eligible.lock().unwrap() = true;
        state.tick_yielded();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert_thread_liveness(&state);
        let engine_after =
            Arc::as_ptr(&(state.engine.lock().unwrap().clone().unwrap())) as usize;
        assert_eq!(prewarm_count_before, engine_after, "runner NOT reconstructed on resume");
        teardown(&state);
    }

    /// Device-absent retry recovery: mid-run loss blocks device-absent; the
    /// tick re-resolves (fresh index — the card moved!) and recovers with a
    /// capture-only respawn.
    #[test]
    fn device_absent_retry_recovers_with_fresh_resolution() {
        let p = FakePlatform::happy();
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Listening);

        // Mid-run loss: the capture loop calls on_device_lost when the
        // source errors; simulate the loss directly + join the capture
        // thread the way the loop's return does.
        p.source_steps
            .lock()
            .unwrap()
            .push_back(SourceStep::Fail(crate::ft8::traits::SourceError::Absent));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while state.axis() != ServiceAxis::Blocked(BlockedReason::DeviceAbsent) {
            assert!(std::time::Instant::now() < deadline, "loss not detected");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Replug on a NEW index: retry must use the fresh resolution.
        *p.resolved.lock().unwrap() = Some(crate::winlink::ax25::devices::ResolvedManagedDevice {
            alsa_plughw: "plughw:CARD=DRA,DEV=0".into(),
            alsa_hw: "hw:3,0".into(),
            card_index: 3,
        });
        state.tick_device_absent();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert_eq!(
            state.lock_inner().resolved.as_ref().unwrap().card_index,
            3,
            "recovery re-resolved to the LIVE index, never a cached name"
        );
        assert_thread_liveness(&state);
        teardown(&state);
    }

    /// Blocked re-entry with a BUSY card lands `Yielded`, never a stale
    /// blocked axis (the set_device-from-blocked path): the sequence entry
    /// re-enters Starting from every non-wedged blocked reason
    /// (on_start_requested — T11 entry match), so step 6's pause writes
    /// Yielded; on_pause from Blocked would have been silently swallowed.
    /// tick_yielded then recovers once the card frees.
    #[test]
    fn set_device_from_blocked_with_busy_card_lands_yielded_then_recovers() {
        let p = FakePlatform::happy();
        *p.resolved.lock().unwrap() = None;
        let state = test_state(p.clone(), cfg_with_device());
        run_sequence(&state);
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::DeviceAbsent));

        // Device replugs, but a modem holds the card.
        *p.resolved.lock().unwrap() =
            Some(crate::winlink::ax25::devices::ResolvedManagedDevice {
                alsa_plughw: "plughw:CARD=DRA,DEV=0".into(),
                alsa_hw: "hw:1,0".into(),
                card_index: 1,
            });
        *p.busy.lock().unwrap() = Err("card busy".into());
        state.execute_start_sequence(false); // the set_device retrigger path
        assert_eq!(
            state.axis(),
            ServiceAxis::Yielded,
            "busy re-entry must yield — a stale blocked axis strands the operator"
        );

        // Card frees (modem already eligible in happy()): the supervisor
        // tick recovers.
        *p.busy.lock().unwrap() = Ok(());
        state.tick_yielded();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        teardown(&state);
    }
```

- [ ] **Step 4: [CI-side] verification** — workspace clippy + tests.

- [ ] **Step 5: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/ft8
git commit -m "feat(ft8): listener service part 3 — stop protocol, yield mechanics, recovery ticks (tuxlink-b026z.3 T13)"
```

**Completion check:** stop's join order is capture → drop-sender → decode →
supervisor with the pinned bounds (2/16/16 s); absent handles skipped;
overrun ⇒ capture-wedged (never a masquerading `yielded`); resume/retry call
`execute_start_sequence(true)` (prewarm skipped, capture-only respawn);
`assert_thread_liveness` is invoked in at least three tests; no state-mutex
hold across any join (audit `stop()` + `pause_capture_for_yield` line by
line).

---

### Task 14: arbiter.rs — `Ft8Arbiter`, `pause_for_modem`, hold latch, rig-session serialization

**Files:**
- Create: `src-tauri/src/ft8/arbiter.rs`
- Modify: `src-tauri/src/ft8/mod.rs` (`pub mod arbiter;`)
- Modify: `src-tauri/src/ft8/service.rs` (the two arbiter accessors, the step-5 `start_rig_labeling` arbiter routing, the `#[cfg(test)]` helpers)
- Modify: `src-tauri/src/ft8/testutil.rs` (the `Park` step + `park_flag`; `band_dead_with_prewarm_gate` was already added in T13)

**Interfaces:**
- Consumes: `Ft8ListenerState` internals via its pub(crate) surface (T11–13),
  `SharedHold`, `ServiceAxis`, `Ft8Platform::{probe_busy, confirm_released,
  modem_resume_eligible}`.
- Produces (consumed by T15's modem seams + T16's sweep + T17's wiring):
```rust
pub enum PauseError { CaptureWedged, ReleaseTimeout }
impl PauseError { pub fn device_busy_message(&self) -> String; }
pub struct Ft8Arbiter;
impl Ft8Arbiter {
    pub fn new(service: Arc<Ft8ListenerState>) -> Arc<Self>;
    pub fn pause_for_modem(&self) -> Result<(), PauseError>;   // BLOCKING-CONTEXT ONLY
    pub fn rig_session<R>(&self, f: impl FnOnce() -> R) -> R;  // serializes ALL FT8 rig sessions
}
pub static FT8_ARBITER: OnceLock<Arc<Ft8Arbiter>>;             // installed by lib.rs setup (T17)
pub fn pause_for_modem_global() -> Result<(), PauseError>;     // Ok(()) when uninstalled
```

**TDD note:** Step 2's tests first.

- [ ] **Step 1: arbiter.rs**

```rust
//! Modem yield/resume arbitration (spec §Arbitration). Design principle
//! (adversarial round 2): resume decisions must not rest on negative
//! evidence alone — every yield LATCHES A HOLD the resume poll honors,
//! cleared by positive evidence (card observed busy = the modem actually
//! acquired it) or a 30 s TTL (an aborted spawn must not wedge FT8).
//!
//! The arbiter also owns ALL rig sessions the FT8 service creates
//! (start-labeling QSY, band-chip QSY, sweep QSY) via
//! [`Ft8Arbiter::rig_session`]: rig_session holds the ARBITER lock (only)
//! around the closure, and pause_for_modem takes the same arbiter lock plus
//! a brief rig-lock await — so a modem connect's pre-audio tune can never
//! overlap an FT8 rig session (the FT-710 dual-CAT-user contention class).
//! The closure itself owns the rig lock; lock order arbiter > rig > state,
//! each acquired at most once per thread.

use std::sync::{Arc, Mutex, OnceLock};

use super::service::Ft8ListenerState;
use tuxlink_capture::state::ServiceAxis;

/// Why a pause could not hand the card over cleanly. The modem seams (T15)
/// surface both as the existing device-busy error class and DO NOT proceed
/// to a doomed spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseError {
    /// The capture join overran its 2 s bound — a hung USB device can park
    /// even the wait-loop. The service is now blocked(capture-wedged);
    /// recovery is app restart.
    CaptureWedged,
    /// `confirm_audio_device_released` timed out: something else still holds
    /// the device path.
    ReleaseTimeout,
}

impl PauseError {
    /// The device-busy-class message the modem seams surface (matches the
    /// tone of `direwolf_probe::device_busy_message`).
    pub fn device_busy_message(&self) -> String {
        match self {
            PauseError::CaptureWedged => {
                "the FT8 listener's audio capture is wedged and may still hold the sound card; \
                 restart Tuxlink"
                    .into()
            }
            PauseError::ReleaseTimeout => {
                "the sound card was not released in time for the modem to start; \
                 try again in a few seconds"
                    .into()
            }
        }
    }
}

pub struct Ft8Arbiter {
    /// THE arbiter lock (lock order: arbiter > state, pinned — spec §Lock
    /// discipline). Serializes pause_for_modem against itself and against
    /// rig sessions.
    lock: Mutex<()>,
    service: Arc<Ft8ListenerState>,
}

impl Ft8Arbiter {
    pub fn new(service: Arc<Ft8ListenerState>) -> Arc<Self> {
        Arc::new(Self { lock: Mutex::new(()), service })
    }

    /// Yield the audio device to a modem that is about to open it.
    ///
    /// **BLOCKING-CONTEXT-ONLY CONTRACT (pinned):** this joins a thread
    /// (≤ 2 s) and polls lsof (≤ 2 s). Every call site MUST run under
    /// `spawn_blocking` or on a plain std thread — never on a tokio worker.
    /// All current call sites comply (ardopcf spawns and Dire Wolf's
    /// spawn_inner run under spawn_blocking; the VARA seam wraps its call —
    /// T15).
    pub fn pause_for_modem(&self) -> Result<(), PauseError> {
        let _arb = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let axis = self.service.axis();
        match axis {
            // stopped: no latch, no state change — a system that never
            // enabled FT8 must never acquire phantom listener state.
            ServiceAxis::Stopped => Ok(()),
            // blocked(*): latch only; the blocked axis + reason stay put.
            ServiceAxis::Blocked(_) | ServiceAxis::Stopping => {
                self.service.hold().latch_now();
                Ok(())
            }
            // yielded: already handed over; refresh the latch so the
            // incoming spawn keeps its protection window.
            ServiceAxis::Yielded => {
                self.service.hold().latch_now();
                Ok(())
            }
            ServiceAxis::Listening => {
                // Cancel/await any in-flight rig session: taking the rig
                // lock waits it out; holding it briefly excludes new ones.
                {
                    let rig = self.service.rig_lock();
                    let _rig_guard = rig.lock().unwrap_or_else(|p| p.into_inner());
                }
                self.service.hold().latch_now();
                self.service
                    .pause_capture_for_yield()
                    .map_err(|_| PauseError::CaptureWedged)?;
                self.confirm_release()
            }
            ServiceAxis::Starting => {
                {
                    let rig = self.service.rig_lock();
                    let _rig_guard = rig.lock().unwrap_or_else(|p| p.into_inner());
                }
                self.service.hold().latch_now();
                // There is never a capture thread during starting (spawned
                // only at step 8, which transitions to listening). The
                // yield-request flag makes the supervisor abandon its
                // sequence at the next between-step check, dropping the PCM
                // if it holds one (post-step-7); pause writes the axis.
                self.service.request_yield_from_starting();
                self.confirm_release()
            }
        }
    }

    fn confirm_release(&self) -> Result<(), PauseError> {
        // The trailing release-confirm absorbs the milliseconds until a
        // post-step-7 supervisor's PCM drop lands.
        let card = self.service.resolved_card_index();
        match card {
            Some(idx) if !self.service.platform.confirm_released(idx) => {
                Err(PauseError::ReleaseTimeout)
            }
            _ => Ok(()),
        }
    }

    /// Serialize an FT8-owned rig session (start-labeling, band chip QSY,
    /// sweep QSY) against pause_for_modem and against other rig sessions.
    ///
    /// **Lock architecture (pinned):** this takes ONLY the arbiter lock —
    /// the closure `f` OWNS the rig-lock acquisition itself (`qsy_to_band`
    /// and `start_rig_labeling` each take the rig lock internally). Lock
    /// order: arbiter > rig > state, each acquired AT MOST ONCE per thread.
    /// `rig_session` must never take the rig lock: std's `Mutex` is
    /// non-reentrant, so taking it here and again inside `f` deadlocks —
    /// the exact composition `rig_session(|| qsy_to_band(..))` that the
    /// pre-fix design deadlocked on and that
    /// `rig_session_composed_with_qsy_does_not_deadlock` pins. The arbiter
    /// lock is what excludes a concurrent `pause_for_modem`; the rig lock
    /// alone never could.
    pub fn rig_session<R>(&self, f: impl FnOnce() -> R) -> R {
        let _arb = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        f()
    }
}

/// Global install point (lib.rs setup, T17). The modem seams call
/// [`pause_for_modem_global`]; before install (unit tests of modem_commands,
/// early startup) it is a no-op Ok — exactly the `stopped` semantics.
pub static FT8_ARBITER: OnceLock<Arc<Ft8Arbiter>> = OnceLock::new();

pub fn pause_for_modem_global() -> Result<(), PauseError> {
    match FT8_ARBITER.get() {
        Some(arb) => arb.pause_for_modem(),
        None => Ok(()),
    }
}
```
Add the two small service accessors the arbiter needs (service.rs):
```rust
    pub(crate) fn request_yield_from_starting(&self) {
        self.yield_request.store(true, Ordering::SeqCst);
        {
            self.lock_inner().machine.on_pause();
        }
        self.emit_listening_change();
    }
    pub(crate) fn resolved_card_index(&self) -> Option<u32> {
        self.lock_inner().resolved.as_ref().map(|r| r.card_index)
    }
```

Also route the step-5 start-labeling call through the arbiter (T11 called
it bare because the arbiter did not exist at that commit — the T11 doc on
`start_rig_labeling` records this). In `execute_start_sequence` step 5:
```rust
            self.start_rig_labeling();
```
becomes:
```rust
            // Through the arbiter when installed: the ARBITER lock is what
            // excludes a concurrent pause_for_modem; start_rig_labeling
            // itself owns the rig lock (lock order arbiter > rig > state,
            // each acquired at most once per thread).
            let label = || self.start_rig_labeling();
            match crate::ft8::arbiter::FT8_ARBITER.get() {
                Some(arb) => arb.rig_session(label),
                None => label(),
            }
```

- [ ] **Step 2: Tests (in arbiter.rs)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Ft8Config;
    use crate::ft8::service::{Ft8Deps, SharedHold, HOLD_LATCH_TTL};
    use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink};
    use crate::winlink::ax25::devices::{StableAudioId, StableIdKind};
    use tuxlink_capture::state::{BlockedReason, ServiceAxis};

    fn setup(platform: Arc<FakePlatform>, cfg: Ft8Config) -> (Arc<Ft8ListenerState>, Arc<Ft8Arbiter>) {
        let state = Ft8ListenerState::new(
            Ft8Deps {
                platform,
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: Arc::new(RecordingSink::default()),
            },
            cfg,
        );
        let arb = Ft8Arbiter::new(state.clone());
        (state, arb)
    }

    fn cfg_with_device() -> Ft8Config {
        let mut c = Ft8Config::default();
        c.device = Some(StableAudioId { kind: StableIdKind::ByIdSymlink, value: "usb-X-00".into() });
        c
    }

    /// Axis arm 1 — stopped: Ok, NO latch, NO state change.
    #[test]
    fn pause_from_stopped_latches_nothing() {
        let (state, arb) = setup(FakePlatform::happy(), cfg_with_device());
        assert_eq!(arb.pause_for_modem(), Ok(()));
        assert!(!state.hold().is_latched(), "stopped pause must not latch");
        assert_eq!(state.axis(), ServiceAxis::Stopped);
    }

    /// Axis arm 2 — blocked(*): latch only; reason untouched.
    #[test]
    fn pause_from_blocked_latches_and_leaves_the_axis() {
        let p = FakePlatform::happy();
        *p.jt9.lock().unwrap() = Err("NotOnPath".into());
        let (state, arb) = setup(p, cfg_with_device());
        {
            state.test_run_sequence();
        }
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::WsjtxAbsent));
        assert_eq!(arb.pause_for_modem(), Ok(()));
        assert!(state.hold().is_latched());
        assert_eq!(state.axis(), ServiceAxis::Blocked(BlockedReason::WsjtxAbsent));
    }

    /// Axis arm 3 — listening: join + yielded + release-confirm + latch.
    #[test]
    fn pause_from_listening_joins_confirms_and_latches() {
        let p = FakePlatform::happy();
        let (state, arb) = setup(p.clone(), cfg_with_device());
        state.test_run_sequence();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert_eq!(arb.pause_for_modem(), Ok(()));
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        assert!(state.hold().is_latched());
        state.test_teardown();
    }

    /// Listening + release-confirm timeout → Err(ReleaseTimeout), no doomed
    /// spawn.
    #[test]
    fn release_timeout_surfaces_as_pause_error() {
        let p = FakePlatform::happy();
        *p.released.lock().unwrap() = false;
        let (state, arb) = setup(p, cfg_with_device());
        state.test_run_sequence();
        assert_eq!(arb.pause_for_modem(), Err(PauseError::ReleaseTimeout));
        state.test_teardown();
    }

    /// Axis arm 4 — starting: flag + latch + yielded; never a capture join.
    /// Park the sequence mid-prewarm (gated engine) on a supervisor-less
    /// helper thread, pause, then release.
    #[test]
    fn pause_during_starting_converts_to_yielded_without_join() {
        use crate::ft8::testutil::FakeEngine;
        let p = FakePlatform::happy();
        let eng = FakeEngine::band_dead_with_prewarm_gate();
        eng.hold_gate();
        *p.engine.lock().unwrap() = eng.clone();
        let (state, arb) = setup(p, cfg_with_device());
        let s2 = state.clone();
        let seq = std::thread::spawn(move || s2.test_run_sequence());
        // Wait until the sequence is inside prewarm.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(arb.pause_for_modem(), Ok(()));
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        assert!(state.hold().is_latched());
        eng.release_gate();
        seq.join().unwrap();
        // The abandoned sequence must NOT have overwritten yielded
        // (the flag never writes the axis; pause already did).
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        state.test_teardown();
    }

    /// Hold latch TTL: a latch older than 30 s reads clear. Driven through
    /// the SharedHold primitive directly (Instant is not fake-able without
    /// a clock trait detour the spec does not require).
    #[test]
    fn hold_latch_ttl_expires_lazily() {
        let hold = SharedHold::default();
        hold.latch_now();
        assert!(hold.is_latched());
        // Backdate: reach inside via the test-only setter.
        hold.test_backdate(HOLD_LATCH_TTL + std::time::Duration::from_secs(1));
        assert!(!hold.is_latched(), "TTL-expired latch reads clear");
        assert!(!hold.is_latched(), "and stays cleared (dropped on observation)");
    }

    /// Positive latch clear: while yielded + latched, the supervisor tick
    /// observing the card BUSY clears the latch (the modem got the card —
    /// positive evidence, not TTL).
    #[test]
    fn latch_clears_on_observed_card_busy() {
        let p = FakePlatform::happy();
        let (state, arb) = setup(p.clone(), cfg_with_device());
        state.test_run_sequence();
        arb.pause_for_modem().unwrap();
        assert!(state.hold().is_latched());
        *p.busy.lock().unwrap() = Err("card busy".into());
        state.tick_yielded();
        assert!(!state.hold().is_latched(), "positive-evidence clear");
        // And with the card still busy, no resume happened.
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        state.test_teardown();
    }

    /// rig_session serializes against pause: a pause issued while a rig
    /// session runs waits for it (no dual-CAT overlap).
    #[test]
    fn rig_session_excludes_pause() {
        let p = FakePlatform::happy();
        let (state, arb) = setup(p, cfg_with_device());
        state.test_run_sequence();
        let arb2 = arb.clone();
        let in_session = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = in_session.clone();
        let t = std::thread::spawn(move || {
            arb2.rig_session(|| {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(300));
                flag.store(false, std::sync::atomic::Ordering::SeqCst);
            })
        });
        while !in_session.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        arb.pause_for_modem().unwrap();
        assert!(
            !in_session.load(std::sync::atomic::Ordering::SeqCst),
            "pause returned while a rig session was mid-flight"
        );
        t.join().unwrap();
        state.test_teardown();
    }

    /// The non-reentrancy composition pin (the pre-fix design deadlocked
    /// EXACTLY here and no test drove it): rig_session takes ONLY the
    /// arbiter lock; the closure owns the rig lock via qsy_to_band. If
    /// rig_session ever re-acquires the rig lock, this composition hangs —
    /// the deadline poll turns the hang into a failure. LOCAL arbiter, not
    /// the process-global OnceLock.
    #[test]
    fn rig_session_composed_with_qsy_does_not_deadlock() {
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        let (state, arb) = setup(p.clone(), cfg_with_device());
        state.test_run_sequence();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        let s2 = state.clone();
        let arb2 = arb.clone();
        let worker = std::thread::spawn(move || {
            arb2.rig_session(|| {
                s2.qsy_to_band("40m", crate::ft8::records::BandSource::CatConfirmed)
            })
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !worker.is_finished() {
            assert!(
                std::time::Instant::now() < deadline,
                "rig_session(qsy_to_band) deadlocked — the non-reentrancy \
                 contract is broken (rig_session must not take the rig lock)"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        worker.join().unwrap().unwrap();
        assert_eq!(*p.tuned_to.lock().unwrap().last().unwrap(), 7_074_000);
        state.test_teardown();
    }

    /// Negative resume gate (spec §Resume — ALL conditions must hold): an
    /// INELIGIBLE modem session (e.g. ConnectedIss) blocks resume even with
    /// the latch clear and the card free; eligibility flipping back is what
    /// releases it.
    #[test]
    fn resume_blocked_while_modem_ineligible() {
        let p = FakePlatform::happy();
        let (state, arb) = setup(p.clone(), cfg_with_device());
        state.test_run_sequence();
        arb.pause_for_modem().unwrap();
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        state.hold().clear(); // latch clear + card free (happy default) ...
        *p.modem_eligible.lock().unwrap() = false; // ... but modem ineligible
        state.tick_yielded();
        assert_eq!(
            state.axis(),
            ServiceAxis::Yielded,
            "an ineligible modem must block resume on its own"
        );
        *p.modem_eligible.lock().unwrap() = true;
        state.tick_yielded();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        state.test_teardown();
    }

    /// Wedged join (spec §Testing: "wedged join → blocked(capture-wedged) +
    /// Err"): a capture thread whose READ blocks past the 2 s join bound —
    /// the hung-USB class the abort flag cannot reach — force-detaches;
    /// pause returns Err(CaptureWedged); the axis says the process can no
    /// longer arbitrate the card.
    #[test]
    fn wedged_capture_join_yields_capture_wedged_error() {
        let p = FakePlatform::happy();
        let (state, arb) = setup(p.clone(), cfg_with_device());
        state.test_run_sequence();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        // Park the source: reads block, ignoring the abort flag (the read
        // itself hangs — SourceStep::Park in testutil).
        let park = crate::ft8::testutil::park_flag();
        p.source_steps
            .lock()
            .unwrap()
            .push_back(crate::ft8::testutil::SourceStep::Park(park.clone()));
        // Give the capture loop time to enter the parked read.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(arb.pause_for_modem(), Err(PauseError::CaptureWedged));
        assert_eq!(
            state.axis(),
            ServiceAxis::Blocked(BlockedReason::CaptureWedged)
        );
        // Hygiene: release the detached thread so the test binary exits.
        park.store(false, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = std::fs::remove_dir_all(state.platform_tmp_for_test());
    }

    /// ProdPlatform's positive modem-eligibility set (spec §Resume): the
    /// resume-eligible ModemStates are EXACTLY Stopped, Error, SocketLost —
    /// `Idle` (listen-only, ardopcf holds the card) stays active.
    ///
    /// BEFORE writing this test, read `modem_status.rs`: the `ModemState`
    /// variant list and the status-constructor/setter names below
    /// (`ModemStatus::stopped()`, `ModemSession::{new, set_status}`) must
    /// match the REAL enum and impl — adjust the code to the file, not the
    /// file to this sketch. Extend the loop to cover EVERY `ModemState`
    /// variant (not just the seven listed), so adding a variant later fails
    /// this test until its eligibility is deliberately classified.
    #[test]
    fn prod_platform_modem_eligibility_is_the_pinned_set() {
        use crate::ft8::traits::{Ft8Platform, ProdPlatform};
        use crate::modem_status::{ModemSession, ModemState, ModemStatus};
        let session = Arc::new(ModemSession::new());
        let plat = ProdPlatform {
            wisdom_dir: std::env::temp_dir(),
            slot_root: std::env::temp_dir(),
            modem: session.clone(),
        };
        let set_state = |st: ModemState| {
            let mut s = ModemStatus::stopped();
            s.state = st;
            session.set_status(s);
        };
        for (st, want) in [
            (ModemState::Stopped, true),
            (ModemState::Error, true),
            (ModemState::SocketLost, true),
            (ModemState::Idle, false),
            (ModemState::Spawning, false),
            (ModemState::Connecting, false),
            (ModemState::ConnectedIss, false),
        ] {
            set_state(st);
            assert_eq!(plat.modem_resume_eligible(), want, "{st:?}");
        }
    }

    /// The global seam: uninstalled → Ok (unit tests of modem paths never
    /// need FT8 state).
    #[test]
    fn global_pause_is_ok_when_uninstalled() {
        // NB: FT8_ARBITER is process-global; this test relies on test
        // binaries not installing it (only lib.rs setup does).
        assert_eq!(pause_for_modem_global(), Ok(()));
    }
}
```
Supporting test-only helpers to add:
- service.rs: `#[cfg(test)] pub(crate) fn test_run_sequence(self: &Arc<Self>)`
  = `{ self.lock_inner().machine.on_start_requested(); }` +
  `self.execute_start_sequence(false)`; and
  `#[cfg(test)] pub(crate) fn test_teardown(self: &Arc<Self>)` = `self.stop()`
  + tmp cleanup (reuse the T12/T13 helper bodies — refactor those tests to
  call these shared helpers instead of their local copies).
- `SharedHold::test_backdate(d: Duration)` under `#[cfg(test)]`:
  `*latched_at = Some(Instant::now() - d)`.
- testutil.rs: `FakeEngine::band_dead_with_prewarm_gate()` — **already added
  in T13 (Step 0b)**; no edit here, just use it.
- testutil.rs: the wedge primitive for the hung-read test:
  ```rust
  pub fn park_flag() -> Arc<std::sync::atomic::AtomicBool> {
      Arc::new(std::sync::atomic::AtomicBool::new(true))
  }
  ```
  plus a `SourceStep` variant whose READ blocks (unlike `Idle`, which
  returns — this models the hung-USB read the abort flag cannot reach):
  ```rust
      /// Block inside read() while the flag is true (50 ms poll). The
      /// capture loop cannot observe its abort flag during this — the
      /// wedged-join class.
      Park(Arc<std::sync::atomic::AtomicBool>),
  ```
  with the matching `read` arm:
  ```rust
              Some(SourceStep::Park(flag)) => {
                  while flag.load(std::sync::atomic::Ordering::SeqCst) {
                      std::thread::sleep(Duration::from_millis(50));
                  }
                  Ok(ReadBatch { frames: 0, mono_ts_us: self.clock.mono_us(), gap: None })
              }
  ```

- [ ] **Step 3: [CI-side] verification** — workspace clippy + tests.

- [ ] **Step 4: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/ft8
git commit -m "feat(ft8): modem yield/resume arbiter with positive hold latch (tuxlink-b026z.3 T14)"
```

**Completion check:** all four axis arms tested (stopped / blocked /
listening / starting) plus both error paths (wedged join is covered by T13's
mechanics test; ReleaseTimeout here); latch has BOTH clear paths tested (TTL
+ positive-evidence); the blocking-context contract is a doc comment on
`pause_for_modem` (verbatim "BLOCKING-CONTEXT-ONLY CONTRACT"); lock order
arbiter > rig > state, each acquired at most once per thread, holds at every
site (audit: `rig_session` takes ONLY the arbiter lock — the closure owns
the rig lock; no arbiter method touches `lock_inner()` while holding the rig
lock except through service methods that take state last); the composition
test `rig_session_composed_with_qsy_does_not_deadlock` exists and drives a
LOCAL arbiter end-to-end under a deadline; the negative resume test covers
modem-ineligibility as the sole blocker.

---

**REVIEW GATE E (after Tasks 11–14):** review the service core. Perspectives:
(1) **lifecycle-table conformance** — walk spec §Lifecycle ownership's
action/threads table row by row against `start`/`stop`/`pause_for_modem`/
`tick_yielded`/`tick_device_absent`; every axis transition has exactly ONE
writer (pause writes yielded; the supervisor writes everything else; flags
never write the axis); (2) **join/lock audit** — no state-mutex hold across
any join, ALSA call, rig session, or emit; handles outside the mutex and
take()n before joins; arbiter > rig > state order everywhere; (3) **counter
semantics** — trace every §Counter semantics rule to `record_slot` +
`RingOutcome::kind()` + the leaf machine (N increments: Failed + all three
drop classes; k untouched by Failed/Dropped; scheduled discards neutral);
(4) **spec §Start sequence step order** — prewarm before PCM, rig session
before busy probe, latch consulted at step 6, interrupt checks between every
step, post-step-7 interrupt drops the PCM. Minimum three rounds; persist
findings to `dev/scratch/b026z.3-gate-E-findings.md` before proceeding.
Files under review: `src-tauri/src/ft8/service.rs`,
`src-tauri/src/ft8/traits.rs`, `src-tauri/src/ft8/testutil.rs`,
`src-tauri/src/ft8/arbiter.rs`, `src-tauri/src/ft8/mod.rs`.

**Gate E push:** after this gate's P1/P2 fixes are committed, the parent
pushes the branch (Global Constraints §Push cadence). This push's CI run
executes T11–T14's **[CI-side]** steps; fix-forward on its findings before
starting Task 15.

---
### Task 15: Modem seam wiring — ardopcf choke wrapper, Dire Wolf pre-probe hook, VARA seam

**Files:**
- Modify: `src-tauri/src/modem_commands.rs` (the wrapper + the four sites + the coverage test)
- Modify: `src-tauri/src/winlink/ax25/managed_direwolf.rs` (`spawn_inner` pre-probe hook)
- Modify: `src-tauri/src/winlink/modem/vara/commands.rs` (`vara_open_session` seam)

**Interfaces:**
- Consumes: `pause_for_modem_global` + `PauseError::device_busy_message`
  (T14), `DwLifecycleError::DeviceBusy` (managed_direwolf.rs).
- Produces:
```rust
// modem_commands.rs
pub(crate) fn spawn_ardop_with_yield<F>(
    make_transport: F, ardop_cfg: ArdopConfig, target: &str,
) -> Result<Box<dyn ModemTransport>, String>
where F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>;
```

**PauseError → existing error surfaces (pinned):** the ardopcf paths' error
type is `String` (every `make_transport` site already returns
`Result<_, String>` and feeds `ModemStatus.last_error`) — the wrapper folds
`PauseError` into that String via `device_busy_message()`. The Dire Wolf
path's existing device-busy class is `DwLifecycleError::DeviceBusy(String)`
(quoted below) — the hook maps into it. The VARA open path's error type is
`String` (the `vara_open_session` command returns `Result<VaraStatus,
String>`).

**TDD note:** the coverage test (Step 4) is written FIRST — it fails against
the un-wrapped tree by counting raw factory invocations.

- [ ] **Step 1: The choke wrapper + the four ardopcf sites**

Add to `modem_commands.rs` (above the first connect function):
```rust
/// The SINGLE choke point between "a code path decided to spawn ardopcf"
/// and the factory that does it (tuxlink-b026z.3, spec §Arbitration).
/// Yields the FT8 listener's audio device (join + release-confirm) BEFORE
/// the spawn; a pause failure surfaces as the device-busy-class String the
/// existing sites already propagate, and the spawn DOES NOT proceed.
///
/// Blocking-context contract: all four call sites run under spawn_blocking
/// (the `pause_for_modem` doc pins it). Every ardopcf spawn MUST route
/// through here — `no_ardop_spawn_path_bypasses_the_ft8_yield_wrapper`
/// (below) enforces it structurally.
pub(crate) fn spawn_ardop_with_yield<F>(
    make_transport: F,
    ardop_cfg: ArdopConfig,
    target: &str,
) -> Result<Box<dyn ModemTransport>, String>
where
    F: FnOnce(ArdopConfig, &str) -> Result<Box<dyn ModemTransport>, String>,
{
    crate::ft8::arbiter::pause_for_modem_global().map_err(|e| e.device_busy_message())?;
    make_transport(ardop_cfg, target)
}
```

**Site 1 — legacy single dial**
(`modem_ardop_connect_post_consume_with_factory`, currently `:475`; quoted
verbatim):
```rust
    // ─── Spawn ───────────────────────────────────────────────────────────
    let mut transport = match make_transport(ardop_cfg, target) {
```
becomes:
```rust
    // ─── Spawn (via the FT8 yield choke point) ───────────────────────────
    let mut transport = match spawn_ardop_with_yield(make_transport, ardop_cfg, target) {
```

**Site 2 — QSY walk** (`dial_one_candidate`, currently `:717`; quoted
verbatim):
```rust
    // ─── Spawn ───────────────────────────────────────────────────────────
    let mut transport = make_transport(ardop_cfg, target)?;
```
becomes:
```rust
    // ─── Spawn (via the FT8 yield choke point) ───────────────────────────
    let mut transport = spawn_ardop_with_yield(&mut *make_transport, ardop_cfg, target)?;
```
(`make_transport` here is `&mut F` with `F: FnMut`; `&mut F` implements
`FnOnce`, so the same wrapper serves both factory shapes.)

**Site 3 — listen-only** (`start_modem_listen_only`, currently `:822`;
quoted verbatim):
```rust
    let mut transport = match make_transport(ardop_cfg, "") {
```
becomes:
```rust
    let mut transport = match spawn_ardop_with_yield(make_transport, ardop_cfg, "") {
```

**Site 4 — open-session** (`spawn_and_init_ardop_inner`, currently `:918`;
quoted verbatim — identical shape to site 3):
```rust
    let mut transport = match make_transport(ardop_cfg, "") {
```
becomes:
```rust
    let mut transport = match spawn_ardop_with_yield(make_transport, ardop_cfg, "") {
```
(Sites 3 and 4 are textually identical one-liners in different functions —
locate each by its enclosing `pub fn` name, NOT by a bare string search, and
verify both were edited: `grep -c "spawn_ardop_with_yield(" src/modem_commands.rs`
must show 5 — 1 definition + 4 sites — after this step.)

- [ ] **Step 2: Dire Wolf pre-probe hook**

In `managed_direwolf.rs` `spawn_inner`, the current Step-2 block (quoted
verbatim):
```rust
        // Step 2: pre-spawn device-busy probe — DO NOT spawn against a held card.
        if let Err(named_msg) = probe_device_busy(&cfg.adevice, cfg.card_index) {
```
gains, immediately BEFORE it (a new Step 1.5 — before the busy probe, which
would otherwise abort on FT8's own hold of the card):
```rust
        // Step 1.5 (tuxlink-b026z.3): yield the FT8 listener BEFORE the busy
        // probe — the probe would otherwise read FT8's own PCM hold as
        // "device busy" and abort a spawn that a yield would have permitted.
        if let Err(e) = crate::ft8::arbiter::pause_for_modem_global() {
            return Err(DwLifecycleError::DeviceBusy(e.device_busy_message()));
        }
```
(Also renumber the step list in `spawn_inner`'s doc comment: insert the
yield as its own numbered step so the RADIO-1/ADR-0015 ordering note stays
accurate.)

- [ ] **Step 3: VARA seam**

In `vara/commands.rs` `vara_open_session` (async command), immediately
before the `match vara_open_session_inner(` call (quoted verbatim anchor):
```rust
    match vara_open_session_inner(
        &session,
        &ui_cfg,
        Some(callsign.as_str()),
        intent,
        transport_kind,
    ) {
```
insert BEFORE it:
```rust
    // tuxlink-b026z.3: yield the FT8 listener before VARA opens its audio
    // path. pause_for_modem is blocking-context-only (2 s join + lsof
    // poll), and this command is async — spawn_blocking honors the
    // contract instead of parking a tokio worker.
    //
    // Residual, disclosed (spec §Arbitration): this covers TUXLINK-initiated
    // VARA use only. VARA launched standalone opens its audio device at its
    // own startup, before any tuxlink involvement — that conflict surfaces
    // in VARA's UI, not here.
    tauri::async_runtime::spawn_blocking(crate::ft8::arbiter::pause_for_modem_global)
        .await
        .map_err(|e| format!("FT8 yield task failed: {e}"))?
        .map_err(|e| e.device_busy_message())?;
```

- [ ] **Step 4: The coverage test (write FIRST; fails pre-wrapper)**

Add to `modem_commands.rs`'s test module:
```rust
    /// tuxlink-b026z.3 (spec §Arbitration): NO ardopcf spawn path may bypass
    /// the FT8 yield choke point. Structural enforcement: in this file, the
    /// factory may be INVOKED — the identifier immediately followed by an
    /// open paren — only inside `spawn_ardop_with_yield` itself. Everything
    /// else must route through the wrapper. Passing the factory BY VALUE
    /// (identifier followed by `,` or `)`) is fine — only invocation is
    /// choked.
    #[test]
    fn no_ardop_spawn_path_bypasses_the_ft8_yield_wrapper() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/modem_commands.rs"),
        )
        .expect("read own source");
        // Built via concat! so THIS test's own source never matches itself.
        let needle = concat!("make_", "transport(");
        let invocations: Vec<usize> = src.match_indices(needle).map(|(i, _)| i).collect();
        assert_eq!(
            invocations.len(),
            1,
            "expected exactly ONE raw factory invocation (inside \
             spawn_ardop_with_yield); every other spawn site must call \
             spawn_ardop_with_yield. Found {} — a new ardopcf spawn path \
             bypassed the FT8 yield choke point.",
            invocations.len()
        );
        // And that one invocation lives inside the wrapper fn.
        let wrapper_start = src
            .find("fn spawn_ardop_with_yield")
            .expect("wrapper exists");
        let wrapper_end = src[wrapper_start..]
            .find("\npub")
            .map(|off| wrapper_start + off)
            .unwrap_or(src.len());
        assert!(
            invocations[0] > wrapper_start && invocations[0] < wrapper_end,
            "the raw factory invocation is OUTSIDE spawn_ardop_with_yield"
        );
    }
```
**Comment hygiene this test imposes:** no comment or doc string in
`modem_commands.rs` may contain the factory identifier immediately followed
by an open paren — rephrase in prose if one appears. Sweep the file for
pre-existing occurrences in comments while editing (the current doc comments
say "make_transport is `FnMut`" without parens — verify with the same
needle).

Run expectation at authoring time (CI): before Step 1's edits this test
counts 4 invocations and FAILS; after, exactly 1 — the TDD red-green pair
happens across the task's own commits in CI history (author test + wrapper
in one commit; CI runs green because both land together — the "failing
first" evidence is the pre-edit count, recorded here).

- [ ] **Step 5: Existing-behavior guard tests**

The four wrapped sites keep their `Result<_, String>` shapes, so the
existing factory-driven tests in `modem_commands.rs` (FnOnce fakes moving
state into transports) compile unchanged — with the global arbiter
UNINSTALLED in unit tests, `pause_for_modem_global()` is `Ok(())` and the
wrapper is transparent. Verify by reading, and add one negative test:
```rust
    /// The wrapper is transparent when no arbiter is installed (unit-test
    /// context) — factory errors pass through untouched.
    #[test]
    fn yield_wrapper_is_transparent_without_an_arbiter() {
        let out = spawn_ardop_with_yield(
            |_cfg, _t| Err::<Box<dyn crate::winlink::modem::ModemTransport>, String>("boom".into()),
            test_ardop_config(),
            "N0CALL",
        );
        assert_eq!(out.unwrap_err(), "boom");
    }
```
(`test_ardop_config()`: reuse whatever minimal `ArdopConfig` constructor the
existing tests in this file use — read them; if none exists, build the
struct literally with loopback defaults.)

- [ ] **Step 6: [CI-side] verification** — workspace clippy + tests.

- [ ] **Step 7: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/modem_commands.rs src-tauri/src/winlink/ax25/managed_direwolf.rs src-tauri/src/winlink/modem/vara/commands.rs
git commit -m "feat(ft8): modem seam wiring — ardop/direwolf/vara yield choke points (tuxlink-b026z.3 T15)"
```

**Completion check:** `grep -c "spawn_ardop_with_yield(" src-tauri/src/modem_commands.rs`
= 5; the Dire Wolf hook precedes the Step-2 busy probe; the VARA pause is
inside `spawn_blocking`; the coverage test's wrapper-boundary assertion
holds; no site's error TYPE changed (String / DwLifecycleError as before).

---
### Task 16: sweep.rs — dwell scheduler, QSY, provenance downgrade

**Files:**
- Create: `src-tauri/src/ft8/sweep.rs`
- Modify: `src-tauri/src/ft8/mod.rs` (`pub mod sweep;`)
- Modify: `src-tauri/src/ft8/service.rs` (supervisor call-site swap + the
  `set_band_live` helper the T17 command reuses)

**Interfaces:**
- Consumes: `ListenerMachine` sweep methods (`sweep_activate`,
  `sweep_deactivate`, `on_qsy_success(next_band_idx)`, `on_qsy_failure()`,
  `dwell_complete(dwell_slots) -> bool`, `on_band_change()`, `sweep()`
  accessor — Task 6 manifest); `Ft8Arbiter::rig_session` (T14);
  `tuxlink_capture::bands`; the `discard_next_slot` flag (T12).
- Produces:
```rust
pub(crate) fn tick(state: &Arc<Ft8ListenerState>);   // supervisor-driven dwell scheduler
impl Ft8ListenerState {
    /// QSY + relabel + k-reset; shared by sweep and ft8_set_band-while-listening.
    pub(crate) fn qsy_to_band(self: &Arc<Self>, band: &str, source: BandSource) -> Result<(), String>;
}
```

**Pinned mechanics (spec §Sweep, restated):** dwell counts
decoded-or-band-dead slots (the machine owns the count — failures freeze the
dwell, INTENDED: rotating a broken pipeline samples nothing); at each dwell
boundary the SUPERVISOR asks the arbiter for a spawn-tune-drop QSY to the
next configured band; the in-progress slot is the transition slot (scheduled
discard); QSY failure → warn + retry next boundary; two consecutive →
`Sweep::FallbackHold` (machine-internal; config untouched; re-arms at next
start/resume — the T11 step-8 `sweep_activate()` already does the re-arm);
**a failed QSY does NOT imply the radio stayed put** (`ManagedRig::tune`
sets freq before mode — a serial drop mid-tune can move the dial AND report
the error), so after ANY QSY failure the label downgrades to
`default-unconfirmed` / `confirmed = None`; sweep never fires while yielded,
while a pause is in progress, or outside listening; RX-only — QSY never
transmits (sweep opt-in is the consent to move the dial).

**TDD note:** Step 2's tests first.

- [ ] **Step 1: sweep.rs + the shared QSY helper**

`src-tauri/src/ft8/sweep.rs`:
```rust
//! Opt-in CAT band sweep (spec §Sweep): round-robin over the configured
//! band list with a fixed dwell, driven by the supervisor tick, executed
//! through the arbiter's rig-session serialization. RX-only; the QSY moves
//! the dial and nothing keys.

use std::sync::Arc;

use super::records::BandSource;
use super::service::Ft8ListenerState;
use tuxlink_capture::state::{ServiceAxis, Sweep};

/// One supervisor tick's sweep bookkeeping. Cheap no-op unless: listening,
/// sweep Active, dwell complete.
pub(crate) fn tick(state: &Arc<Ft8ListenerState>) {
    // Never outside listening (covers yielded + blocked). "While a pause is
    // in progress" needs more than the axis: pause latches the hold FIRST,
    // before it joins capture and writes yielded — so a latched hold is the
    // authoritative "do not move the dial" signal for the in-between window.
    if state.axis() != ServiceAxis::Listening || state.hold().is_latched() {
        return;
    }
    let (dwell_done, band_idx, bands, dwell_slots) = {
        let g = state.lock_inner_for_sweep();
        let (active_idx, cfg) = match g.machine_sweep() {
            Sweep::Active { band_idx, .. } => (band_idx, g.sweep_config()),
            _ => return, // Inactive or FallbackHold: nothing to schedule
        };
        (
            g.machine_dwell_complete(cfg.dwell_slots),
            active_idx,
            cfg.bands.clone(),
            cfg.dwell_slots,
        )
    };
    let _ = dwell_slots;
    if !dwell_done || bands.is_empty() {
        return;
    }
    let next_idx = (band_idx + 1) % bands.len();
    let next_band = bands[next_idx].clone();
    // Through the arbiter when installed (mirrors T17's ft8_set_band): the
    // ARBITER lock is what excludes a concurrent pause_for_modem — the rig
    // lock alone never could. qsy_to_band owns the rig lock; rig_session
    // takes ONLY the arbiter lock (lock order arbiter > rig > state, each
    // acquired at most once per thread — T14's non-reentrancy contract).
    let do_qsy = || state.qsy_to_band(&next_band, BandSource::CatConfirmed);
    let result = match crate::ft8::arbiter::FT8_ARBITER.get() {
        Some(arb) => arb.rig_session(do_qsy),
        None => do_qsy(),
    };
    match result {
        Ok(()) => {
            state.on_sweep_qsy_success(next_idx);
        }
        Err(e) => {
            tracing::warn!(target: "tuxlink::ft8", "sweep QSY to {next_band} failed: {e} — retry next dwell boundary");
            state.on_sweep_qsy_failure(e);
        }
    }
}
```
service.rs additions (the helper both sweep and `ft8_set_band` (T17) use,
plus the narrow sweep accessors — keeping `Inner` private):
```rust
impl Ft8ListenerState {
    /// Spawn-tune-drop QSY to a table band + relabel + k-reset. This helper
    /// OWNS the rig-lock acquisition; every caller (sweep::tick, T17's
    /// ft8_set_band) ADDITIONALLY wraps the call in the arbiter's
    /// rig_session (arbiter-lock-only, T14) — the ARBITER lock is what
    /// excludes a concurrent pause_for_modem; the rig lock only serializes
    /// rig sessions against each other. Lock order arbiter > rig > state,
    /// each acquired at most once per thread. On failure the band label
    /// DOWNGRADES: a failed tune may have moved the dial anyway
    /// (freq-before-mode).
    pub(crate) fn qsy_to_band(
        self: &Arc<Self>,
        band: &str,
        source: BandSource,
    ) -> Result<(), String> {
        let dial = tuxlink_capture::bands::dial_hz(band)
            .ok_or_else(|| format!("{band:?} is not an FT8 band"))?;
        let result = {
            let rig = self.rig_lock();
            let _g = rig.lock().unwrap_or_else(|p| p.into_inner());
            self.platform.rig_tune(dial)
        };
        match result {
            Ok(()) => {
                {
                    let mut g = self.lock_inner();
                    g.band = band.to_string();
                    g.dial_hz = dial;
                    g.band_source = source;
                    g.band_label_confirmed_utc_ms = Some(self.platform.utc_now_ms());
                    g.machine.on_band_change(); // k resets on band change
                    // The slot in progress during the QSY is the transition
                    // slot: a scheduled discard.
                    g.discard_next_slot = Some(DiscardClassDto::QsyTransition);
                }
                self.emit_listening_change();
                Ok(())
            }
            Err(e) => {
                {
                    let mut g = self.lock_inner();
                    // Slots must NOT keep being attributed to the stale band
                    // with confirmed provenance — the dial position is now
                    // unknown.
                    g.band_source = BandSource::DefaultUnconfirmed;
                    g.band_label_confirmed_utc_ms = None;
                    g.last_failure = Some(format!("QSY failed: {e}"));
                }
                self.emit_listening_change();
                Err(e)
            }
        }
    }

    pub(crate) fn on_sweep_qsy_success(self: &Arc<Self>, next_idx: usize) {
        {
            self.lock_inner().machine.on_qsy_success(next_idx);
        }
        self.emit_listening_change();
    }
    pub(crate) fn on_sweep_qsy_failure(self: &Arc<Self>, _diag: String) {
        {
            self.lock_inner().machine.on_qsy_failure();
        }
        self.emit_listening_change();
    }

    // Narrow read accessors for sweep::tick (keep Inner private).
    pub(crate) fn lock_inner_for_sweep(&self) -> SweepView<'_> {
        SweepView { guard: self.lock_inner() }
    }
}

/// Read-only sweep view over the state mutex (one lock, three reads).
pub(crate) struct SweepView<'a> {
    guard: std::sync::MutexGuard<'a, Inner>,
}
impl SweepView<'_> {
    pub(crate) fn machine_sweep(&self) -> tuxlink_capture::state::Sweep {
        self.guard.machine.sweep()
    }
    pub(crate) fn machine_dwell_complete(&self, dwell_slots: u8) -> bool {
        self.guard.machine.dwell_complete(dwell_slots)
    }
    pub(crate) fn sweep_config(&self) -> crate::config::Ft8SweepConfig {
        self.guard.ft8_cfg.sweep.clone()
    }
}
```
Swap the supervisor call site: in `tick_listening` (T11),
`self.sweep_tick_stub();` becomes `crate::ft8::sweep::tick(self);` — and
delete the `sweep_tick_stub` method.
(NOTE: if `Sweep` does not derive `Copy`/`Clone` in Phase A, adjust the view
methods to return by clone — a leaf edit adding `Clone`+`Copy` derives is
also acceptable and locally testable.)

Accepted timing skew, recorded (spec §Sweep): the QSY fires on the 5 s
supervisor tick after the dwell completes, so the dial can move up to ~5 s
past the slot boundary — safe by construction, because the slot in progress
during the QSY is the transition slot (scheduled discard), so mid-slot dial
movement never contaminates a counted slot.

- [ ] **Step 2: Tests (in sweep.rs)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Ft8Config;
    use crate::ft8::records::{BandSource, RingOutcome};
    use crate::ft8::service::Ft8Deps;
    use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink};
    use crate::winlink::ax25::devices::{StableAudioId, StableIdKind};
    use tuxlink_capture::state::{ServiceAxis, Sweep};

    fn sweep_cfg() -> Ft8Config {
        let mut c = Ft8Config::default();
        c.device = Some(StableAudioId { kind: StableIdKind::ByIdSymlink, value: "usb-X-00".into() });
        c.band = "80m".into();
        c.sweep.enabled = true;
        c.sweep.bands = vec!["80m".into(), "40m".into(), "20m".into()];
        c.sweep.dwell_slots = 4;
        c
    }

    fn listening_state_with_sweep() -> (Arc<Ft8ListenerState>, Arc<FakePlatform>) {
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        *p.rig_dial.lock().unwrap() = Ok(3_573_000); // radio already on 80m
        let state = Ft8ListenerState::new(
            Ft8Deps {
                platform: p.clone(),
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: Arc::new(RecordingSink::default()),
            },
            sweep_cfg(),
        );
        state.test_run_sequence();
        assert_eq!(state.axis(), ServiceAxis::Listening);
        assert!(matches!(state.snapshot_sweep(), Sweep::Active { band_idx: 0, .. }));
        (state, p)
    }

    /// Dwell counting via the machine: decoded/band-dead slots advance the
    /// dwell; Failed/Dropped freeze it (intended — jt9-degraded is the
    /// operator's signal); the QSY fires only at dwell_slots good slots.
    #[test]
    fn dwell_counts_good_slots_and_freezes_on_failures() {
        let (state, p) = listening_state_with_sweep();
        // 3 good slots: dwell not complete → tick does not QSY.
        for i in 0..3u64 {
            state.record_slot(state.test_base_record(i, RingOutcome::BandDead));
        }
        tick(&state);
        assert!(p.tuned_to.lock().unwrap().len() <= 1, "no dwell QSY yet (≤ the start tune)");
        // A failure + a drop: dwell frozen — still no QSY.
        state.record_slot(state.test_base_record(3, RingOutcome::Failed { failure: "Timeout".into() }));
        state.record_slot(state.test_base_record(4, RingOutcome::DroppedBackpressure));
        tick(&state);
        let tunes_before = p.tuned_to.lock().unwrap().len();
        // 4th good slot completes the dwell → tick QSYs to 40m.
        state.record_slot(state.test_base_record(5, RingOutcome::Decoded));
        tick(&state);
        let tunes = p.tuned_to.lock().unwrap().clone();
        assert_eq!(tunes.len(), tunes_before + 1, "exactly one dwell QSY");
        assert_eq!(*tunes.last().unwrap(), 7_074_000, "next configured band (40m)");
        assert!(matches!(state.snapshot_sweep(), Sweep::Active { band_idx: 1, .. }));
        state.test_teardown();
    }

    /// The transition slot is a scheduled discard: the next completed slot
    /// after a QSY records Discarded(qsy-transition) and is counter-neutral.
    #[test]
    fn transition_slot_is_discarded_and_counter_neutral() {
        let (state, _p) = listening_state_with_sweep();
        state.qsy_to_band("40m", BandSource::CatConfirmed).unwrap();
        let n_before = state.snapshot().n_consecutive;
        state.test_complete_one_slot(9_000); // T12's handle_completed_slot via helper
        let snap = state.snapshot();
        let last = snap.ring_tail.last().unwrap();
        assert_eq!(
            last.outcome,
            RingOutcome::Discarded { class: crate::ft8::records::DiscardClassDto::QsyTransition }
        );
        assert_eq!(snap.n_consecutive, n_before, "scheduled discard: neither counter");
        state.test_teardown();
    }

    /// Two consecutive QSY failures → FallbackHold; a start/resume re-arms
    /// (sweep_activate at step 8).
    #[test]
    fn double_qsy_failure_enters_fallback_hold_and_rearms_on_resume() {
        let (state, p) = listening_state_with_sweep();
        p.rig_tune_results.lock().unwrap().push_back(Err("serial dropped".into()));
        p.rig_tune_results.lock().unwrap().push_back(Err("serial dropped".into()));
        for i in 0..4u64 {
            state.record_slot(state.test_base_record(i, RingOutcome::BandDead));
        }
        tick(&state); // failure 1
        for i in 4..8u64 {
            state.record_slot(state.test_base_record(i, RingOutcome::BandDead));
        }
        tick(&state); // failure 2 → FallbackHold
        assert!(matches!(state.snapshot_sweep(), Sweep::FallbackHold { .. }));
        // Config untouched.
        assert!(state.snapshot_ft8_cfg().sweep.enabled);
        // FallbackHold: further ticks never QSY.
        let tunes = p.tuned_to.lock().unwrap().len();
        tick(&state);
        assert_eq!(p.tuned_to.lock().unwrap().len(), tunes);
        // Re-arm via yield → resume (steps 1–7 + 8′ re-run sweep_activate).
        state.hold().latch_now();
        state.pause_capture_for_yield().unwrap();
        state.hold().clear();
        state.tick_yielded();
        assert!(matches!(state.snapshot_sweep(), Sweep::Active { .. }), "re-armed on resume");
        state.test_teardown();
    }

    /// Partial-QSY provenance downgrade (spec's named test): tune fails →
    /// band_source = default-unconfirmed, confirmed = None; subsequent slots
    /// are NOT attributed to the stale band with confirmed provenance.
    #[test]
    fn partial_qsy_failure_downgrades_the_band_label() {
        let (state, p) = listening_state_with_sweep();
        p.rig_tune_results.lock().unwrap().push_back(Err("serial dropped mid-tune".into()));
        assert!(state.qsy_to_band("20m", BandSource::CatConfirmed).is_err());
        let snap = state.snapshot();
        assert_eq!(snap.band_source, BandSource::DefaultUnconfirmed);
        assert_eq!(snap.band_label_confirmed_utc_ms, None);
        // A slot recorded now carries the downgraded provenance.
        state.record_slot(state.test_base_record(1, RingOutcome::BandDead));
        assert_eq!(
            state.snapshot().ring_tail.last().unwrap().band_source,
            BandSource::DefaultUnconfirmed
        );
        state.test_teardown();
    }

    /// Sweep never fires while yielded (spec: nor during a pause, nor
    /// outside listening — one guard covers all three).
    #[test]
    fn sweep_never_fires_while_yielded() {
        let (state, p) = listening_state_with_sweep();
        for i in 0..4u64 {
            state.record_slot(state.test_base_record(i, RingOutcome::BandDead));
        }
        state.hold().latch_now();
        state.pause_capture_for_yield().unwrap();
        assert_eq!(state.axis(), ServiceAxis::Yielded);
        let tunes = p.tuned_to.lock().unwrap().len();
        tick(&state);
        assert_eq!(p.tuned_to.lock().unwrap().len(), tunes, "no QSY while yielded");
        state.test_teardown();
    }

    /// cat-absent hold-band: sweep stays Inactive and the snapshot carries
    /// the instructed dial + unconfirmed provenance (spec §Hold-band,
    /// cat-absent arm — the T11 arrow-5 test pins the flag; this pins the
    /// sweep element).
    #[test]
    fn cat_absent_keeps_sweep_inactive_with_instructed_dial() {
        let p = FakePlatform::happy(); // rig_configured = false
        let mut cfg = sweep_cfg();
        cfg.sweep.enabled = true; // enabled in config, but no CAT
        let state = Ft8ListenerState::new(
            Ft8Deps {
                platform: p,
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: Arc::new(RecordingSink::default()),
            },
            cfg,
        );
        state.test_run_sequence();
        assert!(matches!(state.snapshot_sweep(), Sweep::Inactive), "cat-fixed-band ⇒ Inactive");
        let snap = state.snapshot();
        assert!(snap.flags.cat_fixed_band);
        assert_eq!(snap.dial_hz, 3_573_000, "instructed dial for the 80m chip");
        assert_eq!(snap.band_source, BandSource::DefaultUnconfirmed);
        state.test_teardown();
    }
}
```
Supporting test-only service helpers (service.rs, `#[cfg(test)]`):
```rust
    pub(crate) fn snapshot_sweep(&self) -> tuxlink_capture::state::Sweep {
        self.lock_inner().machine.sweep()
    }
    pub(crate) fn snapshot_ft8_cfg(&self) -> crate::config::Ft8Config {
        self.lock_inner().ft8_cfg.clone()
    }
    pub(crate) fn test_base_record(
        &self,
        slot_utc_ms: u64,
        outcome: crate::ft8::records::RingOutcome,
    ) -> crate::ft8::records::SlotRecord {
        self.base_record(slot_utc_ms, outcome, Vec::new(), 0, 0, 0.0, -60.0)
    }
    pub(crate) fn test_complete_one_slot(self: &Arc<Self>, slot_utc_ms: u64) {
        let tx = self.master_tx.lock().unwrap().clone().expect("listening");
        // Reuse the T12 test helper's CompletedSlot constructor.
        self.handle_completed_slot(test_completed_slot(slot_utc_ms), &tx);
    }
```
(hoist T12's local `completed()` fixture into a module-level
`#[cfg(test)] fn test_completed_slot(..)` so both test modules share it.)

- [ ] **Step 3: [CI-side] verification** — workspace clippy + tests.

- [ ] **Step 4: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/ft8
git commit -m "feat(ft8): opt-in CAT band sweep — dwell scheduler, QSY, provenance downgrade (tuxlink-b026z.3 T16)"
```

**Completion check:** the six named tests exist (dwell counting,
transition-slot discard, FallbackHold + re-arm, partial-QSY downgrade,
never-while-yielded, cat-absent-inactive); QSY goes through the rig lock;
`discard_next_slot` is set ONLY on successful QSY (a failed QSY discards
nothing — the slot is real audio on an unknown dial, and the downgraded
provenance is what protects it); the failure path downgrades provenance
BEFORE returning Err.

---
### Task 17: commands.rs + events wiring + lib.rs — the six commands, event names, managed state, autostart

**Files:**
- Create: `src-tauri/src/ft8/commands.rs`
- Modify: `src-tauri/src/ft8/events.rs` (event names + `TauriEventSink`)
- Modify: `src-tauri/src/ft8/mod.rs` (`pub mod commands;`)
- Modify: `src-tauri/src/ft8/service.rs` (`assert_band_operator`, `apply_sweep_enabled`, the `#[cfg(test)]` wedged helper)
- Modify: `src-tauri/src/config.rs` (the crate-wide `update_config` RMW gate — Step 2a)
- Modify: `src-tauri/src/modem_commands.rs` (hoist the TUXLINK_CONFIG_DIR test guard into a crate-visible `test_env` module — Step 3a)
- Modify: `src-tauri/src/lib.rs` (managed state + setup autostart + command registration)

**Interfaces:**
- Consumes: T11–T16's service surface; `write_config_atomic` / `read_config`
  / `Config::validate` (config.rs); `FT8_ARBITER` (T14).
- Produces (the L3 IPC surface — spec §Commands):
```rust
// commands
ft8_listener_start(), ft8_listener_stop(), ft8_listener_snapshot() -> Ft8Snapshot,
ft8_set_device(stable_id: StableAudioId), ft8_set_band(band: String), ft8_set_sweep(enabled: bool)
// events.rs
pub const FT8_SLOT_EVENT: &str = "ft8-decodes:slot";
pub const FT8_LISTENING_EVENT: &str = "ft8-listening:change";
pub struct TauriEventSink { pub app: tauri::AppHandle }
```

**Pinned command semantics (spec §Commands):** `ft8_set_band` validates
against the band table BEFORE persisting; while listening with CAT → QSY +
relabel + reset k; while NOT listening → persist-only, never touches the
radio (consent framing). ALL config-mutating ft8 commands serialize their
read-modify-validate-write through the CRATE-WIDE gate
`config::update_config` (one static writer lock in config.rs — Step 2a;
atomic file replace does not make concurrent RMW cycles atomic, and an
ft8-only mutex would not protect against non-ft8 writers).
`ft8_set_device` and `ft8_listener_start` refuse from `capture-wedged` with
a restart-required error. `ft8_listener_start` is idempotent and sets
`enabled = true`; `ft8_listener_stop` sets `enabled = false`. Autostart
fires on `config.ft8.enabled` ALONE.

**TDD note:** Step 3's tests first (they drive the `_inner` fns).

- [ ] **Step 1: events.rs — names + production sink**

Append to events.rs:
```rust
/// Delta-named events (spec §Events).
pub const FT8_SLOT_EVENT: &str = "ft8-decodes:slot";
pub const FT8_LISTENING_EVENT: &str = "ft8-listening:change";

/// Production sink: Tauri `AppHandle::emit`, fire-and-forget (modem:status
/// precedent — a failed emit is a UI-absent condition, never a service
/// error).
pub struct TauriEventSink {
    pub app: tauri::AppHandle,
}

impl EventSink for TauriEventSink {
    fn emit_listening_change(&self, change: &Ft8ListeningChange) {
        use tauri::Emitter as _;
        let _ = self.app.emit(FT8_LISTENING_EVENT, change);
    }
    fn emit_slot(&self, record: &SlotRecord) {
        use tauri::Emitter as _;
        let _ = self.app.emit(FT8_SLOT_EVENT, record);
    }
}
```

- [ ] **Step 2: commands.rs**

```rust
//! The six FT8 listener Tauri commands (spec §Commands). Thin async
//! wrappers: validation + the ft8 writer mutex + spawn_blocking into the
//! service's blocking-context surface. Testable bodies live in the
//! `_inner` fns; the `#[tauri::command]` shells only extract state.

use std::sync::Arc;

use tauri::State;

use crate::config::{self, Config};
use crate::ft8::service::{Ft8ListenerState, Ft8Snapshot};
use crate::winlink::ax25::devices::StableAudioId;
use tuxlink_capture::state::{BlockedReason, ServiceAxis};

/// One serialized ft8 RMW cycle, delegating to the CRATE-WIDE gate
/// (`config::update_config`, Step 2a): read → mutate → validate → atomic
/// write under config.rs's one static writer lock. Returns the updated
/// Ft8Config so callers can push it into the service.
fn with_ft8_config_writer(
    mutate: impl FnOnce(&mut Config) -> Result<(), String>,
) -> Result<crate::config::Ft8Config, String> {
    config::update_config(mutate).map(|cfg| cfg.ft8)
}

fn wedged_refusal(state: &Ft8ListenerState) -> Result<(), String> {
    if matches!(
        state.axis(),
        ServiceAxis::Blocked(BlockedReason::CaptureWedged)
    ) {
        return Err(
            "the FT8 capture thread is wedged and may still hold the sound card; \
             restart Tuxlink to recover"
                .into(),
        );
    }
    Ok(())
}

// ---- inner (testable) bodies ------------------------------------------------

pub(crate) fn ft8_listener_start_inner(state: &Arc<Ft8ListenerState>) -> Result<(), String> {
    wedged_refusal(state)?;
    let ft8 = with_ft8_config_writer(|c| {
        c.ft8.enabled = true;
        Ok(())
    })?;
    state.set_ft8_config(ft8);
    state.start() // idempotent: live supervisor → sequence re-run signal
}

pub(crate) fn ft8_listener_stop_inner(state: &Arc<Ft8ListenerState>) -> Result<(), String> {
    let ft8 = with_ft8_config_writer(|c| {
        c.ft8.enabled = false;
        Ok(())
    })?;
    state.set_ft8_config(ft8);
    state.stop();
    Ok(())
}

pub(crate) fn ft8_set_device_inner(
    state: &Arc<Ft8ListenerState>,
    stable_id: StableAudioId,
) -> Result<(), String> {
    // From capture-wedged, set_device (like start) refuses: a detached
    // thread may still hold the PCM; a second capture path in a process
    // that can no longer arbitrate the card is worse than refusing.
    wedged_refusal(state)?;
    let ft8 = with_ft8_config_writer(|c| {
        c.ft8.device = Some(stable_id);
        Ok(())
    })?;
    state.set_ft8_config(ft8);
    // From any blocked state except capture-wedged (refused above), a
    // device pick retriggers the start sequence; from stopped it stays
    // persist-only (the operator's start click is the trigger).
    if matches!(state.axis(), ServiceAxis::Blocked(_)) {
        state.start()?;
    }
    Ok(())
}

pub(crate) fn ft8_set_band_inner(
    state: &Arc<Ft8ListenerState>,
    band: String,
) -> Result<(), String> {
    // Validate BEFORE persisting (rejects out-of-table).
    if tuxlink_capture::bands::dial_hz(&band).is_none() {
        return Err(format!("{band:?} is not an FT8 band"));
    }
    let ft8 = with_ft8_config_writer(|c| {
        c.ft8.band = band.clone();
        Ok(())
    })?;
    state.set_ft8_config(ft8);
    if state.axis() == ServiceAxis::Listening {
        if state.platform.rig_configured() {
            // Listening + CAT: the chip is a QSY command. Through the
            // arbiter when installed — the ARBITER lock (rig_session) is
            // what excludes a concurrent pause_for_modem; qsy_to_band owns
            // the RIG lock itself (rig_session takes ONLY the arbiter lock:
            // lock order arbiter > rig > state, each at most once — T14's
            // non-reentrancy contract).
            let do_qsy = || state.qsy_to_band(&band, crate::ft8::records::BandSource::CatConfirmed);
            match crate::ft8::arbiter::FT8_ARBITER.get() {
                Some(arb) => arb.rig_session(do_qsy)?,
                None => do_qsy()?,
            }
        } else {
            // Listening, no CAT: the chip is a STATEMENT — relabel with
            // operator-asserted provenance + instructed dial; k resets.
            state.assert_band_operator(&band)?;
        }
    }
    // Not listening: persist-only — never touches the radio (only a running
    // listener the operator started moves the dial).
    Ok(())
}

pub(crate) fn ft8_set_sweep_inner(
    state: &Arc<Ft8ListenerState>,
    enabled: bool,
) -> Result<(), String> {
    let ft8 = with_ft8_config_writer(|c| {
        c.ft8.sweep.enabled = enabled;
        Ok(()) // validate() enforces sweep.enabled ⇒ rig configured
    })?;
    state.set_ft8_config(ft8);
    state.apply_sweep_enabled(enabled);
    Ok(())
}

// ---- tauri shells -------------------------------------------------------

#[tauri::command]
pub async fn ft8_listener_start(
    state: State<'_, Arc<Ft8ListenerState>>,
) -> Result<(), String> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_listener_start_inner(&s))
        .await
        .map_err(|e| format!("start task failed: {e}"))?
}

#[tauri::command]
pub async fn ft8_listener_stop(
    state: State<'_, Arc<Ft8ListenerState>>,
) -> Result<(), String> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_listener_stop_inner(&s))
        .await
        .map_err(|e| format!("stop task failed: {e}"))?
}

#[tauri::command]
pub fn ft8_listener_snapshot(
    state: State<'_, Arc<Ft8ListenerState>>,
) -> Result<Ft8Snapshot, String> {
    Ok(state.snapshot())
}

#[tauri::command]
pub async fn ft8_set_device(
    state: State<'_, Arc<Ft8ListenerState>>,
    stable_id: StableAudioId,
) -> Result<(), String> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_set_device_inner(&s, stable_id))
        .await
        .map_err(|e| format!("set-device task failed: {e}"))?
}

#[tauri::command]
pub async fn ft8_set_band(
    state: State<'_, Arc<Ft8ListenerState>>,
    band: String,
) -> Result<(), String> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_set_band_inner(&s, band))
        .await
        .map_err(|e| format!("set-band task failed: {e}"))?
}

#[tauri::command]
pub async fn ft8_set_sweep(
    state: State<'_, Arc<Ft8ListenerState>>,
    enabled: bool,
) -> Result<(), String> {
    let s = (*state).clone();
    tauri::async_runtime::spawn_blocking(move || ft8_set_sweep_inner(&s, enabled))
        .await
        .map_err(|e| format!("set-sweep task failed: {e}"))?
}
```
Two small service helpers to add (service.rs):
```rust
    /// Operator chip click with no CAT: a STATEMENT (spec §Band provenance —
    /// an explicit click sets the confirmed timestamp), + instructed dial +
    /// k reset.
    pub(crate) fn assert_band_operator(self: &Arc<Self>, band: &str) -> Result<(), String> {
        let dial = tuxlink_capture::bands::dial_hz(band)
            .ok_or_else(|| format!("{band:?} is not an FT8 band"))?;
        {
            let mut g = self.lock_inner();
            g.band = band.to_string();
            g.dial_hz = dial;
            g.band_source = crate::ft8::records::BandSource::OperatorAsserted;
            g.band_label_confirmed_utc_ms = Some(self.platform.utc_now_ms());
            g.machine.on_band_change();
        }
        self.emit_listening_change();
        Ok(())
    }
    /// Live sweep toggle: (de)activate the machine element when listening;
    /// runtime state only, config already persisted by the command.
    pub(crate) fn apply_sweep_enabled(self: &Arc<Self>, enabled: bool) {
        {
            let mut g = self.lock_inner();
            if enabled && matches!(g.machine.axis(), ServiceAxis::Listening)
                && self.platform.rig_configured()
            {
                g.machine.sweep_activate();
            } else if !enabled {
                g.machine.sweep_deactivate();
            }
        }
        self.emit_listening_change();
    }
```
(NB `rig_configured()` does config I/O in production — restructure so the
platform call happens BEFORE taking the lock, mirroring the lock-discipline
rule: read the flag into a local first. The subagent applies this; it is the
same discipline every other helper follows.)

- [ ] **Step 2a: config.rs — the crate-wide `update_config` RMW gate**

Add to `src-tauri/src/config.rs`, next to `write_config_atomic` (add
`use std::sync::Mutex;` if the file lacks it):
```rust
/// The crate-wide config writer gate (tuxlink-b026z.3): serializes every
/// read-modify-validate-write cycle under ONE static lock.
/// `write_config_atomic` makes the file REPLACE atomic; it does NOT make
/// two concurrent read→mutate→write cycles atomic — without this gate the
/// second writer silently reverts the first writer's field (lost update).
///
/// Scope note: the six ft8 commands route through this from day one. The
/// ~10 pre-existing writers elsewhere in the crate still do bare
/// read→mutate→write; migrating them is OUT OF SCOPE here and tracked by
/// the follow-up bd issue T19 files — they migrate opportunistically as
/// they are touched.
static CONFIG_WRITER: Mutex<()> = Mutex::new(());

pub fn update_config(
    mutate: impl FnOnce(&mut Config) -> Result<(), String>,
) -> Result<Config, String> {
    let _g = CONFIG_WRITER.lock().unwrap_or_else(|p| p.into_inner());
    let mut cfg = read_config().map_err(|e| format!("config read failed: {e}"))?;
    mutate(&mut cfg)?;
    cfg.validate().map_err(|e| e.to_string())?;
    write_config_atomic(&cfg).map_err(|e| format!("config write failed: {e}"))?;
    Ok(cfg)
}
```
(Adjust `read_config`/`write_config_atomic` call shapes to their real
signatures in config.rs — read them first; the gate semantics above are the
contract.)

- [ ] **Step 3: Tests (commands.rs) — validation + writer-gate serialization**

Config-touching tests need `TUXLINK_CONFIG_DIR` env isolation. ONE
crate-visible guard owns it (Step 3a below) — do NOT write a local copy in
this module.

**(3a) Hoist modem_commands' env helper into the crate-visible guard.** In
`modem_commands.rs`, add at TOP level (outside its `#[cfg(test)] mod tests`):
```rust
/// Crate-shared TUXLINK_CONFIG_DIR test guard (tuxlink-b026z.3). ONE static
/// lock serializes every env-mutating test in the binary (std::env::set_var
/// is not thread-safe under parallel tests — tuxlink-j0ij), and the guard
/// RESTORES the prior value on drop — a panicking test can no longer leak
/// its tempdir into a neighbor. Both modem_commands' and ft8::commands'
/// test modules route through this; local copies are banned.
#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard};

    static LOCK: Mutex<()> = Mutex::new(());

    pub(crate) struct ConfigDirGuard {
        _lock: MutexGuard<'static, ()>,
        prior: Option<std::ffi::OsString>,
    }

    /// Point TUXLINK_CONFIG_DIR at `dir` for the guard's lifetime.
    pub(crate) fn lock_config_dir(dir: &std::path::Path) -> ConfigDirGuard {
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prior = std::env::var_os("TUXLINK_CONFIG_DIR");
        // SAFETY: LOCK serializes every env mutation in this test binary.
        unsafe { std::env::set_var("TUXLINK_CONFIG_DIR", dir) };
        ConfigDirGuard { _lock: lock, prior }
    }

    impl Drop for ConfigDirGuard {
        fn drop(&mut self) {
            // SAFETY: still serialized — the lock is held by self._lock.
            unsafe {
                match self.prior.take() {
                    Some(v) => std::env::set_var("TUXLINK_CONFIG_DIR", v),
                    None => std::env::remove_var("TUXLINK_CONFIG_DIR"),
                }
            }
        }
    }
}
```
Then migrate modem_commands' OWN test module onto it: delete its private
`env_lock()` fn (currently `modem_commands.rs:~2198`) and rewrite every
manual set→restore sequence
(`grep -n "TUXLINK_CONFIG_DIR" src-tauri/src/modem_commands.rs` finds them
all) to `let _env = test_env::lock_config_dir(tmp.path());`, deleting each
test's hand-rolled `prior` capture and tail-restore lines. (NB the existing
code wraps `set_var` in `unsafe` — match the file's edition posture; if the
crate's edition has a safe `set_var`, drop the `unsafe` blocks and SAFETY
comments.) Seed a minimal valid config file after taking the guard (mirror
`round_trip_persists_through_config` in modem_commands.rs for the fixture
shape).

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Ft8Config;
    use crate::ft8::service::Ft8Deps;
    use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink};
    use crate::modem_commands::test_env::{lock_config_dir, ConfigDirGuard};

    /// Point TUXLINK_CONFIG_DIR at a fresh pid-suffixed tempdir — via the
    /// crate-shared guard (Step 3a), which serializes env mutation AND
    /// restores the prior value on drop — and write a minimal VALID config
    /// there (mirror the config_json fixture shape in config.rs tests —
    /// read it; the exact JSON body is copied from there with the current
    /// CONFIG_SCHEMA_VERSION). Callers hold the returned guard for the
    /// whole test: `let (_env, _dir) = seed_config();`.
    fn seed_config() -> (ConfigDirGuard, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "tuxlink-ft8-cmd-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let guard = lock_config_dir(&dir);
        let body = serde_json::json!({
            "schema_version": crate::config::CONFIG_SCHEMA_VERSION,
            "wizard_completed": true,
            "connect": { "connect_to_cms": false, "transport": "Telnet" },
            "identity": { "callsign": null, "identifier": "W1TEST", "grid": null },
            "privacy": { "gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid" }
        });
        std::fs::write(dir.join("config.json"), serde_json::to_vec_pretty(&body).unwrap())
            .unwrap();
        (guard, dir)
    }

    /// seed_config + a configured rig (model + serial) so sweep/QSY
    /// validation passes — through the crate-wide gate, like production.
    fn seed_config_with_rig() -> (ConfigDirGuard, std::path::PathBuf) {
        let (guard, dir) = seed_config();
        crate::config::update_config(|c| {
            c.rig.rig_hamlib_model = Some(1043);
            c.rig.cat_serial_path = Some("/dev/ttyUSB0".into());
            Ok(())
        })
        .unwrap();
        (guard, dir)
    }
    // (Verify the config layer honors TUXLINK_CONFIG_DIR by reading how
    // modem_commands.rs's round_trip_persists_through_config seeds its dir;
    // if the env var name or the path shape differs, copy THAT test's exact
    // mechanism — the fixture mechanism is the contract, not this sketch.)

    fn test_stable_id() -> crate::winlink::ax25::devices::StableAudioId {
        crate::winlink::ax25::devices::StableAudioId {
            kind: crate::winlink::ax25::devices::StableIdKind::ByIdSymlink,
            value: "usb-DRA-100-00".into(),
        }
    }

    fn cfg_with_device() -> Ft8Config {
        Ft8Config { device: Some(test_stable_id()), ..Ft8Config::default() }
    }

    fn state_with_platform(
        platform: Arc<FakePlatform>,
        cfg: Ft8Config,
    ) -> Arc<Ft8ListenerState> {
        crate::ft8::service::Ft8ListenerState::new(
            Ft8Deps {
                platform,
                clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
                sink: Arc::new(RecordingSink::default()),
            },
            cfg,
        )
    }

    fn state_with(cfg: Ft8Config) -> Arc<Ft8ListenerState> {
        state_with_platform(FakePlatform::happy(), cfg)
    }

    /// set_band rejects out-of-table BEFORE any persistence.
    #[test]
    fn set_band_rejects_unknown_bands_without_persisting() {
        let (_env, _dir) = seed_config();
        let state = state_with(Ft8Config::default());
        assert!(ft8_set_band_inner(&state, "23cm".into()).is_err());
        let on_disk = crate::config::read_config().unwrap();
        assert_eq!(on_disk.ft8.band, "20m", "rejected band never reached disk");
    }

    /// set_band while NOT listening: persist-only — the radio is never
    /// touched (zero rig_tune calls on the platform fake).
    #[test]
    fn set_band_not_listening_is_persist_only() {
        let (_env, _dir) = seed_config();
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        let state = state_with_platform(p.clone(), Ft8Config::default());
        ft8_set_band_inner(&state, "40m".into()).unwrap();
        assert!(p.tuned_to.lock().unwrap().is_empty(), "persist-only: no QSY");
        assert_eq!(crate::config::read_config().unwrap().ft8.band, "40m");
    }

    /// set_band while listening + CAT: QSY + relabel + k reset.
    #[test]
    fn set_band_listening_with_cat_qsys_and_relabels() {
        let (_env, _dir) = seed_config_with_rig(); // rig model + serial in the seeded file
        let p = FakePlatform::happy();
        *p.rig_configured.lock().unwrap() = true;
        let state = state_with_platform(p.clone(), cfg_with_device());
        state.test_run_sequence();
        // Age k: two band-dead slots.
        state.record_slot(state.test_base_record(1, crate::ft8::records::RingOutcome::BandDead));
        state.record_slot(state.test_base_record(2, crate::ft8::records::RingOutcome::BandDead));
        assert_eq!(state.snapshot().k_consecutive, 2);
        ft8_set_band_inner(&state, "40m".into()).unwrap();
        let snap = state.snapshot();
        assert_eq!(*p.tuned_to.lock().unwrap().last().unwrap(), 7_074_000);
        assert_eq!(snap.band, "40m");
        assert_eq!(snap.band_source, crate::ft8::records::BandSource::CatConfirmed);
        assert_eq!(snap.k_consecutive, 0, "k resets on band change");
        state.test_teardown();
    }

    /// capture-wedged refuses start AND set_device with the
    /// restart-required error.
    #[test]
    fn wedged_refuses_start_and_set_device() {
        let (_env, _dir) = seed_config();
        let state = state_with(cfg_with_device());
        state.test_force_capture_wedged(); // helper: machine.on_capture_wedged()
        let e1 = ft8_listener_start_inner(&state).unwrap_err();
        let e2 = ft8_set_device_inner(&state, test_stable_id()).unwrap_err();
        for e in [e1, e2] {
            assert!(e.contains("restart Tuxlink"), "restart-required error, got: {e}");
        }
    }

    /// Idempotent start: two starts in a row both Ok; one supervisor.
    #[test]
    fn start_is_idempotent() {
        let (_env, _dir) = seed_config();
        let state = state_with(cfg_with_device());
        ft8_listener_start_inner(&state).unwrap();
        ft8_listener_start_inner(&state).unwrap();
        state.test_teardown();
    }

    /// Writer-mutex serialization: two threads doing set_* RMW cycles
    /// concurrently — both mutations land (no lost update). Threads, not
    /// loom (per the plan: loom not required).
    #[test]
    fn writer_mutex_serializes_concurrent_rmw() {
        let (_env, _dir) = seed_config();
        let state = state_with(Ft8Config::default());
        let s1 = state.clone();
        let s2 = state.clone();
        let t1 = std::thread::spawn(move || ft8_set_band_inner(&s1, "40m".into()));
        let t2 = std::thread::spawn(move || {
            ft8_set_device_inner(&s2, test_stable_id())
        });
        t1.join().unwrap().unwrap();
        t2.join().unwrap().unwrap();
        let on_disk = crate::config::read_config().unwrap();
        assert_eq!(on_disk.ft8.band, "40m", "band write survived");
        assert!(on_disk.ft8.device.is_some(), "device write survived (no lost update)");
    }

    /// CRATE-WIDE gate (Step 2a): an ft8 write racing a NON-ft8 config
    /// write — both fields survive. An ft8-only mutex could not protect
    /// this pairing; config::update_config's one static lock does.
    #[test]
    fn ft8_write_racing_non_ft8_write_loses_neither() {
        let (_env, _dir) = seed_config();
        let state = state_with(Ft8Config::default());
        let s1 = state.clone();
        let t1 = std::thread::spawn(move || ft8_set_band_inner(&s1, "40m".into()));
        let t2 = std::thread::spawn(|| {
            crate::config::update_config(|c| {
                c.rig.rig_hamlib_model = Some(1043);
                Ok(())
            })
        });
        t1.join().unwrap().unwrap();
        t2.join().unwrap().unwrap();
        let on_disk = crate::config::read_config().unwrap();
        assert_eq!(on_disk.ft8.band, "40m", "ft8 write survived");
        assert_eq!(
            on_disk.rig.rig_hamlib_model,
            Some(1043),
            "non-ft8 write survived (no cross-subsystem lost update)"
        );
    }

    /// set_sweep(enabled=true) without a rig is rejected by validate()
    /// inside the writer cycle — nothing persists.
    #[test]
    fn set_sweep_without_rig_is_rejected() {
        let (_env, _dir) = seed_config(); // no rig in the seed
        let state = state_with(Ft8Config::default());
        assert!(ft8_set_sweep_inner(&state, true).is_err());
        assert!(!crate::config::read_config().unwrap().ft8.sweep.enabled);
    }
}
```
One service helper remains to add for these tests — service.rs:
`#[cfg(test)] pub(crate) fn test_force_capture_wedged(&self)` =
`{ self.lock_inner().machine.on_capture_wedged(); }`.
**The wedged helper requires the machine to accept `on_capture_wedged` from
a non-started axis — if Phase A's machine rejects that transition, route the
helper through `on_start_requested()` + `on_listening()` first (read the
leaf machine's tests; do not weaken the machine).**

- [ ] **Step 4: lib.rs wiring — managed state + autostart + registration**

**(4a) Managed state + arbiter install + autostart, inside `.setup(|app| {`.**
Place after the modem-status broadcaster block (quoted verbatim anchor — the
end of that block):
```rust
            let _broadcaster_handle = crate::modem_status::ModemStatusBroadcaster::spawn(
                session_for_broadcaster,
                move |s| {
```
…(the closure continues; insert AFTER the statement's closing `);`):
```rust
            // tuxlink-b026z.3: FT8 Station Intelligence listener. Managed
            // state + arbiter are constructed here (paths need the Tauri
            // path API); the service starts ONLY via autostart (below) or
            // the ft8_listener_start command.
            {
                use tauri::Manager as _;
                // Slot dirs: tmpfs — ~2 GB/day must never hit the SD card
                // (spec §WAV writeout: XDG_RUNTIME_DIR, /run/user/<uid>,
                // temp_dir + warning, in that order).
                let slot_root = if let Some(x) = std::env::var_os("XDG_RUNTIME_DIR") {
                    std::path::PathBuf::from(x).join("tuxlink").join("ft8")
                } else {
                    // SAFETY: getuid is always successful (POSIX). libc 0.2
                    // is already a direct dep (src-tauri/Cargo.toml:98);
                    // nix 0.31's enabled feature set lacks `user`, so
                    // nix::unistd::Uid is NOT available here.
                    let uid = unsafe { libc::getuid() };
                    let run_user = std::path::PathBuf::from(format!("/run/user/{uid}"));
                    if run_user.is_dir()
                        && std::fs::metadata(&run_user).map(|m| !m.permissions().readonly()).unwrap_or(false)
                    {
                        run_user.join("tuxlink").join("ft8")
                    } else {
                        eprintln!(
                            "ft8: XDG_RUNTIME_DIR unset and /run/user unavailable — slot WAVs \
                             fall back to {:?}; if that is SD-card-backed, sustained listening \
                             writes ~2 GB/day to it",
                            std::env::temp_dir()
                        );
                        std::env::temp_dir().join("tuxlink").join("ft8")
                    }
                };
                // FFTW wisdom: ONE machine-wide dir (keyed by FFT size/CPU,
                // not by audio device).
                let wisdom_dir = app
                    .path()
                    .app_local_data_dir()
                    .map(|d| d.join("jt9-wisdom"))
                    .unwrap_or_else(|_| std::env::temp_dir().join("tuxlink-jt9-wisdom"));
                let modem_session =
                    (*app.state::<std::sync::Arc<crate::modem_status::ModemSession>>()).clone();
                let platform = std::sync::Arc::new(crate::ft8::traits::ProdPlatform {
                    wisdom_dir,
                    slot_root,
                    modem: modem_session,
                });
                let ft8_cfg = crate::config::read_config()
                    .map(|c| c.ft8)
                    .unwrap_or_default();
                let autostart = ft8_cfg.enabled;
                let ft8_state = crate::ft8::service::Ft8ListenerState::new(
                    crate::ft8::service::Ft8Deps {
                        platform,
                        clock: std::sync::Arc::new(crate::ft8::clock::TimedatectlProbe),
                        sink: std::sync::Arc::new(crate::ft8::events::TauriEventSink {
                            app: app.handle().clone(),
                        }),
                    },
                    ft8_cfg,
                );
                app.manage(ft8_state.clone());
                // The arbiter: managed for command access AND installed
                // globally for the modem seams (T15's choke points).
                let arbiter = crate::ft8::arbiter::Ft8Arbiter::new(ft8_state.clone());
                app.manage(arbiter.clone());
                let _ = crate::ft8::arbiter::FT8_ARBITER.set(arbiter);
                // Autostart on `enabled` ALONE — NOT gated on device
                // presence: an interrupted first-contact operator must find
                // blocked(needs-device-selection), not silent stopped.
                if autostart {
                    std::thread::Builder::new()
                        .name("ft8-autostart".into())
                        .spawn(move || {
                            if let Err(e) = ft8_state.start() {
                                tracing::warn!(target: "tuxlink::ft8", "autostart failed: {e}");
                            }
                        })
                        .ok();
                }
            }
```

**(4b) Command registration.** The `generate_handler![` list's current tail
(quoted verbatim):
```rust
            // T6: memory-fit estimate — Tauri UI command only, NOT an MCP tool.
            crate::elmer::memory_estimate::elmer_estimate_memory,
        ])
```
becomes:
```rust
            // T6: memory-fit estimate — Tauri UI command only, NOT an MCP tool.
            crate::elmer::memory_estimate::elmer_estimate_memory,
            // tuxlink-b026z.3: FT8 Station Intelligence listener (L2). The
            // UI caller is L3; the commands exist now per the epic's
            // layer-wise sanction (spec §Scope).
            crate::ft8::commands::ft8_listener_start,
            crate::ft8::commands::ft8_listener_stop,
            crate::ft8::commands::ft8_listener_snapshot,
            crate::ft8::commands::ft8_set_device,
            crate::ft8::commands::ft8_set_band,
            crate::ft8::commands::ft8_set_sweep,
        ])
```

- [ ] **Step 5: [CI-side] verification** — workspace clippy + tests.

- [ ] **Step 6: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/ft8 src-tauri/src/lib.rs src-tauri/src/config.rs src-tauri/src/modem_commands.rs
git commit -m "feat(ft8): Tauri commands, events, managed state + autostart (tuxlink-b026z.3 T17)"
```

**Completion check:** all six commands registered; every config-mutating
command routes through `with_ft8_config_writer` →
`config::update_config` (grep: ZERO `write_config_atomic` calls in
commands.rs — the only caller in this task's diff is `update_config`
itself); the cross-writer race test exists; modem_commands' tests route
through the shared `test_env` guard and its private `env_lock` is gone;
blocking
service calls are inside `spawn_blocking`; autostart reads `enabled` alone;
the arbiter is BOTH managed and OnceLock-installed; event names match the
delta exactly (`ft8-decodes:slot`, `ft8-listening:change`).

---
### Task 18: E2E fixture test + CI/packaging edits

**Files:**
- Create: `src-tauri/src/ft8/e2e_tests.rs`
- Modify: `src-tauri/src/ft8/mod.rs` (`#[cfg(test)] mod e2e_tests;`)
- Modify: `src-tauri/src/ft8/testutil.rs` (one new `SourceStep` variant)
- Modify: `.github/workflows/ci.yml` (apt package list + cache salt)
- Modify: `.github/workflows/release.yml` (apt package list + cache salt)
- Modify: `.github/workflows/ect-build.yml` (apt install line)
- Modify: `src-tauri/tauri.conf.json` (deb + rpm runtime depends)

**Interfaces:**
- Consumes: the committed 12 kHz SDR fixture
  `src-tauri/tuxlink-ft8/tests/fixtures/sdr/ft8-40m-crowded-20260706T121300Z.wav`
  and its reference decode list
  `ft8-40m-crowded-20260706T121300Z.jt9-d3-ap-off.txt` (15 reference lines);
  the full T10–T17 service; real jt9 (present on the dev Pi AND in CI via the
  `wsjtx` package — but this test still cannot RUN locally: it lives in the
  main crate).
- Produces: the e2e test + green CI on a tree whose main crate now links
  libasound.

**Gating (mirrors L1 exactly — `tuxlink-jt9/tests/real_jt9.rs`):** the L1
suite gates on `discover_jt9(None)` and prints
`eprintln!("SKIP: jt9 not installed (apt install wsjtx) — …")` then
`return`s — NOT `#[ignore]`. Mirror that: CI always runs it (wsjtx is in the
apt closure); a jt9-less environment skips loudly.

- [ ] **Step 1: testutil extension — real-sample step**

`SourceStep` gains a variant (ScriptedSource's `Frames` writes a constant;
the e2e feeds REAL fixture audio):
```rust
    /// Deliver these exact samples (chunked by the caller to ≤ the read
    /// buffer length, multiples of 48 for clean ms accounting).
    Samples { samples: Vec<i16>, gap: Option<GapReport> },
```
and the matching `read` arm (before the `Idle` arm):
```rust
            Some(SourceStep::Samples { samples, gap }) => {
                let n = samples.len().min(buf.len());
                buf[..n].copy_from_slice(&samples[..n]);
                self.clock.advance_ms((n as u64) / 48);
                Ok(ReadBatch { frames: n, mono_ts_us: self.clock.mono_us(), gap })
            }
```
(If `n < samples.len()` the tail would be silently lost — the e2e chunks to
exactly 4,800 so it never happens; add a `debug_assert!(samples.len() <= buf.len())`.)

Also in testutil.rs — the boundary-alignment helpers (the e2e aligns the ONE
shared clock; it must never construct a second `SyntheticClock`, which would
split the platform's and the source's time). `SyntheticClock` gains:
```rust
    /// Set the UTC value directly (test setup). Monotonic is untouched —
    /// the assembler only ever DIFFERENCES monotonic values.
    pub fn set_utc_ms(&self, utc_ms: u64) {
        self.utc_ms.store(utc_ms, Ordering::SeqCst);
    }
```
and `FakePlatform` gains:
```rust
    /// Snap the shared synthetic clock's UTC back to the previous 15 s slot
    /// boundary. happy()'s epoch (1_760_000_000_000 ms) sits 5 s PAST a
    /// boundary (mod 15_000 = 5_000 — i.e. 10 s before the next one); the
    /// e2e needs slot-aligned audio.
    pub fn align_clock_to_slot_boundary(&self) {
        let utc = self.clock.utc_ms();
        self.clock.set_utc_ms(utc - (utc % 15_000));
    }
```

- [ ] **Step 2: the e2e test**

Declare the module in `src-tauri/src/ft8/mod.rs` (one line, alongside the
existing `#[cfg(test)] pub mod testutil;`):
```rust
#[cfg(test)]
mod e2e_tests;
```

`src-tauri/src/ft8/e2e_tests.rs`:
```rust
//! E2E (spec §Testing strategy): committed 12 kHz SDR fixture → ZOH ×4
//! upsample to 48 kHz → SampleSource-faked capture with SYNTHETIC time →
//! assembler → WAV → REAL jt9 decode; ≥ 90 % of the fixture's committed
//! reference decode count.
//!
//! Validity argument (spec, verbatim — NOT "band-limited by construction",
//! which is false for an SDR capture): ZOH images of ≤ 4 kHz content land
//! ≥ 8 kHz (FIR stopband, ≥ 60 dB); 4–6 kHz baseband content stays above
//! jt9's 4007 Hz ceiling; ZOH sinc droop at 4 kHz ≈ −0.10 dB — negligible.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::config::Ft8Config;
use crate::ft8::records::RingOutcome;
use crate::ft8::service::{Ft8Deps, Ft8ListenerState};
use crate::ft8::testutil::{FakeClock, FakePlatform, RecordingSink, SourceStep};
use crate::ft8::traits::{Ft8Platform, Jt9Engine}; // trait in scope: p.wisdom_dir() below
use crate::winlink::ax25::devices::{StableAudioId, StableIdKind};
use tuxlink_jt9::runner::Jt9Runner;
use tuxlink_jt9::types::SLOT_DECODE_TIMEOUT_SECS;

const SLOT_MS: u64 = 15_000;
const IN_SLOT_FRAMES: usize = tuxlink_capture::slot::IN_SLOT_FRAMES; // 720_000

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tuxlink-ft8/tests/fixtures/sdr")
}

/// Minimal canonical-WAV reader for the committed fixture (44-byte header,
/// PCM16 mono 12 kHz — the same shape wavwrite emits and preflight pins).
fn read_fixture_samples(name: &str) -> Vec<i16> {
    let bytes = std::fs::read(fixture_dir().join(name)).expect("committed fixture");
    assert_eq!(&bytes[0..4], b"RIFF", "fixture is canonical RIFF");
    let data = &bytes[44..];
    data.chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect()
}

/// Reference decode count: the committed jt9 -d3 output line count.
fn reference_count(name: &str) -> usize {
    std::fs::read_to_string(fixture_dir().join(name))
        .expect("committed reference list")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

/// ZOH ×4: each 12 kHz sample repeated 4×.
fn zoh_x4(samples_12k: &[i16]) -> Vec<i16> {
    let mut out = Vec::with_capacity(samples_12k.len() * 4);
    for &s in samples_12k {
        out.extend_from_slice(&[s, s, s, s]);
    }
    out
}

#[test]
fn e2e_fixture_through_capture_path_decodes_90_percent_of_reference() {
    // Gate exactly like L1's real_jt9.rs: skip loudly when jt9 is absent.
    let Ok(_bin) = tuxlink_jt9::discover::discover_jt9(None) else {
        eprintln!("SKIP: jt9 not installed (apt install wsjtx) — ft8 capture e2e");
        return;
    };

    let wav_12k = read_fixture_samples("ft8-40m-crowded-20260706T121300Z.wav");
    assert_eq!(wav_12k.len(), 180_000, "fixture is exactly one 15 s slot at 12 kHz");
    let reference = reference_count("ft8-40m-crowded-20260706T121300Z.jt9-d3-ap-off.txt");
    assert!(reference >= 10, "crowded fixture reference sanity");
    let want = (reference * 9).div_ceil(10); // ≥ 90 %

    let p = FakePlatform::happy();
    // REAL engine over the fake platform's tmp wisdom dir.
    let wisdom = p.wisdom_dir();
    std::fs::create_dir_all(&wisdom).unwrap();
    let bin = tuxlink_jt9::discover::discover_jt9(None).unwrap();
    let engine = Arc::new(Jt9Engine::new(Jt9Runner::new(
        bin,
        wisdom,
        Duration::from_secs(SLOT_DECODE_TIMEOUT_SECS),
    )));
    *p.engine.lock().unwrap() = engine;

    // happy()'s clock epoch (1_760_000_000_000 ms) does NOT start on a
    // boundary: mod 15_000 = 5_000, so it sits 5 s past one (10 s before
    // the next). Snap the ONE shared clock (platform + source both hold
    // it) back onto the boundary so the fixture audio is slot-aligned —
    // time is injected data (Step 1's helper).
    p.align_clock_to_slot_boundary();
    assert_eq!(p.clock.utc_ms() % SLOT_MS, 0);

    // Script: slot A = silence (absorbs whichever first-slot semantics the
    // Phase-A assembler pinned: full-silence BandDead or first-slot
    // discard), slot B = the upsampled fixture, then a tail of silence so
    // the B boundary definitely closes.
    let upsampled = zoh_x4(&wav_12k);
    assert_eq!(upsampled.len(), IN_SLOT_FRAMES);
    {
        let mut steps = p.source_steps.lock().unwrap();
        for _ in 0..(IN_SLOT_FRAMES / 4_800) {
            steps.push_back(SourceStep::Frames { frames: 4_800, value: 0, gap: None });
        }
        for chunk in upsampled.chunks(4_800) {
            steps.push_back(SourceStep::Samples { samples: chunk.to_vec(), gap: None });
        }
        for _ in 0..4 {
            steps.push_back(SourceStep::Frames { frames: 4_800, value: 0, gap: None });
        }
    }

    let mut cfg = Ft8Config::default();
    cfg.device = Some(StableAudioId {
        kind: StableIdKind::ByIdSymlink,
        value: "usb-DRA-100-00".into(),
    });
    let state = Ft8ListenerState::new(
        Ft8Deps {
            platform: p.clone(),
            clock: FakeClock::new(crate::ft8::clock::ClockSync::Synced),
            sink: Arc::new(RecordingSink::default()),
        },
        cfg,
    );
    state.test_run_sequence();

    // Wait for the fixture slot's Decoded record (real jt9: allow generous
    // wall time — prewarm + decode ≤ ~15 s on a cold arm64 runner).
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let decoded_count = loop {
        let found = {
            let snap = state.snapshot();
            snap.ring_tail.iter().find_map(|r| match &r.outcome {
                RingOutcome::Decoded => Some(r.decodes.len()),
                _ => None,
            })
        };
        if let Some(n) = found {
            break n;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no Decoded record within the deadline; ring tail: {:?}",
            state
                .snapshot()
                .ring_tail
                .iter()
                .map(|r| r.outcome.clone())
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(
        decoded_count >= want,
        "decoded {decoded_count} < 90 % of reference ({want} of {reference}) — the \
         ZOH→FIR→assembler round trip lost decodes"
    );
    state.test_teardown();
}
```
**NOTE — flake-mitigation escape hatch:** if CI shows a wsjtx-version delta
on the crowded fixture (the L1 suite floors at depth-1 counts for exactly
this reason), floor at `reference_count` of the DEPTH-1 subset per L1
precedent and file a bd issue — do not delete the 90 % assertion silently.
(Clock alignment is handled by Step 1's `align_clock_to_slot_boundary`
helper on the ONE shared clock — never construct a second `SyntheticClock`
here.)

- [ ] **Step 3: CI apt edits**

**(3a) ci.yml** — the verify job's cached package list (quoted verbatim,
current):
```yaml
          packages: libax25-dev libudev-dev libwebkit2gtk-4.1-dev libglib2.0-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libheif-dev libde265-dev libwebp-dev wsjtx
          # Salt bump forces a fresh apt closure. v3: v2 cached an EMPTY result
          # after the 404-aborted install, so it must be discarded too (tuxlink-84vzn).
          # v4: adds wsjtx (tuxlink-b026z.2 T8).
          version: tuxlink-ci-${{ matrix.arch }}-v4
```
becomes:
```yaml
          packages: libax25-dev libudev-dev libwebkit2gtk-4.1-dev libglib2.0-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libheif-dev libde265-dev libwebp-dev wsjtx libasound2-dev
          # Salt bump forces a fresh apt closure. v3: v2 cached an EMPTY result
          # after the 404-aborted install, so it must be discarded too (tuxlink-84vzn).
          # v4: adds wsjtx (tuxlink-b026z.2 T8).
          # v5: adds libasound2-dev (alsa crate, tuxlink-b026z.3 T18).
          version: tuxlink-ci-${{ matrix.arch }}-v5
```

**(3b) release.yml** — the build-linux cached list (quoted verbatim,
current, `release.yml:69-70`):
```yaml
          packages: libax25-dev libudev-dev libwebkit2gtk-4.1-dev libglib2.0-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libheif-dev libde265-dev libwebp-dev
          version: tuxlink-release-${{ matrix.arch }}-v2
```
becomes:
```yaml
          packages: libax25-dev libudev-dev libwebkit2gtk-4.1-dev libglib2.0-dev libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libheif-dev libde265-dev libwebp-dev libasound2-dev
          # v3: adds libasound2-dev (alsa crate, tuxlink-b026z.3). The ardopcf
          # sidecar step installs it too, but the MAIN crate now links
          # libasound — the dependency is declared here, not inherited from a
          # sidecar build step that could move or vanish.
          version: tuxlink-release-${{ matrix.arch }}-v3
```

**(3c) ect-build.yml** — the main install (quoted verbatim, current):
```yaml
          sudo apt-get install -y libax25-dev libudev-dev libwebkit2gtk-4.1-dev \
            build-essential curl wget file libxdo-dev libssl-dev \
            libayatana-appindicator3-dev librsvg2-dev gfortran rsync jq
```
becomes:
```yaml
          sudo apt-get install -y libax25-dev libudev-dev libwebkit2gtk-4.1-dev \
            build-essential curl wget file libxdo-dev libssl-dev \
            libayatana-appindicator3-dev librsvg2-dev gfortran rsync jq \
            libasound2-dev
```

**(3d) Leaf-crate gates — RESOLUTION OF A SPEC-VS-REALITY MISMATCH,
recorded:** spec §CI says "CI adds `cargo test -p tuxlink-capture` + clippy
`-p tuxlink-capture` alongside the existing `-p tuxlink-jt9` gates
(mirroring how L1's crate is wired)." Reality (verified 2026-07-10 against
`ci.yml`): there ARE no `-p tuxlink-jt9` gates — L1's crate is exercised by
the WORKSPACE-WIDE gates the verify job already runs (quoted verbatim):
```yaml
      - name: Rust lint (clippy --workspace --all-targets --locked -D warnings)
        run: cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets --locked -- -D warnings
```
```yaml
      - name: Rust tests
        run: cargo test --manifest-path src-tauri/Cargo.toml --workspace --locked --verbose
```
`--workspace` covers every member, so `tuxlink-capture` (a member since
Task 1) is already gated with zero edits — the spec's `default-members`
concern applies to a BARE `cargo test`, which CI does not run. **No gate
edit is made**; this note + the Global Constraints line ("CI runs
--workspace…") are the record. Verify in the PR's CI logs that
`tuxlink-capture` tests appear in the verify job output.

- [ ] **Step 4: Runtime packaging depends**

The main binary now LINKS libasound (the `alsa` crate); the packaged app
needs the runtime library declared. Spec §CI says "No packaging metadata
change (wsjtx Recommends shipped with L1)" — that sentence is about the
wsjtx Recommends; a linked shared library is a hard runtime Depends that
did not exist when the spec sentence was written, and shipping a .deb that
segfault-links on a minimal system is not deferrable. (Recorded as a spec
delta; Task 19's notes cite it.)

`src-tauri/tauri.conf.json`, deb depends (quoted verbatim, current):
```json
        "depends": [
          "libc6 (>= 2.39)",
          "libsecret-1-0",
          "libheif1",
          "libde265-0",
          "libwebp7",
          "libusb-1.0-0",
          "libudev1",
          "libcap2"
        ],
```
becomes:
```json
        "depends": [
          "libc6 (>= 2.39)",
          "libsecret-1-0",
          "libheif1",
          "libde265-0",
          "libwebp7",
          "libusb-1.0-0",
          "libudev1",
          "libcap2",
          "libasound2 | libasound2t64"
        ],
```
(`libasound2t64` is Ubuntu 24.04's time64-transition name; it Provides
`libasound2`, and the alternation covers both spellings across the support
matrix without a versioned constraint.)

rpm depends (quoted verbatim, current):
```json
        "depends": [
          "libsecret",
          "webkit2gtk4.1",
          "libayatana-appindicator-gtk3",
          "libheif",
          "libde265",
          "libwebp",
          "libusb1",
          "systemd-libs",
          "libcap"
        ],
```
gains `"alsa-lib"` after `"libcap"` (same list-tail comma discipline).

- [ ] **Step 5: [CI-side] verification** — this task's push is the one that
proves the whole phase: workspace clippy + tests on amd64 + arm64, the e2e
runs against real jt9 in the verify job, and build-linux + ect-build produce
installable artifacts with the new depends. Locally: nothing to run (the
fixture-prep helpers are test code).

- [ ] **Step 6: Commit**

```bash
cd "$WT" && pwd
git add src-tauri/src/ft8 src-tauri/tauri.conf.json .github/workflows/ci.yml .github/workflows/release.yml .github/workflows/ect-build.yml
git commit -m "test(ft8)+ci: fixture e2e through the capture path; libasound2-dev + runtime depends (tuxlink-b026z.3 T18)"
```

**Completion check:** the e2e gates like L1 (SKIP-print, not #[ignore]);
the 90 % floor derives from the COMMITTED reference file at test time, not a
hardcoded count; both cache salts bumped (v5 / v3); ect-build's apt line
edited; deb AND rpm depends updated; the 3d mismatch note stays in the plan
as the record of why no `-p` gates were added.

---

**REVIEW GATE F (after Tasks 15–18):** review the integration batch.
Perspectives: (1) **seam completeness** — enumerate every path that opens an
audio device for a modem (ardopcf ×4 via the wrapper, Dire Wolf spawn_inner,
VARA open) and confirm each hits `pause_for_modem_global` BEFORE the open;
confirm the coverage test's needle discipline (no self-match, no
comment-match); (2) **consent + provenance walk** — `ft8_set_band`
persist-only when not listening; QSY only from a running listener or sweep
opt-in; every QSY failure path downgrades provenance; (3) **wiring
reachability** — commands registered, state managed BEFORE any command can
fire, autostart on `enabled` alone, arbiter OnceLock installed exactly once;
(4) **CI honesty** — the e2e actually runs in the verify job (workspace test
includes lib unit tests; confirm no accidental `#[ignore]`), salts bumped so
the apt cache refreshes, Cargo.lock consistent with `--locked` everywhere
after Task 9's single regen. Minimum three rounds; persist findings to
`dev/scratch/b026z.3-gate-F-findings.md` before proceeding. Files under
review: `src-tauri/src/modem_commands.rs`,
`src-tauri/src/winlink/ax25/managed_direwolf.rs`,
`src-tauri/src/winlink/modem/vara/commands.rs`, `src-tauri/src/ft8/sweep.rs`,
`src-tauri/src/ft8/service.rs`, `src-tauri/src/ft8/commands.rs`,
`src-tauri/src/ft8/events.rs`, `src-tauri/src/ft8/e2e_tests.rs`,
`src-tauri/src/ft8/testutil.rs`, `src-tauri/src/ft8/mod.rs`,
`src-tauri/src/config.rs`, `src-tauri/src/lib.rs`,
`.github/workflows/ci.yml`, `.github/workflows/release.yml`,
`.github/workflows/ect-build.yml`, `src-tauri/tauri.conf.json`.

**Gate F push:** after this gate's P1/P2 fixes are committed, the parent
pushes the branch (Global Constraints §Push cadence). This push's CI run
executes T15–T18's **[CI-side]** steps — including the e2e against real jt9
and the packaged-artifact builds; fix-forward on its findings before
starting Task 19.

---
### Task 19: Docs + issue closes — delta v3 notes, implementation log, bd

**Files:**
- Modify: `docs/design/2026-07-10-station-intel-jt9-engine-delta.md` (append the v3 notes section)
- Modify: `docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md` (status line only)
- Modify: `dev/implementation-log.md` (new entry at top)
- Modify: `docs/user-guide/11-signalink-and-others.md` (capture-side AGC note — Step 3b)
- bd operations (no file): close `tuxlink-gujnz`, update `tuxlink-b026z.8`, note on `tuxlink-b026z.3`, file the config-writer migration issue

**TDD note:** docs task — no tests; the completion check is the doc-lint +
grep pass in Step 5.

- [ ] **Step 1: Delta v3 notes**

Append to the END of
`docs/design/2026-07-10-station-intel-jt9-engine-delta.md` (the delta stays
canonical for design; these notes record what the L2 implementation pinned
or amended — spec §Contract edits lists all six):

```markdown
## v3 notes — L2 implementation deltas (2026-07-10, tuxlink-b026z.3)

Recorded at L2 merge. Each note amends or pins a section above; where a
canonical statement moved into code, the note points at it rather than
restating (propagation contract).

1. **Taxonomy: salvage-on-signal parity (tuxlink-gujnz, resolved).** A
   signal-death or nonzero clean exit with ≥ 1 parsed decode line returns
   `Decoded` with `partial = !saw_sentinel` — identical to the timeout arm;
   zero parsed lines keeps `Failed(Signal)`. Arm ordering pinned on ALL
   paths: the `StderrEof` check runs BEFORE salvage (a capture bug must
   never masquerade as decodes). Rationale: jt9's dominant real failure mode
   IS decode-stream-then-SIGSEGV; lines print only after jt9's internal
   CRC-14 accepts a candidate; the strict parser guards corruption; the
   timeout path already trusted the identical stream; discarding biased band
   intelligence against exactly the slots proving the band alive. Sentinel
   semantics: a crash AFTER `<DecodeFinished>` yields complete records
   (`partial = false`). Downstream does not distinguish timeout-salvage from
   crash-salvage. Canonical doc: `tuxlink-jt9/src/types.rs`
   (`Ft8Decode::partial`, `SlotFailure::Signal`).
2. **Service axis: two new blocked reasons.** `needs-device-selection` (no
   persisted device identity — distinct from `device-absent`, a persisted
   identity that no longer resolves) and `capture-wedged` (a force-detached
   capture thread may still hold the PCM; arbitration is dead until app
   restart). `device-absent` is narrowed to "persisted identity
   unresolvable" and is supervisor-retried every 5 s (self-healing on USB
   replug). No-auto-pick is a product rule: the picker always asks, even
   with one device present (operator decision, L2 spec header).
3. **Sweep element added to the state model.** The axis list above lacks
   it: `Sweep::{Inactive, Active{band_idx, dwell_progress},
   FallbackHold{failures}}` is a named part of the machine, runtime-only
   (`config.sweep.enabled` is never mutated by the machine); `FallbackHold`
   (two consecutive QSY failures) re-arms to `Active` at the next start or
   resume.
4. **Counter scoping: scheduled discards excluded.** The canonical sentence
   lives at `tuxlink-jt9/src/types.rs` (the `SlotFailure` doc block): the
   N=5 degraded counter folds L2 backpressure, lost-frames, and
   storage-error drops; scheduled discards (partial first slot after
   start/resume, QSY transition slot, clock-anomaly abandonment) count
   toward neither N nor k.
5. **Band-chip semantics under cat-absent.** The chip is an operator
   STATEMENT, not a command: the service labels records
   `operator-asserted` and the snapshot carries the dial the panel should
   instruct. Until an operator click or a CAT read confirms, the label is
   `default-unconfirmed` and downstream surfaces (L3/L4) MUST render it as
   unconfirmed — the service never claims a band nobody asserted.
   Provenance travels as `band_source` + `band_label_confirmed_utc_ms` on
   every `SlotRecord` and snapshot.
6. **Arbitration: VARA exception disclosed; hold-latch resume model.** The
   "conflict is self-inflicted" premise holds for ardopcf/Dire Wolf only:
   VARA launched standalone opens its audio device before any tuxlink
   involvement, and that conflict surfaces in VARA's UI (the listener must
   be stopped, or L3's pause affordance used, first). The bare "FT8
   auto-resumes on modem shutdown" model above is superseded by the
   positive hold latch: every yield latches a hold (30 s TTL, cleared on
   observed card-busy — positive evidence); the resume poll requires latch
   clear + card probe free + `ModemState ∈ {Stopped, Error, SocketLost}`.
   L2 spec §Arbitration is canonical.

Additional implementation-pinned deltas (not in the spec's six, recorded
for completeness): the L2 spec's "no packaging metadata change" predates
the `alsa` crate linking libasound — the .deb/.rpm gained a runtime
`libasound2`/`alsa-lib` Depends at L2 (plan T18); CI's leaf-crate gates are
satisfied by the existing `--workspace` clippy/test rather than new `-p`
lines (plan T18 step 3d records the reality check).
```

- [ ] **Step 2: Spec status flip**

The spec's line 3 currently reads `Status: v4 — REVIEWED. …`. Prepend the
implementation marker so the header carries the outcome:
```
Status: v4 — IMPLEMENTED (tuxlink-b026z.3, plan
docs/superpowers/plans/2026-07-10-station-intel-l2-capture.md). Five
adversarial rounds applied 2026-07-10
```
(i.e. replace only the token `REVIEWED.` with
`IMPLEMENTED (tuxlink-b026z.3, plan docs/superpowers/plans/2026-07-10-station-intel-l2-capture.md).` and keep
the rest of the sentence verbatim.)

- [ ] **Step 3: Implementation-log entry**

Top of `dev/implementation-log.md` (reverse-chronological; match the
existing entry style — read the current top entry first):

```markdown
## 2026-07-10 — Station Intelligence L2: capture + slot-decode service (tuxlink-b026z.3)

Plan `docs/superpowers/plans/2026-07-10-station-intel-l2-capture.md`
executed (19 tasks, 3+3 review gates). Shipped: `tuxlink-capture` leaf crate
(51-tap Kaiser 48k→12k decimator with response-verified const table,
wall-clock-true 15 s slot assembler with two-clock-domain gap/anomaly rules,
canonical slot-WAV writer, listener state machine with N=5/k=20 counters +
sweep element, FT8 band table); salvage-on-signal parity in `tuxlink-jt9`
(resolves tuxlink-gujnz) + 3 types.rs contract doc edits; main-crate
`src/ft8/` service (ALSA hw:-only capture source, supervisor/capture/decode
threads with rendezvous backpressure, waterfall tap, 240-slot ring, tmpfs
slot dirs, timedatectl clock probe, pipe-fd watermark for b026z.8), modem
yield/resume arbiter with positive hold latch + choke-point wiring into all
ardopcf/Dire Wolf/VARA spawn paths, opt-in CAT band sweep with provenance
downgrade, six `ft8_*` Tauri commands + `ft8-decodes:slot` /
`ft8-listening:change` events + autostart. E2E: committed SDR fixture
ZOH-upsampled through the faked-source capture path into real jt9, ≥ 90 %
of reference decodes. CI: libasound2-dev in all compiling workflows,
libasound2 runtime Depends. No UI caller by design — the epic's layer-wise
sanction; wire-walk gate runs when L3/L4 make FT8 user-reachable. Delta v3
notes appended (6 contract deltas + 2 implementation-pinned).
```

- [ ] **Step 3b: Capture-setup AGC note (user guide)**

The FT8 listener records THROUGH the codec's capture path, and CM108-class
codecs (DigiRig / DRA / SignaLink-generation interfaces) ship with mic Auto
Gain Control enabled by default — AGC pumps the receive level with band
activity, burying weak signals. The user guide's audio-interface chapter is
where operators do capture setup; `docs/user-guide/11-signalink-and-others.md`
already carries the "## Audio calibration" section, so the note lands there.
Append to the END of that section (before "## Picking an interface"):

```markdown
### Capture-side AGC on CM108-class codecs

Many USB audio interfaces in the DigiRig / DRA / SignaLink class are built
on C-Media CM108/CM119 codecs, and most ship with the codec's microphone
**Auto Gain Control enabled by default**. AGC continuously rewrites the
capture level: weak-signal decoders (the FT8 listener, ARDOP, VARA) see a
receive audio floor that pumps up and down with band activity, which buries
weak signals and invalidates any level calibration.

Disable it once per interface, at capture setup:

1. Run `alsamixer`, press `F6`, and select the USB codec card.
2. Press `F4` (capture view). If a control named `Auto Gain Control` (or
   `AGC`) shows `[on]`, press `m` to switch it off.
3. Persist it across replug/reboot: `sudo alsactl store`.

Then set the capture level itself to a fixed value (start near 60–70 %) and
leave it there — a steady, slightly-low level decodes better than a hot or
moving one.
```

(Reading check: confirm the section heading names in 11-signalink-and-others.md
still match before inserting; if the chapter gained an FT8-specific section
since this plan was written, put the note there instead — one location only.)

- [ ] **Step 4: bd operations**

```bash
cd "$WT" && pwd
bd close tuxlink-gujnz --reason "Resolved by tuxlink-b026z.3 (plan T7): salvage-on-signal parity shipped. Decision: SALVAGE — signal-death or nonzero clean exit with ≥1 parsed decode line returns Decoded with partial=!saw_sentinel, identical to the timeout arm; zero lines keeps Failed(Signal); StderrEof beats salvage on ALL paths. Rationale: jt9's dominant real failure mode is decode-stream-then-SIGSEGV; lines print only post-CRC-14; the timeout path already trusted the identical stream; discarding biased band intel against exactly the slots proving the band alive. Tests: signal_death_salvages_parsed_decodes (+3 new arms) in tuxlink-jt9/tests/fake_jt9.rs."

bd update tuxlink-b026z.8 --notes "Disposition (tuxlink-b026z.3): ACCEPTED BOUND + watermark observability. The residual leak (detached drain threads + 2 pipe read-fds per event when a killed/cleanly-exited jt9 leaves a pipe-holding grandchild) is accepted for v1 — jt9 does not fork in practice, and group-kill needs libc in a deliberately std-only crate. L2's supervisor counts pipe-type /proc/self/fd entries every 100 slot boundaries; >16 over the service-start baseline logs a warning naming this issue (service.rs check_pipe_watermark). A real observation reopens this with data."
bd close tuxlink-b026z.8

bd update tuxlink-b026z.3 --notes "L2 shipped per plan docs/superpowers/plans/2026-07-10-station-intel-l2-capture.md: tuxlink-capture leaf (T1–6), gujnz salvage (T7), src/ft8 service + arbiter + sweep + commands (T8–17), e2e + CI (T18), docs (T19). No UI caller by design (epic layer-wise sanction; wire-walk deferred to L3/L4 reachability). Delta v3 notes appended; spec flipped to IMPLEMENTED. Close on PR merge."

bd create "Migrate the pre-existing bare config read→mutate→write sites onto config::update_config" -t chore -p 3 -d "tuxlink-b026z.3 T17 added the crate-wide RMW gate (config.rs update_config: one static writer lock around read→mutate→validate→write_config_atomic) and routed the six ft8 commands through it. The ~10 pre-existing writers elsewhere in the crate (grep 'write_config_atomic(' outside config.rs) still run unserialized read→mutate→write cycles and can lose updates against ANY concurrent writer. Migrate opportunistically as each site is touched; each migration is mechanical (wrap the mutation in config::update_config). Cross-writer race pinned by ft8_write_racing_non_ft8_write_loses_neither in src/ft8/commands.rs."
```
(Adjust flag names to the installed bd version if `--reason`/`--notes`
differ — `bd close --help` / `bd update --help` first; the TEXT above is
the deliverable, the flag spelling is not.)

- [ ] **Step 5: AGENTS.md parity check**

One-line disposition, recorded here per the CLAUDE.md upkeep discipline:
**no CLAUDE.md rule changed in this plan** (all changes are code + design
docs + this plan), so **no AGENTS.md edit is required**. (If a later task
in this PR does touch CLAUDE.md, re-run the parity check then.)

- [ ] **Step 6: Doc lint + commit**

```bash
cd "$WT" && pwd
pnpm lint:docs
```
Expected: clean (the delta + spec + log edits carry no broken links; run
`pnpm install --frozen-lockfile` first if this worktree lacks
node_modules).
```bash
cd "$WT" && pwd
git add docs/design/2026-07-10-station-intel-jt9-engine-delta.md docs/superpowers/specs/2026-07-10-station-intel-l2-capture-design.md dev/implementation-log.md docs/user-guide/11-signalink-and-others.md
git commit -m "docs(ft8): delta v3 notes, spec status, implementation log, capture AGC note (tuxlink-b026z.3 T19, resolves tuxlink-gujnz)"
```

**Completion check:** all six spec-listed delta notes present verbatim in
substance + the two implementation-pinned extras; the types.rs edits are
NOT restated in the delta (pointers only — propagation contract); the
capture-AGC note landed in the user guide (Step 3b); bd ops executed
including the config-writer migration issue (or their exact text handed to
the parent if bd is unavailable in the subagent's shell); AGENTS.md
disposition recorded; `pnpm lint:docs` green.

---

## Phase C completion

After Task 19 + Review Gate F findings are resolved:

1. **Parent pushes the final commits** (T19's docs commit + any Gate-F
   fixes). The Gate D / E / F pushes already ran each batch's CI red-green
   (Global Constraints §Push cadence); this final push is the last CI run
   (amd64 + arm64) and the branch-complete verdict. Fix-forward on CI
   findings — clippy nits and API-name drift in the `alsa` crate surface
   are expected classes at the Gate D/E pushes; each fix is a normal commit
   on the branch.
2. **Wire-walk disposition (explicit):** per spec §Scope, L2 merges with no
   UI caller BY DESIGN — the epic (tuxlink-b026z) sanctions layer-wise
   landing and the wire-walk gate runs when L3/L4 make FT8 user-reachable.
   This plan does NOT run wire-walk at L2 close; the sanction + this line
   are the record (same sanction L1 shipped under).
3. **PR:** title `[<moniker>] feat(ft8): Station Intelligence L2 — capture +
   slot-decode service (tuxlink-b026z.3)`; body enumerates the layer
   sanction, the gujnz resolution, the b026z.8 disposition, and the two
   spec deltas T18 recorded (packaging Depends, workspace-gate reality).
   No draft parking — once CI is green on both arches, mark ready + merge
   (project rule: CI both arches IS the gate; fix-forward).











