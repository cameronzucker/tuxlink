# Position Subsystem — Restoration After pjih (v3)

- **Date:** 2026-06-01
- **bd issue:** tuxlink-c79g (closes); references tuxlink-pjih, PR #189 (reverts)
- **Status:** v3 — 5-round adrev applied + explicit-referent revision; operator review pending
- **Authors:** bison-condor-grouse + operator
- **Amends:** [`docs/superpowers/specs/2026-05-22-position-subsystem-design.md`](2026-05-22-position-subsystem-design.md)
- **Adrev:** R1 Codex + R2 UX + R3 contract + R4 tests + R5 holistic — 47 findings total (6 P0, 21 P1, 20 P2); all P0 + all P1 applied; P2 selectively per cost/value.

## Vocabulary (referenced throughout — read this once)

The position-subsystem restoration design references several named UI elements, backend symbols, and operator-visible states. Every reference below uses the explicit name; do NOT translate to a shorter pronoun. The vocabulary list is exhaustive for the surface this design covers.

**Operator-visible states** (combinations of `source × fix_state × manual_grid_set`):

| Label | Predicate | Operator sees |
|---|---|---|
| **State 1** | `source = Manual && no fresh fix` | The MANUAL source chip (amber). The grid value `CN87` (or operator's last commit). No status text. |
| **State 2** | `source = Manual && fresh fix exists` | The MANUAL source chip (amber). The grid value `CN87`. Passive status text "GPS ready" beside the MANUAL source chip. |
| **State 3** | `source = Gps && fresh fix exists` | The GPS source chip (green, locked). The grid value `DM33` (live fix, precision-reduced). No status text. |
| **State 4** | `source = Gps && no fresh fix && manual_grid set` | The GPS source chip (dimmed). The grid value `· CN87` (interpunct prefix from `manual_grid`). Status text "GPS no fix · broadcasting fallback". The `Set manually` button. |
| **State 5** | `source = Gps && no fresh fix && manual_grid = None` | The GPS source chip (dimmed). The grid value `—` (em-dash placeholder). Status text "GPS no fix". The `Set manually` button. |
| **State 6** | `source = Manual && manual_grid = None` | The MANUAL source chip (amber). The grid value `—` (em-dash placeholder). |

**Named UI elements** (in the ribbon's Grid cell):

| Name | DOM type | Lives in |
|---|---|---|
| **Source chip when `source = Manual`** | `<button>` | `GridEdit.tsx`, replaces the pre-pjih non-interactive `<span>` |
| **Source chip when `source = Gps`** | `<span role="status">` | `GridEdit.tsx`, non-interactive |
| **Grid value** | `<button>` (inline-edit trigger) | `GridEdit.tsx`, pre-pjih behavior |
| **Grid input** | `<input>` (active during edit) | `GridEdit.tsx`, pre-pjih behavior |
| **GPS-ready status text** | `<span>` (passive, NOT a button) | `GridEdit.tsx`, replaces the pre-pjih `<button data-testid="use-gps">` |
| **`Set manually` button** | `<button>` with `aria-controls` | `GridEdit.tsx`, new in this restoration |

**Named backend symbols** (in the position subsystem):

| Name | File:line range | Role |
|---|---|---|
| `arbiter.set_manual(grid)` | `src-tauri/src/position/arbiter.rs` | Mutates `manual_grid` + `source` |
| `arbiter.use_gps()` | `src-tauri/src/position/arbiter.rs` | Switches `source = Gps` |
| `arbiter.active_grid()` | `src-tauri/src/position/arbiter.rs` | Returns the displayed grid value |
| `arbiter.broadcast_grid()` | `src-tauri/src/position/arbiter.rs` | Returns the on-air grid value (precision-reduced) |
| `config_set_grid(grid)` command | `src-tauri/src/ui_commands.rs` | Tauri command; wraps `arbiter.set_manual` + persists `cfg.identity.grid` + `cfg.privacy.position_source` |
| `position_set_source(source)` command | `src-tauri/src/ui_commands.rs` | Tauri command; wraps `arbiter.use_gps` + persists `cfg.privacy.position_source` |
| `position_status` command | `src-tauri/src/ui_commands.rs` | Tauri command; returns `PositionStatusDto` to the UI |
| `effective_broadcast_locator(cfg, arbiter)` | `src-tauri/src/position/mod.rs` | Top-level on-air locator computation with privacy gate |

**Named source-contract amendments** (this design):

| Name | Scope |
|---|---|
| **The `use_gps()` + `position_set_source('Gps')` relaxation** | Both symbols lose their `has_fresh_fix` gate. See [§1.1](#11--the-use_gps--position_set_source-gps-relaxation). |

---

## TL;DR

The 2026-05-22 position-subsystem design is correct as-written; pjih (PR #189, merged `a6db716`) violated the 2026-05-22 source contract. Operator confirmed 2026-06-01: *"The original spec was fine. We had it working for a while. Each fix was only to address regression."*

The position-subsystem restoration covers:

1. **Revert** the pjih backend + frontend changes (restore the 2026-05-22 source contract).
2. **Close two pre-pjih implementation gaps** that the 2026-05-22 spec required but were never coded:
   - The source chip when `source = Manual` was rendered as a non-interactive `<span>` despite the 2026-05-22 spec defining click semantics.
   - The `Set manually` button was never added despite the 2026-05-22 spec defining State 4 + State 5 affordance.
3. **Apply the `use_gps()` + `position_set_source('Gps')` relaxation** (chosen explicitly over two alternatives — see [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch)): both symbols lose their `has_fresh_fix` gate. Operators without GPS hardware can reach State 4 from State 1 by clicking the source chip when `source = Manual`.

The 2026-05-22 spec remains the authoritative source contract. Every section below either points back to the 2026-05-22 spec or extends the 2026-05-22 spec for the gap-closures.

---

## Motivation

The 2026-05-22 spec defined an explicit operator-owned source contract: `Manual` is sticky against GPS, and returning to GPS is a deliberate operator act via the source chip. The implementation that shipped (tuxlink-686) honored the sticky-Manual backend semantics in `arbiter.set_manual` but **left two 2026-05-22 spec lines unimplemented on the frontend**:

- **2026-05-22 spec §4 ("Source chip"), line 102:** *"Clicking the chip switches source explicitly."* — `GridEdit.tsx` rendered the source chip when `source = Manual` as a non-interactive `<span>`.
- **2026-05-22 spec §"Ribbon states", row 3:** *"Gps + none usable → `CN87 · GPS no fix` (fallback to last grid) + obvious 'set manually'."* — `GridEdit.tsx` never rendered the `Set manually` button; State 4 and State 5 visual states had no operator-actionable element.

Without the source-chip-as-button and without the `Set manually` button, an operator in State 1 (or State 2 with `gpsReady = true` but no actionable click) had no in-UI path back to `source = Gps`. The conditional "● GPS ready — tap to switch" button in pre-pjih `GridEdit.tsx` only rendered when `source === 'Manual' && gpsReady === true`, gated on a gpsd fix that operators without GPS hardware never see.

PR #189 (pjih) attempted to fix the operator-stuck-in-Manual situation by deleting the Manual semantic entirely: `arbiter.set_manual` no longer pinned `source`, the source chip in `GridEdit.tsx` read a derived `effective_source` from arbiter state, and the "GPS ready — tap to switch" button was removed as "structurally unreachable." The pjih decisions violated the 2026-05-22 source contract — and the operator hit the consequence on 2026-06-01:

> "GPS is now fully broken. It doesn't switch to manual entry with 'gps available' it just says GPS green at all times and accepts/displays whatever input. That's major regression."

The position-subsystem restoration reverts pjih and closes the two 2026-05-22 spec-implementation gaps that triggered the original operator complaint that pjih over-fixed.

---

## Scope

In scope:

- Revert PR #189's backend + frontend code (precise list in [§3 backend changes](#3--backend-changes-revert-pjih--extend-the-relaxation-to-the-command-layer) + [§4 frontend changes](#4--frontend-changes-revert-pjih--source-chip-as-button--set-manually-button--optimistic-update)).
- Restore the 2026-05-22 source contract — see [2026-05-22 spec §"The source contract"](2026-05-22-position-subsystem-design.md#the-source-contract).
- Render the source chip as a `<button>` when `source = Manual` (close the 2026-05-22 spec §4 line 102 implementation gap).
- Render the `Set manually` button in State 4 and State 5 (close the 2026-05-22 spec row 3 implementation gap).
- Apply the `use_gps() + position_set_source('Gps')` relaxation.

Out of scope (handled in separate tracks — but see [§8 Track B interactions](#8--track-b-interactions)):

- Settings → GPS+Privacy panel cleanup (tuxlink-jmfm, Track B).
- ARDOP panel widening (tuxlink-8rng, Track B).
- Visual redesign of the source chip beyond the actionable/status distinction defined in [§2.1](#21--source-chip-dom-type-and-click-semantics-by-source-value).
- Any change to the gpsd client or fix-quality gate.

---

## §1 — Source contract (RESTORED from the 2026-05-22 spec + the relaxation amendment)

The position-subsystem restoration restores verbatim from the 2026-05-22 spec ([§"The source contract"](2026-05-22-position-subsystem-design.md#the-source-contract)):

| Rule | Behavior |
|---|---|
| Explicit source | `source ∈ {Manual, Gps}`. There is no implicit/auto source. |
| Manual is sticky | `arbiter.set_manual(grid)` pins `source = Manual`. A GPS fix arriving while `source = Manual` updates `arbiter.last_fix` but does NOT update `arbiter.active_grid`. |
| Operator-only switching | Returning to `source = Gps` is a deliberate operator act via clicking the source chip when `source = Manual` (never automatic). |
| Precision is source-independent | `arbiter.broadcast_grid()` applies `position_precision` to whatever value `arbiter.active_grid()` returns — independent of which source produced the value. |
| Source is always visible | The source chip in `GridEdit.tsx` always shows the current `source` value. A change to `source` is never invisible. |

### §1.1 — The `use_gps()` + `position_set_source('Gps')` relaxation

**The 2026-05-22 spec gap that motivates the relaxation amendment.** The 2026-05-22 spec's `arbiter.use_gps()` requires a usable recent fix:

> *"`use_gps()` → switches source = Gps (requires a usable recent fix; otherwise reports 'no fix' and leaves the prior position visible)."*

The strict 2026-05-22 `arbiter.use_gps()` makes State 4 and State 5 unreachable from State 1 for operators without GPS hardware. The strict 2026-05-22 `arbiter.use_gps()` is the root recoverability gap that motivated pjih.

**The relaxation amendment, full extent.** BOTH symbols below lose their `has_fresh_fix` gate:

- `arbiter.use_gps()` — signature changes from `Result<(), &'static str>` to `()` (infallible). Sets `source = Gps` unconditionally.
- `position_set_source('Gps')` command — removes the `arbiter.has_fresh_fix()` pre-check + the `UiError::Unavailable { reason: "Cannot switch to GPS: no usable GPS fix" }` error path. The `position_set_source('Gps')` command still persists `cfg.privacy.position_source = Gps` before mutating the arbiter, per the existing persist-first ordering in `src-tauri/src/ui_commands.rs`.

**What the relaxation amendment changes:** the State 4 / State 5 state — previously only reachable as an initial-state quirk under the 2026-05-22 spec — becomes a stable operator-driven destination reachable from State 1 by clicking the source chip when `source = Manual`. The source chip when `source = Gps` shows GPS (operator preference). The on-air locator is `arbiter.manual_grid` (the State 4 fallback per the 2026-05-22 spec row 3). See [§2.5 Broadcasting in State 4 and State 5](#25--broadcasting-in-state-4-and-state-5-clarity-not-divergence) for the State 4 broadcast-clarity treatment.

**What the relaxation amendment does NOT change:**
- Sticky-Manual semantics — `arbiter.set_manual` still pins `source = Manual`; a GPS fix arriving never overrides `source = Manual`.
- The privacy gate in `effective_broadcast_locator` — keys on `arbiter.source()` (the stored preference), as before.
- The gpsd client (fix-quality gate, staleness window, reconnect backoff) — unchanged.
- The initial `source` default value (`Gps`) — per the [`project_gps_precision_reduction.md`](../../../...) memory + 2026-05-22 spec.

### §1.2 — Why the `use_gps()` relaxation over "Clear Manual pin" or "confirm-then-switch"

R5 P0 #2 caught that the position-subsystem restoration v1 picked the `use_gps()` relaxation without comparing alternatives. Three mechanisms were on the table:

| Mechanism | Pros | Cons | Decision |
|---|---|---|---|
| **The `use_gps() + position_set_source('Gps')` relaxation** | Smallest delta from the 2026-05-22 spec. The source chip when `source = Manual` remains the single switch surface. No new UI elements. Matches the operator's "original spec but not broken" framing. | Two named backend symbols change, not "one amendment" — see [§1.1](#11--the-use_gps--position_set_source-gps-relaxation) for the full extent. State 4 requires the "broadcasting fallback" status text — see [§2.5](#25--broadcasting-in-state-4-and-state-5-clarity-not-divergence). | **CHOSEN by operator 2026-06-01.** |
| Dedicated "Clear Manual pin" button beside the source chip | No 2026-05-22 spec contract amendment. The `arbiter.use_gps()` semantic preserved as-written. Each UI element does exactly one thing. | New UI surface (a second button beside the source chip) adds visual weight to the ribbon Grid cell. Operator must learn the new button. Two paths from State 1 (clicking the source chip vs clicking the new Clear-Manual-pin button) create UX ambiguity about which path is canonical. | Rejected by operator: introduces UI noise without solving the case where a source switch must land WITHOUT a fresh fix. |
| Two-stage confirm-then-switch on source-chip click | Prevents accidental source switches. | Operator has previously flagged confirmation modals as ceremony (`feedback_radio1_governs_tx_not_ui` memory). Adds friction without addressing the root recoverability gap. | Rejected by operator: friction without solving the gap. |

---

## §2 — UI surface (source chip clickable + `Set manually` button + State 1 vs State 4 differentiation)

### §2.1 — Source chip DOM type and click semantics by `source` value

Strong convergence across three adrev rounds (Codex R1 #2, R2 #1, R5 #3) that an enabled `<button>` rendering of the source chip when `source = Gps` that intentionally does nothing on click is a UX failure. The restoration design:

| `source` value | DOM type for the source chip | Click handler |
|---|---|---|
| `Manual` | `<button>` with `aria-label="Switch position source to GPS"` | Calls `position_set_source({ source: 'Gps' })` (now infallible per [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation)). The source chip when `source = Manual` is the single explicit switch surface. |
| `Gps` | `<span role="status">` (NOT `<button>`, NOT focusable, NOT `aria-disabled` either) | None. Status-only. Switching INTO `source = Manual` is via the grid value's inline-edit (per the 2026-05-22 spec). |

Rationale for the source chip when `source = Gps` rendering as `<span role="status">`: the source chip's PURPOSE in the ribbon is operator action. When the operator is already in `source = Gps`, the source chip has no operator action available (entering `source = Manual` is via the grid value inline-edit). A `<button>` that doesn't react fails the principle of least astonishment.

The actionable/status distinction also makes the source chip visually different between `source = Manual` (raised, button-shaped) and `source = Gps` (flat, span-shaped) — reinforces operator-available action without text.

### §2.2 — Source chip and GPS-ready status redundancy in State 2

In pre-pjih `GridEdit.tsx`, when `source === 'Manual' && gpsReady === true` (State 2), BOTH the source chip when `source = Manual` AND the "● GPS ready — tap to switch" `<button>` were clickable. The restoration design changes the "GPS ready" element from a `<button>` into passive `<span>` status text:

```
[ MANUAL ] · GPS ready                ← passive <span>, NOT <button>
```

The source chip when `source = Manual` remains the single click surface in State 2. The GPS-ready status text provides contextual information ("a fresh fix is currently available") without duplicating the click action.

### §2.3 — `Set manually` button (State 4 and State 5)

For State 4 and State 5 (`source = Gps && no fresh fix`, now reachable per [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation)), the restoration design renders a `<button>` labeled `Set manually`:

```
[ Grid cell ]  · CN87   [ GPS dimmed ]   GPS no fix · broadcasting fallback   [ ▸ Set manually ]
                ^                                                                  ^
                grid value (fallback from manual_grid)                             Set manually button (focuses grid input on click)
```

Spec for the `Set manually` button:

- DOM: `<button aria-controls={gridInputId}>▸ Set manually</button>` — `aria-controls` programmatically associates the `Set manually` button with the grid input that the `Set manually` button activates.
- Visual cue: small right-arrow `▸` icon to convey the focus-jump action.
- Tab order: source chip → `Set manually` button → grid value.
- Enter/Space invokes the same inline-edit path as clicking the grid value (calls the same `enterEdit()` handler in `GridEdit.tsx`).
- The newly-mounted grid input receives focus on mount (R2 #3 + Codex P2 #6 explicit).
- The vitest assertion for the `Set manually` button asserts `document.activeElement === gridInput` after clicking the `Set manually` button (not just "enters edit mode").

### §2.4 — State 1 vs State 4 visual differentiation (R2 #4 — the failure mode pjih shipped)

R2 #4 identifies the UX failure mode that pjih literally shipped: at first glance, State 1 (`source = Manual` + no GPS) and State 4 (`source = Gps + no fresh fix + manual_grid set`) can look identical — same grid value, same chip shape, same surrounding ribbon chrome. The operator cannot tell at a glance whether the operator is broadcasting the manual_grid as a Manual pin or as a Gps fallback.

The State 1 vs State 4 differentiation matrix:

| State label | Source chip color + shape | Grid value prefix | Status text beside the source chip |
|---|---|---|---|
| State 1 (`source = Manual` + no GPS) | Amber `MANUAL`, button-shaped, saturated | (none) | (none) |
| State 2 (`source = Manual` + fresh fix) | Amber `MANUAL`, button-shaped, saturated, with a small green dot | (none) | "GPS ready" (passive `<span>`) |
| State 3 (`source = Gps` + fresh fix) | Green `GPS`, span-shaped, locked | (none) | (none) |
| State 4 (`source = Gps` + no fresh fix + manual_grid set) | Green `GPS`, span-shaped, DIMMED | `· ` interpunct prefix on the grid value | "GPS no fix · broadcasting fallback" (passive `<span>`) |
| State 5 (`source = Gps` + no fresh fix + manual_grid = None) | Green `GPS`, span-shaped, DIMMED | (none — grid value is `—` em-dash) | "GPS no fix" (passive `<span>`) |
| State 6 (`source = Manual` + manual_grid = None) | Amber `MANUAL`, button-shaped, saturated | (none — grid value is `—` em-dash) | (none) |

The interpunct prefix (`· `), the dimmed chip styling, and the status text together make State 4 visually distinct from State 1. The 2026-05-22 spec row 3 wording (`CN87 · GPS no fix`) defines the interpunct as part of the canonical State 4 + State 5 grid-value display.

### §2.5 — Broadcasting in State 4 and State 5 (clarity, not divergence)

R3 F2 P0 flagged a perceived display-vs-on-air divergence in State 4: the source chip says GPS, while the on-air locator is `manual_grid` via the `effective_broadcast_locator` else-branch. After re-tracing the 2026-05-22 spec, the State 4 behavior is consistent with operator intent (operator preference = `Gps`; `manual_grid` is the spec-defined fallback for both display and on-air per the 2026-05-22 spec row 3). The State 4 behavior needs explicit clarity in the UI and the spec, NOT a semantic change.

**Spec text (added).** When `source = Gps && no fresh fix && manual_grid is set` (State 4):

- The grid value displayed in the ribbon = `manual_grid` (precision-reduced).
- The on-air locator (`effective_broadcast_locator(cfg, &arbiter)`) = `manual_grid` (precision-reduced) — via the `effective_broadcast_locator` else-branch — same value.
- The displayed grid value and the on-air locator are NOT divergent — both are the same `manual_grid` value. The source chip when `source = Gps` reflects OPERATOR PREFERENCE (`Gps`); the grid value reflects WHAT IS BROADCAST (`manual_grid`, the State 4 fallback per the 2026-05-22 spec row 3).

**UI text (added).** The State 4 status text reads `"GPS no fix · broadcasting fallback"` rather than `"GPS no fix"`. The "broadcasting fallback" suffix explicitly tells the operator that the displayed grid value IS the on-air locator (not zero / not nothing / not the pre-fix value).

### §2.6 — ASCII state mockups (the six states)

State 1 — `source = Manual` && no fresh fix:
```
┌───────────────────────────────────────────────┐
│ Grid │  CN87   ┌──────┐                       │
│      │         │MANUAL│ ← <button>, amber     │
│      │         └──────┘   (click → position_set_source('Gps')) │
└───────────────────────────────────────────────┘
```

State 2 — `source = Manual` && fresh fix exists:
```
┌──────────────────────────────────────────────────────┐
│ Grid │  CN87   ┌──────┐  ● GPS ready                  │
│      │         │MANUAL│   (passive <span>, NOT button) │
│      │         └──────┘                               │
└──────────────────────────────────────────────────────┘
```

State 3 — `source = Gps` && fresh fix:
```
┌─────────────────────────────────────────────┐
│ Grid │  DM33   ┌──────┐                     │
│      │         │ GPS  │ ← <span role=status>│
│      │         └──────┘   green, locked     │
└─────────────────────────────────────────────┘
```

State 4 — `source = Gps` && no fresh fix && manual_grid set:
```
┌──────────────────────────────────────────────────────────────────┐
│ Grid │  · CN87  ┌──────┐  GPS no fix · broadcasting fallback     │
│      │  ^       │ GPS  │  [ ▸ Set manually ]                      │
│      │  interpunct (chip dimmed)                                  │
└──────────────────────────────────────────────────────────────────┘
```

State 5 — `source = Gps` && no fresh fix && manual_grid = None:
```
┌──────────────────────────────────────────────────────┐
│ Grid │  —      ┌──────┐  GPS no fix                  │
│      │  em-dash│ GPS  │  [ ▸ Set manually ]           │
│      │         └──────┘   (chip dimmed)               │
└──────────────────────────────────────────────────────┘
```

State 6 — `source = Manual` && manual_grid = None:
```
┌─────────────────────────────────────────────┐
│ Grid │  —      ┌──────┐                     │
│      │  em-dash│MANUAL│ ← <button>, amber   │
│      │         └──────┘                     │
└─────────────────────────────────────────────┘
```

---

## §3 — Backend changes (revert pjih + extend the relaxation to the command layer)

### §3.1 — Backend revert + relaxation table

| Backend symbol | Pre-pjih state | pjih state (current `main`) | Restoration target |
|---|---|---|---|
| `arbiter.active_grid()` | `Manual → manual_grid`; `Gps + fresh fix → fix.grid`; `Gps + no fix → manual_grid fallback` | GPS-fresh always wins regardless of `source` | Restore the pre-pjih source-gated behavior. |
| `arbiter.set_manual(grid)` | Pins `source = Manual` AND updates `manual_grid` | Updates only `manual_grid`; does not touch `source` | Restore the source-pinning. |
| `arbiter.effective_source()` | Did not exist | Returns `Gps` when fresh fix exists, else `Manual` | Remove entirely. |
| `arbiter.use_gps()` | Required `has_fresh_fix`; signature `Result<(), &'static str>` | Same as pre-pjih | Relax: signature `()` (infallible); always sets `source = Gps`. See [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation). |
| `position_set_source('Gps')` command | Pre-checked `has_fresh_fix` and returned `UiError::Unavailable` on miss | Same as pre-pjih (pjih did not touch the `position_set_source` command) | Relax: remove the `has_fresh_fix` pre-check + the `UiError::Unavailable` error path. See [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation). |
| `config_set_grid(grid)` command | Persisted `cfg.privacy.position_source = Manual` AND called `arbiter.set_manual` | Updates `cfg.identity.grid` only; does not persist `cfg.privacy.position_source`; calls `arbiter.set_manual` (which under pjih does not touch `source`) | Restore the `cfg.privacy.position_source = Manual` persistence + restore the source-pinning side effect via the restored `arbiter.set_manual`. |
| `PositionStatusDto` struct | `{ gps_ready, broadcast_grid }` | `{ gps_ready, broadcast_grid, active_source }` | Remove the `active_source` field. |
| `position_status` command | Populates `gps_ready` from `arbiter.has_fresh_fix()` + `broadcast_grid` from `effective_broadcast_locator` | Same + populates `active_source` from `arbiter.effective_source()` | Drop the `active_source` population. |

### §3.2 — Backend keep-as-is (no change from current `main`)

- `effective_broadcast_locator(cfg, arbiter)` in `src-tauri/src/position/mod.rs` — already keys the privacy gate on `arbiter.source()` (the stored preference). The current `main` behavior is correct post-restore. No change.
- All gpsd-client code in `src-tauri/src/position/gpsd.rs` — unaffected.
- The precision-reduction helper `broadcast_grid()` in `src-tauri/src/config.rs` — unaffected.

### §3.3 — Concurrency invariants for `config_set_grid` and `position_set_source` (R3 F1 + R3 F7)

R3 caught that the `config_set_grid` command and the `position_set_source` command are each non-atomic three-step sequences (read config → write config file atomically → mutate the arbiter → push the new config to the live backend). Without an explicit concurrency invariant, rapid-fire operator actions (chip-click + grid inline-edit + chip-click + grid inline-edit) can leave the on-disk config, the arbiter, and the live-backend snapshot disagreeing.

**Invariant added in this restoration spec:** the `config_set_grid` command and the `position_set_source` command MUST each hold the arbiter's `inner` mutex from "read config" through "mutate the arbiter" — i.e., for the entire critical section. The arbiter `inner` mutex is dropped only after the in-memory arbiter has been updated. The "push the new config to the live backend" step occurs after the arbiter `inner` mutex is released (the live backend's own config snapshot is eventually-consistent with the arbiter — pre-existing behavior).

**Implementation note:** the arbiter's `Mutex<Inner>` is the natural serialization point. The `config_set_grid` command and the `position_set_source` command each clone the `Arc<PositionArbiter>` and call methods that lock-and-update atomically. The `config::read_config()` and `config::write_config_atomic()` calls happen INSIDE the locked region (not outside) to close the TOCTOU between "read" and "mutate."

**Test (new):** `concurrent_config_set_grid_and_position_set_source_serialize` — spawns 100 concurrent tokio tasks alternating the `config_set_grid` command and the `position_set_source` command from different sources. Asserts that the final arbiter state is consistent with the LAST committed write per Mutex ordering; asserts no panic and no poisoned Mutex.

### §3.4 — State-space invariants (R3 F4 — all 36 cells of `source × fix_state × gps_state × manual_grid_set`)

R3 walked all 36 cells of the state space `source × fix_state × gps_state × manual_grid_set`. 6 cells were undefined under the position-subsystem restoration v1. The position-subsystem restoration v3 invariants:

| Invariant | Behavior |
|---|---|
| **I1** | When `source = Manual && manual_grid = None` (State 6): `arbiter.active_grid()` returns `None`; the grid value displayed in `GridEdit.tsx` is `—` (em-dash placeholder); the source chip when `source = Manual` remains a `<button>` (still actionable to switch to `source = Gps`); the on-air locator from `effective_broadcast_locator` falls back to `config.identity.grid` (may be `None` → empty on-air string). |
| **I2** | When `source = Manual` (any `fix_state`, any `gps_state`, `manual_grid` set — i.e., State 1 or State 2): `arbiter.active_grid()` returns `manual_grid`. The sticky-Manual property holds across ALL `gps_state` values — privacy semantics do not change source semantics. |
| **I3** | When `source = Gps && fresh fix` (State 3): `arbiter.active_grid()` returns `fix.grid`; the on-air locator from `effective_broadcast_locator` respects `gps_state` per the existing privacy gate. |
| **I4** | When `source = Gps && no fresh fix && manual_grid set` (State 4): `arbiter.active_grid()` returns `manual_grid` (fallback per the 2026-05-22 spec row 3). The grid value displayed in State 4 = `manual_grid`; the on-air locator from `effective_broadcast_locator` = `manual_grid` (precision-reduced) per the 2026-05-22 spec row 3 — see [§2.5](#25--broadcasting-in-state-4-and-state-5-clarity-not-divergence). |
| **I5** | When `source = Gps && no fresh fix && manual_grid = None` (State 5): `arbiter.active_grid()` returns `None`; the grid value displayed in State 5 = `—` (em-dash); the on-air locator from `effective_broadcast_locator` falls back to `config.identity.grid` (which under the I6 invariant is synchronized with `manual_grid`; if `manual_grid` is `None`, `config.identity.grid` is also `None`; the on-air string is empty — this is a config-file integrity assumption, not a runtime failure). |
| **I6** | `arbiter.manual_grid` and `config.identity.grid` are synchronized by the `config_set_grid` command (which writes both atomically). The `arbiter.new()` constructor reads the initial `manual_grid` from `config.identity.grid`. No code path mutates one without the other except gpsd fix arrival (which writes only `arbiter.last_fix`, never `arbiter.manual_grid`). |

Tests for the I1–I6 invariants: see [§6.1](#61--backend-tests-cargo---lib) for the state-space matrix test.

---

## §4 — Frontend changes (revert pjih + source chip as button + `Set manually` button + optimistic update)

### §4.1 — Frontend revert table

| Frontend surface | pjih state (current `main`) | Restoration target |
|---|---|---|
| `DashboardRibbon.tsx` GridEdit invocation | `<GridEdit ... onCommit={...} />` — no `onUseGps` prop passed | Restore `onUseGps={() => invoke('position_set_source', { source: 'Gps' })}` on the GridEdit invocation. |
| `GridEdit.tsx` `GridEditProps` interface | No `onUseGps` member | Restore `onUseGps: () => void` member in `GridEditProps`. |
| `GridEdit.tsx` `<button data-testid="use-gps">` element | Removed entirely | NOT restored as a `<button>` — replaced with a passive `<span>` showing "GPS ready" status text in State 2 (see [§2.2](#22--source-chip-and-gps-ready-status-redundancy-in-state-2)). |
| `useStatus.ts` `PositionStatusDto` interface | `{ gps_ready, broadcast_grid, active_source }` | Remove the `active_source` member. |
| `useStatus.ts` `useStatusData` hook | `position_source: positionStatus?.active_source ?? config?.position_source ?? 'Gps'` | Restore `position_source: config?.position_source ?? 'Gps'`. |

### §4.2 — Source chip as button + GPS-ready as passive span + `Set manually` button

Per [§2.1](#21--source-chip-dom-type-and-click-semantics-by-source-value):
- Source chip when `source === 'Manual'`: `<button onClick={onUseGps}>` with `aria-label="Switch position source to GPS"` in `GridEdit.tsx`.
- Source chip when `source === 'Gps'`: `<span role="status" aria-label={...}>` — non-interactive, non-focusable.
- The pre-pjih `<button data-testid="use-gps">` element in `GridEdit.tsx` is REPLACED by `<span className="dash-gps-ready-status">GPS ready</span>` — passive text only, rendered when `source === 'Manual' && gpsReady === true` (State 2).

Per [§2.3](#23--set-manually-button-state-4-and-state-5):
- The `Set manually` button: `<button aria-controls={gridInputId} onClick={enterEdit}>▸ Set manually</button>` rendered when `source === 'Gps' && !gpsReady` (State 4 or State 5). Tab order: source chip → `Set manually` button → grid value. On clicking the `Set manually` button, `GridEdit.tsx`'s `enterEdit()` handler runs and the newly-mounted grid input receives focus.

### §4.3 — Optimistic update after `config_set_grid` and `position_set_source` (Codex P1 #4)

Dropping `active_source` from `PositionStatusDto` means the source chip's `source` value comes from `useStatusData`'s `config_read` poll (5-second interval). Without an optimistic update, the source chip lags 0–5 seconds behind a successful operator action that changes `source`.

**Optimistic-update spec for `GridEdit.tsx` + `DashboardRibbon.tsx`:**

- After the `invoke('config_set_grid', ...)` call resolves successfully: call `queryClient.invalidateQueries(['config'])` to force an immediate `config_read` refresh. The source chip's `source` value reflects `Manual` within one React render cycle.
- After the `invoke('position_set_source', { source: 'Gps' })` call resolves successfully: call the same `queryClient.invalidateQueries(['config'])` to force an immediate `config_read` refresh. The source chip's `source` value reflects `Gps` within one React render cycle.

**Alternative considered and rejected:** local optimistic source-chip state via `useState` + sync from `config_read` poll. The local-`useState` alternative is rejected because two sources of truth for the source chip's `source` value (local `useState` + on-disk config) risk divergence on error paths (e.g., the backend write fails; the local `useState` value stays `Manual`; the on-disk config still says `Gps`; the source chip renders the wrong `source` value).

**Implementation note:** the `useStatus.ts` `useStatusData` hook is the natural place to expose an `invalidate()` callback that `GridEdit.tsx` and `DashboardRibbon.tsx` each call after a successful write.

### §4.4 — A11y treatment (R2 #6 + R2 #3)

- Replace pre-pjih "tap to switch" mouse-centric language with "Switch to GPS" in the source chip's `aria-label` AND in the GPS-ready passive status text.
- The source chip when `source = Manual` (`<button>`) gets `aria-pressed={false}` (the source chip when `source = Gps` is a `<span>`, not a button, so `aria-pressed` does not apply).
- The `Set manually` button gets `aria-controls={gridInputId}` — the grid input's DOM `id` attribute.

---

## §5 — Migration (REWRITTEN per Codex P1 #5 + R3 F8)

The position-subsystem restoration v1's §5 migration table was literally backwards. The position-subsystem restoration v3 correction:

**Actual on-disk `cfg.privacy.position_source` after the pjih merge — by operator path:**

| Operator path | `cfg.privacy.position_source` on disk |
|---|---|
| First-install AFTER pjih merge (pjih-only operator) | `Gps` (the pjih `config_set_grid` command never persists `Manual`). |
| First-install BEFORE pjih merge, used pre-pjih `config_set_grid` command | `Manual` (the pre-pjih `config_set_grid` command persisted `Manual`). |
| First-install BEFORE pjih merge, never used the `config_set_grid` command | `Gps` (default). |
| First-install BEFORE pjih merge, used pre-pjih `config_set_grid` command, then merged pjih, then used the pjih `config_set_grid` command | `Manual` from the pre-pjih write; the pjih `config_set_grid` command does NOT overwrite to `Gps`, just leaves the existing `Manual` value. |
| First-install BEFORE pjih merge, used `position_set_source('Gps')` command after a fresh fix | `Gps` (the `position_set_source('Gps')` command persisted `Gps` on successful switch). |

**Post-restore operator experience — by on-disk source value:**

| On-disk `cfg.privacy.position_source` | Operator experience post-restore |
|---|---|
| `Gps` (most operators per the above table) | Default state: State 3 if fresh fix; State 4 or State 5 if no fresh fix. Editing the grid value pins `source = Manual` (sticky). Clicking the source chip when `source = Manual` escapes back to `source = Gps`. |
| `Manual` (pre-pjih operators who used the pre-pjih `config_set_grid` command) | Sticky `source = Manual` from disk (State 1 if no fresh fix; State 2 if a fresh fix arrives). The source chip when `source = Manual` is actionable; clicking the source chip when `source = Manual` calls the `position_set_source('Gps')` command (which now succeeds unconditionally per [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation)); `source` flips to `Gps`; State 3 or State 4 renders. No data loss. |

**No one-time migration code is required.** Per Codex P1 #5: pjih cannot distinguish deliberate pre-pjih `Manual` intent from pjih-window confusion, and resetting would violate the restored source/privacy contract. The pjih-era operators with `Gps` on disk simply use the now-actionable source chip when `source = Manual`; the pre-pjih operators with `Manual` on disk get the same source chip when `source = Manual` + the now-actionable click semantics from [§2.1](#21--source-chip-dom-type-and-click-semantics-by-source-value).

**The reported regression that motivated the position-subsystem restoration:** the operator's complaint *"GPS is now fully broken... accepts/displays whatever input"* matches the pjih-window-write code path. The pjih `config_set_grid` command updates `cfg.identity.grid` without persisting `cfg.privacy.position_source = Manual`, and the pjih `arbiter.set_manual` does not pin `source`. The operator sees: editing the grid value → the displayed grid value updates (because the pjih `arbiter.active_grid` returns the typed value as the only available data) + the source chip stuck on `Gps` (from the pjih `arbiter.effective_source` derived from arbiter state where `arbiter.last_fix` may or may not exist). Post-restore: editing the grid value → the displayed grid value updates + the source chip flips to `Manual` (sticky) + the on-air locator uses `arbiter.manual_grid`.

---

## §6 — Test plan

### §6.1 — Backend tests (cargo `--lib`)

**Restore from pre-pjih, with R4 P0 #1 strengthening:**

- `set_manual_pins_source_and_is_sticky_against_gps` — RESTORE but EXTEND for R4 P0 #1: the restored test exercises the temporal sequence `arbiter.set_manual("EM75") → arbiter.apply_gps_fix(Fix::test("DM33ab")) → assert arbiter.source() == Manual && arbiter.active_grid() == "EM75" && arbiter.last_fix is recorded`. The pre-pjih test asserted only the post-`set_manual` snapshot; the restored test pins the GPS-arrival regression class.

- `gps_fix_updates_active_only_when_source_is_gps` — RESTORE as-is.

- `arbiter_set_manual_pins_manual_source` (in `ui_commands::tests`) — RESTORE as-is.

**Add per R4 P0 #2 + Codex P1 #3 + Codex P0 #1:**

- `use_gps_succeeds_without_fresh_fix_and_yields_manual_fallback` (replaces the renamed-but-removed `use_gps_requires_a_usable_fix`): asserts that `arbiter.use_gps()` returns `()` (infallible); asserts `arbiter.source()` flips to `Gps`; asserts `arbiter.active_grid()` equals `arbiter.manual_grid` (the State 4 fallback per the 2026-05-22 spec row 3). Requires `arbiter.manual_grid` to be set in the arbiter setup.

- `manual_source_ignores_fresh_gps_fix_at_broadcast_boundary` (Codex P1 #3): sets `arbiter = (source: Manual, manual_grid: "EM75")` + `cfg.privacy.gps_state = BroadcastAtPrecision`; calls `arbiter.apply_gps_fix(Fix::test("DM33ab"))`; asserts `effective_broadcast_locator(cfg, &arbiter) == "EM75"` (precision-reduced). Pins the "Manual broadcasts manual_grid regardless of fresh GPS fix" invariant at the broadcast boundary.

- `config_set_grid_pins_manual_source_in_config_and_arbiter` (Codex P1 #3): drives the `config_set_grid` command end-to-end with input `"EM75"`; asserts both `arbiter.source() == Manual` AND `read_config().privacy.position_source == Manual`. Pins the cross-layer persistence invariant.

- `position_set_source_gps_succeeds_without_fresh_fix` (Codex P0 #1): asserts the `position_set_source` command mirrors the `arbiter.use_gps()` relaxation — returns `Ok(())` even when `arbiter.last_fix` is `None`; persists `cfg.privacy.position_source = Gps`; `arbiter.source()` flips to `Gps`.

- `concurrent_config_set_grid_and_position_set_source_serialize` (per [§3.3](#33--concurrency-invariants-for-config_set_grid-and-position_set_source-r3-f1--r3-f7)): spawns 100 concurrent tokio tasks alternating the `config_set_grid` command and the `position_set_source` command; asserts no panic, no poisoned Mutex, final on-disk config + final arbiter state agree.

- **State-space matrix tests** (R3 F4): one test per non-trivial cell of the I1–I6 invariants from [§3.4](#34--state-space-invariants-r3-f4--all-36-cells-of-source--fix_state--gps_state--manual_grid_set). The matrix tests combine via `proptest` over `(source, fix_state, gps_state, manual_grid_set)`; each generated case asserts `arbiter.active_grid()` + `effective_broadcast_locator` output match the I1–I6 invariant for that case.

**Remove (the tests no longer apply post-restore):**

- The five pjih-era arbiter tests: `set_manual_updates_grid_without_changing_stored_source`, `fresh_gps_fix_wins_over_manual_grid_regardless_of_stored_source`, `manual_grid_used_when_gps_fix_is_stale_or_absent`, `arbiter_set_manual_updates_grid_without_changing_stored_source` (in `ui_commands::tests`), and the `position_status_dto_*` `active_source` assertions in `ui_commands::tests`.

### §6.2 — Frontend tests (vitest)

**Restore from pre-pjih, with [§2.2](#22--source-chip-and-gps-ready-status-redundancy-in-state-2) strengthening:**

- `shows GPS-ready hint when a fix is available while Manual` in `GridEdit.test.tsx` — RESTORE but assert the GPS-ready hint is a `<span>` (passive text), NOT a `<button>`. Per [§2.2](#22--source-chip-and-gps-ready-status-redundancy-in-state-2).

**Add per Codex P1 #3 + Codex P1 #4 + R4 P1 #3 + R4 P1 #4 + R4 P1 #5 + R5 #7:**

- `source_chip_is_a_button_when_source_is_Manual_and_calls_onUseGps_on_click` (in `GridEdit.test.tsx`, per R4 P1 #3): fires a click on the source chip element when `source = 'Manual'`; asserts `onUseGps` mock was called.

- `source_chip_is_a_span_when_source_is_Gps_with_no_click_handler` (in `GridEdit.test.tsx`, per R4 P1 #3): asserts `getByTestId('source-chip').tagName === 'SPAN'` when `source = 'Gps'`; asserts the `onUseGps` mock was NOT called on a click attempt (defensive).

- `set_manually_button_is_present_in_State_4` (in `GridEdit.test.tsx`, per R4 P1 #5): asserts the `Set manually` button is rendered when `source = 'Gps' && gpsReady = false`.

- `set_manually_button_is_absent_in_State_1` (in `GridEdit.test.tsx`, per R4 P1 #5): asserts the `Set manually` button is NOT rendered when `source = 'Manual' && gpsReady = false`.

- `set_manually_button_is_absent_in_State_3` (in `GridEdit.test.tsx`, per R4 P1 #5): asserts the `Set manually` button is NOT rendered when `source = 'Gps' && gpsReady = true`.

- `set_manually_button_is_absent_in_State_2` (in `GridEdit.test.tsx`, per R4 P1 #5 — closes the 4-quadrant matrix): asserts the `Set manually` button is NOT rendered when `source = 'Manual' && gpsReady = true`.

- `set_manually_button_focuses_the_grid_input_on_click` (in `GridEdit.test.tsx`, per Codex P2 #6): asserts `document.activeElement === gridInput` after clicking the `Set manually` button.

- `ribbon_source_chip_stays_Manual_when_config_says_Manual_even_if_gpsReady_is_true` (in `useStatus.test.ts` hook test, per Codex P1 #3): mocks `config_read.position_source = 'Manual'` and `position_status.gps_ready = true`; asserts `result.current.position_source === 'Manual'`. Pins the source-chip-source-comes-from-config-not-from-arbiter invariant.

- `source_chip_flips_to_Manual_within_one_render_cycle_after_config_set_grid_resolves` (in `DashboardRibbon.test.tsx`, per Codex P1 #4): mocks `invoke('config_set_grid')` to resolve; asserts the source chip's text changes from `GPS` to `MANUAL` within one render cycle, NOT after the 5-second `config_read` poll. Verifies the optimistic-refresh path from [§4.3](#43--optimistic-update-after-config_set_grid-and-position_set_source-codex-p1-4).

- `source_chip_flips_to_Gps_within_one_render_cycle_after_position_set_source_resolves` (in `DashboardRibbon.test.tsx`, per Codex P1 #4): mocks `invoke('position_set_source')` to resolve; asserts the source chip's text changes from `MANUAL` to `GPS` within one render cycle.

- `State_4_grid_value_has_interpunct_prefix_and_chip_is_dimmed` (in `GridEdit.test.tsx`, per R2 #4): mocks `source = 'Gps' && gpsReady = false && grid = 'CN87'`; asserts presence of the `· ` interpunct prefix on the grid value AND the `dimmed` modifier class on the source chip. Pins the State 4 vs State 1 differentiation from [§2.4](#24--state-1-vs-state-4-visual-differentiation-r2-4--the-failure-mode-pjih-shipped).

**Remove (the test no longer applies):**

- `no Use-GPS affordance is rendered (tuxlink-pjih)` in `GridEdit.test.tsx` — pjih-era absence-assertion.

### §6.3 — Cross-layer integration test (R4 P1 #4 + R5 #7 — the test class pjih violated)

R5 #7 caught the root cause of pjih's undetected merge: per-layer tests on backend (`arbiter`) and frontend (`GridEdit.tsx`) passed independently, but no test exercised the composed flow that justifies the entire restoration. The position-subsystem restoration adds:

**`integration_clicking_source_chip_in_State_1_with_no_fix_lands_in_State_4`** (Playwright OR `@testing-library` `renderHook` with mocked Tauri commands):

1. Mount the full `GridEdit` + `useStatus` hook with mocked Tauri commands.
2. Set the initial mocked state: `config.position_source = 'Manual'` + `manual_grid = 'EM75'` + `position_status.gps_ready = false`.
3. Assert the rendered state is State 1 (source chip says `MANUAL` + grid value `EM75`).
4. Click the source chip when `source = 'Manual'`.
5. Assert `invoke('position_set_source', { source: 'Gps' })` was called.
6. Update the mocked `config_read.position_source` to `'Gps'`.
7. Assert the rendered state is State 4 (source chip says `GPS` dimmed + grid value `· EM75` interpunct + status text `"GPS no fix · broadcasting fallback"` + the `Set manually` button is present).
8. Click the `Set manually` button.
9. Assert the grid input mounts AND the grid input receives focus (`document.activeElement === gridInput`).

The cross-layer integration test would have caught the pjih regression at merge time.

### §6.4 — Operator smoke (REWRITTEN per R2 #5 — no-GPS-hardware case)

R2 #5 caught that the position-subsystem restoration v1's §6 smoke did not exercise the no-GPS-hardware case that [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation) exists to fix. The v3 operator smoke (start gpsd or stop gpsd between steps as noted):

1. **Sticky Manual against an arriving GPS fix (GPS present).** Start with gpsd running. Inline-edit the grid value to `EM75`. Assert the source chip says `MANUAL` (amber, button-shaped). Wait for gpsd to publish a fresh fix to `DM33`. Assert the grid value STAYS `EM75` (sticky Manual). Assert the source chip STAYS `MANUAL`.

2. **Source-chip escape from Manual (GPS present).** From step 1, click the source chip when `source = Manual`. Assert the source chip flips to `GPS` (green, span-shaped, locked). Assert the grid value flips to the live GPS fix `DM33`.

3. **Source-chip escape from Manual (no GPS — the case [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation) exists to fix).** Stop gpsd (`sudo systemctl stop gpsd`). Inline-edit the grid value to `EM75`. Assert State 1 (the source chip says `MANUAL` + grid value `EM75`). Click the source chip when `source = Manual`. Assert State 4 (the source chip says `GPS` dimmed + grid value `· EM75` interpunct + status text `"GPS no fix · broadcasting fallback"` + the `Set manually` button is present). Assert the operator is no longer stuck.

4. **`Set manually` button from State 4.** From step 3, click the `Set manually` button. Assert the grid input mounts and receives focus and is editable. Type `DM33` and press Enter. Assert State 1 (source chip says `MANUAL` + grid value `DM33`).

5. **GPS happy path (State 3).** Restart gpsd (`sudo systemctl start gpsd`). With `source = Gps`, assert the grid value tracks the live fix. Assert State 3 (source chip says `GPS` green, locked).

6. **Privacy gate intact under `gps_state = LocalUiOnly`.** Set `cfg.privacy.gps_state = LocalUiOnly`. Confirm the on-air locator (via the CMS-exchange operator smoke from PR #185) falls back to `cfg.identity.grid`, NOT the live GPS fix.

---

## §7 — Why pjih landed undetected (R5 #5 hypothesis)

R5 #5 caught that without a hypothesis for why the operator did not detect the pjih regression at merge time, the same merge-undetected dynamic could reproduce for the position-subsystem restoration.

**Hypothesis.** pjih's PR description and pjih's own adrev focused on the immediate symptom ("setting manual grid breaks GPS-derived display"). The agent that wrote pjih re-interpreted "GPS-derived display" as "the operator wants GPS-derived display even after setting Manual" — a semantic interpretation that REQUIRED removing the 2026-05-22 sticky-Manual contract. The pjih PR's stated goal was framed as "decouple grid-set from source-pin," which sounded like a clean refactor.

The operator approved the pjih PR based on the pjih framing, not by walking the actual UX with the pjih change in hand. The CI gates were green (per-layer tests passed). The full operator-visible flow (Manual → GPS sticky → escape via source chip OR State 4 fallback) was not exercised by any test, so the contract violation in pjih did not surface until the operator hit the pjih regression in smoke.

**Watched failure mode for the position-subsystem restoration.** The operator must walk the [§6.4 operator smoke](#64--operator-smoke-rewritten-per-r2-5--no-gps-hardware-case) steps end-to-end with the restoration changes in hand. If the operator smoke cannot run (no GPS hardware), steps 3 and 4 still exercise the no-GPS recoverability path. Adrev caught what unit tests cannot: cross-layer narrative coherence. The [§6.3 cross-layer integration test](#63--cross-layer-integration-test-r4-p1-4--r5-7--the-test-class-pjih-violated) is the automated form of that adrev — the single test that pins the position-subsystem restoration's primary correctness story.

---

## §8 — Track B interactions

Track B (tuxlink-jmfm + tuxlink-8rng) covers the Settings → GPS+Privacy panel cleanup + the radio-panel widening to 400 px. R5 #6 surfaced potential interactions between Track A (the position-subsystem restoration) and Track B:

**Settings panel cleanup (tuxlink-jmfm).** The `SettingsPanel.tsx` file is currently named the "GPS privacy" panel per the file header comment in `SettingsPanel.tsx`. The SettingsPanel currently contains GPS-state controls + precision controls + an ARDOP fieldset. Track B deletes the ARDOP fieldset from `SettingsPanel.tsx`. After Track A + Track B both land:
- The SettingsPanel is purely a GPS-privacy panel (its original intent).
- The position-subsystem restoration's `gps_state` privacy gate in `effective_broadcast_locator` continues to work; the SettingsPanel's GPS-state radio + precision radio elements are unchanged by Track A.

**No interaction between Track A code and Track B code.** Track B's ARDOP-fieldset delete touches no code in the position subsystem. Track A and Track B can ship in either order. If Track B ships first, the SettingsPanel becomes a "pure GPS-privacy panel" and Track A's source-chip-as-button work is unaffected. If Track A ships first, the SettingsPanel still has the ARDOP fieldset until Track B lands; the GPS+Privacy controls work as expected.

**Radio panel widening (tuxlink-8rng).** Entirely orthogonal to the position-subsystem restoration. The radio-panel chrome lives in `radio/RadioPanel.css`. The dashboard ribbon's GridEdit lives in `shell/`. Zero overlap.

---

## §9 — Adversarial review disposition

| Round | Reviewer | Findings | Disposition |
|---|---|---|---|
| R1 | Codex (GPT-5) | 6 (1 P0, 4 P1, 1 P2) | All P0 + P1 applied. Codex P0 #1 (the `use_gps` relaxation extends to the `position_set_source` command) → [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation) + [§3.1](#31--backend-revert--relaxation-table) + [§6.1 new test](#61--backend-tests-cargo---lib). Codex P1 #2 (source chip when `source = Gps` is status-only) → [§2.1](#21--source-chip-dom-type-and-click-semantics-by-source-value). Codex P1 #3 (cross-layer source-sequence tests) → [§6.1 added 4 tests](#61--backend-tests-cargo---lib). Codex P1 #4 (optimistic update) → [§4.3](#43--optimistic-update-after-config_set_grid-and-position_set_source-codex-p1-4). Codex P1 #5 (§5 migration text wrong) → [§5 rewritten](#5--migration-rewritten-per-codex-p1-5--r3-f8). Codex P2 #6 (focus contract) → [§2.3](#23--set-manually-button-state-4-and-state-5) + [§6.2 strengthened](#62--frontend-tests-vitest). |
| R2 | Claude (UX) | 8 (5 P1, 3 P2) | All P1 applied. R2 #1 (source-chip click semantics justification) → [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch) + [§2.1](#21--source-chip-dom-type-and-click-semantics-by-source-value). R2 #2 (source chip + GPS-ready button redundancy) → [§2.2](#22--source-chip-and-gps-ready-status-redundancy-in-state-2) + [§4.2](#42--source-chip-as-button--gps-ready-as-passive-span--set-manually-button). R2 #3 (`Set manually` button a11y) → [§2.3](#23--set-manually-button-state-4-and-state-5). R2 #4 (State 1 vs State 4 visual differentiation) → [§2.4](#24--state-1-vs-state-4-visual-differentiation-r2-4--the-failure-mode-pjih-shipped) + [§6.2 new test](#62--frontend-tests-vitest). R2 #5 (operator smoke missing no-GPS case) → [§6.4 step 3](#64--operator-smoke-rewritten-per-r2-5--no-gps-hardware-case). R2 P2 #6 (mouse-centric "tap" language) → [§4.4](#44--a11y-treatment-r2-6--r2-3). R2 P2 #7 (subsumed by source-chip-as-span when `source = Gps`). R2 P2 #8 (width budget) — deferred to implementation polish. |
| R3 | Claude (contract/races) | 10 (1 P0, 3 P1, 6 P2) | All P0 + P1 applied. R3 F2 P0 (privacy-gate clarity for State 4) → [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation) + [§2.5](#25--broadcasting-in-state-4-and-state-5-clarity-not-divergence). R3 F1 + F7 P1 (concurrency invariants) → [§3.3](#33--concurrency-invariants-for-config_set_grid-and-position_set_source-r3-f1--r3-f7). R3 F8 P1 (§5 migration narrative wrong) → [§5 rewritten](#5--migration-rewritten-per-codex-p1-5--r3-f8). R3 F4 P1 (state-space) → [§3.4](#34--state-space-invariants-r3-f4--all-36-cells-of-source--fix_state--gps_state--manual_grid_set) + [§6.1 matrix tests](#61--backend-tests-cargo---lib). R3 P2 — implementation-detail; tracked for the plan. |
| R4 | Claude (tests) | 11 (2 P0, 4 P1, 5 P2) | All P0 + P1 applied. R4 P0 #1 (temporal sticky test) → [§6.1 sticky test extended](#61--backend-tests-cargo---lib). R4 P0 #2 (`use_gps` `active_grid` assertion) → [§6.1 new test](#61--backend-tests-cargo---lib). R4 P1 #3 (source-chip-element-type test) → [§6.2 added](#62--frontend-tests-vitest). R4 P1 #4 (composed flow) → [§6.3 integration test](#63--cross-layer-integration-test-r4-p1-4--r5-7--the-test-class-pjih-violated). R4 P1 #5 (4-quadrant `Set manually` matrix) → [§6.2 expanded](#62--frontend-tests-vitest). R4 P1 #6 (3 missing backend invariants) → [§6.1 added](#61--backend-tests-cargo---lib). R4 P2 — partially applied to §6 strengthening. |
| R5 | Claude (holistic) | 12 (2 P0, 5 P1, 5 P2) | All P0 + P1 applied. R5 P0 #1 ("one amendment" understated) → [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation) (full extent) + [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch). R5 P0 #2 (alternatives not compared) → [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch). R5 P1 #3 (source-chip click semantics) → [§2.1](#21--source-chip-dom-type-and-click-semantics-by-source-value) + [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch). R5 P1 #4 ("GPS by default" axiomatic) — operator + 2026-05-22 spec confirmed; documented as a decision in [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch). R5 P1 #5 (why pjih undetected) → [§7](#7--why-pjih-landed-undetected-r5-5-hypothesis). R5 P1 #6 (Track B interactions) → [§8](#8--track-b-interactions). R5 P1 #7 (cross-layer test class) → [§6.3 integration test](#63--cross-layer-integration-test-r4-p1-4--r5-7--the-test-class-pjih-violated). R5 P2 — partially applied. |

**Total: 47 findings; 6 P0 + 21 P1 = 27 must-apply, all applied. P2: 20, selectively applied per cost/value.**

---

## Appendix A — Reference: the 2026-05-22 spec

See [`docs/superpowers/specs/2026-05-22-position-subsystem-design.md`](2026-05-22-position-subsystem-design.md) for the authoritative source contract, the gpsd-client design, the manual-entry approach (Approach A), the source-chip spec (Approach A), and the full ribbon-states table.

The position-subsystem restoration spec extends — does not supersede — the 2026-05-22 spec.
