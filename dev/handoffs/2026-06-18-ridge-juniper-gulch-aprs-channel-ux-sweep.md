# Handoff — APRS Tac Chat UX bug sweep (10/11 shipped; sprites deferred)

Agent: ridge-juniper-gulch · 2026-06-18

## TL;DR
Operator QA'd the APRS Tac Chat surface and reported 11 bugs across the session.
**10 are fixed, tested, and pushed** on branch `bd-tuxlink-hzwc/aprs-channel-ux`
(off `origin/main`, bd issue `tuxlink-hzwc`). The 11th — authentic APRS symbol
icons on map pins — is **deferred to the pre-existing design-gated issue
`tuxlink-90xb`** (it needs a vendored sprite asset + render pipeline + a
public-repo licensing nod + a WebKitGTK smoke; not a quick fix).

PR: **#816** — https://github.com/cameronzucker/tuxlink/pull/816 (CI running).

## IMPORTANT: this checkout was 1977 commits behind
The operator's main checkout (`bd-tuxlink-xygm/recover-handoffs`) is far behind
`origin/main`; all APRS code lives on `origin/main` under `src/aprs/` +
`src-tauri/src/winlink/aprs/`. Work was done in a worktree off `origin/main`. The
operator runs the **converged build** (`pnpm dev:converged`), which builds
`origin/main` — so these fixes appear there once the PR merges.

## Shipped (10) — each its own commit, all `pnpm typecheck` + `vitest` green
1. **#2 Packet decode** — non-message frames (position/WX/MIC-E/status/telemetry)
   were dumped raw into the feed (the `tuxlink-8tz1` diagnostic) and read as
   gibberish. New `src/aprs/aprsDecode.ts` (TDD vs the operator's real captures)
   renders APRSIS-32-style monitor lines with a category tag; raw packet kept on
   the row `title` (show-raw). Backend: added `MsgKind::{Message,Raw}` discriminator
   to `InboundMsg` so the UI never mis-decodes a `>`-leading broadcast.
2. **#3 Compose target** — tap-a-station now sets a removable `→ CALL ✕` chip and
   the field holds only the body (was: callsign written into field AND a duplicate
   `→ CALL` indicator). Typing `CALL:` still auto-lifts into the chip.
3. **#4 Station Data pop-out empty** — the pop-out's env accumulator started empty
   and filled only on new beacons. Added a cross-window snapshot handshake (host =
   main shell answers; client = pop-out requests on mount) so it shows the live
   roster + history immediately.
4. **#5 Tab order** → `[APRS Chat · Station Data · Modem]` (Modem far right).
5. **#6 Ctrl+Shift+M** — no longer disables the Modem tab with the keystroke as the
   only way back. Dock Modem tab is always reachable (Telnet console by default);
   the accelerator is context-aware (dock: flip Modem⇄Chat; standalone: pin toggle,
   existing test preserved).
6. **#7 Map scale** — MapLibre ScaleControl (imperial + metric, bottom-left).
7. **#8 Map "spaghetti"** — root cause was road DENSITY, not color (desaturation was
   tried and **rejected by the operator** — see below). Fixed by gating minor /
   residential / service / link / unclassified road classes to `minzoom 13` in
   `basemapStyle.ts` so mid-zoom shows arterials + highways only. **Floor is
   tunable — needs a converged-build smoke to confirm the level.**
8. **#9 Pop-out button** — was a rounded default button (missing from the dock's
   `border-radius:0` + hover overrides); now matches dock chrome.
9. **#10 Station Data resized other tabs** — the env-panel body was omitted from the
   dock flex-fill rule and the tab row wasn't pinned; both fixed.
10. **#11 Unread counter** — climbed forever while sitting on the open Chat tab.
    Now the watermark advances while the Chat tab is the active view (count = 0
    there) and only accrues while away.

## Deferred (1)
- **#1 Authentic APRS symbol icons on map pins → `tuxlink-90xb`** (gate lifted,
  noted on the issue). The map still draws generic circles + callsign labels +
  the symbol name in the click popup; the operator wants the authentic icon ON
  the pin. `tuxlink-90xb` already scoped this and reached the same conclusion:
  emoji renders as tofu in WebKitGTK, so the correct vehicle is a **sprite sheet
  via `map.addImage` + a symbol layer with `icon-image` keyed off
  `symbolTable/symbolCode` through `lookupAprsSymbol`**. PLAN on the issue:
  vendor `hessu/aprs-symbol-index` (CC0; the set aprs.fi uses — note its sprites
  are build-generated, so run its generator or build from the SVGs), add
  attribution to LICENSE/NOTICE, smoke via grim. Not done this session because a
  binary third-party asset + an unverifiable (headless) render is the wrong thing
  to rush at session tail.

## Smoke-gated (verify on the converged build)
This environment has **no WebKitGTK map render** (jsdom has no WebGL/canvas;
chromium ≠ WebKitGTK). So **#7 (scale), #8 (road density level)** are verified
structurally (unit tests) but their *visual* result needs an operator smoke. The
backend `MsgKind` change (#2) compiles only in **CI** (the Pi can't cold-build
cargo) — watch the PR's `verify` job.

## Operator decisions captured this session
- Packet display = **decode inline in the feed** (APRSIS-32 style), not filter-out.
- Compose = **removable target chip + clean field** (reverses the earlier
  "inline addressing, no To field" call — operator chose the chip on 2026-06-18).
- Map symbols = **authentic icons** (not emoji / category-pins).
- **Desaturating road colors is NOT an acceptable fix for #8** — "the same
  spaghetti mess with less color." The fix must reduce density.

## Working-tree / worktree state
- Worktree `worktrees/bd-tuxlink-hzwc-aprs-channel-ux` on
  `bd-tuxlink-hzwc/aprs-channel-ux`, pushed, clean after the final commit.
  Untracked: `node_modules/` (installed), `target/` not built here.
- Sibling worktree from another session: `worktrees/bd-tuxlink-qjgx-alpha-logging`
  (untouched).
- Main checkout `bd-tuxlink-xygm/recover-handoffs` carries the pre-existing
  uncommitted README rewrite + untracked handoffs — NOT this session's; untouched.
