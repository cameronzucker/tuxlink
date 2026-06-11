# U1 voacapl plan — adversarial-review findings & dispositions

> `build-robust-features` Step 2 record. 4 reviewers, 2 providers: 3 Claude
> agents (Rust/Tauri integration; security/sidecar-boundary; subagent-readiness/
> completeness) + 1 Codex round (RF/VOACAP correctness). Raw Codex transcript:
> `dev/adversarial/2026-06-10-u1-voacapl-rf-correctness-codex.md` (gitignored,
> 5211 lines, local-only per the public-repo cleanliness call). This doc is the
> tracked convergence summary; dispositions are folded into the revised plan
> (`2026-06-10-u1-voacapl-prediction.md`).

Agent: isthmus-condor-kingfisher. Reviewed plan rev 1 (commit `3eead1d`).

## Convergence highlights (multiple reviewers / cross-provider)

- **Frequency identity loss** (Completeness B1 + Codex P2): the parser re-derived
  `frequency_khz` from VOACAP's lossy 1-decimal display header, so 7103/7108 kHz
  collapse to "7.1" → 7100 kHz and predictions can't map back to the operator's
  exact dial. **Both providers, independently.** Architectural fix.
- **Sidecar path resolution wrong** (Integration B1 + Completeness M5): `externalBin`
  is placed adjacent to the main exe, NOT under `BaseDirectory::Resource`; the plan's
  `resolve("binaries", Resource).join("voacapl")` breaks in the packaged `.deb`
  (the `test_production_mount_path` failure class).
- **SSN year hardcoded 2026** (Codex P1 + Completeness M3): `ssn_for(2026, month)`
  ignores the real year; `Clock` was claimed-reused but never wired.

## Ranked findings + dispositions

| # | Sev | Finding | Disposition |
|---|---|---|---|
| F1 | BLOCKER | Parser derives `frequency_khz` from lossy display header (3.6→3600≠3590; collapses 7103/7108) | **FIX in plan:** `PredictionInputs.frequencies_khz` carried through by index into `ChannelReliability` (new `frequency_khz` = exact input dial; `voacap_mhz` = rounded compute slot, informational). Parser keyed by column index, not re-parsed freq. (Tasks 1, 3, 6) |
| F2 | BLOCKER | Sidecar resolved under `Resource`; wrong for `externalBin` in packaged build | **FIX:** resolve via `tauri_plugin_shell::ShellExt::shell(&app).sidecar("voacapl")` (project already uses `ShellExt`), fall back to `current_exe().parent().join("voacapl")`. Gated live test must run against the packaged path. (Tasks 6, 7) |
| F3 | BLOCKER | Every task lacks the build-robust-features TDD preamble / completion check / per-group review loop | **FIX:** add the mandated preamble + completion check to every task and a review-loop block after each task group. (All tasks) |
| F4 | BLOCKER | `DATA_START=5`/`DATA_END=60` fragile (accidentally correct; drops the 11th of 11 freq slots) | **FIX:** tokenize the data region up to the label column (col 67), not a hardcoded 60; validate `freqs_khz.len() == freq_count`. (Task 3) |
| F5 | BLOCKER | `version.w32` (+ other read-only `database/` files) must be in the bundled itshfbc or voacapl hard-aborts (`voacapw.for:491`) | **FIX:** Task 7 asserts the bundled tree includes `database/version.w32`, `voacap.def`, `north_pole.txt`; gated live test runs against the *bundled* tree, not `~/itshfbc`. |
| F6 | BLOCKER (grounding) | IO-CONTRACT.md (authoritative formats + build recipe) was gitignored / main-checkout-only — unreadable by the worktree executor | **FIXED:** committed `docs/reference/voacapl-io-contract.md` (tracked); voacapl build recipe inlined into Task 7. |
| F7 | P1/major | REL is reliability vs VOACAP's generic `REQ.SNR=73 dB` — mis-calibrated for VARA/ARDOP data modes; using it as the sole ranking metric distorts the map | **FIX (design refinement):** set a data-mode-appropriate `REQ.SNR` + bandwidth in the SYSTEM card; expose **SNR and MUFday alongside REL** in the DTO so U3 ranks by a defensible composite (REL gated by SNR-margin/MUF), not raw REL-vs-73dB. Document the chosen defaults. (Tasks 1, 2) |
| F8 | P1/major | SSN year hardcoded 2026; `Clock` never wired | **FIX:** inject `crate::catalog::stations_cache::Clock`; derive UTC year+month; `ssn_for(year, month)` uses the real year. (Tasks 4, 6) |
| F9 | major | Frequency F5.2 overflow for ≥100 MHz silently corrupts the deck; >11 freqs truncated | **FIX:** clamp/reject to the HF window (≈1.8–30 MHz) in `build_deck`; on >11 valid HF dials, error/batch explicitly (not silent `.take(11)`). (Task 2) |
| F10 | major | Scratch dir leaks on error paths; `/tmp` fallback on a shared Pi (predictable name, TOCTOU) | **FIX:** use `tempfile::TempDir` (RAII cleanup on all paths; `tempfile` is already a dep); fail-closed if `app_cache_dir()` is unavailable rather than dropping to `/tmp`. (Task 5) |
| F11 | major | NOTICE framing wrong: core VOACAP is US-gov **public domain** (NTIA) + Watson **CC0**; only `dst2csv/dst2ascii` are GPL-3 | **FIX:** Task 7 bundles the NTIA disclaimer + CC0 attribution (the "no warranty as to accuracy/suitability" notice is also operationally relevant), not a GPL-3-only NOTICE. |
| F12 | major | No SSN value/provenance in output DTO — U3 can't render "solar data N old" (spec §5/§7) | **FIX:** add `ssn`, `month`, `year` to `PathPrediction`. (Task 1) |
| F13 | major | `lib.rs` edited by Task 1 AND Task 6 (merge/order hazard); private `mod` vs the file's `pub mod` convention (also breaks the `tests/` live test) | **FIX:** `pub mod propagation;`; explicitly sequence Task 1 before Task 6; note both touch `lib.rs`. |
| F14 | minor | Symlinked read-only `database/` write-through fear | **RESOLVED (source-verified):** the headless P2P path opens `voacap.def`/`north_pole.txt`/`version.w32` `status='old'` (read-only); only `run/` is written. Symlinking the bundled tree is safe. Optionally adopt voacapl's `--run-dir=` flag for cleaner isolation. (Task 5 note) |
| F15 | minor | CIRCUIT card hardcodes `S` (short path) — DX long-path stations falsely "unreachable" | **DOCUMENT:** v1 is short-path-only (correct for regional/NVIS emcomm, the primary use case); add to spec/U3 open items as a known limitation, not presented as general truth. (plan §10 + spec follow-up) |
| F16 | minor | `snr_by_hour` length unguarded (only REL checked) | **FIX:** symmetric 24-length guard on SNR. (Task 3) |
| F17 | minor | `.setup()` uses `?` on resource resolve — would abort app startup if it fails | **FIX:** degrade with `match`+`eprintln!` (the pattern the rest of `.setup()` uses); prediction is optional, must not block launch. (Task 6) |
| F18 | minor | tauri.conf bundle snippet reads as a replacement of the existing `bundle` object | **FIX:** Task 7 says "extend `resources` + add `externalBin`", preserving existing icon/deb/license config. |
| F19 | minor | azimuth/distance parse uses magic `len-4`/`last` indices; only one fixture exercises it | **FIX:** keep, but assert against the fixture + add a guard comment; flag for a multi-path fixture later. (Task 3) |

## Deferred (spec-sanctioned, explicitly flagged — NOT silent gaps)

- SSN **on-disk cache write + opportunistic network refresh** (spec §12 open item):
  v1 reads the bundled forecast table (always offline-correct). The writable cache
  under `app_data_dir()` + refresh source is a follow-up sub-task. Must never add a
  network precondition; never touch the keyring. Flagged for operator scoping at
  execution (alpha = vettedness — confirm read-only-table is acceptable for U1).
- **Antenna-model config surface** (spec §12): v1 ships defensible defaults; the
  operator-configurable antenna picker is U3/follow-up. `tx_power_w` IS plumbed.
- **Area-coverage ranking (mode b)**: deferred per spec §5; v1 is point-to-point.

## Process note (learning-sandbox)

The cross-provider gate earned its keep: Codex independently confirmed F1/F4/F8
and contributed the deepest finding (F7 — REL is mis-calibrated against a generic
73 dB target), which a same-provider panel sharing my framing was less likely to
surface. The security agent's tracing of the actual Fortran (`voacapw.for`)
*disproved* my own open-item fear (F14) and found a real new blocker (F5,
`version.w32`) — reading the engine source beat reasoning from the I/O contract.
