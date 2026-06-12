# Handoff — GPS setup-assistance wizard chrome (tuxlink-9xy1 Unit D) — kite-taiga-hawk

Date: 2026-06-12 · Agent: kite-taiga-hawk · Branch: `bd-tuxlink-9xy1/gps-setup-assist` · PR #631

## TL;DR

**tuxlink-9xy1 slice 1 is now feature-complete.** The previous session
(marten-falcon-gully) shipped Units A/B/C (probes, GpsSourcePicker, Settings →
Location) and handed off with "wizard chrome remains." That session's process was
terminated mid-work on an operator host reboot — its *code* was safe on PR #631;
only the running session was lost. This session (kite-taiga-hawk) built the
remaining piece: **Unit D, the wizard Location step.** Commit `1a346494`.

GPS setup assistance is now reachable in **both** chromes the operator asked for:
Settings → Location (Unit C) and the first-run wizard (Unit D).

## Unit D — what shipped (commit 1a346494)

A dedicated `location` wizard step that every identity path threads through before
`complete`, so first-run onboarding includes the guided GPS setup the mocks
promised — a real win over WLE, which leaves Linux GPS (dialout group,
ModemManager grabbing the port) as an undocumented wall.

- **`src/wizard/StepLocation.tsx`** (new) — renders the shared `GpsSourcePicker`
  (source detection + dialout/ModemManager triage with copy-paste fix commands +
  manual grid) in wizard chrome. Continue → `ADVANCE_FROM_LOCATION` → complete.
- **Reducer** (`wizardReducer.ts` / `types.ts`) — `SUBMIT_CREDENTIALS_SUCCESS(skip)`
  / `SUBMIT_OFFLINE_SUCCESS` / `SKIP_CMS_VERIFY` now route to `'location'` instead
  of straight to `'complete'`; added `'location'` to `WizardStep` and
  `ADVANCE_FROM_LOCATION → 'complete'`. These three actions were the *only* paths to
  `complete`, so one interception threads Location through CMS-verified,
  CMS-skipped, and offline flows alike.
- **Grid moved OUT** of `Step2Credentials` + `Step2OfflineIdentity` into the
  Location step (no more duplicate grid input across steps). The `wizard_persist_*`
  commands keep their `grid` param (passed empty); the Location step writes the real
  grid via `config_set_grid` — the same path Settings uses.
- **`src/location/useLocationConfig.ts`** (new) — extracted hook so the wizard and
  Settings chromes share one `config_read` seed + `config_set_grid` /
  `position_set_source` persistence path. `LocationSettings` slimmed to use it.

**Zero Rust changes** — reuses `config_set_grid` / `position_set_source` /
`gps_probe_*`. The `WizardPhase` resumability model (preserved on origin
`bd-tuxlink-9xy1/gps-foundation`) was deliberately **not** ported: it's a separate
resumable-onboarding concern, out of slice-1 scope. Tracked as a possible follow-up.

## Gates

- typecheck: clean (`tsc --noEmit`, exit 0).
- vitest (src/wizard + src/location): **154 pass / 14 files** — incl. the extracted
  hook (LocationSettings still green), new StepLocation (4), and the rerouted
  Step2*/Step3 flow assertions.
- CI on PR #631 (commit 1a346494): **all 4 green** — verify amd64 8m29s / arm64
  13m10s, build-linux amd64 11m19s / arm64 11m5s. PR marked ready for review.
- Rust verified via CI only (Pi contention — no local cargo;
  `[[feedback_no_cold_cargo_on_contended_pi]]`). This change has no Rust delta, so CI
  is a formality here.

## Remaining

1. **Mark PR #631 ready** once CI is green (it's a draft). Operator is eager to test.
2. **Operator WebKitGTK smoke** of the new render surface — the wizard Location step
   + Settings → Location. Per `[[feedback_browser_smoke_before_ship]]` this is
   *post-merge opportunistic*, NOT a pre-merge gate (no compute for a 20-min compile
   on the contended Pi). Chromium/vitest miss WebKitGTK CSP/render bugs
   (`[[feedback_chromium_not_webkitgtk_proxy]]`); validate via grim if needed.
3. **s0r1's 3 Find-a-Station fixes (PR #618, merged)** still aren't in a release —
   they landed ~1s after v0.55.0 tagged. `gh workflow run release-please.yml` to cut
   a version containing them (+ this once merged).
4. **Slice 2 = `tuxlink-m9ej`** — the "Fix it for me" triage buttons (pkexec helper);
   they ship disabled here with "Coming in the next release."
5. Dispose stale `worktrees/bd-tuxlink-9xy1-gps-foundation` (989 behind) via the
   ADR-0009 ritual when convenient.

## Worktree

- **Active:** `worktrees/bd-tuxlink-9xy1-gps-setup-assist` (off main, PR #631).
  `node_modules` installed; no `target/` (Rust via CI). Clean after commit + push.
- This handoff rides PR #631 (same PR as the code) because the main checkout is held
  by another live session (`bd-tuxlink-xygm/recover-handoffs`, 16 uncommitted files —
  not touched).
