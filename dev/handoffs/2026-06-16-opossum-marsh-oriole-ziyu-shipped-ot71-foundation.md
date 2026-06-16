# Handoff — opossum-marsh-oriole — ziyu shipped (CI-green); ot71 foundation (L1+L2) in draft PR

**Date:** 2026-06-16 · **Agent:** opossum-marsh-oriole
**Operator branch at session start:** `bd-tuxlink-xygm/recover-handoffs` (main checkout, lease-locked by 3 other live sessions all session — all my work was in worktrees off `origin/main`).

## Shipped this session

### PR #763 — tuxlink-ziyu (reachability recompute ~5s "feels like a bug") — **CI-GREEN, ready to merge**
**Measured first (operator directive).** Timed staged `voacapl` on a real METHOD-30 deck:
single run **~5–13ms warm** (47ms cold); six concurrent **~11ms total** on this 4-core Pi.
⇒ ~50 stations of pure voacapl ≈ **~80ms**, three orders of magnitude under the observed 5s.
**voacapl latency is NOT the cause** — operator intuition confirmed.

**Root cause:** uncoalesced multi-firing of the full N-station re-sweep. `AntennaControl`
height is a `type="range"` slider whose `onChange` fires on every grid-index crossing
mid-drag; SNR/power are `type="number"` firing per keystroke. Each event ran
`handlePrefsChange` → prefs write + `predictReload++` → a complete `useReachabilityMap`
sweep, with **no debounce**. `reach.loading` was computed but surfaced nowhere → the
churning map read as frozen.

**Fix:** `useDebouncedCommit` hook (coalesce burst → one trailing commit; flush-on-unmount);
`StationFinderPanel` routes the prefs change through a 300ms debounced commit (setPrefs stays
immediate for live UI); surfaced `reach.loading` as an "updating reachability…" status.
Tests: hook (5) + panel burst→single-persist regression + controls affordance; tsc clean;
**all 4 CI checks pass**. Branch `bd-tuxlink-ziyu/reachability-recompute-perf`.
→ **Operator: smoke (drag height/power → map re-colors ONCE after settle), then merge.**

### Worktree disposal (operator directive) — DONE
Disposed the 3 merged-dead worktrees (all tips in `origin/main`):
- `bd-tuxlink-bl01-antenna-realpatterns-design` — clean → removed directly.
- `bd-tuxlink-bl01-phase1-picker-library` — had a staged CSS change + gitignored `dev/scratch`
  artifacts → **archived** to `.claude/worktree-archives/` then removed.
- `bd-tuxlink-ytay-theme-consistency` — had a **staged-but-uncommitted handoff doc**
  (`2026-06-16-basin-fern-spruce-...`, unique, not on origin/main) → **archived** then removed.
Local branch refs remain (merged-dead, harmless). Stashes were repo-global (`refs/stash`,
identical across all three) — left untouched. Archives are local-only (gitignored).

## In progress

> **UPDATE (post-handoff, same session):** operator directed merging #765 for today's
> combined release. **#765 is now MERGED to main** (merge commit `50e48b27`) — Layers 1+2
> are in `main`. The `bd-tuxlink-ot71/propagation-update` branch is merged-dead and its
> worktree was disposed. **L3–5 continue on a FRESH worktree off the updated main**
> (e.g. `bd-tuxlink-ot71/propagation-update-l3`), NOT the old worktree. tuxlink-ot71 stays
> open. The merged foundation is inert (no user-facing surface yet); the runtime-mutable
> change is behavior-identical today since nothing writes the writable forecast.

### PR #765 (MERGED) — tuxlink-ot71 "Update propagation data" — Layers 1+2 of 5
**Operator source decision (2026-06-16):** NOAA SWPC, **model-correct** posture —
`predicted-solar-cycle.json` → monthly smoothed `predicted_ssn` (the VOACAP input, keyed
`YYYY-MM`, maps directly onto `SsnForecast.monthly`); `wwv.txt` → live SFI/K **context only**.
Both endpoints verified live. RF catalog fallback (PROP_WWV/PROP_SGAS) + runtime-mutable
forecast build regardless.

**Plan:** `docs/superpowers/plans/2026-06-16-ot71-update-propagation-data.md` (grounded design,
layered build, wire-walk DoD).

**Built + pushed (CI compiling at handoff write; check `gh pr checks 765`):**
- **L1** `solar.rs` (pure parsers, tested vs REAL fetched formats): `parse_swpc_predicted_ssn`,
  `parse_wwv` (reads K after "was" so the "1200 UTC" timestamp isn't mistaken for K),
  `derive_ssn_from_sfi` (published Covington/NOAA F10.7↔SSN relation, RF path only, floored at 0).
  `ssn.rs` runtime-mutable forecast: `forecast_path`, `load_writable_then_bundled` (degrade
  silently to bundled on missing/empty/corrupt), atomic `persist`; `Serialize`+`Default`.
- **L2** `solar_update.rs` (orchestration, tempdir-tested): `SolarSnapshot` (indices + freshness
  stamp + provenance → `solar-snapshot.json`, separate from the forecast); `apply_swpc_update`
  (internet); `apply_rf_solar_reply` (over-radio: derive SSN from SFI into current month).
  `commands.rs`: `propagation_predict_path` now loads the forecast **fresh per call**
  (`load_writable_then_bundled`) → an update applies WITHOUT restart (no Arc<RwLock> needed;
  forecast is tiny + sweeps are debounced post-ziyu).

**CI:** ot71 `verify` (cargo build + clippy `-D warnings` + cargo test) **PASSED on both
arm64 + amd64** — the blind-written Rust compiles clean and every unit test passes. (build-linux
packaging was still running at handoff; verify is the code-correctness gate.)

**Codex adrev — NOT usable.** It bogged the entire run trying to run `cargo test`/`cargo build`
locally (the "no cold cargo on this Pi" trap, hitting Codex) and produced **no findings block**.
The transcript (`dev/adversarial/2026-06-16-ot71-solar-parsers-codex.md`, gitignored) is just
exploration + cargo thrash. **Next session: re-run the L1+L2 (and L3+L4) adrev with the prompt
explicitly saying "STATIC READ ONLY — do NOT run cargo/build/test; grep+read source only."**
The L1+L2 correctness is currently backed by CI-green unit tests, not an adrev.

### REMAINING — ot71 Layers 3–5 (this branch, before un-drafting)
- **L3 (Rust):** reqwest fetch of the 2 SWPC endpoints (reqwest 0.13 already a dep — see
  `tiles/fetch.rs` pattern) wrapped around `apply_swpc_update`; a `propagation_update(force)`
  Tauri command + `propagation_indices_read` (for the conditions bar); register both in
  `lib.rs` invoke_handler. **RF ingestion (trickiest):** route a PROP_WWV catalog reply →
  `apply_rf_solar_reply` — detect a solar reply in the inbox (cf. `catalog_ingest_listing_reply`
  + `reply.rs` `parse_reply`) and call a new backend command. The offline RF path is the
  operator's **mandatory, defining scenario** — make it robust.
- **L4 (UI):** "Update propagation data" button in the **reserved slot** in
  `StationFinderControls` (lines ~74–77, beside "Update station list"; a non-functional button is
  a stub — wire it). Live SFI/K in the conditions bar via the existing `sfi`/`kIndex` props ←
  `SolarSnapshot`. Wire the "solar data N old" caption to the snapshot stamp.
  Map-layer/tier changes don't HMR — verify via full `tauri dev` relaunch + grim (WebKitGTK),
  not Playwright.
- **L5:** finish Codex adrev on L3+L4; **wire-walk gate** (operator supplies the real flows
  greenfield — the offline cold-boot RF flow is mandatory) before un-drafting / closing ot71.

## Working-tree / worktree state
- `worktrees/bd-tuxlink-ziyu-reachability-recompute-perf` — PR #763, committed+pushed, CI-green,
  `node_modules` installed. No uncommitted source.
- `worktrees/bd-tuxlink-ot71-propagation-update` — PR #765 (draft), committed+pushed (2 commits),
  `node_modules` installed. Untracked gitignored: `dev/adversarial/2026-06-16-ot71-...-codex.md`.
  No uncommitted source.
- This handoff is written to the main checkout `dev/handoffs/` **untracked** — the main checkout
  was lease-locked all session (same as the prior towhee-fen-peregrine handoff). Commit it on
  `recover-handoffs` when the lease frees, or next session does so.

## Not actioned (pre-existing, seen)
- 70 MB file literally named `2` at repo root (untracked) + untracked mock PNGs + bug-hunt md —
  pre-existing, not investigated.
- tuxlink-kiaa (Favorites have no top-level home) — still open, needs the operator mock brainstorm
  per the prior handoff; untouched this session.

## Added late-session — tuxlink-fwse: high-contrast Daylight theme (PR #776)
Recurring operator request (AZ-sun outdoor readability, release gate). After a long
visual-companion brainstorm (I undershot "high contrast" repeatedly — see memory
`feedback_high_contrast_means_dramatic`: overshoot-then-dial-back; theme is palette-ONLY,
don't change shapes/fonts/borders). Operator approved: **colors only**, the key being
**bold stateful fills** — selected message + active folder become a solid `--accent #a83800`
fill with white text (daylight-scoped, other themes untouched), plus a palette punch
(near-black text, darkened secondary text, saturated accent/green). `src/App.css` only;
borders left untouched ("no added lines"). **PR #776, needs operator field-test in the real
WebKitGTK app (grim + sun) — mocks were untrustworthy.** Deferred: `--reach-*` map-tier
daylight overrides (operator: main-UI not map). Worktree `worktrees/bd-tuxlink-fwse-daylight-high-contrast`
(node_modules installed; `.superpowers/brainstorm/` mocks are gitignored).
