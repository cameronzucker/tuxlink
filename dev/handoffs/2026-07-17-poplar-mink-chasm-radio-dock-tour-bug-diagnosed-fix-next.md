# Handoff — Onboarding tour radio-dock stop: DIAGNOSED, fix is the next session's job

- **Agent:** poplar-mink-chasm
- **Date:** 2026-07-17
- **bd:** `tuxlink-fh53x` (P2 bug, open, carries the full diagnosis in its description)
- **Ended:** operator asked to hand off so a fresh session starts the fix in its own bd-bound worktree.

## READ THIS FIRST

The bug is fully diagnosed by code trace (against `origin/main`). Do NOT re-investigate from scratch. `bd show tuxlink-fh53x` has the complete root cause with file:line. This doc adds the starting context and the one open decision.

**Not yet done:** runtime reproduction and the fix. The diagnosis is a deterministic code trace, not a live repro. The fix session should build + verify, not assume.

## The bug in three lines

Onboarding tour radio-dock stop (renders "Step 4 of 5") shows a popover pinned to screen-center referencing nothing when the radio dock is not showing. Root cause is two compounding defects in that single stop, not a systemic overlay-framework bug. The framework's missing-anchor handling is deliberate and correct; this stop just uses it wrong.

## Root cause (verified by reading, `origin/main`)

1. **Anchor on a boxless, conditionally-mounted element.** `data-tour-anchor="radio-dock"` is on `.radio-drawer` (`RadioDrawer.tsx:48`). `.radio-drawer` is `display: contents` on desktop (`RadioDrawer.css:8`), so it has no layout box and `getBoundingClientRect()` is all zeros; `findMountedAnchor` treats a zero-rect element as not-found (`domAnchor.ts:16`). The drawer is also only mounted when `radioPanelMode !== null || aprsOpen` (`AppShell.tsx:2427`). So the anchor is unresolvable when no radio mode is active (element absent) and, on desktop, even when it is (boxless).
2. **Wrong fallback.** `radio-dock` uses `fallback: 'center'` (`tourRegistry.ts:30`); `HintOverlay.tsx:122` then takes the `showCentered` branch and renders the popover centered with no spotlight (`HintOverlay.tsx:268`). The sibling conditionally-present stop `mailbox` correctly uses `fallback: 'skip'` (`tourRegistry.ts:16`, auto-advance at `HintOverlay.tsx:106-119`).

The `requiredPanelState: 'radio-dock-open'` probe (registered `AppShell.tsx:1258`) is only consulted on the `point_at` path (`HintProvider.handlePointAt`), NOT in tour rendering. The tour is driven purely by `findMountedAnchor`.

## Recommended fix (two parts, both needed)

Fixing only the fallback would make the stop silently skip on desktop forever, so do both:

1. **Relocate the anchor to a boxed element** that exists when the dock is showing. `.radio-drawer` and `.radio-drawer-body` are both `display:contents` on desktop by design (the panel flows into the grid), so neither can host it. The anchor needs the innermost real panel-content element, or a new minimal non-`contents` wrapper added for the purpose. This is the judgment part of the fix; confirm what actually gets a box on desktop when a mode is active.
2. **`radio-dock` `fallback: 'center'` -> `'skip'`** (match `mailbox`). Then: dock showing -> real spotlight (from part 1); no mode active -> clean skip instead of a centered empty card.

**Regression test** (`src/onboarding/`): assert the radio-dock stop auto-advances when the drawer is unmounted, and yields a non-null rect / spotlight when a boxed anchor is mounted.

**One open decision (the fix session should settle it, default recommended):** default = skip when the dock is absent (above). Alternative = keep `'center'` but render `entry.openHint` in the centered card (today it shows only `entry.body`, never `openHint`) so a fresh user learns to open the dock. Not recommended: a centered modal about an off-screen surface is the confusing thing that was reported.

## Files

`src/onboarding/tourRegistry.ts`, `src/shell/RadioDrawer.tsx` (+ maybe `RadioDrawer.css`), `src/onboarding/HintOverlay.tsx` (only if the openHint route is chosen), tests under `src/onboarding/`.

## Starting prompt for the next session

```
Fix bd tuxlink-fh53x (onboarding tour radio-dock stop centers a contentless
card). FIRST: read dev/handoffs/2026-07-17-poplar-mink-chasm-radio-dock-tour-bug-diagnosed-fix-next.md
and `bd show tuxlink-fh53x` — the bug is already root-caused by code trace, do
NOT re-investigate. Create a bd-bound worktree off origin/main (mandatory per the
main-checkout hook). Recommended fix is two parts: (1) move the data-tour-anchor
="radio-dock" off the display:contents .radio-drawer onto a boxed element that
exists when the dock is showing; (2) change radio-dock fallback 'center' -> 'skip'
to match the mailbox stop. Add a regression test in src/onboarding/. Verify with a
real build/repro, not by assertion. One product decision to settle: skip-when-absent
(recommended) vs teach-via-openHint centered card.
```
