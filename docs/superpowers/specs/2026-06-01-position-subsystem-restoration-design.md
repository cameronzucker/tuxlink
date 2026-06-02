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
| **Source segmented control** | `<div role="radiogroup" aria-label="Position source">` containing two `<button role="radio">` children — the **GPS segment** and the **MANUAL segment** | `GridEdit.tsx`, supersedes the conditional `<button>`/`<span role="status">` chip pattern (2026-06-02 follow-up — tuxlink-z5pz; T12 pattern superseded) |
| **GPS segment** | `<button type="button" role="radio" data-testid="source-segment-gps">` with `aria-checked={source === 'Gps'}`; always rendered + always tab-reachable; selected when `source = Gps`; click fires `onUseGps` when not selected (no-op when selected) | `GridEdit.tsx`, child of the source segmented control |
| **MANUAL segment** | `<button type="button" role="radio" data-testid="source-segment-manual">` with `aria-checked={source === 'Manual'}`; always rendered + always tab-reachable; selected when `source = Manual`; click fires `onUseManual` (enters local edit mode) when not selected (no-op when selected) | `GridEdit.tsx`, child of the source segmented control |
| **Grid value** | `<button>` (inline-edit trigger) | `GridEdit.tsx`, pre-pjih behavior |
| **Grid input** | `<input>` (active during edit) | `GridEdit.tsx`, pre-pjih behavior |
| **In-segment GPS-ready indicator** | `' ●'` text suffix rendered inside the GPS segment's label when `source === 'Manual' && gpsReady === true` | `GridEdit.tsx`, supersedes the standalone `<span data-testid="gps-ready-status">` sibling hint (2026-06-02 follow-up — tuxlink-z5pz; T11 pattern superseded) |
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
| `effective_broadcast_locator(cfg, arbiter)` | `src-tauri/src/position/mod.rs` | **On-air locator only.** Computes what gets transmitted on RF/CMS. Privacy-gated by `cfg.privacy.gps_state`. Under `LocalUiOnly` or `Off` with `source = Gps`, falls back to `config.identity.grid` (the static fallback) — even when `arbiter.has_fresh_fix()` is true. The single source of truth for what crosses the air. NOT the ribbon display locator — see `effective_ui_locator`. |
| `effective_ui_locator(cfg, arbiter)` | `src-tauri/src/position/mod.rs` | **Ribbon display locator only.** Computes what the operator sees in the local Grid cell. NOT privacy-gated for `LocalUiOnly` or `BroadcastAtPrecision` source=Gps fresh-fix cases — under those, the live precision-reduced fix is displayed (the operator can see GPS locally; the privacy gate applies to on-air only, not to the operator's own UI). Under `source = Manual`, `source = Gps + Off`, or `source = Gps` with no fresh fix, falls back the same way `arbiter.active_grid()` does (then precision-reduced). See [§3.4 I3 + I4 + I7](#34--state-space-invariants-r3-f4--all-36-cells-of-source--fix_state--gps_state--manual_grid_set) for the full state-space behavior. |

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
- Visual redesign of the source chip beyond the actionable/status distinction defined in [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern).
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

**What the relaxation amendment changes:** the State 4 / State 5 state — previously only reachable as an initial-state quirk under the 2026-05-22 spec — becomes a stable operator-driven destination reachable from State 1 by clicking the source chip when `source = Manual`. The source chip when `source = Gps` shows GPS (operator preference). The on-air locator is `arbiter.manual_grid` (the State 4 fallback per the 2026-05-22 spec row 3) — though see [§2.5](#25--ribbon-display-locator-vs-on-air-locator-intentionally-distinct-concerns) for the corrected on-air-vs-display treatment under `LocalUiOnly`. See [§2.5](#25--ribbon-display-locator-vs-on-air-locator-intentionally-distinct-concerns) for the State 4 broadcast-clarity treatment.

**What the relaxation amendment does NOT change:**
- Sticky-Manual semantics — `arbiter.set_manual` still pins `source = Manual`; a GPS fix arriving never overrides `source = Manual`.
- The privacy gate in `effective_broadcast_locator` — keys on `arbiter.source()` (the stored preference), as before.
- The gpsd client (fix-quality gate, staleness window, reconnect backoff) — unchanged.
- The initial `source` default value (`Gps`) — per the [`project_gps_precision_reduction.md`](../../../...) memory + 2026-05-22 spec.

### §1.2 — Why the `use_gps()` relaxation over "Clear Manual pin" or "confirm-then-switch"

R5 P0 #2 caught that the position-subsystem restoration v1 picked the `use_gps()` relaxation without comparing alternatives. Three mechanisms were on the table:

| Mechanism | Pros | Cons | Decision |
|---|---|---|---|
| **The `use_gps() + position_set_source('Gps')` relaxation** | Smallest delta from the 2026-05-22 spec. The source chip when `source = Manual` remains the single switch surface. No new UI elements. Matches the operator's "original spec but not broken" framing. | Two named backend symbols change, not "one amendment" — see [§1.1](#11--the-use_gps--position_set_source-gps-relaxation) for the full extent. State 4 requires the "broadcasting fallback" status text — see [§2.5](#25--ribbon-display-locator-vs-on-air-locator-intentionally-distinct-concerns). | **CHOSEN by operator 2026-06-01.** |
| Dedicated "Clear Manual pin" button beside the source chip | No 2026-05-22 spec contract amendment. The `arbiter.use_gps()` semantic preserved as-written. Each UI element does exactly one thing. | New UI surface (a second button beside the source chip) adds visual weight to the ribbon Grid cell. Operator must learn the new button. Two paths from State 1 (clicking the source chip vs clicking the new Clear-Manual-pin button) create UX ambiguity about which path is canonical. | Rejected by operator: introduces UI noise without solving the case where a source switch must land WITHOUT a fresh fix. |
| Two-stage confirm-then-switch on source-chip click | Prevents accidental source switches. | Operator has previously flagged confirmation modals as ceremony (`feedback_radio1_governs_tx_not_ui` memory). Adds friction without addressing the root recoverability gap. | Rejected by operator: friction without solving the gap. |

---

## §2 — UI surface (source chip clickable + `Set manually` button + State 1 vs State 4 differentiation)

### §2.1 — Source segmented control DOM and click semantics (2026-06-02 follow-up — tuxlink-z5pz; supersedes the T12 conditional-chip pattern)

**Historical note (preserved adrev lineage).** The original v3 §2.1 prescribed a conditional pattern: `<button>` when `source = Manual` (with `aria-label="Switch position source to GPS"`), `<span role="status">` when `source = Gps`. That T12 pattern was grounded in three adrev rounds (Codex R1 #2, R2 #1, R5 #3) converging on "an enabled `<button>` rendering of the source chip when `source = Gps` that does nothing on click is a UX failure." The T12 pattern is ARIA-correct but the operator reported (2026-06-01, tuxlink-z5pz) that it is **undiscoverable**: "no human would think to click MANUAL to switch to GPS." A status-shaped chip reads as a badge, not a switch surface. The T12 lineage is preserved here; the implementation is superseded by the radio-group segmented control below. See [§9 the 2026-06-02 follow-up consultation entry for tuxlink-z5pz](#9--adversarial-review-disposition) for the full lineage write-up.

**The source segmented control.** The source surface is a radio-group of two segments — the GPS segment and the MANUAL segment — both always rendered, both always clickable, with the selected segment marked by `aria-checked="true"`:

```tsx
<div className="dash-source-segments" role="radiogroup" aria-label="Position source">
  <button
    type="button"
    role="radio"
    aria-checked={source === 'Gps'}
    data-testid="source-segment-gps"
    className={`dash-source-segment gps ${source === 'Gps' ? 'selected' : ''} ${gpsReady ? 'gps-ready' : ''}`}
    onClick={source === 'Gps' ? undefined : onUseGps}
  >
    GPS{gpsReady && source !== 'Gps' ? ' ●' : ''}
  </button>
  <button
    type="button"
    role="radio"
    aria-checked={source === 'Manual'}
    data-testid="source-segment-manual"
    className={`dash-source-segment manual ${source === 'Manual' ? 'selected' : ''}`}
    onClick={source === 'Manual' ? undefined : onUseManual}
  >
    MANUAL
  </button>
</div>
```

**Click semantics.**
- Clicking the already-selected segment is a no-op (`onClick={undefined}` on that segment). The selected segment remains tab-reachable so screen readers can announce "<label>, selected" without trapping the operator in an action loop.
- Clicking the OTHER segment fires the source switch:
  - **GPS segment when `source = Manual`** fires `onUseGps`, which calls `invoke('position_set_source', { source: 'Gps' })` (infallible per [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation)).
  - **MANUAL segment when `source = Gps`** fires `onUseManual`, which calls `GridEdit.tsx`'s `enterEdit()` handler — the same affordance the `Set manually` button uses. The operator then types their manual grid; on Enter-commit, the existing T4-restored `config_set_grid(grid)` command persists `cfg.privacy.position_source = Manual` AND the new grid value. The MANUAL segment click does NOT call `position_set_source('Manual')` — that command's `'Manual'` arm returns `UiError::Rejected` by existing-spec design (the operator-only path INTO `source = Manual` is via the `config_set_grid` command with a grid value, not via `position_set_source` with a bare source token). See [§3.1](#31--backend-revert--relaxation-table) for the reuse of the T4 path.

**Props (`GridEditProps`).**
- `onUseGps: () => void` — preserved from T10. Fires on the GPS segment click when `source = Manual`.
- `onUseManual: () => void` — NEW in this amendment. Fires on the MANUAL segment click when `source = Gps`. Mirrors `onUseGps`'s prop shape. The `DashboardRibbon.tsx` parent passes `onUseManual={() => gridEditRef.current?.enterEdit()}` (or the equivalent local handler), reusing the same `enterEdit()` entry point the `Set manually` button uses.

**Rationale for the radio-group pattern.** The T12 conditional-chip pattern was ARIA-correct (it used `aria-pressed` on the Manual `<button>`) but visually read as a status badge — an operator looking at a `MANUAL` chip with no visible "or GPS" alternative had no cue that there was a switch available. The radio-group pattern always shows both options side-by-side; selection is unambiguous (selected segment is highlighted, the other is outlined); both segments are clickable (no dead `<span>` masquerading as a chip). The pattern matches the WAI-ARIA Authoring Practices "Radio Group" example and the `RibbonDots` radio pattern already used elsewhere in tuxlink's ribbon UI.

The actionable/status distinction of the T12 design (raised vs flat) is replaced by an equivalent unambiguous-selection distinction (filled vs outlined segments). The visual cue carries over; the discoverability problem does not.

### §2.2 — GPS-ready indicator folds into the GPS segment (2026-06-02 follow-up — tuxlink-z5pz; supersedes the T11 standalone-span pattern)

**Historical note (preserved adrev lineage).** The original v3 §2.2 prescribed a passive `<span data-testid="gps-ready-status">● GPS ready</span>` sibling hint, rendered when `source === 'Manual' && gpsReady === true` (State 2). The T11 sibling-hint pattern was the de-duplication fix for the pre-pjih dual-clickable surface (both the source chip AND the GPS-ready button were clickable in State 2). The T11 pattern is preserved here as adrev lineage; the implementation is superseded by the in-segment indicator below.

**The in-segment GPS-ready indicator.** With the source segmented control (§2.1), the GPS segment is always visible — there is no need for a separate sibling element to announce "GPS is available." The ready indicator folds into the GPS segment as a `' ●'` text suffix, rendered when `source === 'Manual' && gpsReady === true`:

```
[ GPS ● ] [ MANUAL ]    ← GPS segment shows "GPS ●" suffix; MANUAL segment is selected
```

The dot inside the GPS segment carries the same semantic load as the T11 sibling hint ("a fresh fix is currently available; you could switch to GPS"), but the visual lives on the segment that already invites the click. The separate `<span data-testid="gps-ready-status">` sibling element is REMOVED.

**Behavior matrix.**

| Condition | GPS segment label |
|---|---|
| `source = Gps` (regardless of `gpsReady`) | `GPS` (no suffix — the segment is selected; the dot would be redundant) |
| `source = Manual && gpsReady = false` | `GPS` (no suffix — no fresh fix available) |
| `source = Manual && gpsReady = true` | `GPS ●` (the in-segment indicator) |

The State 2 visual is cleaner (one element instead of two) and the discoverability problem is solved structurally: an operator who can see the GPS segment can see whether it has a fresh fix available, without scanning sideways for a separate hint span.

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

### §2.4 — State 1 vs State 4 visual differentiation (R2 #4 — the failure mode pjih shipped; updated 2026-06-02 for the segmented control)

R2 #4 identifies the UX failure mode that pjih literally shipped: at first glance, State 1 (`source = Manual` + no GPS) and State 4 (`source = Gps + no fresh fix + manual_grid set`) can look identical — same grid value, same surrounding ribbon chrome. The operator cannot tell at a glance whether the operator is broadcasting the manual_grid as a Manual pin or as a Gps fallback.

With the source segmented control from [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern), the State 1 vs State 4 differentiation now lives in **which segment is selected + how that segment is styled**, not in raised-vs-flat chip shape:

| State label | GPS segment | MANUAL segment | Grid value prefix | Status text |
|---|---|---|---|---|
| State 1 (`source = Manual` + no GPS) | outlined, no `' ●'` suffix | selected (filled, amber) | (none) | (none) |
| State 2 (`source = Manual` + fresh fix) | outlined, `' ●'` suffix (in-segment indicator) | selected (filled, amber) | (none) | (none — the dot is the cue) |
| State 3 (`source = Gps` + fresh fix) | selected (filled, green) | outlined | (none) | (none) |
| State 4 (`source = Gps` + no fresh fix + manual_grid set) | selected (filled, green) BUT **dimmed** via `.dash-source-segment.gps.selected:not(.gps-ready)` | outlined | `· ` interpunct prefix on the grid value | "GPS no fix · broadcasting fallback" (passive `<span>`) |
| State 5 (`source = Gps` + no fresh fix + manual_grid = None) | selected (filled, green) BUT **dimmed** via the same selector | outlined | (none — grid value is `—` em-dash) | "GPS no fix" (passive `<span>`) |
| State 6 (`source = Manual` + manual_grid = None) | outlined | selected (filled, amber) | (none — grid value is `—` em-dash) | (none) |

**The distinguishing CSS for State 1 vs State 4** is `.dash-source-segment.gps.selected:not(.gps-ready)` — the dimmed-selected styling that applies only to the GPS segment when it is selected but does NOT have a fresh fix. The CSS class names `gps`, `selected`, and `gps-ready` are emitted by the `className` template in [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern):
- State 1: MANUAL segment is `.dash-source-segment.manual.selected`; GPS segment is `.dash-source-segment.gps` (not selected, no dim).
- State 4: GPS segment is `.dash-source-segment.gps.selected` (selected, dimmed via the `:not(.gps-ready)` selector); MANUAL segment is `.dash-source-segment.manual` (not selected).

The differentiation property is preserved: the interpunct prefix (`· `) on the grid value, the dimmed GPS-segment styling, and the status text together make State 4 visually distinct from State 1. The 2026-05-22 spec row 3 wording (`CN87 · GPS no fix`) defines the interpunct as part of the canonical State 4 + State 5 grid-value display.

### §2.5 — Ribbon display locator vs on-air locator (intentionally distinct concerns)

**Important amendment (2026-06-02).** The earlier v3 framing of §2.5 argued that displayed grid value and on-air locator are NOT divergent in State 4 because both resolve to `manual_grid`. That framing was a narrow defense of the Codex P1-B (R3 F2 P0) finding's collapse of "ribbon display" and "on-air locator" onto a single `effective_broadcast_locator` helper. The narrow-State-4 argument is correct in isolation, but the helper-collapse is wrong as a general design — it broke `gps_state = LocalUiOnly` semantics entirely.

**Operator-reported regression (tuxlink-va1i, 2026-06-02).** With `source = Gps + gps_state = LocalUiOnly + fresh GPS fix + position_precision = SixCharGrid + identity.grid = "DM33"`, the ribbon displays `DM33` (the static config fallback) instead of `DM33ww` (the live precision-reduced fix grid). The operator's literal intent under `LocalUiOnly` is *"see GPS locally, don't broadcast"* — the live local display is precisely what `LocalUiOnly` was named to preserve. Collapsing display onto the privacy-gated on-air helper broke that intent.

**Spec text (corrected).** Ribbon display and on-air locator are **intentionally distinct concerns**, served by **two separate helpers**:

- `effective_broadcast_locator(cfg, &arbiter)` — on-air locator only. Privacy-gated by `cfg.privacy.gps_state`. Under `source = Gps + gps_state ∈ {LocalUiOnly, Off}`, returns `config.identity.grid` (the static fallback) regardless of whether a fresh fix is available. The single source of truth for what crosses RF/CMS.
- `effective_ui_locator(cfg, &arbiter)` — ribbon display locator only. NOT privacy-gated for `LocalUiOnly` / `BroadcastAtPrecision` source=Gps fresh-fix cases (the operator can see GPS locally; `LocalUiOnly` controls the air, not the operator's own UI). Falls back the same way `arbiter.active_grid()` does under `source = Manual`, `source = Gps + Off`, or `source = Gps` with no fresh fix.

The two helpers **MAY coincide** (most state-cells produce the same value — e.g. State 3 with `BroadcastAtPrecision`, or any State under `Off`). The two helpers **ARE NOT collapsed** onto one function — under `source = Gps + LocalUiOnly + fresh fix`, they intentionally diverge:

| Cell | `effective_ui_locator` (display) | `effective_broadcast_locator` (on-air) |
|---|---|---|
| `source = Gps + LocalUiOnly + fresh fix + identity.grid = "DM33" + SixCharGrid` | `"DM33ww"` (live precision-reduced fix) | `"DM33"` (static config fallback — privacy honored) |
| `source = Gps + BroadcastAtPrecision + fresh fix + identity.grid = "DM33" + SixCharGrid` | `"DM33ww"` (live precision-reduced fix) | `"DM33ww"` (live precision-reduced fix — same value, both helpers coincide) |
| `source = Gps + Off + fresh fix + identity.grid = "DM33" + SixCharGrid` | `"DM33"` (config fallback — operator chose Off, UI honors that) | `"DM33"` (config fallback) |
| `source = Gps + LocalUiOnly + no fresh fix + identity.grid = "DM33" + manual_grid = "CN87"` (State 4) | `"CN87"` (fallback to `manual_grid`, then precision-reduced) | `"DM33"` (config fallback — privacy honored, since `gps_state = LocalUiOnly` chooses the static fallback for on-air regardless of whether `manual_grid` is set) |
| `source = Manual + (any gps_state) + manual_grid = "CN87"` | `"CN87"` (precision-reduced) | `"CN87"` (precision-reduced) |

**The Codex P1-B argument preserved as partial truth.** The original Codex P1-B finding ("operator should see what they'd transmit") identified a real concern — operator awareness of what gets transmitted MATTERS. But collapsing ribbon display onto the on-air helper is the wrong UX surface for that concern. The correct surface for "show the operator what they'd transmit" is a **separate broadcast-grid badge / tooltip / settings affordance** (out of scope for this restoration; tracked as a follow-up if operator surfaces it as a wanted feature). Collapsing the two helpers onto one optimizes for a feature that was never explicitly wanted, at the cost of breaking `LocalUiOnly` semantics that ARE explicitly wanted.

**Spec text — State 4 broadcasting under the corrected helpers.** When `source = Gps && no fresh fix && manual_grid is set` (State 4):

- The grid value displayed in the ribbon (`effective_ui_locator(cfg, &arbiter)`) = `manual_grid` (precision-reduced) — the live precision-reduced fix is unavailable, so display falls back to `manual_grid`.
- The on-air locator (`effective_broadcast_locator(cfg, &arbiter)`) under `gps_state = BroadcastAtPrecision`: also `manual_grid` (precision-reduced). Under `gps_state ∈ {LocalUiOnly, Off}`: `config.identity.grid` (static fallback).
- Under `BroadcastAtPrecision` the two helpers coincide in State 4. Under `LocalUiOnly` / `Off` they may diverge if `manual_grid != config.identity.grid` — display reflects `manual_grid` (the operator's most recent intent), on-air honors privacy by falling back to the static config value.

**UI text (unchanged).** The State 4 status text reads `"GPS no fix · broadcasting fallback"` rather than `"GPS no fix"`. The "broadcasting fallback" suffix tells the operator that the displayed value is the position the system would use absent a fresh fix; it does NOT promise the displayed value is the literal on-air locator (under `LocalUiOnly`, the on-air locator is the privacy-gated static fallback). For operators who need the literal on-air locator to be visible in-app, the broadcast-grid badge / tooltip follow-up is the right place — not a collapse of the two helpers.

### §2.6 — ASCII state mockups (the six states; updated 2026-06-02 for the segmented control)

The mockups below show the source segmented control from [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern). In ASCII, a `█` border indicates the SELECTED segment (filled); a `─` border indicates the UNSELECTED segment (outlined). The earlier v3 §2.6 single-chip mockups are superseded.

State 1 — `source = Manual` && no fresh fix:
```
┌──────────────────────────────────────────────────────────────┐
│ Grid │  CN87   ┌─────┬──────┐                                │
│      │         │ GPS │MANUAL│   GPS outlined; MANUAL selected │
│      │         └─────┴██████┘   (click GPS → onUseGps)        │
└──────────────────────────────────────────────────────────────┘
```

State 2 — `source = Manual` && fresh fix exists:
```
┌──────────────────────────────────────────────────────────────┐
│ Grid │  CN87   ┌───────┬──────┐                              │
│      │         │ GPS ● │MANUAL│   GPS outlined w/ in-segment │
│      │         └───────┴██████┘   dot; MANUAL selected        │
│      │                              (the ' ●' replaces the    │
│      │                               old gps-ready-status     │
│      │                               sibling span)            │
└──────────────────────────────────────────────────────────────┘
```

State 3 — `source = Gps` && fresh fix:
```
┌─────────────────────────────────────────────────────────────┐
│ Grid │  DM33   ┌─────┬──────┐                               │
│      │         │ GPS │MANUAL│   GPS selected (filled, green) │
│      │         └█████┴──────┘   MANUAL outlined              │
│      │                          (click MANUAL → onUseManual  │
│      │                           → enterEdit())              │
└─────────────────────────────────────────────────────────────┘
```

State 4 — `source = Gps` && no fresh fix && manual_grid set:
```
┌──────────────────────────────────────────────────────────────────────┐
│ Grid │  · CN87  ┌─────┬──────┐  GPS no fix · broadcasting fallback   │
│      │  ^       │ GPS │MANUAL│  [ ▸ Set manually ]                    │
│      │  interpunct└█████┴──────┘ GPS selected BUT dimmed              │
│      │            (.dash-source-segment.gps.selected:not(.gps-ready))│
└──────────────────────────────────────────────────────────────────────┘
```

State 5 — `source = Gps` && no fresh fix && manual_grid = None:
```
┌──────────────────────────────────────────────────────────────┐
│ Grid │  —      ┌─────┬──────┐  GPS no fix                    │
│      │  em-dash│ GPS │MANUAL│  [ ▸ Set manually ]              │
│      │         └█████┴──────┘  GPS selected BUT dimmed         │
│      │                         (same selector as State 4)      │
└──────────────────────────────────────────────────────────────┘
```

State 6 — `source = Manual` && manual_grid = None:
```
┌─────────────────────────────────────────────────────────────┐
│ Grid │  —      ┌─────┬──────┐                               │
│      │  em-dash│ GPS │MANUAL│   GPS outlined; MANUAL selected │
│      │         └─────┴██████┘                                │
└─────────────────────────────────────────────────────────────┘
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
| `PositionStatusDto` struct | `{ gps_ready, broadcast_grid }` | `{ gps_ready, broadcast_grid, active_source }` | **Amended 2026-06-02:** Remove the `active_source` field AND add a new `ui_grid: String` field. Post-amendment shape: `{ gps_ready, ui_grid, broadcast_grid }`. |
| `position_status` command | Populates `gps_ready` from `arbiter.has_fresh_fix()` + `broadcast_grid` from `effective_broadcast_locator` | Same + populates `active_source` from `arbiter.effective_source()` | **Amended 2026-06-02:** Drop the `active_source` population. Populate `gps_ready` from `arbiter.has_fresh_fix() && cfg.privacy.gps_state != GpsState::Off` (Codex 2026-06-01 adjacent fix: `gps_ready` should be `false` when the operator chose `Off`, even if the arbiter still holds a stale fix from before the state switch). Populate `broadcast_grid` from `effective_broadcast_locator(cfg, &arbiter)` (unchanged). Populate `ui_grid` from `effective_ui_locator(cfg, &arbiter)` (NEW). |

**Note (2026-06-02 follow-up — tuxlink-z5pz, segmented-control amendment): no backend changes.** The source segmented control from [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern) does NOT introduce any new backend command. In particular:

- The GPS-segment click (when `source = Manual`) reuses the existing `position_set_source` command — same path the T12 chip-button used.
- The MANUAL-segment click (when `source = Gps`) does NOT call `position_set_source('Manual')` (whose `'Manual'` arm returns `UiError::Rejected` by existing-spec design). Instead, the MANUAL-segment click fires `enterEdit()` in `GridEdit.tsx` — the same affordance the `Set manually` button (per [§2.3](#23--set-manually-button-state-4-and-state-5)) uses. The operator types their grid, hits Enter, and the existing T4-restored `config_set_grid(grid)` command persists `cfg.privacy.position_source = Manual` + the new grid value in one atomic write. The cross-layer T4 path is reused; no new backend code is needed for the segmented control.

### §3.2 — Backend keep-as-is (no change from current `main`)

- `effective_broadcast_locator(cfg, arbiter)` in `src-tauri/src/position/mod.rs` — already keys the privacy gate on `arbiter.source()` (the stored preference). The current `main` behavior is correct **for the on-air path** post-restore. No change to this helper's body. **Amended 2026-06-02:** this helper remains canonical ONLY for the on-air locator. The ribbon display path uses `effective_ui_locator` (NEW, sibling helper added in [§3.1](#31--backend-revert--relaxation-table)). Update the rustdoc on `effective_broadcast_locator` to call out that it is the single source of truth for what crosses RF/CMS — not the ribbon display.
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
| **I3** | When `source = Gps && fresh fix` (State 3): `arbiter.active_grid()` returns `fix.grid`. The on-air locator from `effective_broadcast_locator` respects `gps_state` per the existing privacy gate — under `BroadcastAtPrecision`: precision-reduced fix; under `LocalUiOnly` or `Off`: `config.identity.grid` fallback. **Amended 2026-06-02:** The ribbon display locator from `effective_ui_locator` does NOT privacy-gate on `LocalUiOnly` — under `BroadcastAtPrecision` OR `LocalUiOnly`: precision-reduced live fix; under `Off`: `config.identity.grid` fallback. |
| **I4** | When `source = Gps && no fresh fix && manual_grid set` (State 4): `arbiter.active_grid()` returns `manual_grid` (fallback per the 2026-05-22 spec row 3). The grid value displayed in State 4 (`effective_ui_locator`) = `manual_grid` (precision-reduced). The on-air locator (`effective_broadcast_locator`) under `BroadcastAtPrecision` = `manual_grid` (precision-reduced); under `LocalUiOnly` or `Off` = `config.identity.grid` (static fallback) — see [§2.5](#25--ribbon-display-locator-vs-on-air-locator-intentionally-distinct-concerns). |
| **I5** | When `source = Gps && no fresh fix && manual_grid = None` (State 5): `arbiter.active_grid()` returns `None`; the grid value displayed in State 5 = `—` (em-dash); both `effective_ui_locator` and `effective_broadcast_locator` fall back to `config.identity.grid` (which under the I6 invariant is synchronized with `manual_grid`; if `manual_grid` is `None`, `config.identity.grid` is also `None`; both locator strings are empty — this is a config-file integrity assumption, not a runtime failure). |
| **I6** | `arbiter.manual_grid` and `config.identity.grid` are synchronized by the `config_set_grid` command (which writes both atomically). The `arbiter.new()` constructor reads the initial `manual_grid` from `config.identity.grid`. No code path mutates one without the other except gpsd fix arrival (which writes only `arbiter.last_fix`, never `arbiter.manual_grid`). |
| **I7** (NEW, 2026-06-02) | **`effective_ui_locator` is NOT privacy-gated for `LocalUiOnly` or `BroadcastAtPrecision` source=Gps fresh-fix cases.** Specifically: under `source = Gps && fresh fix && gps_state ∈ {LocalUiOnly, BroadcastAtPrecision}`, `effective_ui_locator` returns the precision-reduced live fix grid (the live position the operator wants to see locally). Under `source = Gps && gps_state = Off`, `effective_ui_locator` falls back to `config.identity.grid` (operator chose "no GPS"; UI honors that intent). Under `source = Manual`, `effective_ui_locator` returns `manual_grid` (precision-reduced) regardless of `gps_state`. The matrix proptest in [§6.1](#61--backend-tests-cargo---lib) (`ui_locator_matrix`) pins all 5 states × 3 `gps_state` cells. |

Tests for the I1–I7 invariants: see [§6.1](#61--backend-tests-cargo---lib) for the state-space matrix tests (both `active_grid_matrix` for `arbiter.active_grid()` + `effective_broadcast_locator`, and the new `ui_locator_matrix` for `effective_ui_locator`).

**Visual rendering note (2026-06-02 — tuxlink-z5pz, segmented-control amendment).** The I1–I7 invariants govern state SEMANTICS (what the helpers return), not visual rendering. The visual rendering of each invariant in the source segmented control from [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern):

| Invariant | State | GPS segment | MANUAL segment |
|---|---|---|---|
| I1 | State 6 (`source = Manual && manual_grid = None`) | outlined | selected (filled, amber) |
| I2 | State 1 / State 2 (`source = Manual`) | outlined; `' ●'` in-segment suffix iff `gpsReady` | selected (filled, amber) |
| I3 | State 3 (`source = Gps && fresh fix`) | selected (filled, green, bright) | outlined |
| I4 | State 4 (`source = Gps && no fix && manual_grid set`) | selected (filled, green, dimmed via `:not(.gps-ready)`) | outlined |
| I5 | State 5 (`source = Gps && no fix && manual_grid = None`) | selected (filled, green, dimmed via the same selector) | outlined |
| I6 | (any) | (no segment-specific visual) | (no segment-specific visual) |
| I7 | (any UI-display case) | (visual matches the invariant for the corresponding state above) | (same) |

The visual rendering note is a layer below the invariants — the invariants pin backend semantics; the table above pins the frontend translation of those semantics into the segmented control's `aria-checked` + `className` outputs.

---

## §4 — Frontend changes (revert pjih + source chip as button + `Set manually` button + optimistic update)

### §4.1 — Frontend revert table

| Frontend surface | pjih state (current `main`) | Restoration target |
|---|---|---|
| `DashboardRibbon.tsx` GridEdit invocation | `<GridEdit ... onCommit={...} />` — no `onUseGps` prop passed | Restore `onUseGps={() => invoke('position_set_source', { source: 'Gps' })}` on the GridEdit invocation. **Amended 2026-06-02 (tuxlink-z5pz):** Also pass `onUseManual={() => gridEditRef.current?.enterEdit()}` (or the equivalent local handler) — the new prop fired by the MANUAL segment when `source = Gps`. |
| `GridEdit.tsx` `GridEditProps` interface | No `onUseGps` member | Restore `onUseGps: () => void` member in `GridEditProps`. **Amended 2026-06-02 (tuxlink-z5pz):** Also add `onUseManual: () => void` member — fires on the MANUAL segment click when `source = Gps`; mirrors `onUseGps`'s prop shape. |
| `GridEdit.tsx` source chip (`<button>`-or-`<span>` conditional, T12 pattern) | T12 pattern: `<button aria-pressed={false}>` when `source = Manual`, `<span role="status">` when `source = Gps` | **Amended 2026-06-02 (tuxlink-z5pz):** REPLACED by the `<div role="radiogroup" aria-label="Position source">` with two `<button role="radio" aria-checked={...}>` children (the GPS segment and the MANUAL segment) per [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern). The T12 `data-testid="source-chip"` is replaced by `data-testid="source-segment-gps"` + `data-testid="source-segment-manual"`. |
| `GridEdit.tsx` `<span data-testid="gps-ready-status">` element (T11 pattern) | Pre-pjih: a `<button data-testid="use-gps">`; T11 replaced it with a passive `<span data-testid="gps-ready-status">● GPS ready</span>` sibling hint rendered when `source = Manual && gpsReady` | **Amended 2026-06-02 (tuxlink-z5pz):** REMOVED. The ready indicator folds INTO the GPS segment as a `' ●'` text suffix on the GPS segment's label when `source = Manual && gpsReady = true` per [§2.2](#22--gps-ready-indicator-folds-into-the-gps-segment-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t11-standalone-span-pattern). The standalone sibling span is deleted. |
| `GridEdit.tsx` `<button data-testid="use-gps">` element (pre-pjih + then-deleted-by-pjih) | Removed entirely by pjih | NOT restored as a `<button>` — replaced FIRST by the T11 `<span data-testid="gps-ready-status">` passive hint (above), and now (post-tuxlink-z5pz) by the in-segment indicator inside the GPS segment of the radio-group. |
| `useStatus.ts` `PositionStatusDto` interface | `{ gps_ready, broadcast_grid, active_source }` | **Amended 2026-06-02:** Remove the `active_source` member AND add a `ui_grid: string` member. Post-amendment shape: `{ gps_ready, ui_grid, broadcast_grid }`. |
| `useStatus.ts` `useStatusData` hook | `position_source: positionStatus?.active_source ?? config?.position_source ?? 'Gps'` | Restore `position_source: config?.position_source ?? 'Gps'`. |
| `useStatus.ts` `liveGrid` derivation (NEW row, 2026-06-02) | `liveGrid = positionStatus?.broadcast_grid ?? null` (pre-amendment — reads the on-air locator) | **Amended 2026-06-02:** `liveGrid = positionStatus?.ui_grid ?? null` (reads the ribbon display locator). The on-air locator (`broadcast_grid`) remains exposed on the DTO for any future broadcast-grid badge / tooltip surface but is NOT what the ribbon displays. |

### §4.2 — Source segmented control + GPS-ready as in-segment indicator + `Set manually` button (2026-06-02 follow-up — tuxlink-z5pz; supersedes the T11 + T12 patterns)

**Historical note.** The original v3 §4.2 prescribed the T12 conditional chip pattern (`<button onClick={onUseGps}>` when `source = Manual`; `<span role="status">` when `source = Gps`) plus the T11 passive `<span className="dash-gps-ready-status">GPS ready</span>` sibling hint. Both are superseded by the segmented-control pattern below; the T11 + T12 lineage is preserved in [§9 the 2026-06-02 tuxlink-z5pz entry](#9--adversarial-review-disposition).

**The source segmented control.** Per [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern):
- A `<div className="dash-source-segments" role="radiogroup" aria-label="Position source">` wraps two `<button role="radio">` children.
- The GPS segment (`data-testid="source-segment-gps"`) has `aria-checked={source === 'Gps'}`; its `onClick` is `undefined` when `source = Gps` (no-op) and `onUseGps` otherwise.
- The MANUAL segment (`data-testid="source-segment-manual"`) has `aria-checked={source === 'Manual'}`; its `onClick` is `undefined` when `source = Manual` (no-op) and `onUseManual` otherwise.

**The in-segment GPS-ready indicator.** Per [§2.2](#22--gps-ready-indicator-folds-into-the-gps-segment-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t11-standalone-span-pattern): when `source === 'Manual' && gpsReady === true`, the GPS segment's text content is `GPS ●` (a `' ●'` suffix is appended to the segment label). When `source === 'Gps'` or `gpsReady === false`, the GPS segment's text content is `GPS`. The separate `<span data-testid="gps-ready-status">` sibling element is deleted.

**The `Set manually` button.** Per [§2.3](#23--set-manually-button-state-4-and-state-5): the `Set manually` button (`<button aria-controls={gridInputId} onClick={enterEdit}>▸ Set manually</button>`) is rendered when `source === 'Gps' && !gpsReady` (State 4 or State 5). It is preserved as-is from the original v3 design; the segmented control does NOT replace the `Set manually` button (the two affordances coexist — clicking the MANUAL segment from State 3 OR clicking the `Set manually` button from State 4/State 5 both call `enterEdit()`, the operator can use whichever surface is closer to the pointer/focus). Tab order: GPS segment → MANUAL segment → `Set manually` button (when rendered) → grid value. On clicking the `Set manually` button OR the MANUAL segment, `GridEdit.tsx`'s `enterEdit()` handler runs and the newly-mounted grid input receives focus.

### §4.3 — Optimistic update after `config_set_grid` and `position_set_source` (Codex P1 #4)

Dropping `active_source` from `PositionStatusDto` means the source chip's `source` value comes from `useStatusData`'s `config_read` poll (5-second interval). Without an optimistic update, the source chip lags 0–5 seconds behind a successful operator action that changes `source`.

**Optimistic-update spec for `GridEdit.tsx` + `DashboardRibbon.tsx`:**

- After the `invoke('config_set_grid', ...)` call resolves successfully: call `queryClient.invalidateQueries(['config'])` to force an immediate `config_read` refresh. The source chip's `source` value reflects `Manual` within one React render cycle.
- After the `invoke('position_set_source', { source: 'Gps' })` call resolves successfully: call the same `queryClient.invalidateQueries(['config'])` to force an immediate `config_read` refresh. The source chip's `source` value reflects `Gps` within one React render cycle.

**Alternative considered and rejected:** local optimistic source-chip state via `useState` + sync from `config_read` poll. The local-`useState` alternative is rejected because two sources of truth for the source chip's `source` value (local `useState` + on-disk config) risk divergence on error paths (e.g., the backend write fails; the local `useState` value stays `Manual`; the on-disk config still says `Gps`; the source chip renders the wrong `source` value).

**Implementation note:** the `useStatus.ts` `useStatusData` hook is the natural place to expose an `invalidate()` callback that `GridEdit.tsx` and `DashboardRibbon.tsx` each call after a successful write.

### §4.4 — A11y treatment (R2 #6 + R2 #3; updated 2026-06-02 for the segmented control — tuxlink-z5pz)

**Historical note.** The original v3 §4.4 prescribed `aria-pressed={false}` on the Manual `<button>` of the T12 conditional-chip pattern. That treatment was Codex P1-B + R2 #3 + R2 #6's response to the toggle-button semantics of the T12 design. The `aria-pressed` framing is now MOOT because the segmented control uses `aria-checked` (the WAI-ARIA radio-group convention), not `aria-pressed` (the toggle-button convention). The T12 spec-deviation that Codex caught (the missing `aria-pressed=false`) is no longer applicable. The lineage is preserved in [§9](#9--adversarial-review-disposition).

**Segmented-control a11y (the current spec).**

- The source segmented control is a `<div role="radiogroup" aria-label="Position source">`.
- Each segment is a `<button>` with `role="radio"` AND `aria-checked={source === '<this-segment-source>'}`.
- The selected segment is announced by screen readers as "GPS, selected" (or "Manual, selected"); the unselected segment is announced as just "GPS" / "Manual" (or, depending on the screen reader, "GPS, not selected").
- The segment text content carries the in-segment GPS-ready indicator (`' ●'` suffix on the GPS segment when `source = Manual && gpsReady`); the visual dot is read aloud by some screen readers as "bullet" or "dot." The semantic load — "a fresh fix is available; you could switch" — is carried by the visual dot for sighted operators; for screen-reader operators, the `aria-checked` state of the GPS segment already conveys "you can switch to this," so the dot is information-redundant for the screen-reader path (this is acceptable — the dot is a sighted-operator affordance, not the primary signal).
- Keyboard reachability: each segment is a real `<button>`, so `Tab` moves focus between the segments (segments are NOT focus-trapped); `Space` / `Enter` activates the focused segment's `onClick`. The radio-group keyboard convention from WAI-ARIA Authoring Practices (left/right arrow keys move focus between radios, with selection following focus) is OPTIONAL polish for this restoration — if not implemented in the initial PR, file as a P2 follow-up. The Tab-between-segments behavior is sufficient for first-ship a11y.
- Mouse-centric "tap to switch" language remains banned (R2 #6); the segment `aria-label` attributes are NOT needed because each segment's visible text label (`"GPS"` / `"MANUAL"`) is itself the accessible name. The radio-group's `aria-label="Position source"` provides the grouping context.
- The `Set manually` button retains `aria-controls={gridInputId}` (R2 #3) — the grid input's DOM `id` attribute. No change.

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

**No one-time migration code is required.** Per Codex P1 #5: pjih cannot distinguish deliberate pre-pjih `Manual` intent from pjih-window confusion, and resetting would violate the restored source/privacy contract. The pjih-era operators with `Gps` on disk simply use the now-actionable source chip when `source = Manual`; the pre-pjih operators with `Manual` on disk get the same source chip when `source = Manual` + the now-actionable click semantics from [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern).

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

**Add per the 2026-06-02 amendment (tuxlink-va1i — ribbon display vs on-air locator decoupling):**

- `ui_locator_matrix` (proptest, in `src-tauri/src/position/mod.rs` tests): a sibling proptest to `active_grid_matrix`, covering the 5 operator-visible states × `{Off, LocalUiOnly, BroadcastAtPrecision}` (15 cells). For each generated case, asserts `effective_ui_locator(cfg, &arbiter)` returns the value defined by I7 in [§3.4](#34--state-space-invariants-r3-f4--all-36-cells-of-source--fix_state--gps_state--manual_grid_set). Specifically: under `source = Gps && fresh fix && gps_state ∈ {LocalUiOnly, BroadcastAtPrecision}` → precision-reduced live fix grid; under `Off` → `config.identity.grid` fallback; under `source = Manual` → `manual_grid` (precision-reduced) regardless of `gps_state`.

- `ui_locator_diverges_from_broadcast_locator_under_localui_only_with_fresh_fix` (the operator-reported regression test, in `src-tauri/src/position/mod.rs` tests): set `cfg.privacy.gps_state = LocalUiOnly`, `cfg.privacy.position_source = Gps`, `cfg.privacy.position_precision = SixCharGrid`, `cfg.identity.grid = "DM33"`; set `arbiter.apply_gps_fix(Fix::test_at_grid("DM33ww"))` so `arbiter.has_fresh_fix() == true`. Assert `effective_ui_locator(cfg, &arbiter) == "DM33ww"` (precision-reduced live fix — the operator can see GPS locally). Assert `effective_broadcast_locator(cfg, &arbiter) == "DM33"` (static config fallback — privacy honored, since `LocalUiOnly` chooses the static fallback for on-air). The two helpers INTENTIONALLY differ in this cell — assert both in one test so the divergence is the test's load-bearing claim.

- `gps_ready_false_under_off` (in `src-tauri/src/ui_commands.rs` tests): set `arbiter.apply_gps_fix(Fix::test_at_grid("DM33ww"))` so `arbiter.has_fresh_fix() == true`; set `cfg.privacy.gps_state = Off`; invoke the `position_status` command; assert the returned `PositionStatusDto.gps_ready == false`. Pins the Codex 2026-06-01 adjacent fix: `gps_ready` should be `false` when the operator chose `Off`, even if the arbiter still holds a stale fix from a prior `BroadcastAtPrecision` / `LocalUiOnly` session.

- `position_status_dto_serializes_ui_grid_and_broadcast_grid_when_they_differ` (in `src-tauri/src/ui_commands.rs` tests): drive `position_status` with the same operator-reported-regression cell (`Gps + LocalUiOnly + fresh fix + DM33 + SixCharGrid`); assert the serialized DTO JSON contains BOTH `"ui_grid":"DM33ww"` AND `"broadcast_grid":"DM33"` in snake_case. Pins the DTO contract that both fields are present and serialize independently.

**Remove (the tests no longer apply post-restore):**

- The five pjih-era arbiter tests: `set_manual_updates_grid_without_changing_stored_source`, `fresh_gps_fix_wins_over_manual_grid_regardless_of_stored_source`, `manual_grid_used_when_gps_fix_is_stale_or_absent`, `arbiter_set_manual_updates_grid_without_changing_stored_source` (in `ui_commands::tests`), and the `position_status_dto_*` `active_source` assertions in `ui_commands::tests`.

### §6.2 — Frontend tests (vitest; updated 2026-06-02 for the segmented control — tuxlink-z5pz)

**The segmented-control amendment renames + adds + removes tests.** The original v3 tests asserted against `data-testid="source-chip"` (the T12 single-chip pattern) and `data-testid="gps-ready-status"` (the T11 sibling-hint span). With the radio-group from [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern), tests address `source-segment-gps` + `source-segment-manual` instead, and the standalone gps-ready-status testid is gone.

**Restore from pre-pjih (T11 lineage), now folded into the GPS segment:**

- `shows in-segment GPS-ready indicator when a fix is available while Manual` in `GridEdit.test.tsx` — REPLACES the T11 `shows GPS-ready hint when a fix is available while Manual` test. Asserts the GPS segment's text content includes `' ●'` (the in-segment indicator) when `source = 'Manual' && gpsReady === true`. The standalone `gps-ready-status` testid is REMOVED — assertions move to the GPS segment's text content. Per [§2.2](#22--gps-ready-indicator-folds-into-the-gps-segment-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t11-standalone-span-pattern).

**Segmented-control tests (new — per [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern) and tuxlink-z5pz):**

- `radiogroup_has_role_and_aria_label` (in `GridEdit.test.tsx`): asserts the segmented control's container element has `role="radiogroup"` AND `aria-label="Position source"`. Pins the WAI-ARIA grouping convention.

- `gps_segment_is_selected_when_source_is_Gps` (in `GridEdit.test.tsx`): renders with `source = 'Gps'`; asserts `getByTestId('source-segment-gps')` has `aria-checked="true"` AND `getByTestId('source-segment-manual')` has `aria-checked="false"`.

- `manual_segment_is_selected_when_source_is_Manual` (in `GridEdit.test.tsx`): renders with `source = 'Manual'`; asserts `getByTestId('source-segment-manual')` has `aria-checked="true"` AND `getByTestId('source-segment-gps')` has `aria-checked="false"`.

- `clicking_GPS_segment_when_source_is_Manual_fires_onUseGps` (in `GridEdit.test.tsx`): renders with `source = 'Manual'`; fires a click on `getByTestId('source-segment-gps')`; asserts the `onUseGps` mock was called.

- `clicking_MANUAL_segment_when_source_is_Gps_fires_onUseManual_and_enters_edit_mode` (in `GridEdit.test.tsx`): renders with `source = 'Gps'`; fires a click on `getByTestId('source-segment-manual')`; asserts the `onUseManual` mock was called; asserts the grid input mounts and receives focus (`document.activeElement === gridInput`). Pins the MANUAL-segment-click → `enterEdit()` flow from [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern).

- `clicking_already_selected_GPS_segment_is_a_noop` (in `GridEdit.test.tsx`): renders with `source = 'Gps'`; fires a click on `getByTestId('source-segment-gps')`; asserts NEITHER `onUseGps` NOR `onUseManual` mock was called.

- `clicking_already_selected_MANUAL_segment_is_a_noop` (in `GridEdit.test.tsx`): renders with `source = 'Manual'`; fires a click on `getByTestId('source-segment-manual')`; asserts NEITHER mock was called.

- `in_segment_gps_ready_indicator_renders_dot_on_GPS_segment_when_source_is_Manual_and_gpsReady` (in `GridEdit.test.tsx`): renders with `source = 'Manual' && gpsReady === true`; asserts `getByTestId('source-segment-gps').textContent === 'GPS ●'`. REPLACES the T11 `gps-ready-status` testid assertion.

- `in_segment_gps_ready_indicator_is_absent_when_gpsReady_is_false` (in `GridEdit.test.tsx`): renders with `source = 'Manual' && gpsReady === false`; asserts `getByTestId('source-segment-gps').textContent === 'GPS'` (no `' ●'` suffix).

- `in_segment_gps_ready_indicator_is_absent_when_source_is_Gps_even_if_gpsReady` (in `GridEdit.test.tsx`): renders with `source = 'Gps' && gpsReady === true`; asserts `getByTestId('source-segment-gps').textContent === 'GPS'` (no `' ●'` suffix — the dot is redundant when the segment is selected).

**`Set manually` button tests (preserved from v3 — no segmented-control impact):**

- `set_manually_button_is_present_in_State_4` (in `GridEdit.test.tsx`, per R4 P1 #5): asserts the `Set manually` button is rendered when `source = 'Gps' && gpsReady = false`.

- `set_manually_button_is_absent_in_State_1` (in `GridEdit.test.tsx`, per R4 P1 #5): asserts the `Set manually` button is NOT rendered when `source = 'Manual' && gpsReady = false`.

- `set_manually_button_is_absent_in_State_3` (in `GridEdit.test.tsx`, per R4 P1 #5): asserts the `Set manually` button is NOT rendered when `source = 'Gps' && gpsReady = true`.

- `set_manually_button_is_absent_in_State_2` (in `GridEdit.test.tsx`, per R4 P1 #5 — closes the 4-quadrant matrix): asserts the `Set manually` button is NOT rendered when `source = 'Manual' && gpsReady = true`.

- `set_manually_button_focuses_the_grid_input_on_click` (in `GridEdit.test.tsx`, per Codex P2 #6): asserts `document.activeElement === gridInput` after clicking the `Set manually` button.

**Optimistic-refresh tests (preserved from v3 — assertions retargeted to the segments):**

- `ribbon_source_chip_stays_Manual_when_config_says_Manual_even_if_gpsReady_is_true` (in `useStatus.test.ts` hook test, per Codex P1 #3): mocks `config_read.position_source = 'Manual'` and `position_status.gps_ready = true`; asserts `result.current.position_source === 'Manual'`. Pins the source-segment-selection-comes-from-config-not-from-arbiter invariant. (Hook-level test — unaffected by the DOM rename.)

- `MANUAL_segment_becomes_selected_within_one_render_cycle_after_config_set_grid_resolves` (in `DashboardRibbon.test.tsx`, per Codex P1 #4): mocks `invoke('config_set_grid')` to resolve; asserts the MANUAL segment's `aria-checked` flips from `"false"` to `"true"` within one render cycle, NOT after the 5-second `config_read` poll. Verifies the optimistic-refresh path from [§4.3](#43--optimistic-update-after-config_set_grid-and-position_set_source-codex-p1-4). RENAMED from `source_chip_flips_to_Manual...`.

- `GPS_segment_becomes_selected_within_one_render_cycle_after_position_set_source_resolves` (in `DashboardRibbon.test.tsx`, per Codex P1 #4): mocks `invoke('position_set_source')` to resolve; asserts the GPS segment's `aria-checked` flips to `"true"` within one render cycle. RENAMED from `source_chip_flips_to_Gps...`.

- `State_4_grid_value_has_interpunct_prefix_and_GPS_segment_is_dimmed` (in `GridEdit.test.tsx`, per R2 #4): mocks `source = 'Gps' && gpsReady = false && grid = 'CN87'`; asserts presence of the `· ` interpunct prefix on the grid value AND that `getByTestId('source-segment-gps')` has the `.dash-source-segment.gps.selected:not(.gps-ready)` class chain (i.e., classes `selected` AND NOT `gps-ready`). Pins the State 4 vs State 1 differentiation from [§2.4](#24--state-1-vs-state-4-visual-differentiation-r2-4--the-failure-mode-pjih-shipped-updated-2026-06-02-for-the-segmented-control). RENAMED from `..._chip_is_dimmed`.

**Remove (T11 + T12 tests that no longer apply post-segmented-control):**

- `no Use-GPS affordance is rendered (tuxlink-pjih)` in `GridEdit.test.tsx` — pjih-era absence-assertion (carries over from original §6.2 removal).
- Any test asserting `data-testid="gps-ready-status"` exists as a sibling element — REMOVED. The testid no longer exists; assertions move to the GPS segment's `textContent`.
- The T12 `source_chip_is_a_button_when_source_is_Manual_and_calls_onUseGps_on_click` test — REPLACED by `clicking_GPS_segment_when_source_is_Manual_fires_onUseGps` above.
- The T12 `source_chip_is_a_span_when_source_is_Gps_with_no_click_handler` test — REPLACED by `clicking_already_selected_GPS_segment_is_a_noop` above.
- The T12 `source_chip_has_aria_pressed_false_when_source_is_Manual` regression test (if present from Codex P1-B / R2 #6) — REMOVED. The `aria-pressed` attribute no longer applies; the equivalent assertion is `aria-checked="false"` on the GPS segment when `source = Manual`, which the `gps_segment_is_selected_when_source_is_Gps` + `manual_segment_is_selected_when_source_is_Manual` tests above already cover symmetrically.

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

1. **Sticky Manual against an arriving GPS fix (GPS present).** Start with gpsd running. Inline-edit the grid value to `EM75`. Assert the MANUAL segment is selected (`aria-checked="true"`, filled, amber) and the GPS segment is outlined. Wait for gpsd to publish a fresh fix to `DM33`. Assert the grid value STAYS `EM75` (sticky Manual). Assert the MANUAL segment STAYS selected. Assert the GPS segment now shows the in-segment `' ●'` indicator (`GPS ●` textContent).

2. **GPS-segment escape from Manual (GPS present).** From step 1, click the **GPS segment** in the source segmented control. Assert the GPS segment flips to selected (filled, green, locked); the MANUAL segment flips to outlined. Assert the grid value flips to the live GPS fix `DM33`.

3. **GPS-segment escape from Manual (no GPS — the case [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation) exists to fix).** Stop gpsd (`sudo systemctl stop gpsd`). Inline-edit the grid value to `EM75`. Assert State 1 (the MANUAL segment is selected, the GPS segment is outlined, the grid value is `EM75`). Click the **GPS segment** in the source segmented control. Assert State 4 (the GPS segment is selected but **dimmed** via `.dash-source-segment.gps.selected:not(.gps-ready)`, the MANUAL segment is outlined, the grid value is `· EM75` with interpunct prefix, the status text reads `"GPS no fix · broadcasting fallback"`, and the `Set manually` button is present). Assert the operator is no longer stuck.

4. **`Set manually` button from State 4.** From step 3, click the `Set manually` button. Assert the grid input mounts and receives focus and is editable. Type `DM33` and press Enter. Assert State 1 (MANUAL segment selected; grid value `DM33`).

5. **MANUAL-segment click from State 3 enters edit mode.** From a clean State 3 (gpsd running, `source = Gps`, fresh fix), click the **MANUAL segment** in the source segmented control. Assert the grid input mounts and receives focus (`document.activeElement === gridInput`) — this is the same `enterEdit()` affordance the `Set manually` button provides, surfaced via the segmented control for State 3 discoverability. Type `EM75` and press Enter. Assert the source flips to Manual (MANUAL segment selected, grid value `EM75`). This step verifies the MANUAL-segment-click → `onUseManual` → `enterEdit()` flow that addresses the tuxlink-z5pz discoverability complaint.

6. **GPS happy path (State 3).** Restart gpsd (`sudo systemctl start gpsd`). Click the **GPS segment** to return to `source = Gps`. Assert the grid value tracks the live fix. Assert State 3 (GPS segment selected, filled, green, bright; MANUAL segment outlined).

7. **Privacy gate intact under `gps_state = LocalUiOnly`.** Set `cfg.privacy.gps_state = LocalUiOnly`. Confirm the on-air locator (via the CMS-exchange operator smoke from PR #185) falls back to `cfg.identity.grid`, NOT the live GPS fix.

8. **(NEW 2026-06-02 — tuxlink-va1i regression smoke) LocalUiOnly + GPS-acquired position display.** Set `cfg.privacy.gps_state = LocalUiOnly`, `cfg.privacy.position_source = Gps`, `cfg.privacy.position_precision = SixCharGrid`, `cfg.identity.grid = "DM33"`. Restart gpsd and confirm it is producing TPV mode=3 fixes at the operator's location (verify via `gpspipe -w | head -5`). Restart the tuxlink dev shell so the new config is picked up. Confirm the ribbon Grid cell displays the **live precision-reduced fix grid** (e.g. `"DM33ww"`), NOT the static config grid (`"DM33"`). Then confirm the **on-air locator** (verified via the CMS-exchange operator smoke from PR #185 OR via B2F frame inspection) IS the static `"DM33"` — privacy gate honored, on-air does not leak the live fix. The two values INTENTIONALLY differ in this cell: ribbon = live, on-air = static fallback. This step is the operator-facing analog of the `ui_locator_diverges_from_broadcast_locator_under_localui_only_with_fresh_fix` backend test from [§6.1](#61--backend-tests-cargo---lib) — both must pass before declaring the va1i regression closed.

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
| R1 | Codex (GPT-5) | 6 (1 P0, 4 P1, 1 P2) | All P0 + P1 applied. Codex P0 #1 (the `use_gps` relaxation extends to the `position_set_source` command) → [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation) + [§3.1](#31--backend-revert--relaxation-table) + [§6.1 new test](#61--backend-tests-cargo---lib). Codex P1 #2 (source chip when `source = Gps` is status-only) → [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern). Codex P1 #3 (cross-layer source-sequence tests) → [§6.1 added 4 tests](#61--backend-tests-cargo---lib). Codex P1 #4 (optimistic update) → [§4.3](#43--optimistic-update-after-config_set_grid-and-position_set_source-codex-p1-4). Codex P1 #5 (§5 migration text wrong) → [§5 rewritten](#5--migration-rewritten-per-codex-p1-5--r3-f8). Codex P2 #6 (focus contract) → [§2.3](#23--set-manually-button-state-4-and-state-5) + [§6.2 strengthened](#62--frontend-tests-vitest-updated-2026-06-02-for-the-segmented-control--tuxlink-z5pz). |
| R2 | Claude (UX) | 8 (5 P1, 3 P2) | All P1 applied. R2 #1 (source-chip click semantics justification) → [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch) + [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern). R2 #2 (source chip + GPS-ready button redundancy) → [§2.2](#22--gps-ready-indicator-folds-into-the-gps-segment-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t11-standalone-span-pattern) + [§4.2](#42--source-segmented-control--gps-ready-as-in-segment-indicator--set-manually-button-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t11--t12-patterns). R2 #3 (`Set manually` button a11y) → [§2.3](#23--set-manually-button-state-4-and-state-5). R2 #4 (State 1 vs State 4 visual differentiation) → [§2.4](#24--state-1-vs-state-4-visual-differentiation-r2-4--the-failure-mode-pjih-shipped-updated-2026-06-02-for-the-segmented-control) + [§6.2 new test](#62--frontend-tests-vitest-updated-2026-06-02-for-the-segmented-control--tuxlink-z5pz). R2 #5 (operator smoke missing no-GPS case) → [§6.4 step 3](#64--operator-smoke-rewritten-per-r2-5--no-gps-hardware-case). R2 P2 #6 (mouse-centric "tap" language) → [§4.4](#44--a11y-treatment-r2-6--r2-3-updated-2026-06-02-for-the-segmented-control--tuxlink-z5pz). R2 P2 #7 (subsumed by source-chip-as-span when `source = Gps`). R2 P2 #8 (width budget) — deferred to implementation polish. |
| R3 | Claude (contract/races) | 10 (1 P0, 3 P1, 6 P2) | All P0 + P1 applied. R3 F2 P0 (privacy-gate clarity for State 4) → [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation) + [§2.5](#25--ribbon-display-locator-vs-on-air-locator-intentionally-distinct-concerns) (note: §2.5's earlier framing was further corrected by the 2026-06-02 follow-up below). R3 F1 + F7 P1 (concurrency invariants) → [§3.3](#33--concurrency-invariants-for-config_set_grid-and-position_set_source-r3-f1--r3-f7). R3 F8 P1 (§5 migration narrative wrong) → [§5 rewritten](#5--migration-rewritten-per-codex-p1-5--r3-f8). R3 F4 P1 (state-space) → [§3.4](#34--state-space-invariants-r3-f4--all-36-cells-of-source--fix_state--gps_state--manual_grid_set) + [§6.1 matrix tests](#61--backend-tests-cargo---lib). R3 P2 — implementation-detail; tracked for the plan. |
| R4 | Claude (tests) | 11 (2 P0, 4 P1, 5 P2) | All P0 + P1 applied. R4 P0 #1 (temporal sticky test) → [§6.1 sticky test extended](#61--backend-tests-cargo---lib). R4 P0 #2 (`use_gps` `active_grid` assertion) → [§6.1 new test](#61--backend-tests-cargo---lib). R4 P1 #3 (source-chip-element-type test) → [§6.2 added](#62--frontend-tests-vitest-updated-2026-06-02-for-the-segmented-control--tuxlink-z5pz). R4 P1 #4 (composed flow) → [§6.3 integration test](#63--cross-layer-integration-test-r4-p1-4--r5-7--the-test-class-pjih-violated). R4 P1 #5 (4-quadrant `Set manually` matrix) → [§6.2 expanded](#62--frontend-tests-vitest-updated-2026-06-02-for-the-segmented-control--tuxlink-z5pz). R4 P1 #6 (3 missing backend invariants) → [§6.1 added](#61--backend-tests-cargo---lib). R4 P2 — partially applied to §6 strengthening. |
| R5 | Claude (holistic) | 12 (2 P0, 5 P1, 5 P2) | All P0 + P1 applied. R5 P0 #1 ("one amendment" understated) → [§1.1 the relaxation](#11--the-use_gps--position_set_source-gps-relaxation) (full extent) + [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch). R5 P0 #2 (alternatives not compared) → [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch). R5 P1 #3 (source-chip click semantics) → [§2.1](#21--source-segmented-control-dom-and-click-semantics-2026-06-02-follow-up--tuxlink-z5pz-supersedes-the-t12-conditional-chip-pattern) + [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch). R5 P1 #4 ("GPS by default" axiomatic) — operator + 2026-05-22 spec confirmed; documented as a decision in [§1.2](#12--why-the-use_gps-relaxation-over-clear-manual-pin-or-confirm-then-switch). R5 P1 #5 (why pjih undetected) → [§7](#7--why-pjih-landed-undetected-r5-5-hypothesis). R5 P1 #6 (Track B interactions) → [§8](#8--track-b-interactions). R5 P1 #7 (cross-layer test class) → [§6.3 integration test](#63--cross-layer-integration-test-r4-p1-4--r5-7--the-test-class-pjih-violated). R5 P2 — partially applied. |

**Total: 47 findings; 6 P0 + 21 P1 = 27 must-apply, all applied. P2: 20, selectively applied per cost/value.**

### 2026-06-02 follow-up consultation (tuxlink-va1i — ribbon display vs on-air locator decoupling)

A cross-provider Codex consultation on the operator-reported regression `tuxlink-va1i` (ribbon shows `config_grid` under `source = Gps + gps_state = LocalUiOnly + fresh fix` instead of the live precision-reduced fix grid) confirmed the bug and endorsed the corrective design: **split `effective_ui_locator` from `effective_broadcast_locator`** rather than (a) parameterize the existing helper with a `Purpose::{Ui, OnAir}` enum or (b) collapse harder and make the on-air helper lie. Two named helpers > one helper with narrowed semantics > one helper with a purpose enum, because each call site becomes self-documenting (on-air uses the broadcast helper; ribbon uses the UI helper) and the call-site contract resists misuse.

The original adrev's **Codex P1-B** finding ("operator should see what they'd transmit") correctly identified the concern that operator awareness of the on-air locator MATTERS. The Codex P1-B finding's chosen implementation (collapse ribbon display onto `effective_broadcast_locator`) was a UX-cost mismatch for the `LocalUiOnly` case — `LocalUiOnly` literally means "see GPS locally, don't broadcast," and collapsing the helpers broke the local-display half of that intent. The Codex P1-B concern is preserved as a partial truth in the amended [§2.5](#25--ribbon-display-locator-vs-on-air-locator-intentionally-distinct-concerns): operator-facing visibility of the literal on-air locator is a legitimate concern, but the correct UX surface is a separate broadcast-grid badge / tooltip / settings affordance (out of scope for this restoration; tracked as a follow-up if the operator surfaces it as a wanted feature). The amendment does NOT delete the Codex P1-B lineage — the finding is retained as a documented partial truth that informed a corrective design rather than as a deleted historical mistake.

The amendment also surfaced one **adjacent fix** that Codex flagged during the consultation: `gps_ready` in `PositionStatusDto` is currently `arbiter.has_fresh_fix()` regardless of `cfg.privacy.gps_state`. If the operator switches `gps_state` to `Off` mid-session, the gpsd-client task is not killed and fresh fixes keep arriving, so `gps_ready` continues to report `true` even though the operator chose "no GPS." The amendment to [§3.1](#31--backend-revert--relaxation-table) tightens this: `gps_ready` is now `arbiter.has_fresh_fix() && cfg.privacy.gps_state != GpsState::Off`. The `gps_ready_false_under_off` test in [§6.1](#61--backend-tests-cargo---lib) pins this.

**Sections amended for this 2026-06-02 follow-up:** Vocabulary, §2.5, §3.1, §3.2, §3.4, §4.1, §6.1, §6.4, §9 (this entry).

### 2026-06-02 follow-up — operator-reported UX regression (tuxlink-z5pz)

After PR #233 (tuxlink-va1i) merged and gpsd integration was confirmed working, the operator reported that the single-chip toggle pattern (T12: `<button>` when `source = Manual`, `<span role="status">` when `source = Gps`; click-current-state to switch) is undiscoverable — *"no human would think to click MANUAL to switch to GPS."* The T12 design is ARIA-correct (it uses `aria-pressed={false}` per the toggle-button convention from Codex P1-B + R2 #3 + R2 #6) but visually reads as a status badge, not a switch surface. The T11 `● GPS ready` sibling-hint span (replacing the pre-pjih `<button data-testid="use-gps">`) only renders under `Manual + fresh fix`; under `Manual + no fresh fix`, there is no visual cue that GPS is even an available option. The operator with no fresh fix sees a single `MANUAL` chip with no neighboring switch affordance, and is stuck without a discoverable path to GPS.

This amendment replaces the T12 toggle pattern with a standard **radio-group segmented control** where both options are always visible, both are clickable, and selection is unambiguous (selected segment is filled; the other is outlined). The control is a `<div role="radiogroup" aria-label="Position source">` containing two `<button role="radio" aria-checked={...}>` children — the GPS segment and the MANUAL segment. The T11 `● GPS ready` sibling-hint span folds INTO the GPS segment as a `' ●'` text suffix when `source = Manual && gpsReady` (the visual cue is preserved; the DOM is simpler). The T12 `aria-pressed/Codex P1-B/R2 #6` lineage is preserved in §9 history but the implementation is superseded by the radiogroup pattern (the `aria-pressed` attribute no longer applies — the segments use `aria-checked` per the WAI-ARIA radio-group convention).

The discoverability fix is structural: an operator looking at the source surface sees `[ GPS ] [ MANUAL ]` side-by-side at all times. The selected segment shows the current source; the other segment is the "switch to me" affordance. No mental model leap ("click the current state to switch") is required.

**No backend changes.** The GPS-segment click reuses the existing `position_set_source('Gps')` command (same path the T12 chip-button used). The MANUAL-segment click reuses the existing T4-restored path: `enterEdit()` opens the grid input, the operator types their grid, Enter fires `config_set_grid(grid)` which atomically persists `cfg.privacy.position_source = Manual` AND the new grid value (per the T4 spec). The MANUAL-segment click does NOT call `position_set_source('Manual')` (whose `'Manual'` arm returns `UiError::Rejected` by existing-spec design — the operator-only path INTO Manual is via the `config_set_grid` command with a grid value, not via the bare-source-token command).

**Sections amended for this tuxlink-z5pz follow-up:** Vocabulary, §2.1, §2.2, §2.4, §2.6 ASCII mockups, §3.1 (note about no new backend), §3.4 (visual-rendering note table), §4.1, §4.2, §4.4, §6.2, §6.4, §9 (this entry). Reference: bd issue `tuxlink-z5pz`.

---

## Appendix A — Reference: the 2026-05-22 spec

See [`docs/superpowers/specs/2026-05-22-position-subsystem-design.md`](2026-05-22-position-subsystem-design.md) for the authoritative source contract, the gpsd-client design, the manual-entry approach (Approach A), the source-chip spec (Approach A), and the full ribbon-states table.

The position-subsystem restoration spec extends — does not supersede — the 2026-05-22 spec.
