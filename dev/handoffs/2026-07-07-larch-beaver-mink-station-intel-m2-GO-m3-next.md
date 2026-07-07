# Handoff — 2026-07-07 (larch-beaver-mink): Station Intelligence M2 = **GO**, M3 next

M2 (the real-WAV acquisition slice) is **complete and the verdict is GO.** The
clean-room channelizer + Costas synchronizer feed M1's shipped soft-demapper and
decode **all six** `gen_ft8` fixtures to their exact known messages with **zero
false decodes**. **Next: M3 — the full single-pass pipeline + oracle comparator
against the committed real RTL-SDR captures.**

## ⭐ THE GO/NO-GO RESULT (M2)

The plan's **second hard-stop gate PASSES decisively** (STOP was only triggered on
failure; it did not fail):

- **6/6 fixtures decode to their exact message, zero false** — 5 single-signal
  `gen_ft8` fixtures (types 1/0.0/4 at 1500 Hz; standard at 800/2400 Hz) + 1
  deliberately off-grid carrier (`std_cq_1509_offgrid.wav`).
- **Acquisition is bit-exact:** LDPC converges at **iteration 0** on every fixture;
  the `extracted_symbols_match_transmitted` KAT matches **58/58** extracted
  info-symbol tones to an independent re-encoding of the message. The front-end
  lands the exact codeword before any BP iteration — not FEC-rescued.
- **Zero-false guard** (`converged` AND CRC-14 AND `sync_metric ≥ SYNC_FLOOR`)
  validated against silence, white noise, non-finite (NaN) input, and a **strong
  unmodulated CW carrier**.

Reproduce (leaf crate compiles+tests locally in ~45–120s):
```
cargo test -p tuxlink-ft8 --manifest-path src-tauri/Cargo.toml
```

## What shipped this session (M2, PR #1040)

`tuxlink-ft8` crate, commits `85726bd1` → `feb3f9ae` (branch
`bd-tuxlink-b026z.1/station-intel-ft8-m2`, off `origin/main` @ `49cf4976`):
- **T2.1** `src/channelize.rs` — Hann-windowed symbol-length FFT spectrogram
  (`FFT_LEN=3840`→3.125 Hz bins, ¼-symbol 480-sample hop, 372 windows) + an
  arbitrary-frequency single-bin DFT (`tone_power`) for sub-bin refine/extract.
- **T2.2** `src/sync.rs` — coarse 2-D `(fc,t0)` search over DT [−2.5,+5]s, ranked +
  4-Hz-deduped candidates, per-candidate fine time (±40 ms) / freq (**±2 tones**)
  refine, 58×8 info-symbol tone-power extraction, and the `decode_samples` pipeline.
  **Sync metric = ft8_lib's dB neighbour-contrast (`ft8_sync_score`)**, chosen over
  WB2FKO's raw `t/tN` ratio (the ratio rewards spectral emptiness — a near-empty
  region out-scored the true signal 874:14).
- **T2.3** `tests/e2e_gen.rs` — thin end-to-end KAT: exact-set decode assertion per
  fixture + silence/noise/NaN/CW-carrier zero-false negatives.
- **Type-4 non-standard-callsign unpack** (`src/message.rs`, receive-only) — an
  M0-deferred gap, pulled forward because the `nonstd_cq` fixture is a type-4
  message and the gate demands the exact string. Clean-room from ft8_lib
  `message.c`, 4 KATs, no existing M0 behavior changed. TX-side `pack58` stays
  deferred (RADIO-1 receive-only).
- Deterministic `gen_ft8` fixtures under `tests/fixtures/gen/` (+ README with
  provenance, format, and the centered-frame timing: frame at sample offsets
  [14160, 165840)).
- 78 crate tests green (75 lib + 3 e2e), clippy `--all-targets -D warnings` clean,
  MSRV 1.75.

## Review trail (3 rounds + Codex, all clean of Critical/Important)
- **Round 1 task review** (opus) → verified the metric is a faithful `ft8_sync_score`
  port and the type-4 byte layout is bit-exact vs ft8_lib. 1 Important
  (sync.rs module doc misdescribed the metric as WB2FKO `t/tN`) + 3 Minor → fixed
  `dac4d6c8`.
- **Round 2 Codex adversarial** (gpt-5.5 xhigh; 9824-line transcript, empirically
  reproduced via `gen_ft8`) → 6 findings, 4 fixed `72019d1f`:
  1. **Off-grid fine-refine miss** — the dB-contrast coarse metric mislocates an
     off-grid carrier by >1 tone (measured ~6.6 Hz), so the old ±6.25 Hz fine window
     never evaluated the true carrier. Widened fine freq search to **±2 tones**; added
     the `std_cq_1509_offgrid.wav` fixture + KAT.
  2. **Non-finite metric bypass** — a NaN metric slipped past the floor
     (`NaN < FLOOR` is false). `coarse_candidates` now drops non-finite metrics;
     `nonfinite_input_yields_no_decode` KAT added.
  3. **e2e used `any()`** — would pass with spurious extra decodes. Now asserts the
     exact decode set per fixture.
  4. Provenance: `MAX_CANDIDATES` reframed as implementation cap; `POWER_EPS`
     documented as a numerical guard.
- **Round 3 final whole-branch review** (opus) → independently re-validated the DSP
  arithmetic, the ft8_lib-faithful metric + type-4 unpack, and both prior rounds'
  fixes. **Ready to merge: Yes**, only Minors. Two test-quality Minors closed
  `feb3f9ae` (CW-carrier KAT; strengthened sync-metric separation assertion).
- Raw Codex transcript: `dev/adversarial/2026-07-07-m2-ft8-acquisition-codex.md`
  (gitignored, local-only).

## Branch / PR / worktree / CI state
- Branch `bd-tuxlink-b026z.1/station-intel-ft8-m2`, HEAD `feb3f9ae`, **pushed**.
- **PR #1040** (ready) — M2 milestone PR. M1's spike PR (#1024) was merged by the
  operator, so this follows the per-milestone-merge pattern. **CI running on
  `feb3f9ae`** at handoff time (CI + Release build + ECT low-floor build, all
  in_progress). **Verify green by SHA before merge** (`gh run list --branch … --json
  headSha,conclusion` matching `feb3f9ae`); merge no-ff (ADR 0010).
- **Worktree** `worktrees/bd-tuxlink-b026z.1-station-intel-ft8-m2` — bd
  `tuxlink-b026z.1` claims it, in_progress. `node_modules/` was installed (fresh
  worktree needs it for the pre-push `lint:docs` hook). Untracked/gitignored on disk:
  `node_modules/`, `src-tauri/target/`, `.superpowers/sdd/` (ledger + review
  packages), `dev/adversarial/` (Codex transcript). Nothing at-risk beyond committed.
- The old M1 worktree (`…-station-intel-ft8-m1`) was disposed this session (its HEAD
  `b8339e10` verified merged to main first).

## bd state
- `tuxlink-b026z.1` (L0 spike): **in_progress** — M0 ✓ (#1020), M1 ✓ (#1024, GO),
  **M2 ✓ (this session, GO, #1040), M3–M4 pending.** Do NOT close .1 — M3–M4 remain.
- Carry-forwards recorded via `bd remember` (`ft8-m2-m3-carryforwards`).

## NEXT: M3 — full single-pass pipeline + oracle comparator (plan T3.1–T3.3)
From `docs/plans/2026-07-05-station-intel-l0-ft8-decoder-plan.md`, M3 section:
- **T3.1 Full slot decode:** iterate the ranked candidate list, decode each, dedup
  within-slot (multiset on normalized message identity), apply the zero-false guard.
- **T3.2 Oracle comparator:** `fixtures/`-driven harness comparing our decodes vs a
  reference decode list (AP-disabled), per-type match keys, multiset, hash-class.
  Emit parity % + false count. Ships as the permanent regression harness.
- **T3.3 Ordinary-capture parity:** run against ft8_lib bundled sample WAVs first;
  the committed real RTL-SDR captures in `tests/fixtures/sdr/` (with the
  `.jt9-ap-off.txt` reference logs) are the final gate input.

### M3/M4 carry-forwards (from Codex + final review; correct-for-M2, wrong-for-crowded-band)
1. **Type-4 hash-table population** — `message.rs` `unpack_nonstd` takes `&HashTable`;
   must call `save_callsign` on the decoded 58-bit call so later h12/h10 refs in a
   multi-signal slot resolve to `<CALL>` not `<...>` (ft8_lib `message.c`). Inert for
   M2 single-signal; needed when multi-message dedup lands (M3).
2. **Frequency-only coarse dedup** — `sync.rs` collapses two signals at the same
   carrier / different DT into one; revisit for the crowded-band M4 arm.
3. **Coarse frequency localization** — the dB-contrast coarse metric mislocates
   off-grid carriers by >1 tone; M2 works around it with the ±2-tone fine window, but
   weak-signal M3/M4 wants a more robust coarse estimator, and the ±2-tone window can
   reach a ~2-tone-away neighbour in a crowded slot.
4. **`SYNC_FLOOR = 10 dB`** calibrated on noiseless fixtures + one LCG noise
   realization — re-tune against real-SNR captures. For M2 the `converged && CRC` legs
   are the real backstop.

## Reference material (re-fetch — /tmp is ephemeral, but a copy survived this session)
- ft8_lib (MIT, `9fec6ca`) + QEX/WB2FKO PDFs + `gen_ft8` were recovered into this
  session's scratchpad; re-`git clone --depth 1 https://github.com/kgoba/ft8_lib` and
  `make gen_ft8` for M3 fixture generation if needed.
- Clean-room discipline unchanged: QEX/WB2FKO/ft8_lib(MIT)/RustFT8(MIT) only; wsjtr/
  WSJT-X = binary oracle only; every constant → provenance comment + PROVENANCE.md row.
