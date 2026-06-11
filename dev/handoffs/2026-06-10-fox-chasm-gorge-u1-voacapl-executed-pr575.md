# 2026-06-10 fox-chasm-gorge — U1 voacapl prediction: EXECUTED → PR #575

## One-sentence frame

Executed the **U1 offline-HF-prediction plan** end-to-end via
`subagent-driven-development` (7 TDD tasks, per-group review loops), applied every
planning-adrev disposition (F1–F19) **and** a fresh cross-provider **implementation**
Codex adrev (P1–P3) — the prediction engine is complete, validated against real
voacapl, and open as **PR #575** (CI-gated); packaging into the `.deb` is the one
tracked follow-up.

## Branch / PR / worktree state (re-verified at session end)

- **Feature branch:** `bd-tuxlink-ipjt/u1-voacapl-prediction` — **PR #575 MERGED to
  main** 2026-06-11T00:54Z (merge commit `85b58dd`). U1 prediction engine is on
  `main` (released alongside 0.48.0). bd `tuxlink-ipjt` CLOSED.
  - **Post-PR fix:** CI `verify` initially failed (the per-task gate ran
    `cargo test --lib` + clippy, neither of which runs **doctests**; CI runs full
    `cargo test --doc`, and an untagged ` ``` ` fence around an ASCII diagram in
    `engine.rs` compiled as a doctest and failed on the `→`/`<tempdir>` tokens).
    Fixed by tagging the fence ` ```text ` (commit `fa28c67`) + merging `origin/main`
    in (commit `f52abc7`, clean auto-merge — the "conflicts" the operator saw were
    the red check + GitHub computing against the pre-merge HEAD; git found no real
    conflict). CI went green (build+verify, both arches) → merged.
  - **Lesson for next time:** the pre-push gate MUST run full `cargo test` (incl.
    `--doc`), not just `cargo test --lib`. Untagged code fences in `///`/`//!` doc
    comments are compiled as doctests — tag diagrams ` ```text `.
- **Main checkout** is on `bd-tuxlink-xygm/recover-handoffs` (this handoff committed
  there). That docs branch is ~1044 behind `origin/main`; all real code is on the
  feature branch / `origin/main`. There is a pre-existing untracked handoff in the
  main checkout (`...arroyo-lichen-grouse-...md`) left by an earlier session — NOT
  touched by this session.
- **In-flight worktree:** `worktrees/bd-tuxlink-ipjt-u1-voacapl-prediction/`
  (claimed by bd `tuxlink-ipjt`, IN_PROGRESS). **Keep it** (PR open, may need
  iteration). Untracked/gitignored on disk:
  - `node_modules/` (installed for the push hook's docs-link lint),
  - `target/` (cargo build artifacts — safe to `rm -rf` to reclaim disk),
  - `dev/adversarial/2026-06-10-u1-voacapl-rf-correctness-codex.md` (planning adrev,
    local-only) + `dev/adversarial/2026-06-10-u1-voacapl-impl-codex.md` (this
    session's implementation adrev, 7675 lines, local-only — gitignored),
  - `src-tauri/binaries/` and `src-tauri/resources/itshfbc/` — gitignored, each holds
    only a tracked `.gitkeep` (the per-arch binary + itshfbc tree are CI-generated).
- **External (non-repo) state:** `~/.local/bin/voacapl` (1.29 MB aarch64) + `~/itshfbc/`
  (1.4 MB; has `database/version.w32`) — **leave in place**, the gated live test uses
  them. `/tmp/voacapl-stash-fcg` is a stray copy of the staged sidecar binary moved
  aside to prove the clean-checkout build — **safe to delete**.

## What was completed (all pushed)

All 7 plan tasks, each TDD + clippy-clean, with per-group multi-perspective review loops:
1. `mod.rs` DTOs + `PropagationError` (F1/F12/F13).
2. `deck.rs` VOACAP deck builder — byte-for-byte vs the captured golden deck;
   `active_hf_frequencies_khz` single-source-of-truth; HF-window filter + >11 error
   (F9); parameterized REQ.SNR (F7); Clock-supplied year (F8).
3. `parse.rs` `voacapx.out` parser — **carries exact input kHz by column index**
   (7103≠7108 never collapse to "7.1" — F1); tokenizes to the label column (F4);
   fail-closed 24-length guards on REL/SNR/MUFday (F16). (TDD caught a real
   label-prefix bug: `"SNR"` matched `"SNR LW"/"SNR UP"` → 72 rows; the F16 guard
   surfaced it.)
4. `ssn.rs` bundled forecast + fallback. **Bundles only the verified anchor
   2026-06=100** (from the grounding run), not a fabricated trend (Codex P3).
5. `engine.rs` — `tempfile::TempDir` RAII scratch + symlinked read-only itshfbc trees
   (F10/F14); bounded run that kills a runaway voacapl and reaps on every path
   (a `try_wait`-error zombie leak was found+fixed in review).
6. `commands.rs` — `propagation_predict_path`: Clock-injected UTC (F8); sidecar
   resolved adjacent-to-exe not via Resource (F2); `spawn_blocking`; **always-managed
   `Ready|Unavailable` state** so a soft-disabled engine returns a clean
   `UiError::Unavailable` rather than a Tauri extractor error (Codex P2).
7. Bundling/test/attribution — tauri bundle extended (F18); `#[ignore]` gated live
   test guards `version.w32` (F5) + the F1 carry-through against real voacapl;
   attribution corrected to **NTIA public-domain + Watson CC0**, not GPL-3 (F11).

**Validation:** 35 unit tests + 1 gated live test (real voacapl DM43→DM34: bearing
301.65°, 215.2 km, 7103/7108 distinct) green; `clippy --all-targets -D warnings`
clean; **clean-checkout `cargo build` confirmed** (the `externalBin` line is
deliberately OUT of the committed `tauri.conf.json` — Tauri validates it at
cargo-build time and it would otherwise break the CI verify gate). Backend-only (zero
TS), so `vitest` is unaffected.

**Cross-provider adrev (the unique-value gate):** planning adrev (Claude×3 + Codex,
F1–F19) was done last session; THIS session ran a fresh Codex round on the *shipped
implementation* — it independently confirmed the parser/deck/process work and found 3
issues (P1 bundle-not-wired, P2 extractor-error-not-Unavailable, P3 fabricated-SSN).
P2+P3 fixed in code; P1 is the tracked packaging follow-up below.

## Deferred / pending decision (filed in bd, NOT hidden gaps)

- **`tuxlink-hhxs` (P2):** wire voacapl into release CI (externalBin + itshfbc
  injection per `docs/reference/voacapl-ci-bundling.md`). **OPERATOR DECISION: arm64
  build via native `ubuntu-24.04-arm` runner vs cross-compile** (the plan's flagged
  "largest infra unknown"). Until this lands, a *packaged* `.deb` build degrades the
  command to `Unavailable` — the engine is fully validated in dev, just not bundled.
- **`tuxlink-l6ol` (P2):** extend SSN with authoritative SWPC values + the
  spec-§12 writable cache/refresh. **PENDING operator confirm** that the read-only
  single-anchor table is acceptable for the U1 alpha.
- **`tuxlink-s9o1` (P3):** long-path (CIRCUIT 'L') for DX/antipodal stations; v1 is
  short-path-only (documented inline in `deck.rs`; correct for regional/NVIS emcomm).

All three depend on `tuxlink-ipjt` (which stays IN_PROGRESS until PR #575 merges).
