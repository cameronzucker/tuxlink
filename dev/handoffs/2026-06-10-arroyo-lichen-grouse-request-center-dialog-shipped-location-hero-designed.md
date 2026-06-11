# 2026-06-10 arroyo-lichen-grouse — Request Center: dialog + render fixes SHIPPED; location-hero DESIGNED (build pending)

Long session, four phases. Three PRs merged to main; one feature designed and specced for a fresh build session.

## Shipped to main this session
- **PR #559** — Request Center visual re-skin (Direction-C). Merged.
- **PR #564** — WebKitGTK render + geo fixes: icon `display:block`, visible close control, **6-char `gridToLatLon` decode** (the app stores grids at full 6-char precision; the strict 2/4-only decode had collapsed the whole location hero), app-language buttons. Added `dev/render-harness/` (no-compile WebKitGTK snapshot tool). Merged.
- **PR #576** — full-viewport overlay → **centered content-sized dialog** over a dimmed backdrop; **inbox glyph** (the old `radio` glyph read as Wi-Fi); icon-tile centering (reset UA `<button>` padding/line-height). Merged (tuxlink-4ls0 closed).

Net: the Request Center is now a centered dialog, icons centered, glyph fixed, 6-char grids resolve, location chip correct. All verified in real WebKitGTK at the operator's **1920×1080** (the resolution that mattered — see lesson below).

## DESIGNED but NOT built — the next session's job (tuxlink-96lu)
Replace the coarse State+Marine "For your location" section with the complete set of location products that **resolve AND apply** for the operator's grid:
- **Zone forecast** (primary) — exact NWS public forecast zone (e.g. CN87uo → "Seattle and Vicinity", `WAZ558` → catalog `WA_ZON_SEA`).
- **Regional radar** — tightest `WX_US_RAD` region (CN87uo → `US.RAD.PSND` Puget Sound).
- **Marine forecast** — sea-area text, coastal only.
- Adaptive: inland = zone+radar; coastal = zone+radar+marine. Everything else (state forecast, other in-state zones, buoy/NAVTEX/satellite/fax) → Browse. METAR excluded (catalog has 0 US airports).

**Source of truth:**
- Spec: `docs/superpowers/specs/2026-06-10-request-center-location-hero-design.md`
- Mock: `docs/design/mockups/2026-06-10-request-center-location-hero.html` (render: `dev/scratch/location-hero-mock/02-corrected-1920.png`)
- Branch `bd-tuxlink-loc/location-hero` (off main, pushed, NO PR). Worktree `worktrees/bd-tuxlink-loc-location-hero` (node_modules symlinked).

**Resolution architecture (data-grounded, validated by polling NWS api.weather.gov + the bundled catalog):** grid → point-in-polygon over **bundled NWS public-zone geometry** → NWS zone → Winlink filename via a **VETTED** mapping. The Winlink zone-forecast descriptions ARE the NWS zone names; most match by normalized name, a tail are length-abbreviated (`"F-hills & Valleys of cent King County Cascades"`) and must be **hand-resolved** (a fuzzy auto-match would fail the alpha vettedness bar). A mapping-completeness test enforces it. Plus a curated radar-region table; the existing `latLonToSeaArea` for marine.

**The heavy part is the data, not the UI:** bundling NWS zone geometry + the vetted zone→filename mapping across the **~30 states the catalog covers**. Budget for it.

**NEXT-SESSION FLOW:** read the spec → `writing-plans` → `subagent-driven-development`.

## CRITICAL process lesson (do not repeat)
Twice this session UI shipped that was broken on the operator's real screen, because harness renders were taken at 1366/1600 — **not the operator's 1920×1080**. The dialog/icon defects (#576) and the 6-char collapse only showed at full-screen / with the real grid.
**Rule: render UI in real WebKitGTK at 1920×1080 (and with a 6-char grid) BEFORE claiming any UI work done.** The harness makes this a ~5-second PNG, no compile:
```
pnpm dev    # in the worktree, :1420
WEBKIT_DISABLE_COMPOSITING_MODE=1 LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
  python3 dev/render-harness/snapshot.py "http://localhost:1420/dev/render-harness/harness.html?grid=CN87uo&view=home" /tmp/out.png 1920 1080 2800
```
Also: **merged ≠ released.** The operator runs released builds (0.47.0 contained #564 only because release-please cut it right after the merge). Tell the operator which release a fix lands in; renders are how they "see" it without a 30-min compile.

## Open follow-ups
- **tuxlink-96lu** (location hero) — design locked, build pending. THE next work item.
- **tuxlink-jzaj** (P2) — official grim WebKitGTK smoke on warm main. Largely covered by the harness renders this session, but the on-real-device warm-main pass is still nominally open.

## Worktree / disposal state (deferred — busy Pi, ~120 worktrees, concurrent sessions)
- `bd-tuxlink-hbbw-...` (nested under eymu), `bd-tuxlink-eymu-...`, `bd-tuxlink-lfz4-...`, `bd-tuxlink-rcdlg-...` — all **merged-dead** (PRs #513/#559/#564/#576). Dispose via ADR 0009 ritual at a quieter moment.
- `bd-tuxlink-loc-location-hero` — **active** (location design; the next session builds here or branches a `bd-<id>/...` build branch off it). node_modules symlinked (gitignored). Untracked: `dev/render-harness/*.png` (gitignored).
- node_modules in lfz4/rcdlg/loc worktrees are **symlinks** to the hbbw worktree's node_modules.

## Main checkout
On `bd-tuxlink-xygm/recover-handoffs` (operator state; concurrent sessions active all session — the main-checkout-race hook denied several ops, handled via worktrees). This handoff is written **untracked** into the main checkout's `dev/handoffs/` for the operator's batch-commit (no PR for handoffs).
