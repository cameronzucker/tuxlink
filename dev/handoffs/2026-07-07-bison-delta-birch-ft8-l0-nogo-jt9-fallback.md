# Handoff — 2026-07-07 (bison-delta-birch): FT-8 L0 spike = **NO-GO** → jt9/wsjtr fallback

M3 (full single-pass pipeline + oracle comparator) is built, tested, and merged-ready,
and it produced the spike's decisive answer: **the clean-room single-pass decoder is
NOT competitive with jt9 on real captures.** Operator decision: **fall back to the
jt9/wsjtr dependency** (design-doc option), save the M3 work, revisit only if that
dependency proves problematic or the project matures with spare capacity.

## ⭐ The go/no-go result (M3)

| Capture | jt9 (WSJT-X `-8`, AP-off) | Our single-pass | Gate |
|---|---|---|---|
| 40m ordinary | 5/5 | **1/5 (20 %)** | ≥85 % |
| 20m quiet | 2/2 | **0/2 (0 %)** | ≥85 % |

Zero false decodes throughout (the `converged && CRC-14 && sync-floor` guard held).

**Root cause (diagnosed, not guessed):** weak-signal coarse **TIME** localization,
NOT the sync floor. Evidence (reproduce with `floor_calibration_diag`, below):
- Candidates land within **1–3 Hz** of every reference carrier — frequency is fine.
- But their `t0` (frame-start) is scattered/implausible (−960…−24000 samples), and a
  **floor-free** decode targeted at the exact reference carriers **still fails** —
  the coarse dB-contrast metric mislocates `t0` beyond the ±40 ms fine-refine window
  on −14…−19 dB signals.
- ref 2691 Hz has no candidate within 91 Hz (nearest pinned at the 2600 Hz
  `FREQ_MAX_HZ` search ceiling).
- **jt9 decodes 5/5 and 2/2 from the same WAVs** → the audio is good; this is our
  decoder's limit. M2's GO only ever proved *noiseless synthetic* decode.

The missing lever is a robust sub-sample time-sync stage (ft8_lib/WB2FKO
`sync8d`-class), plus likely M4 multi-pass subtraction — bigger than an M3 tuning
task. That is exactly the L-effort the spike exists to gate.

## Operator decision (2026-07-07): B + C

> "Save what we have but fall back to known. We can't spend more time on that at
> this stage. If the dependency proves problematic we can revisit, or if the
> project becomes so mature that we have nothing else to do."

- **Fall back to jt9/wsjtr** for Station Intelligence decode. Do NOT sink more
  L-effort into clean-room weak-signal acquisition unless a revisit condition holds.
- **Save** the M3 work (this PR). The `tuxlink-ft8` crate stays as a tested
  reference/learning artifact.
- **L1 (tuxlink-b026z.2)** is superseded by the fallback; **M4 not attempted.**

## What shipped this session (M3)

Branch `bd-tuxlink-b026z.1/station-intel-ft8-m3` off `origin/main`, commit(s):
- `e648b9a7` **feat(ft8):** M3 code —
  - **T3.1a hash-table population on the UNPACK path** (carry-forward #1):
    `unpack28`/`unpack_std`/`unpack_nonstd` now take `&mut HashTable` and
    `save_callsign` each decoded base call *after* resolving the message's own
    hashed slot, so a later multi-signal-slot message resolves `<CALL>` not `<...>`.
    4 KATs incl. cross-message resolution.
  - **T3.1b within-slot dedup on normalized message identity** (replaces M2's
    frequency-only guard). Extracts reusable `try_decode_candidate` +
    `decode_samples_with_floor`. 2 e2e KATs (same-message-two-carriers collapses;
    two distinct messages both survive).
  - **T3.2 `src/oracle.rs`** — the permanent regression harness: parse jt9 log →
    hash-class-aware multiset compare → parity % + false count. 8 unit KATs.
  - **T3.3 `tests/sdr_parity.rs`** — drives the real captures; reframed to a
    **zero-false regression guard** (recall is a documented known gap), plus the
    `#[ignore]`d `floor_calibration_diag` that reproduces the parity + `t0` evidence.
- (pending) **docs commit** — plan outcome banner + this handoff.
- **96 tests green** (87 lib + 5 e2e + 4 sdr); `clippy --all-targets -D warnings`
  clean; MSRV 1.75.

## Review trail
- Self-review: 3 passes over the hash-save ordering, dedup key, and comparator
  match rules. No findings.
- **Codex adversarial round (2 passes, gitignored transcripts
  `dev/adversarial/2026-07-07-m3-ft8-codex*.md`):** the custom-prompt pass
  brute-forced type-4 hash collisions (the carry-forward #1 risk) and confirmed
  22/12-bit hashes collide — but that is **protocol-inherent** to FT8 (identical
  exposure in WSJT-X/ft8_lib; our save-on-decode mirrors ft8_lib, and the `oracle`
  hash-class rule treats bracketed callsigns as unverifiable, so a collision is
  neither masked-as-false nor rewarded). The structured pass read the full diff +
  fixtures + Cargo config. **Neither pass emitted a synthesized findings block
  before hitting the `codex review` turn/time limit** (large diff + slow Pi
  `cargo test`). No defect surfaced; combined with the 3-pass self-review, no
  code change warranted. (Shelved crate — impact is low per calibrate-to-impact.)

## bd / branch / worktree state
- `tuxlink-b026z.1` (L0 spike): **in_progress**, notes updated with the NO-GO
  verdict. **Close it after this PR merges** (it still claims the worktree).
- `tuxlink-b026z.2` (L1 core engine): notes updated — **superseded**, do not start
  without re-confirming direction.
- `bd remember` key `ft8-l0-nogo-jt9-fallback` records the decision.
- **Worktree** `worktrees/bd-tuxlink-b026z.1-station-intel-ft8-m3` — dispose after
  merge (ADR 0009 ritual). Gitignored-on-disk: `src-tauri/target/`, `node_modules/`
  (installed for the pre-push `lint:docs` hook), `dev/adversarial/` (Codex transcript),
  `dev/scratch/m3-codex-prompt.txt`. Nothing at-risk beyond committed.
- PR: opened against `main` (see PR link in session output). CI running by SHA.

## If/when a revisit happens (not now)
1. The single highest-leverage fix is coarse **time** sync: replace the dB-contrast
   `t0` argmax with a proper sync-symbol correlation (ft8_lib `sync8d`), sub-sample.
   Re-run `floor_calibration_diag` to confirm `t0` lands within ±40 ms of truth.
2. Widen `FREQ_MAX_HZ` above 2600 Hz (ref carriers reach 2714 Hz; currently clipped).
3. Then M4 multi-pass subtraction for the crowded arm.
The `oracle` harness + committed SDR fixtures make progress measurable immediately.

## Housekeeping flagged (not done this session)
- `MEMORY.md` is ~20 KB, approaching the 24 KB read limit; a curation/compaction
  pass is due (deferred — risky to rush; orthogonal to M3).
