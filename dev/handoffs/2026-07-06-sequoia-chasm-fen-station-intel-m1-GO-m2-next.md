# Handoff — 2026-07-06 (sequoia-chasm-fen): Station Intelligence M1 = **GO**, M2 next

M1 (the decoder go/no-go) is **complete and the verdict is GO.** The clean-room
soft-demapper + normalized min-sum LDPC core decodes at the theoretical limit for its
configuration. **Next: M2 — the real-WAV acquisition slice (channelize + Costas sync +
thin end-to-end), which is the plan's SECOND hard STOP gate.**

## ⭐ THE GO/NO-GO RESULT (M1)

**50%-decode crossing = −19.69 dB** (SNR in 2500 Hz ref BW, AWGN), **zero false decodes.**
- **−0.09 dB** from the FAIR anchor −19.6 dB (QEX Table 5, FT8 AWGN, **N=1;BP** — our exact
  config: single-symbol demap + BP, no OSD, no block detection). Essentially exact.
- **+1.11 dB** from the HEADLINE −20.8 dB (QEX Table 6, BP+OSD, no-AP). This gap is the
  *known, expected* OSD + block-detection headroom, recoverable in L1/L2.
- Comfortably inside the plan's "within 1–2 dB of −20.8" pass window; nowhere near the
  −16.8 dB STOP line (≥4 dB worse). **The LLR-scaling + min-sum core is SOUND → the
  native-decoder bet is funded. Proceed to M2.**

Reproduce the full curve (leaf crate compiles+runs locally — see below):
```
cargo test -p tuxlink-ft8 --manifest-path src-tauri/Cargo.toml --release -- --ignored awgn_snr_curve --nocapture
```

## What shipped this session (M1, on PR #1024)

`tuxlink-ft8` crate, commits `67f306dc` → `aecf00b1` (branch
`bd-tuxlink-b026z.1/station-intel-ft8-m1`):
- **T1.1** `src/llr.rs` (max-log soft-demapper + `sqrt(24/variance)` normalization),
  `src/decode.rs` (normalized min-sum BP over the (174,91) Tanner graph), `src/ldpc.rs`
  (+ the `MN` variable→check table). LLR convention pinned `log(P1/P0)` (positive⟹bit 1).
- **T1.2** `src/awgn.rs` (test-only): noncoherent-8-FSK AWGN model, SNR₂₅₀₀ conversion,
  **self-verifying calibration** (model SER vs closed-form Pe, RED-demonstrated), the
  decode-probability sweep, deterministic SplitMix64+Box–Muller RNG (no new dep).
- 57/57 crate tests green (1 `#[ignore]`d: the 190k-decode full sweep), clippy
  `-D warnings` clean, MSRV 1.75.

## Review trail (4 rounds + Codex, all clean)
- T1.1 task review → Approved. T1.2 task review → Approved (reviewer independently
  re-derived every number).
- **Codex cross-provider adversarial round → 5 findings, ALL fixed in `aecf00b1`:**
  1. **Real decoder bug:** an all-zero / no-signal / erased input collapsed to the
     all-zero LDPC word, which has zero syndrome AND passes CRC-14 → `converged && check_crc`
     admitted empty slots as false decodes. Fixed: `ldpc_decode_ms` now rejects the
     all-zero word (mirrors ft8_lib `bp_decode`'s `plain_sum==0` break). **Carry this
     forward into M3/M4:** gate real decodes on `converged` (which now excludes all-zero)
     PLUS the sync-metric floor, not CRC alone.
  2. Calibration self-test didn't gate `--ignored awgn_snr_curve` (libtest skips
     non-ignored tests) → now called inline at the top of the sweep.
  3. Loosened the full-sweep assertion to the plan's real −16.8 dB STOP line.
  4. Added a −20 dB near-threshold point to the CI-default smoke.
  5. Made the provenance ledger self-consistent (two-tier rule: FT8-protocol expression
     from QEX/WB2FKO/ft8_lib only vs standard/public-domain algorithms cited to their own
     literature) + corrected a RustFT8 overstatement (available, not transcribed).
  Re-verified after the fixes: full --release sweep STILL −19.69 dB, zero false.
- Round 4 final whole-branch review → **READY TO MERGE, zero findings.** MN table
  byte-identical to ft8_lib (522 edges), min-sum sign structure faithful, no clean-room
  violation. Raw Codex transcript: `dev/adversarial/2026-07-06-m1-ft8-decoder-gonogo-codex.md`
  (gitignored, local-only).

## KEY OPERATIONAL FACT (saved to auto-memory)
The `tuxlink-ft8` leaf crate **compiles + runs its tests locally on the Pi in ~14s**
(`cargo test -p tuxlink-ft8 --manifest-path src-tauri/Cargo.toml`). The "Pi can't
cold-compile Rust" rule is about the full Tauri workspace — NOT this leaf crate. **Do
real local red→green TDD for M2–M4; don't CI-round-trip for iteration.** (The 190k-decode
`awgn_snr_curve` is `#[ignore]`d and needs `--release`; everything else runs in debug.)

## Branch / PR / worktree / CI state
- Branch `bd-tuxlink-b026z.1/station-intel-ft8-m1`, HEAD `aecf00b1`, **pushed**.
- **PR #1024** (draft) — the L0 spike's continuous-CI PR (per the plan; M0 was the earlier
  #1020, already merged). It accumulates M1→M4; keep it draft while the spike is in
  progress; mark ready + merge (no-ff, ADR 0010) when L0 completes or the go/no-go outcome
  dictates. NOT parked — an in-progress spike PR.
- CI on `aecf00b1`: verify green by SHA (`gh run list --branch … --json headSha,conclusion`
  matching `aecf00b1`). Earlier `fe36ab31` went green on all three workflows.
- **Worktree** `worktrees/bd-tuxlink-b026z.1-station-intel-ft8-m1` — **stays alive; M2
  continues here** (same crate, same branch). bd `tuxlink-b026z.1` claims it, in_progress.
  Untracked/gitignored on disk: `node_modules/`, `src-tauri/target/`, `.superpowers/sdd/`
  (the SDD ledger + review packages), `dev/adversarial/` (Codex transcript). Nothing
  at-risk; nothing to propagate beyond what's committed.
- Working tree clean at handoff (this doc is the only add).

## bd state
- `tuxlink-b026z.1` (L0 spike): **in_progress** — M0 ✓ (merged #1020), M1 ✓ (GO, this
  session), **M2–M4 pending.** Recorded the GO result via `bd remember`
  (`m1-go-no-go-go-2026-07-06`). Do NOT close .1 — M2–M4 remain within it.
- Epic `tuxlink-b026z`; children .2–.6 = L1–L5; .7 = clean-room CI grep-guard follow-up.

## NEXT: M2 — real-WAV acquisition (plan T2.1–T2.3), the 2nd hard STOP gate
From `docs/plans/2026-07-05-station-intel-l0-ft8-decoder-plan.md`, M2 section:
- **T2.1 Channelize + baseband:** WAV (hound) → resample/mix/decimate to 12000 Hz
  baseband with proper windowing (NOT a naive 32-pt FFT at native rate — decimate so bins
  resolve 6.25 Hz). Cite WB2FKO/ft8_lib.
- **T2.2 Costas sync:** coarse 2-D search (freq ≤3.125 Hz, time ≤40–80 ms over DT∈[−2.5,+5]s),
  sync metric = summed Costas-tone power minus off-tone, noise-normalized across ALL THREE
  Costas blocks (WB2FKO); RANKED candidate list (top ~200–300), not a fixed threshold; then
  per-candidate fine time/freq refinement.
- **T2.3 Thin end-to-end KAT:** WSJT-X-generated SINGLE-signal WAVs (one per message type)
  → channelize → sync → demod → LLR → LDPC → assert the exact known message.
- **Setup dependency:** M2 needs single-signal FT8 WAVs as fixtures. Generate them with
  ft8_lib's encoder/`gen_ft8` (MIT, already cloned to the ft8-refs scratch — re-fetch, /tmp
  is ephemeral) or WSJT-X's own encoder. These are DETERMINISTIC known-answer fixtures
  (distinct from the committed M3/M4 RTL-SDR real-off-air fixtures in `tests/fixtures/sdr/`).
- **HARD STOP after M2** (plan): if the real-WAV thin slice fails badly, STOP + surface
  before building the full multi-candidate detector (M3) / multi-pass subtraction (M4).
- **Carry-forwards into M2+:** the zero-false GUARD stack is `converged` (now excludes
  all-zero) + CRC-14 + a **sync-metric floor** (M2 provides the sync metric) — CRC alone is
  ~1/16384 false-accept, so the guard stack (not CRC) delivers zero false.

## Reference material (re-fetch — /tmp is ephemeral)
- ft8_lib (MIT): `git clone --depth 1 https://github.com/kgoba/ft8_lib` — read
  `ft8/decode.c` (channelize/sync/demod), `ft8/constants.c`.
- QEX PDF `https://wsjt.sourceforge.io/FT4_FT8_QEX.pdf` (§6 decode, §8 channels);
  WB2FKO `https://www.sportscliche.com/wb2fko/FT8sync.pdf` (sync metric — the M2 core).
- Clean-room discipline unchanged: implement only from QEX/WB2FKO/ft8_lib(MIT)/RustFT8(MIT);
  wsjtr/WSJT-X = binary oracle only; every constant → provenance comment + PROVENANCE.md row.
