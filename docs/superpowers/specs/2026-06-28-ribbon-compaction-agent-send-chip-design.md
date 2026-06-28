# Ribbon compaction — "Agent send" chip + popover (design)

**Issue:** tuxlink-yfezs
**Date:** 2026-06-28
**Status:** approved (operator, 2026-06-28)
**Mockup:** `dev/scratch/ribbon-compaction/ribbon-mock.html` (+ `ribbon-mock-render.png`)

## Problem

`EgressArmControl` (the agent-send arm gate, MCP phase 3.6 — `src/shell/EgressArmControl.tsx`) renders inline in the 56px `DashboardRibbon` (`src/shell/DashboardRibbon.tsx`, the egress item is wired at the `<EgressArmControl>` site after the APRS item). In its **disarmed** state — the common one — it renders a label, a status dot, the word `OFF`, **and a row of three duration-preset buttons** (`15 min` / `1 hour` / `4 hours`, from `EGRESS_DURATION_PRESETS` in `src/security/egressTypes.ts`).

That preset row sits on top of an already-dense ribbon (identity/callsign, grid, position, clock, connection, optional review-inbound, APRS) and immediately before the right-pinned Connect button (`.dash-connect`, `margin-left: auto`). The result: existing items are squished and Connect is pushed against the right edge. The ribbon is a `flex` row with `overflow: hidden`, so the overflow is hidden/clipped rather than wrapped.

## Goal

Reclaim the horizontal space the preset-button row consumes while keeping the agent-send **state** glanceable, so Connect and the other ribbon items have room. Agent send is a safety/security control: the operator must always see whether it is OFF / ON / LOCKED at a glance. Therefore the **state** stays visible in the ribbon; only the arm/disarm **actions** collapse into a click-to-open popover.

Non-goal (explicitly out of scope): an overflow "⋯" menu that collapses grid / position / APRS / review-inbound (the "option B" idea). It is a separate, later change if the row still feels tight after this lands.

## Design

Split the single inline `EgressArmControl` into a **chip** (always rendered in the ribbon) and a **popover** (rendered on demand). The component remains presentational: `useEgressArm` continues to live in `AppShell` (`src/shell/AppShell.tsx`), and `status` / `onArm` / `onDisarm` / `busy` / `error` continue to flow down as props. Only the rendering changes.

### The chip (always visible)

A single button, `data-testid="egress-arm-control"` (the id the current root carries, kept so existing references resolve), showing:

- a status dot — disarmed `idle`, armed `on`, tainted `lock` (mirror the current `dotClass` logic: `tainted` → locked, else `armed` → on, else idle);
- the label `Agent send`;
- the state word: `OFF` (disarmed) / `ON` (armed) / `LOCKED` (tainted), in `data-testid="egress-state"` with the existing `data-armed` / `data-tainted` attributes;
- when **armed**, the live countdown inline (`data-testid="egress-countdown"`, e.g. `12:43 left`) so remaining authority is always visible without opening the popover;
- a caret affordance (`▾` closed, `▴` open).

The chip is a compact pill (`dash-egress-chip`) matching the ribbon's other interactive chips (callsign/IdentitySwitcher, APRS control): `surface-2` background, `border-strong` border, accent border on hover/open.

### The popover (on click)

Anchored under the chip, it holds the **actions** for the current state:

- **Disarmed** → a "Arm send-authority for:" label + the three preset buttons (`data-testid="egress-presets"` group; each `data-testid="egress-arm-{secs}"`, calling `onArm(secs)`), plus a one-line help note ("While armed, an MCP agent may transmit / change settings. Disarms automatically when the timer ends.").
- **Armed** → the countdown (repeated here for context) + a full-width **Disarm now** button (`data-testid="egress-disarm"`, calling `onDisarm()`).
- **Tainted** → the locked message (`data-testid="egress-locked"`, "Session tainted — restart Tuxlink to re-enable agent send."); no arm/disarm affordance.
- **Error** (any state) → the inline alert (`data-testid="egress-error"`, `role="alert"`) when `error` is non-null.

`busy` disables the preset and disarm buttons while an arm/disarm round-trip is in flight (unchanged from today).

### Popover mechanism — reuse, do not invent

Use the **same popover mechanism as `IdentitySwitcher`** (`src/shell/IdentitySwitcher.tsx`): a local `open` state, a ref to the trigger, anchor coordinates computed in a layout effect, **Esc-to-close**, and outside-click-to-close. This keeps the chip's open/close behavior and accessibility consistent with the sibling ribbon chip and avoids adding a new dropdown primitive. The popover panel chrome (border, radius, shadow, z-index) mirrors the existing ribbon dropdown styling.

### Live countdown

Keep the existing scoped `CountdownCell` subtree (seed from the polled `armedRemainingSecs`, tick locally each second, re-seed when the poll changes). It renders in the chip (and may also render in the armed popover). Scoping the tick to its own text node keeps the 1-second update from repainting the ribbon, exactly as today.

## Components and interfaces

- `EgressArmChip` — props `{ status, onArm, onDisarm, busy, error }` (the current `EgressArmControlProps`, unchanged). Owns the open/close state, renders the chip, and renders `EgressArmPopover` when open. Replaces the current `EgressArmControl` export, or `EgressArmControl` is refactored in place to this shape; the import site in `DashboardRibbon.tsx` and the prop wiring in `AppShell.tsx` do not change.
- `EgressArmPopover` — presentational; given `status` / `onArm` / `onDisarm` / `busy` / `error` and an `onRequestClose`, renders the per-state action body. May be a sub-component in the same file (the file stays small).
- `CountdownCell` — unchanged.

No backend, IPC, or `useEgressArm` changes. No change to `egressTypes.ts` or `EGRESS_DURATION_PRESETS`.

## Testing

Component tests (`EgressArmControl.test.tsx` / new `EgressArmChip.test.tsx`):

- Chip shows `OFF` + idle dot when disarmed; `ON` + on dot + countdown when armed; `LOCKED` + lock dot when tainted.
- Clicking the chip opens the popover; `Esc` and outside-click close it.
- Disarmed popover: the three presets render; clicking `egress-arm-{secs}` calls `onArm(secs)`.
- Armed popover: `egress-disarm` calls `onDisarm`; the countdown renders.
- Tainted: no arm/disarm affordance; `egress-locked` message renders.
- `error` renders `egress-error` with `role="alert"`.
- `busy` disables the action buttons.

DashboardRibbon integration test: with the chip in place, the ribbon renders the egress chip and the Connect button without the inline preset row (assert the presets are not in the document until the chip is opened).

All existing `egress-*` testids remain reachable; tests that previously found a preset/disarm directly now open the chip first.

## Self-review

- Placeholder scan: none. Every state, testid, and interaction is specified.
- Internal consistency: state→dot mapping mirrors the current component; data-down flow and testids preserved; popover reuse named to a concrete existing component.
- Scope: single component refactor + its tests + one ribbon integration assertion. Option B (overflow menu) explicitly excluded. No backend/IPC.
- Ambiguity: the armed countdown renders on the chip (decided: always glanceable), and may additionally render in the armed popover for context. The chip root keeps `data-testid="egress-arm-control"`.
