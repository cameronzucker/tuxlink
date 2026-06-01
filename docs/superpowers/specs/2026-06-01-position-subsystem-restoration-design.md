# Position Subsystem — Restoration After pjih

- **Date:** 2026-06-01
- **bd issue:** tuxlink-c79g (closes); references tuxlink-pjih, PR #189 (reverts)
- **Status:** Brainstorm in operator review
- **Authors:** bison-condor-grouse + operator
- **Amends:** [`docs/superpowers/specs/2026-05-22-position-subsystem-design.md`](2026-05-22-position-subsystem-design.md)

## TL;DR

The 2026-05-22 position-subsystem design is correct as-written; pjih (PR #189, merged `a6db716`) violated the design contract on the assumption that the operator wanted a new semantic. Operator confirmed 2026-06-01: *"The original spec was fine. We had it working for a while. Each fix was only to address regression."*

This doc covers only:

1. **Revert** the pjih backend + frontend changes (restore the 2026-05-22 source contract).
2. **Close two pre-pjih implementation gaps** that the original spec required but were never coded — and which probably motivated the original "GPS regression" complaint that pjih over-fixed.
3. **One spec amendment** to make the original spec actually escapable from the Manual state when the operator has no working GPS hardware.

The 2026-05-22 spec remains the authoritative source contract. Everything below either points back to it or extends it for the gap-closures.

---

## Motivation

The 2026-05-22 spec defined an explicit operator-owned source contract: `Manual` is sticky, GPS never overrides until the operator explicitly switches via the source chip. The implementation that shipped (tuxlink-686) honored the sticky-Manual backend semantics but **left two spec lines unimplemented on the frontend**:

- **Spec §4 ("Source chip"), line 102:** *"Clicking the chip switches source explicitly."* — The chip was rendered as a non-interactive `<span>`.
- **Spec §"Ribbon states (Grid cell)", row 3:** *"Gps + none usable → `CN87 · GPS no fix` (fallback to last grid) + obvious 'set manually'."* — No "set manually" affordance was ever added; the row 3 visual state is empty.

Without the chip-as-switcher and without the row-3 affordance, an operator who engaged Manual (intentionally or accidentally) and didn't have a live GPS fix had no in-UI way back to Gps mode. The conditional "● GPS ready — tap to switch" hint only renders when `source === 'Manual' && gpsReady` — gated on a gpsd fix, which operators without GPS hardware never see.

PR #189 (pjih) attempted to fix this by deleting the Manual semantic entirely: `set_manual` no longer pinned source, the chip read a derived `effective_source` from arbiter state, and the Use-GPS button was removed as "structurally unreachable." That decision violated the design contract — and the operator hit the consequence on 2026-06-01:

> "GPS is now fully broken. It doesn't switch to manual entry with 'gps available' it just says GPS green at all times and accepts/displays whatever input. That's major regression."

The fix is to revert pjih and close the original spec-implementation gaps that triggered the original complaint pjih was over-reacting to.

---

## Scope

In scope:

- Revert PR #189's backend + frontend code (precise list in §3 + §4 below).
- Restore the 2026-05-22 source contract — see [§"The source contract"](2026-05-22-position-subsystem-design.md#the-source-contract) of the prior spec.
- Make the source chip clickable (close spec gap §4 line 102).
- Add a "Set manually" affordance for the Gps + no-fix display state (close spec gap row 3).
- One spec amendment: relax `use_gps()` so the source switch succeeds without a current fresh fix.

Out of scope (handled in separate tracks):

- Settings → GPS+Privacy panel cleanup (tuxlink-jmfm, Track B).
- ARDOP panel widening (tuxlink-8rng, Track B).
- Visual redesign of the chip beyond "make it interactive."
- Any change to the gps_state privacy gate or precision reduction.
- Any change to the gpsd client or fix-quality gate.

---

## §1 — Source contract (RESTORED + one amendment)

Restored verbatim from the 2026-05-22 spec ([§"The source contract"](2026-05-22-position-subsystem-design.md#the-source-contract)):

| Rule | Behavior |
|---|---|
| Explicit source | `source ∈ {Manual, Gps}`. There is no implicit/auto source. |
| Manual is sticky | Setting a grid by hand pins `source = Manual`. GPS **never** overrides a manual position. |
| Operator-only switching | Returning to GPS is a deliberate act (the source chip), never automatic. |
| Precision is source-independent | Broadcast precision is enforced on whatever source is active. |
| Source is always visible | The ribbon labels the active source; a change is never invisible. |

**Amendment** (new in this restoration):

> The original spec's `use_gps()` *"requires a usable recent fix; otherwise reports 'no fix' and leaves the prior position visible."* This made the `Gps + no fix` state (row 3 of the spec table) unreachable from `Manual` for operators without GPS hardware — the root recoverability gap.
>
> **Restored amended:** `use_gps()` succeeds unconditionally. It flips `source = Gps`. The display then renders per spec row 3 (`CN87 · GPS no fix (fallback to last grid) + obvious 'set manually'`). If a fresh fix later arrives, the display upgrades to row 2 (`CN87` live + `GPS` chip). The destination state was always in the spec; this just makes it reachable as a real destination, not only as an initial-state quirk.

---

## §2 — UI surface (chip clickable + row-3 affordance)

### Chip-click semantics

Default picked (Codex adrev will refine if a stronger reading emerges):

| Operator action | Current source | Behavior |
|---|---|---|
| Click chip | `Manual` | Call `position_set_source('Gps')`. With the spec amendment above, this succeeds unconditionally. |
| Click chip | `Gps` | No-op. (The path INTO Manual is the inline-edit on the grid value — per spec.) |
| Click grid value | `Manual` or `Gps` | Inline-edit. Enter commits → `config_set_grid` pins `source = Manual`. |

### Chip + row-3 states (ASCII mockup)

State 1 — `source = Manual`, no fresh fix (default Manual operator):
```
┌────────────────────────────────────────────┐
│ Grid │  [ CN87 ]   ┌──────┐                │
│      │             │MANUAL│ ← clickable    │
│      │             └──────┘   (→ Gps)      │
└────────────────────────────────────────────┘
```

State 2 — `source = Manual`, fresh fix exists (the spec's "GPS ready" hint case):
```
┌──────────────────────────────────────────────────────────────────┐
│ Grid │  [ CN87 ]   ┌──────┐   ● GPS ready — tap to switch         │
│      │             │MANUAL│                                       │
│      │             └──────┘                                       │
└──────────────────────────────────────────────────────────────────┘
```
(The "GPS ready" hint button stays. The chip is also clickable now — redundancy is harmless; both call `position_set_source('Gps')`.)

State 3 — `source = Gps`, fresh fix exists (the "GPS happy path"):
```
┌────────────────────────────────────────────┐
│ Grid │  [ DM33 ]   ┌──────┐                │
│      │             │ GPS  │ ← green-locked │
│      │             └──────┘   (no-op click)│
└────────────────────────────────────────────┘
```

State 4 — `source = Gps`, no fresh fix (spec row 3 — the new reachable state):
```
┌──────────────────────────────────────────────────────────────────┐
│ Grid │  [ CN87 ]   ┌──────┐   GPS — no fix   [ Set manually ]    │
│      │  (fallback) │ GPS  │   (subtle)        ← new affordance   │
│      │             └──────┘                                      │
└──────────────────────────────────────────────────────────────────┘
```
"Set manually" is a button that focuses the grid input (same as clicking the value). It does NOT auto-pin Manual; the operator types + presses Enter as usual to commit.

---

## §3 — Backend changes

### Revert

| Symbol | Pre-pjih state | pjih state (current `main`) | Restoration |
|---|---|---|---|
| `arbiter.active_grid` | `Manual → manual_grid`; `Gps → fresh fix else manual_grid fallback` | GPS-fresh always wins regardless of source | Restore source-gated behavior |
| `arbiter.set_manual` | Pins `source = Manual` | No source change | Restore source-pinning |
| `arbiter.effective_source` | (did not exist) | Returns `Gps` when fresh fix exists | **Remove entirely** |
| `arbiter.use_gps` | Required fresh fix | Required fresh fix | **Relax** (see §1 amendment) — no fresh-fix gate |
| `config_set_grid` command | Persisted `cfg.privacy.position_source = Manual` | Does not touch `position_source` | Restore the persistence |
| `PositionStatusDto` | `{ gps_ready, broadcast_grid }` | `{ gps_ready, broadcast_grid, active_source }` | **Remove `active_source`** |
| `position_status` command | (matches DTO above) | Populates `active_source` from `arbiter.effective_source()` | Drop the `active_source` population |

### Keep as-is (no change from current `main`)

- `effective_broadcast_locator` already keys the privacy gate on `a.source()` (the stored preference) — that's the correct behavior post-restore.
- `position_set_source` command shape stays; only its `use_gps()` callee changes per §1 amendment.
- All gpsd-client code (`crate::position::gpsd`) — unaffected.
- Precision-reduction helper (`broadcast_grid` in `config.rs`) — unaffected.

### Test changes

Restore (cargo `--lib`, `native_mailbox::tests`, `position::arbiter::tests`, `ui_commands::tests`):

- `set_manual_pins_source_and_is_sticky_against_gps` — restore as-is.
- `gps_fix_updates_active_only_when_source_is_gps` — restore as-is.
- `arbiter_set_manual_pins_manual_source` in `ui_commands::tests` — restore as-is.

Remove (no longer apply post-restore):

- `set_manual_updates_grid_without_changing_stored_source` (pjih test).
- `fresh_gps_fix_wins_over_manual_grid_regardless_of_stored_source` (pjih test).
- `manual_grid_used_when_gps_fix_is_stale_or_absent` (pjih test).
- `arbiter_set_manual_updates_grid_without_changing_stored_source` in `ui_commands::tests` (pjih test).
- `position_status_dto_*` `active_source` assertions in `ui_commands::tests` (pjih additions).

Amend (existing test that asserts use_gps strictness):

- `use_gps_requires_a_usable_fix` in `position::arbiter::tests` — rename + invert to `use_gps_succeeds_unconditionally`, asserting source flips to Gps regardless of fix availability.

---

## §4 — Frontend changes

### Revert

| Surface | pjih state (current `main`) | Restoration |
|---|---|---|
| `DashboardRibbon.tsx` | `<GridEdit ... onCommit={...} />` — no `onUseGps` prop | Restore `onUseGps={() => invoke('position_set_source', { source: 'Gps' })}` |
| `GridEdit.tsx` props | No `onUseGps` | Restore `onUseGps: () => void` in `GridEditProps` |
| `GridEdit.tsx` button | "GPS ready — tap to switch" removed | Restore the conditional `<button data-testid="use-gps">` |
| `useStatus.ts` `PositionStatusDto` | `{ gps_ready, broadcast_grid, active_source }` | Remove `active_source` field |
| `useStatus.ts` `useStatusData` | `position_source: positionStatus?.active_source ?? config?.position_source ?? 'Gps'` | Restore `position_source: config?.position_source ?? 'Gps'` |

### Add (closes spec gaps)

- **Source chip becomes a clickable button** (spec §4 line 102). The existing `<span className="dash-source-chip ...">` becomes a `<button>` (or a `<span role="button" tabIndex={0}>` if there's a CSS-cascading reason; default to `<button>`). `onClick` handler:
  - When `source === 'Manual'`: call `onUseGps()`.
  - When `source === 'Gps'`: no-op (matches §2 click-semantics).
- **"Set manually" affordance for the Gps + no-fix state** (spec row 3). Renders when `source === 'Gps' && !gpsReady`. Clicking it focuses the grid input (same path as clicking the value). Does NOT auto-pin Manual.
- **Subtle "GPS — no fix" status text** for the same state. Renders alongside the chip.

### Test changes

Restore (vitest):

- `shows GPS-ready affordance when a fix is available while Manual` in `GridEdit.test.tsx` — restore as positive assertion.

Add (close spec gaps):

- `chip_is_clickable_and_calls_onUseGps_when_source_is_Manual` — fire click on the chip element, assert `onUseGps` mock called.
- `chip_click_is_a_noop_when_source_is_Gps` — fire click, assert `onUseGps` not called.
- `set_manually_affordance_is_rendered_in_Gps_no_fix_state` — assert the affordance is present when `source === 'Gps' && gpsReady === false`.
- `set_manually_affordance_focuses_grid_input_on_click` — assert click triggers edit mode.
- `set_manually_affordance_is_absent_when_gpsReady_true` — assert affordance is not rendered in the Gps + fix-exists happy path.

Restore (`status.test.ts`):

- `PositionStatusDto` fixtures drop the `active_source` field (matches restored DTO shape).

Remove (no longer apply):

- `no Use-GPS affordance is rendered (tuxlink-pjih)` in `GridEdit.test.tsx` — was an absence-assertion baked in by pjih.

---

## §5 — Migration

Operator configs in the wild after the pjih merge:

| Operator state | `config.privacy.position_source` on disk | Post-restore behavior |
|---|---|---|
| First install since pjih merge | `Gps` (default; pjih never persists `Manual`) | Spec-correct: GPS by default, Manual after first inline-edit. |
| Pre-pjih operator with stored `Manual` | `Manual` (from pre-pjih `config_set_grid` write) | Spec-correct: stays Manual; can escape via the now-clickable chip. |

No migration code is needed. The arbiter's `new()` constructor already accepts both stored sources; the difference is purely in what `active_grid` returns and what `set_manual` does — both restored.

The pjih-era operator who set a manual grid expecting GPS to keep showing **will see different post-restore behavior**: their stored grid is now sticky-Manual, and the source chip shows MANUAL. Clicking the chip switches them to Gps (succeeds unconditionally per the spec amendment).

---

## §6 — Test plan (operator smoke)

After implementation lands:

1. **Manual sticky** — Inline-edit the grid to a known value. Confirm the chip shows MANUAL. Confirm the displayed grid stays at the typed value even if a fresh GPS fix arrives (verify with `journalctl -u gpsd` or `cgps` running alongside).
2. **Chip click escapes Manual** — While in Manual, click the chip. Confirm the chip flips to GPS. Confirm the displayed grid is the fresh fix if one exists, or shows the "GPS — no fix" row 3 state if no fix.
3. **Row 3 reachable** — From the chip-click escape above, if no fresh fix is currently available, confirm the "Set manually" button is rendered and clicking it enters inline-edit mode.
4. **GPS happy path** — With gpsd publishing fixes and `source = Gps`, confirm the displayed grid tracks the live fix and the chip is GPS-green-locked.
5. **Privacy gate intact** — With `gps_state = LocalUiOnly`, confirm the broadcast locator falls back to the stored config grid (not the live GPS fix), per `effective_broadcast_locator`.

---

## §7 — Adversarial review preparation

This restoration spec is a candidate for the 5-round Codex adrev cycle that the BRF pipeline mandates. Specific attack angles for adrev to chase:

1. **The chip-click semantics §2 (i/ii/iii) choice** — does the picked (ii) "Gps-click is no-op" reading actually match the spec line 102's "switches source explicitly"? Or should it toggle both ways?
2. **The use_gps relaxation §1 amendment** — does it have any cascading effect on the privacy gate (`effective_broadcast_locator`) or on `position_set_source`'s persistence logic?
3. **Migration of pjih-era operators §5** — is there any failure mode where a pre-pjih operator with `position_source = Manual` ends up in an unexpected state on first launch after restore?
4. **Test coverage** — the test list in §3 + §4 is restoration-focused; does it actually pin the original spec's full source contract, or are there spec lines that no test would catch a regression on?
5. **Row 3 affordance §2 + §4** — is "focus the grid input" the right interaction for the "Set manually" button, or should it open a small inline modal? Does the affordance need an alternative path for keyboard-only operators?

Codex adrev will be invoked per the BRF pipeline (≥1 round Codex, 5 rounds total). Findings get applied inline to this spec; any that aren't applicable get a brief "rejected because…" in the spec history.

---

## Appendix A — Reference: 2026-05-22 spec

See [`docs/superpowers/specs/2026-05-22-position-subsystem-design.md`](2026-05-22-position-subsystem-design.md) for the authoritative source contract, gpsd client design, manual-entry approach (Approach A), source-chip spec (Approach A), and the full ribbon-states table.

This restoration spec extends — does not supersede — that document.
