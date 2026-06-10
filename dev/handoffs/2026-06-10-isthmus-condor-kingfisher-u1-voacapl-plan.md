# 2026-06-10 isthmus-condor-kingfisher — U1 voacapl prediction: grounded, planned, adrev'd

## One-sentence frame

Executed the **voacapl grounding spike** (the gated can't-fabricate first action),
then wrote the **U1 offline-HF-prediction plan** against the real captured I/O
format and ran it through **build-robust-features cross-provider adversarial
review** — the plan is hardened (rev 2) and **ready to EXECUTE**; no U1 Rust code
exists yet.

## What was completed (all pushed)

1. **gfortran installed** (operator-approved sudo). **voacapl built from source on
   arm64** (`~/Code/voacapl`, `--prefix=$HOME/.local`, no sudo for install),
   `makeitshfbc` ran → `~/itshfbc/` materialised. Build clean on aarch64 — proves
   arm64 field-feasibility. Binary at `~/.local/bin/voacapl`.
2. **Real I/O captured.** Ran a DM43→DM34 circuit with N0DAJ's VARA HF dials; output
   round-trips (215.2 km / az 301.65° / 24 hourly REL blocks, physically sane). Card
   formats taken from voacapl source `src/voacapw/voacap.for` WRITE statements —
   authoritative, not guessed.
3. **U1 plan written** (`docs/superpowers/plans/2026-06-10-u1-voacapl-prediction.md`,
   7 TDD tasks) grounded in the real format + real origin/main code (reuses
   `position::grid_to_lat_lon`, `Gateway.frequencies_khz`, the `Clock` pattern,
   `UiError`, `ManagedModem` discipline).
4. **build-robust-features adrev** — 4 reviewers / 2 providers (3 Claude angles +
   1 Codex RF round). 19 findings (F1–F19) + dispositions recorded in
   `docs/superpowers/plans/2026-06-10-u1-voacapl-prediction-adrev.md`. Raw Codex
   transcript (5211 lines) is local-only in gitignored `dev/adversarial/` (in the
   worktree). **Plan revised to rev 2:** DTO contract fix + TDD scaffolding folded
   in inline; remaining per-finding fixes specified by task#.
5. **Tracked artifacts:** real fixtures `src-tauri/tests/fixtures/voacap/{dm43-dm34-input-deck.dat,dm43-dm34-voacapx.out}`; the I/O contract `docs/reference/voacapl-io-contract.md` (was gitignored/main-only → unreadable by the executor; F6).

### Top adrev findings the executor MUST apply (full table in the adrev doc)
- **F1 (both providers):** carry the EXACT input `frequencies_khz` through results by
  index — never re-derive from VOACAP's lossy 1-decimal display (7103/7108 → "7.1").
- **F7 (Codex, deepest):** REL is vs VOACAP's generic `REQ.SNR=73 dB`; mis-calibrated
  for VARA/ARDOP. Expose SNR+MUFday alongside REL; set a data-mode `req_snr_db`.
- **F2:** resolve the sidecar via `ShellExt::shell(&app).sidecar("voacapl")`, NOT
  `BaseDirectory::Resource` (packaged-`.deb` break otherwise).
- **F5:** bundle `database/version.w32` or voacapl hard-aborts (verified in Fortran).
- **F8:** inject `Clock`, use the real UTC year (was hardcoded 2026).

## In progress / NEXT

- **EXECUTE the U1 plan.** Critical first action next session: read the plan's
  **REVISION 2 banner + the adrev dispositions table**, then run
  `subagent-driven-development` (or `executing-plans`) **applying every "FIX in plan"
  disposition**. Start with task group **{1,2,3}** (DTO + deck + parser — pure logic,
  fixture-backed). voacapl + `~/itshfbc` are already installed for the gated live test.

## Repo / worktree / external state

- **Main checkout** on `bd-tuxlink-xygm/recover-handoffs`, in sync with its origin,
  no rebase. (This is a docs branch **1044 commits behind `origin/main`** — all real
  code lives on `origin/main`; do NOT plan/build against this checkout's tree.)
- **In-flight worktree:** `worktrees/bd-tuxlink-ipjt-u1-voacapl-prediction/` (branch
  `bd-tuxlink-ipjt/u1-voacapl-prediction`, off `origin/main`, **pushed; no PR yet**,
  claimed by bd `tuxlink-ipjt`). Untracked/gitignored on disk: `node_modules/`
  (installed for the push hook), `dev/adversarial/2026-06-10-u1-voacapl-rf-correctness-codex.md`
  (the raw Codex transcript). No `target/` yet (no cargo build run there). All
  *tracked* work is committed + pushed. **Keep the worktree** — execution continues there.
- **External (non-repo) state needed by the gated live test:** `~/Code/voacapl`
  (built tree), `~/.local/bin/voacapl`, `~/itshfbc/` (coeff data). Leave in place.
- **bd:** `tuxlink-ipjt` IN_PROGRESS (planning+adrev done, execution next), dep on
  umbrella `tuxlink-axq0`.

## Pending decision (flag at execution, do not silently resolve)

- **SSN on-disk cache write + opportunistic refresh** is deferred (spec §12-sanctioned);
  v1 reads the bundled forecast table. Confirm read-only-table is acceptable for U1
  (alpha = vettedness) or scope the writable cache in.
- **Short-path-only** prediction (CIRCUIT card `S`): fine for regional/NVIS emcomm;
  DX long-path is a documented v1 limitation (F15).
