# Position Subsystem — Restoration After pjih (v2)

- **Date:** 2026-06-01
- **bd issue:** tuxlink-c79g (closes); references tuxlink-pjih, PR #189 (reverts)
- **Status:** v2 — 5-round adrev applied, operator review pending
- **Authors:** bison-condor-grouse + operator
- **Amends:** [`docs/superpowers/specs/2026-05-22-position-subsystem-design.md`](2026-05-22-position-subsystem-design.md)
- **Adrev:** R1 Codex + R2/R3/R4/R5 Claude — 47 findings (6 P0, 21 P1, 20 P2); all P0 + all P1 applied; P2 selectively per cost/value.

## TL;DR

The 2026-05-22 position-subsystem design is correct as-written; pjih (PR #189, merged `a6db716`) violated the design contract on the assumption that the operator wanted a new semantic. Operator confirmed 2026-06-01: *"The original spec was fine. We had it working for a while. Each fix was only to address regression."*

This v2 spec covers:

1. **Revert** the pjih backend + frontend changes (restore the 2026-05-22 source contract).
2. **Close two pre-pjih implementation gaps** (chip clickability + row-3 "Set manually" affordance) that the original spec required but were never coded — and which probably motivated the original "GPS regression" complaint pjih over-fixed.
3. **One spec amendment** (chosen explicitly over two alternatives — see [§1.5](#15-why-a-relaxed-use_gps-over-b-clear-manual-pin-or-c-confirm-then-switch)): relax `use_gps()` AND its command-layer counterpart `position_set_source('Gps')` so that source-switching succeeds unconditionally. Operators without GPS hardware can reach the spec's row-3 destination state from Manual.

The 2026-05-22 spec remains the authoritative source contract. Everything below either points back to it or extends it for the gap-closures.

---

## Motivation

The 2026-05-22 spec defined an explicit operator-owned source contract: `Manual` is sticky, GPS never overrides until the operator explicitly switches via the source chip. The implementation that shipped (tuxlink-686) honored the sticky-Manual backend semantics but **left two spec lines unimplemented on the frontend**:

- **Spec §4 ("Source chip"), line 102:** *"Clicking the chip switches source explicitly."* — The chip was rendered as a non-interactive `<span>`.
- **Spec §"Ribbon states (Grid cell)", row 3:** *"Gps + none usable → `CN87 · GPS no fix` (fallback to last grid) + obvious 'set manually'."* — No "set manually" affordance was ever added; the row-3 visual state is empty.

Without the chip-as-switcher and without the row-3 affordance, an operator who engaged Manual (intentionally or accidentally) and didn't have a live GPS fix had no in-UI way back to Gps mode. The conditional "● GPS ready — tap to switch" hint only renders when `source === 'Manual' && gpsReady` — gated on a gpsd fix, which operators without GPS hardware never see.

PR #189 (pjih) attempted to fix this by deleting the Manual semantic entirely: `set_manual` no longer pinned source, the chip read a derived `effective_source` from arbiter state, and the Use-GPS button was removed as "structurally unreachable." That decision violated the design contract — and the operator hit the consequence on 2026-06-01:

> "GPS is now fully broken. It doesn't switch to manual entry with 'gps available' it just says GPS green at all times and accepts/displays whatever input. That's major regression."

The fix is to revert pjih and close the original spec-implementation gaps that triggered the original complaint pjih was over-reacting to.

---

## Scope

In scope:

- Revert PR #189's backend + frontend code.
- Restore the 2026-05-22 source contract — see [§"The source contract"](2026-05-22-position-subsystem-design.md#the-source-contract).
- Make the source chip clickable when source = `Manual` (close spec gap §4 line 102).
- Add a "Set manually" affordance for the Gps + no-fix state (close spec gap row 3).
- Spec amendment: relax `use_gps()` AND `position_set_source('Gps')` to succeed unconditionally.

Out of scope (handled in separate tracks — but see [§8 Track B interactions](#8--track-b-interactions)):

- Settings → GPS+Privacy panel cleanup (tuxlink-jmfm, Track B).
- ARDOP panel widening (tuxlink-8rng, Track B).
- Visual redesign of the chip beyond the actionable/status distinction below.
- Any change to gpsd client or fix-quality gate.

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

**Spec amendment (full disclosure of contract impact — R5 P0 #1):**

The 2026-05-22 spec's `use_gps()` requires a usable recent fix. This makes the "Gps + no fix" state (row 3 of the spec table) unreachable from `Manual` for operators without GPS hardware — the root recoverability gap that motivated pjih.

**v2 amendment**: BOTH `arbiter.use_gps()` AND its command-layer caller `position_set_source('Gps')` succeed unconditionally. They set `source = Gps`; if a fresh fix exists, the display upgrades to row 2 (`CN87` live + `GPS` chip); if not, the display renders row 3 (`CN87 · GPS no fix (fallback to last grid)` + "Set manually" affordance). The display's "fallback to last grid" is `arbiter.manual_grid` (the last hand-set grid); on-air follows the same value via `effective_broadcast_locator`'s existing else-branch.

**What changes in the contract (full extent — not "one line"):**
- `use_gps()` signature changes from `Result<(), &'static str>` to `()` (infallible).
- `position_set_source('Gps')` command: removes the `arbiter.has_fresh_fix()` pre-check + the `UiError::Unavailable { reason: "Cannot switch to GPS: no usable GPS fix" }` error path.
- The `Gps + no fix + manual_grid set` state — previously only reachable as an initial-state quirk — becomes a stable operator-driven destination that broadcasts the manual_grid through the GPS path. The chip says GPS (operator preference); the on-air locator is `manual_grid` (fallback per spec row 3). See [§2.5 Broadcasting in the Gps + no-fix state](#25--broadcasting-in-the-gps--no-fix-state-clarity-not-contract-violation) for the clarity treatment.

**What does NOT change:**
- Sticky-Manual semantics (set_manual still pins Manual; GPS never overrides).
- Privacy gate (`effective_broadcast_locator` keys on `a.source()` as before).
- gpsd client (fix-quality gate, staleness, reconnect backoff — all unchanged).
- Initial source default = `Gps` (per "GPS by default" memory + 2026-05-22 spec).

---

## §1.5 — Why (a) "relaxed `use_gps`" over (b) "Clear Manual pin" or (c) "confirm-then-switch"

R5 P0 #2 caught that v1 picked the relaxation without comparing alternatives. Three were on the table:

| Mechanism | Pros | Cons | Decision |
|---|---|---|---|
| **(a) Relax `use_gps()` + `position_set_source('Gps')`** | Smallest delta from 2026-05-22 spec. Chip remains the single switch surface. No new UI elements. Matches operator's "original spec but not broken" framing. | Contract amendment is broader than initial framing (Codex P0 #1, R5 P0 #1) — both arbiter primitive AND command must change. Row-3 "no fix + broadcasting fallback" requires UX clarity treatment. | **CHOSEN.** |
| (b) Dedicated "Clear Manual pin" affordance | No spec contract amendment. `use_gps()` semantic preserved as-written. Each UI element does exactly one thing. | New UI surface (a second button beside the chip) — adds visual weight to the ribbon's Grid cell. Operator must learn the new affordance. Two paths from Manual (chip vs. button) creates UX ambiguity about which is canonical. | Rejected: adds UI noise without solving the case where source-switch needs to land WITHOUT fresh fix. |
| (c) Two-stage confirm-then-switch on chip click | Prevents accidental source switches. | Operator has previously flagged confirmation modals as ceremony ([memory: `feedback_radio1_governs_tx_not_ui.md`](../../../...)). Adds friction without addressing the root recoverability issue. | Rejected: friction without solving the gap. |

Operator confirmed (a) 2026-06-01.

---

## §2 — UI surface (chip clickable + row-3 affordance + State-1/4 differentiation)

### §2.1 — Chip-click semantics + element choice (Codex #2 + R2 #1 + R5 #3)

Strong convergence across three rounds (Codex, R2, R5) that "an enabled `<button>` that intentionally does nothing" is a UX failure mode. v2 design:

| Source | Element | Interaction |
|---|---|---|
| `Manual` | `<button>` | `onClick` = call `position_set_source('Gps')`. The chip is the single explicit switch surface. |
| `Gps` | `<span role="status">` (no role="button") | Status-only. Not focusable. No click handler. Switching INTO Manual is via the grid-value inline-edit (per 2026-05-22 spec). |

Rationale: the chip's purpose is OPERATOR ACTION. When the operator is already in Gps mode, the chip has nothing to do for them (Manual is reached via grid-edit). A button that doesn't react fails the principle of least astonishment.

This also makes the chip visually distinct between actionable (Manual) and status (Gps), which reinforces the operator's available action without text.

### §2.2 — Chip + Use-GPS button redundancy (R2 #2)

When `source === 'Manual' && gpsReady`, both the (newly-clickable) chip AND the "● GPS ready — tap to switch" button are clickable. v2 changes the hint button into a passive status text:

```
[ MANUAL ] · GPS ready                ← passive text, not a button
```

The chip remains the single click surface. The hint provides contextual information ("a fix is available"); it doesn't duplicate the action.

### §2.3 — "Set manually" affordance for row 3 (Codex #6 + R2 #3)

For the `Gps + no-fix` state (spec row 3 — now reachable per §1 amendment):

```
[ Grid ]  CN87 · GPS no fix   [ ▸ Set manually ]
   ^                              ^
   fallback grid                  button — focuses grid input
```

Spec line per Codex #6:
- `Set manually` is a `<button>` rendered AFTER the source chip in DOM/tab order.
- Visual cue: small right-arrow `▸` icon to convey the focus-jump.
- ARIA: `aria-controls={gridInputId}` — programmatically associates the button with the input it activates.
- Enter/Space invokes the same inline-edit path as clicking the grid value.
- The newly-mounted grid input receives focus on mount (R2 #3 + Codex #6 explicit).
- Strengthen the vitest from "enters edit mode" to assert `document.activeElement === grid-input`.

### §2.4 — State 1 vs State 4 visual differentiation (R2 #4 — CRITICAL)

R2 #4 identifies the failure mode pjih literally shipped: at first glance, State 1 (Manual + no GPS) and State 4 (Gps + no fix) can look identical — same grid value, same chip-shape, same surrounding chrome. The operator can't tell at a glance whether they're broadcasting their manual entry as Manual or as a Gps fallback.

v2 differentiation:

| State | Chip color | Grid prefix | Status text |
|---|---|---|---|
| 1. Manual + no GPS | amber `MANUAL` (saturated) | (none) | (none) |
| 2. Manual + GPS-ready | amber `MANUAL` + green dot | (none) | "GPS ready" (passive text) |
| 3. Gps + fresh fix | green `GPS` (locked) | (none) | (none) |
| 4. Gps + no fix | dimmed `GPS` outline | `· ` interpunct prefix | "GPS no fix" |

The grid-value prefix `· ` + dimmed chip + status text make State 4 visually distinct from State 1 at any zoom level. Per the 2026-05-22 spec's row 3 wording ("`CN87 · GPS no fix`"), the interpunct is part of the canonical display.

### §2.5 — Broadcasting in the Gps + no-fix state (clarity, not contract violation)

R3 F2 P0 flagged a perceived display/on-air divergence: chip says GPS, on-air is manual_grid via the GPS path. After re-tracing the 2026-05-22 spec, this is consistent with operator intent (operator preference = Gps; manual_grid is the spec-defined fallback for on-air), but needs explicit clarity in the UI and the spec:

**Spec text (added):** When `source = Gps && no fresh fix && manual_grid is set`:
- Display: row 3 (`CN87 · GPS no fix` + chip dimmed + Set manually affordance).
- On-air: `manual_grid` (precision-reduced) — broadcast via `effective_broadcast_locator`'s else-branch — same as the displayed value.
- These are NOT divergent — they're the same `manual_grid` value displayed two ways. The chip reflects OPERATOR PREFERENCE (Gps); the value reflects WHAT IS BROADCAST (the fallback per spec row 3).

**UI text (added):** the row-3 status text says `"GPS no fix · broadcasting fallback"` rather than just `"GPS no fix"`. The "broadcasting fallback" suffix explicitly tells the operator that the displayed grid IS the on-air locator (not zero / not nothing).

### §2.6 — ASCII state mockups (refined)

State 1 — `source = Manual`, no fresh fix:
```
┌───────────────────────────────────────────────┐
│ Grid │  CN87   ┌──────┐                       │
│      │         │MANUAL│ ← amber, clickable    │
│      │         └──────┘   (→ position_set_source('Gps')) │
└───────────────────────────────────────────────┘
```

State 2 — `source = Manual`, fresh fix exists:
```
┌──────────────────────────────────────────────────────┐
│ Grid │  CN87   ┌──────┐  ● GPS ready                  │
│      │         │MANUAL│   (passive text — NOT button) │
│      │         └──────┘                               │
└──────────────────────────────────────────────────────┘
```

State 3 — `source = Gps`, fresh fix:
```
┌─────────────────────────────────────────────┐
│ Grid │  DM33   ┌──────┐                     │
│      │         │ GPS  │ ← green-locked      │
│      │         └──────┘   (span, status)    │
└─────────────────────────────────────────────┘
```

State 4 — `source = Gps`, no fresh fix (spec row 3, now reachable):
```
┌──────────────────────────────────────────────────────────────────┐
│ Grid │  · CN87  ┌──────┐  GPS no fix · broadcasting fallback     │
│      │  ^       │ GPS  │  [ ▸ Set manually ]                      │
│      │  interpunct (dimmed)                                       │
└──────────────────────────────────────────────────────────────────┘
```

---

## §3 — Backend changes (revert pjih + extend the relaxation to the command layer)

### §3.1 — Revert table

| Symbol | Pre-pjih state | pjih state (current `main`) | v2 restoration |
|---|---|---|---|
| `arbiter.active_grid` | `Manual → manual_grid`; `Gps → fresh fix else manual_grid fallback` | GPS-fresh always wins regardless of source | Restore source-gated behavior |
| `arbiter.set_manual` | Pins `source = Manual` | No source change | Restore source-pinning |
| `arbiter.effective_source` | (did not exist) | Returns `Gps` when fresh fix exists | **Remove entirely** |
| `arbiter.use_gps` | Required fresh fix | Required fresh fix | **Relax: infallible** (see §1 amendment); signature → `()` not `Result<(), _>` |
| `position_set_source('Gps')` command | Required fresh fix (`UiError::Unavailable`) | Same (pjih didn't touch this command) | **Relax: remove the `has_fresh_fix` pre-check + error path** (Codex P0 #1) |
| `config_set_grid` command | Persisted `cfg.privacy.position_source = Manual` | Does not touch `position_source` | Restore the persistence |
| `PositionStatusDto` | `{ gps_ready, broadcast_grid }` | `{ gps_ready, broadcast_grid, active_source }` | **Remove `active_source`** |
| `position_status` command | (matches DTO above) | Populates `active_source` from `arbiter.effective_source()` | Drop the `active_source` population |

### §3.2 — Keep as-is (no change from current `main`)

- `effective_broadcast_locator` already keys the privacy gate on `a.source()` (the stored preference). That's correct post-restore. No change.
- All gpsd-client code (`crate::position::gpsd`) — unaffected.
- Precision-reduction helper (`broadcast_grid` in `config.rs`) — unaffected.

### §3.3 — Concurrency invariants (R3 F1 + R3 F7)

R3 caught that `config_set_grid` and `position_set_source` are non-atomic three-step sequences (read config → write file → mutate arbiter → push to backend). Without an explicit invariant, rapid-fire chip-clicks + inline-edits can leave disk / arbiter / backend snapshots disagreeing.

**Invariant (added to spec):** Both `config_set_grid` and `position_set_source` MUST hold the arbiter's `inner` mutex from "read config" through "mutate arbiter" (i.e., for the entire critical section). The mutex is dropped only after the in-memory arbiter has been updated. The "push to live backend" step occurs after the mutex is released (the backend's own snapshot is eventually-consistent with the arbiter — that's pre-existing behavior).

**Implementation note:** the arbiter's `Mutex<Inner>` is the natural serialization point. Both commands clone the `Arc<PositionArbiter>` and call methods that lock-and-update atomically. The `config::read_config` + `write_config_atomic` calls happen INSIDE the locked region (not outside) to close the TOCTOU.

**Test:** a new backend test `concurrent_config_set_grid_and_position_set_source_serialize` issues 100 concurrent calls of both commands from different tokio tasks; asserts the final arbiter state is consistent with the LAST committed write (per Mutex ordering); asserts no panic, no poisoned mutex.

### §3.4 — State-space invariants (R3 F4)

R3 walked all 36 cells of `source × fix_state × gps_state × manual_grid_set`. 6 cells were undefined under v1. v2 invariants:

| Invariant | Behavior |
|---|---|
| **I1** | When `source = Manual && manual_grid = None`: `active_grid = None`; display shows `—` (em-dash placeholder); chip = MANUAL (still actionable to switch to Gps); on-air = `effective_broadcast_locator` falls back to `config.identity.grid` (may be None → empty on-air). |
| **I2** | When `source = Manual` (any `fix_state`, any `gps_state`, `manual_grid` set): `active_grid = manual_grid`. The Manual chip's sticky-against-GPS property holds across ALL gps_state values — privacy doesn't change source semantics. |
| **I3** | When `source = Gps && fix fresh`: `active_grid = fix.grid`; on-air respects gps_state per existing privacy gate. |
| **I4** | When `source = Gps && no fresh fix && manual_grid set`: `active_grid = manual_grid` (fallback per spec row 3). Display = row 3; on-air = `manual_grid` (precision-reduced) per spec row 3 ("broadcasting fallback"). |
| **I5** | When `source = Gps && no fresh fix && manual_grid = None`: `active_grid = None`; display = "no grid" (em-dash); on-air = falls back to `config.identity.grid` (which should be in sync with manual_grid via the existing config-set path; if it isn't, on-air may be empty — this is a config-file integrity assumption, not a runtime fail). |
| **I6** | `manual_grid` and `config.identity.grid` are synchronized by `config_set_grid` (writes both atomically). The arbiter's `new()` reads the initial `manual_grid` from `config.identity.grid`. No code path mutates one without the other except gpsd fix arrival (which only writes `last_fix`, never `manual_grid`). |

Tests for invariants: see §6 ("Test plan") for the matrix.

---

## §4 — Frontend changes (revert pjih + chip-as-button + row-3 affordance + optimistic update)

### §4.1 — Revert table

| Surface | pjih state (current `main`) | v2 restoration |
|---|---|---|
| `DashboardRibbon.tsx` | `<GridEdit ... onCommit={...} />` — no `onUseGps` prop | Restore `onUseGps={() => invoke('position_set_source', { source: 'Gps' })}` |
| `GridEdit.tsx` props | No `onUseGps` | Restore `onUseGps: () => void` in `GridEditProps` |
| `GridEdit.tsx` Use-GPS button | Removed entirely | NOT restored as a button — replaced with passive "GPS ready" status text (R2 #2) |
| `useStatus.ts` `PositionStatusDto` | `{ gps_ready, broadcast_grid, active_source }` | Remove `active_source` field |
| `useStatus.ts` `useStatusData` | `position_source: positionStatus?.active_source ?? config?.position_source ?? 'Gps'` | Restore `position_source: config?.position_source ?? 'Gps'` |

### §4.2 — Chip-as-button + row-3 affordance + GPS-ready passive text

Per §2.1:
- Source chip when `source === 'Manual'`: `<button onClick={onUseGps}>` with `aria-label="Switch position source to GPS"`.
- Source chip when `source === 'Gps'`: `<span role="status" aria-label={...}>` — non-interactive, non-focusable.
- Existing "● GPS ready — tap to switch" button is REPLACED by `<span className="dash-gps-ready-status">GPS ready</span>` — passive text only.

Per §2.3:
- Row-3 affordance: `<button aria-controls={gridInputId} onClick={enterEdit}>▸ Set manually</button>` rendered when `source === 'Gps' && !gpsReady`. Tab order: chip → set-manually → grid value. On click, `enterEdit()` is called and the grid input receives focus on mount.

### §4.3 — Optimistic update after writes (Codex P1 #4)

Dropping `active_source` from `PositionStatusDto` means the chip's source state comes from `config_read` (5s poll). Without an optimistic update, the chip lags 0-5s behind the operator's click.

**Add to frontend:**
- After successful `config_set_grid` invoke: call `queryClient.invalidateQueries(['config'])` (or equivalent) to force an immediate config_read refresh. The chip should reflect `source = Manual` within one render cycle.
- After successful `position_set_source('Gps')` invoke: same — force config_read refresh.

**Alternative considered + rejected:** local optimistic state on the chip (`useState` + sync from config_read). Rejected because two sources of truth for source state risk divergence on error paths (e.g., backend write fails, optimistic state stays Manual, config still says Gps).

**Implementation note:** the `useStatus.ts` `useStatusData` hook is the natural place to expose an `invalidate()` callback that GridEdit + DashboardRibbon can call after writes.

### §4.4 — A11y notes (R2 #6 + R2 #3)

- Replace "tap to switch" mouse-centric language with "Switch to GPS" (in the chip's aria-label and the passive status text).
- The chip-as-button has `aria-pressed` reflecting `source === 'Gps'` (false when Manual, true when Gps — though button is hidden as span in Gps state, so this is only visible in the Manual state).
- The `Set manually` button has `aria-controls={gridInputId}` (the grid-value input's ID).

---

## §5 — Migration (REWRITTEN per Codex P1 #5 + R3 F8)

v1's §5 migration table was literally backwards. v2 correction:

**Reality on disk after the pjih merge:**

| Operator path | `config.privacy.position_source` on disk |
|---|---|
| First install AFTER pjih merge | `Gps` (default; pjih's `config_set_grid` never persisted `Manual`). |
| First install BEFORE pjih merge, used `config_set_grid` (pre-pjih code) | `Manual` (pre-pjih `config_set_grid` persisted `Manual`). |
| First install BEFORE pjih merge, never touched grid | `Gps` (default). |
| First install BEFORE pjih merge, used `config_set_grid` (pre-pjih), then merged pjih, then used `config_set_grid` again (pjih code) | `Manual` from the pre-pjih write; pjih's `config_set_grid` did NOT overwrite to `Gps`, just left it. |
| First install BEFORE pjih merge, used `position_set_source('Gps')` after | `Gps` (the command did persist `Gps` on successful switch). |

**Post-restore behavior matrix:**

| On-disk source | Operator experience post-restore |
|---|---|
| `Gps` (most operators per above) | Default state: row 2 if fresh fix; row 3 if no fix. Editing grid pins Manual sticky. Chip-click escapes back to Gps. |
| `Manual` (pre-pjih operators who set a grid) | Sticky Manual from disk. Chip is actionable; clicking it calls `position_set_source('Gps')` which now succeeds unconditionally; source flips to Gps; row 2 or row 3 renders. No data loss. |

**No one-time migration code is required.** Codex P1 #5: pjih cannot distinguish deliberate pre-pjih Manual intent from pjih-window confusion, and resetting would violate the restored source/privacy contract. The pjih-era operators with `Gps` on disk simply use the restored chip; the pre-pjih operators with `Manual` on disk get the same restored chip + the now-actionable click.

**The reported regression that motivated this fix:** the operator's complaint *"GPS is now fully broken... accepts/displays whatever input"* matches the pjih-window-write code path — `config_set_grid` updates `cfg.identity.grid` without persisting `position_source = Manual`, and `arbiter.set_manual` doesn't pin source. So the operator sees: edits grid → display updates (because pjih's `active_grid` returns the typed value as the only available data) + chip stuck at `Gps` (from `effective_source` derived from arbiter state where last_fix may or may not exist). Post-restore: edits grid → display updates + chip flips to MANUAL (sticky) + on-air uses manual_grid.

---

## §6 — Test plan

### §6.1 — Backend tests (cargo `--lib`)

**Restore (from pre-pjih, with R4 P0 #1 strengthening):**

- `set_manual_pins_source_and_is_sticky_against_gps` — RESTORE but EXTEND for R4 P0 #1: the test now exercises the temporal sequence `set_manual("EM75") → apply_gps_fix(Fix::test("DM33ab")) → verify source still Manual && active_grid == "EM75" && last_fix is recorded`. The pre-pjih test only pinned the post-set snapshot; v2's version pins the GPS-arrival regression class.

- `gps_fix_updates_active_only_when_source_is_gps` — RESTORE as-is.

- `arbiter_set_manual_pins_manual_source` (in `ui_commands::tests`) — RESTORE as-is.

**Add (R4 P0 #2 + Codex P1 #3):**

- `use_gps_succeeds_without_fresh_fix_and_yields_manual_fallback` (replaces the renamed `use_gps_requires_a_usable_fix`): asserts `use_gps()` returns `()` (infallible), asserts `source` flips to `Gps`, asserts `active_grid()` equals `manual_grid` (fallback per spec row 3). Requires a `manual_grid` to be set in the arbiter setup.

- `manual_source_ignores_fresh_gps_fix_at_broadcast_boundary` (Codex P1 #3): sets `arbiter = (source: Manual, manual_grid: "EM75")` + `gps_state = BroadcastAtPrecision`, then `apply_gps_fix(Fix::test("DM33ab"))`, then asserts `effective_broadcast_locator(cfg, &arbiter) == "EM75"` (precision-reduced). This pins the "Manual broadcasts manual regardless of fresh GPS fix" invariant at the broadcast boundary.

- `config_set_grid_pins_manual_source_in_config_and_arbiter` (Codex P1 #3): drives `config_set_grid("EM75")` end-to-end via the command (not the arbiter primitive); asserts both `arbiter.source() == Manual` AND `read_config().privacy.position_source == Manual`. Pins the cross-layer persistence invariant.

- `position_set_source_gps_succeeds_without_fresh_fix` (Codex P0 #1): asserts the command path mirrors the arbiter relaxation — returns `Ok(())` even with no fresh fix; persists `position_source = Gps` to config; arbiter source flips.

- `concurrent_config_set_grid_and_position_set_source_serialize` (§3.3): spawns 100 concurrent tokio tasks alternating both commands; asserts no panic, no poisoned mutex, final config + arbiter agree.

- **State-space matrix tests** (R3 F4): one test per non-trivial cell of §3.4's I1-I6 invariants. Combine via `proptest` over `(source, fix_state, gps_state, manual_grid_set)` quadrants; assert active_grid + broadcast result for each.

**Remove (no longer apply post-restore):**

- All five pjih-era arbiter tests (`set_manual_updates_grid_without_changing_stored_source`, `fresh_gps_fix_wins_over_manual_grid_regardless_of_stored_source`, `manual_grid_used_when_gps_fix_is_stale_or_absent`, the equivalent in `ui_commands::tests`, the `position_status_dto_*` `active_source` assertions).

### §6.2 — Frontend tests (vitest)

**Restore (with strengthening):**

- `shows GPS-ready hint when a fix is available while Manual` (GridEdit) — RESTORE but assert the hint is a `<span>` (passive text), NOT a `<button>`. Per §2.2.

**Add (Codex P1 #3 + Codex P1 #4 + R4 P1 #3-#5 + R5 #7):**

- `chip_is_a_button_when_source_is_Manual_and_calls_onUseGps_on_click` (GridEdit, per R4 P1 #3): fire click on the chip element; assert `onUseGps` mock called.

- `chip_is_a_span_when_source_is_Gps_with_no_click_handler` (GridEdit, per R4 P1 #3): assert `getByTestId('source-chip').tagName === 'SPAN'`; assert `onUseGps` mock NOT called on element click (defensive).

- `set_manually_affordance_is_present_in_Gps_no_fix_state` (GridEdit, per R4 P1 #5): assert affordance rendered with `source='Gps' && gpsReady=false`.

- `set_manually_affordance_is_absent_in_Manual_state` (GridEdit, per R4 P1 #5): assert affordance NOT rendered with `source='Manual'`.

- `set_manually_affordance_is_absent_in_Gps_with_fresh_fix` (GridEdit, per R4 P1 #5): assert affordance NOT rendered with `source='Gps' && gpsReady=true`.

- `set_manually_affordance_is_absent_in_Manual_with_GPS_ready` (GridEdit, per R4 P1 #5 — closes the 4-quadrant matrix): assert affordance NOT rendered with `source='Manual' && gpsReady=true`.

- `set_manually_focuses_the_grid_input_on_click` (GridEdit, per Codex P2 #6): assert `document.activeElement === grid-input` after click.

- `ribbon_source_stays_Manual_when_config_says_Manual_even_if_gps_ready_is_true` (useStatus hook test, per Codex P1 #3): mock `config_read.position_source = Manual` and `position_status.gps_ready = true`; assert `result.current.position_source === 'Manual'`. Pins the chip-from-config-not-from-arbiter invariant.

- `chip_flips_to_Manual_immediately_after_config_set_grid_completes` (DashboardRibbon, per Codex P1 #4): mock `invoke('config_set_grid')` to resolve; assert chip text changes within one render cycle (not after 5s poll). Verifies the optimistic refresh.

- `chip_flips_to_Gps_immediately_after_position_set_source_completes` (DashboardRibbon, per Codex P1 #4): mock `invoke('position_set_source')` to resolve; assert chip text changes within one render cycle.

- `chip_for_source_Gps_with_no_fix_renders_State_4_distinguishable_from_State_1` (GridEdit, per R2 #4): mock `source='Gps' && gpsReady=false && grid='CN87'`; assert presence of the `· ` interpunct prefix on grid value AND the `dimmed` modifier class on the chip. Differentiates row 3 from row 1.

**Remove:**

- `no Use-GPS affordance is rendered (tuxlink-pjih)` in `GridEdit.test.tsx` — pjih-era absence-assertion.

### §6.3 — Cross-layer integration test (R4 P1 #4 + R5 #7 — the test class pjih violated)

R5 #7 caught the root cause of pjih's undetected merge: per-layer tests on backend (arbiter) and frontend (GridEdit) passed independently, but no test exercised the composed flow that justifies the entire restoration. v2 adds:

- **`integration_chip_click_from_Manual_to_Gps_no_fix_renders_row_3` (Playwright or @testing-library `renderHook` with mocked Tauri)**:
  1. Mount the full GridEdit + useStatus hook with mocked Tauri commands.
  2. Initial state: `config.position_source = 'Manual'` + `manual_grid = 'EM75'` + `position_status.gps_ready = false`.
  3. Verify State 1 (Manual chip + grid value `EM75`).
  4. Click the source chip.
  5. Verify `invoke('position_set_source', { source: 'Gps' })` was called.
  6. Update mock config to `position_source = 'Gps'`.
  7. Verify State 4 (Gps dimmed chip + `· EM75` interpunct prefix + "GPS no fix · broadcasting fallback" + Set manually affordance).
  8. Click Set manually.
  9. Verify the grid input mounts AND receives focus.

This single test would have caught the pjih regression at merge time.

### §6.4 — Operator smoke (REWRITTEN per R2 #5)

R2 #5 caught that v1's §6 smoke didn't exercise the no-GPS-hardware case that the entire §1 amendment exists to fix. v2 smoke (kill `gpsd` or unplug GPS receiver between steps as noted):

1. **Manual sticky (GPS present)** — start with gpsd running. Inline-edit grid to `EM75`. Confirm chip shows MANUAL (amber). Apply a fresh GPS fix to `DM33` (via gpsd). Confirm grid value STAYS `EM75` (sticky). Confirm chip STAYS MANUAL.

2. **Chip click escapes Manual (GPS present)** — from step 1, click the MANUAL chip. Confirm chip flips to GPS (green-locked). Confirm grid value flips to the live GPS fix `DM33`.

3. **Chip click escapes Manual (no GPS — the case pjih existed to fix)** — kill gpsd (`sudo systemctl stop gpsd`). Inline-edit grid to `EM75`. Confirm State 1 (Manual chip + `EM75`). Click the MANUAL chip. Confirm State 4 (GPS chip dimmed + `· EM75` interpunct + "GPS no fix · broadcasting fallback" + Set manually affordance). Confirm operator is no longer stuck.

4. **Set manually from row 3** — from step 3, click Set manually. Confirm grid input receives focus and is editable. Type `DM33` + Enter. Confirm State 1 (MANUAL chip + `DM33`).

5. **GPS happy path** — restart gpsd (`sudo systemctl start gpsd`). With `source = Gps`, confirm grid value tracks the live fix. Confirm State 3 (green-locked GPS chip).

6. **Privacy gate intact** — set `gps_state = LocalUiOnly`. Confirm the broadcast locator (e.g., via the CMS-exchange operator-smoke from PR #185) falls back to the stored config grid, not the live GPS fix.

---

## §7 — Why pjih landed undetected (R5 #5 hypothesis)

R5 #5: without a hypothesis for why the operator didn't catch the pjih regression at merge time, the same dynamic could reproduce here.

**Hypothesis:** pjih's PR description and adrev focused on the immediate symptom ("setting manual grid breaks GPS-derived display"). The agent (me, on the prior turn) re-interpreted "GPS-derived display" as "the operator wants GPS-derived display even after setting Manual" — a semantic that REQUIRED removing Manual stickiness. The PR's stated goal was framed as "decouple grid-set from source-pin," which sounded like a clean refactor.

The operator approved the PR based on the framing, not by walking the actual UX with the change in hand. The CI gates were green (per-layer tests passed). The full operator-visible flow (Manual → GPS sticky → escape via chip OR row 3 fallback) was not exercised by any test, so the contract violation didn't surface until the operator hit it in smoke.

**Watched failure mode for v2:** the operator must walk the §6.4 smoke steps end-to-end with the v2 changes in hand. If the smoke can't run (no GPS hardware), step 3 + 4 still exercise the critical no-GPS recoverability path. Adrev caught what unit tests can't: cross-layer narrative coherence. The integration test (§6.3) is the automated form of that adrev — it's the single test that pins the spec's primary correctness story.

---

## §8 — Track B interactions

Track B (tuxlink-jmfm + tuxlink-8rng) covers Settings → GPS+Privacy ARDOP block removal + radio-panel widening to 400px. R5 #6 surfaced potential interactions with this Track A restoration:

**Settings panel (jmfm):** the Settings panel is named "GPS privacy" panel (per the file header comment in `SettingsPanel.tsx`). It currently contains GPS-state + precision controls AND an ARDOP fieldset. Track B deletes the ARDOP fieldset. After Track B + Track A both land:
- Settings panel is purely a GPS-privacy panel (its original intent).
- The Track A restoration's `gps_state` privacy gate continues to work; the Settings panel's GPS-state radio + precision radio are unchanged.

**No interaction**: Track B's ARDOP-block delete touches no code in the position subsystem. The two tracks can ship in either order. If Track B ships first, the Settings panel becomes a "pure GPS-privacy panel" and Track A's chip-as-button work is unaffected. If Track A ships first, the Settings panel still has the ARDOP fieldset until Track B lands; the GPS+Privacy controls work as expected.

**Radio panel widening (8rng):** entirely orthogonal — radio panel chrome is in `radio/RadioPanel.css`, the dashboard ribbon's GridEdit is in `shell/`. Zero overlap.

---

## §9 — Adversarial review status

| Round | Reviewer | Findings | Disposition |
|---|---|---|---|
| R1 | Codex (GPT-5) | 6 (1 P0, 4 P1, 1 P2) | All applied: P0 #1 (extend relaxation to command layer) → §1 amendment + §3.1 + §6.1 new test. P1 #2 (Gps-chip status-only) → §2.1. P1 #3 (cross-layer source-sequence tests) → §6.1 added 4 tests. P1 #4 (optimistic update) → §4.3. P1 #5 (migration text wrong) → §5 rewritten. P2 #6 (focus contract) → §2.3 + §6.2 strengthened. |
| R2 | Claude (UX) | 8 (5 P1, 3 P2) | All P1 applied. #1 (chip-click justification) → §1.5 + §2.1. #2 (chip/button redundancy) → §2.2 + §4.2. #3 (set-manually focus a11y) → §2.3. #4 (State 1 vs 4 visual differentiation) → §2.4 + §6.2 new test. #5 (smoke missing no-GPS case) → §6.4 step 3. P2 #6 (mouse-centric "tap") → §4.4. P2 #7 (subsumed by chip-as-span). P2 #8 (width budget) — deferred to implementation polish. |
| R3 | Claude (contract/races) | 10 (1 P0, 3 P1, 6 P2) | P0 F2 (privacy gate clarity) → §1 amendment + §2.5 explicit text. P1 F1+F7 (concurrency invariants) → §3.3. P1 F8 (§5 narrative wrong) → §5 rewritten. P1 F4 (state-space) → §3.4 + §6.1 matrix tests. P2 (vestigial pre-checks, fix-aging tests, gpsd-client documentation, etc.) — implementation-detail; tracked for the plan. |
| R4 | Claude (tests) | 11 (2 P0, 4 P1, 5 P2) | All P0 + P1 applied. P0 #1 (temporal sticky) → §6.1 sticky test extended. P0 #2 (use_gps active_grid assertion) → §6.1 new test. P1 #3 (chip-element-type test) → §6.2 added. P1 #4 (composed flow) → §6.3 integration test. P1 #5 (4-quadrant affordance matrix) → §6.2 expanded. P1 #6 (3 missing backend invariants) → §6.1 added. P2 — partially applied to §6 strengthening. |
| R5 | Claude (holistic) | 12 (2 P0, 5 P1, 5 P2) | P0 #1 (one-amendment understated) → §1 amendment now full disclosure + §1.5. P0 #2 (alternatives not compared) → §1.5 added. P1 #3 (chip-click semantics) → §2.1 + §1.5. P1 #4 ("GPS by default" axiomatic) — confirmed by operator + 2026-05-22 spec; documented as a decision in §1.5. P1 #5 (why pjih undetected) → §7. P1 #6 (Track B interactions) → §8. P1 #7 (cross-layer test class) → §6.3. P2 — partially applied. |

**Total: 47 findings; 6 P0 + 21 P1 = 27 must-apply, all applied. P2: 20, selectively applied per cost/value.**

---

## Appendix A — Reference: 2026-05-22 spec

See [`docs/superpowers/specs/2026-05-22-position-subsystem-design.md`](2026-05-22-position-subsystem-design.md) for the authoritative source contract, gpsd client design, manual-entry approach (Approach A), source-chip spec (Approach A), and the full ribbon-states table.

This restoration spec extends — does not supersede — that document.
