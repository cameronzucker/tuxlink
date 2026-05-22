# Position Subsystem — Design

- **Date:** 2026-05-22
- **bd issue:** tuxlink-686 (absorbs tuxlink-2y5 manual grid, tuxlink-2ob GPS device)
- **Status:** Approved (brainstorm) — pending operator spec review
- **Authors:** basin-arroyo-osprey + operator

## Motivation

The dashboard Grid is read from config and is not editable at runtime, and there is
no GPS source wired in. Two operator needs follow from that:

1. **GPS-denied operation.** Field operators are frequently stationed where a GNSS
   receiver cannot get a fix — metal-roofed stadiums and gyms, firehouses, basement
   command posts, concrete structures. They must be able to set their Maidenhead grid
   by hand, quickly, without hunting through menus.
2. **Live GPS when available.** When a fix is available, position should track it
   automatically (the project's "GPS on by default" convention), precision-reduced for
   privacy.

These are two faces of one subsystem: a single position source-of-truth that arbitrates
between a **manual** grid and a **GPS** fix under an explicit, operator-owned contract.

## The source contract

Position has exactly one **source** at a time, and nothing changes it silently.

| Rule | Behavior |
|---|---|
| Explicit source | `source ∈ {Manual, Gps}`. There is no implicit/auto source. |
| Manual is sticky | Setting a grid by hand pins `source = Manual`. GPS **never** overrides a manual position. |
| Operator-only switching | Returning to GPS is a deliberate act (the source chip), never automatic. |
| Precision is source-independent | Broadcast precision (`position_precision`, default 4-char) is enforced on whatever source is active — GPS included. |
| Source is always visible | The ribbon labels the active source (`MANUAL`/`GPS`); a change is never invisible. |

### Wizard-grid vs deliberate manual pin (confirmed)

A grid set during the onboarding **wizard is an initial value, not a manual pin**:
`source` defaults to `Gps`, so GPS takes over once a fix exists. A **deliberate manual
pin happens only via the runtime inline-edit**, which is the privacy-coarsening case
(e.g. typing `DM33` on purpose). That deliberate pin is sticky and GPS-protected; the
wizard's convenience grid is overridable by a real fix.

## Privacy: precision is enforced at the broadcast boundary

`position_precision` (`FourCharGrid` default, opt-in `SixCharGrid`) MUST be enforced
**where the grid leaves the application** — primarily the CMS exchange locator — not only
in the ribbon display.

**Existing gap (to correct):** today the ribbon truncates for display
(`formatGridForDisplay`), but the CMS locator is built from the full stored grid with no
reduction (`winlink_backend.rs` ≈ line 627: `let locator = config.identity.grid.clone()`).
A 6-char stored grid is therefore transmitted at full precision regardless of the privacy
setting. This subsystem introduces a single precision-reduction helper applied at the
locator-construction point (and any other broadcast surface), governed by
`position_precision`, independent of whether the grid came from Manual or GPS. The grid
remains **stored** at full precision; only the **broadcast** copy is reduced.

> This gap affects shipping v0.0.1 (a 6-char wizard grid would broadcast at full
> precision). Flagged to the operator; track as a v0.0.1 privacy fix (separate issue) if
> confirmed, or fold the reduction helper into this subsystem.

## Components

### 1. Position arbiter (Rust)

Single source of truth: `{ source: Manual|Gps, grid: Option<Grid>, last_fix: Option<Fix> }`.
All consumers — the ribbon, the CMS locator — read position through the arbiter, which
applies the source contract. The arbiter is the only writer of the active position.

- `set_manual(grid)` → validates, stores at full precision, pins `source = Manual`.
- `apply_gps_fix(fix)` → updates `last_fix`; takes effect as the active position **only
  when `source == Gps`**. A no-op on the active position while `source == Manual`.
- `use_gps()` → switches `source = Gps` (requires a usable recent fix; otherwise reports
  "no fix" and leaves the prior position visible).
- `broadcast_grid()` → active grid reduced to `position_precision`.

### 2. gpsd client (Rust, background task)

Connects to `gpsd` at `127.0.0.1:2947`, issues `?WATCH={"enable":true,"json":true}`,
parses TPV reports. Owns nothing about the serial device — gpsd already owns the LC29C
(`/dev/ttyAMA0` + `/dev/pps0`, confirmed serving). Per fix:

- **Fix-quality gate:** accept `mode` 2 (2D) or 3 (3D); reject `mode` 0/1 (no fix).
  Treat a fix older than a staleness window (default 30 s) as no-fix.
- Convert `lat`/`lon` → Maidenhead (6-char) → `Fix { grid, mode, time }` → arbiter.
- Reconnect with backoff if gpsd is unavailable; absence of gpsd is a normal "no GPS"
  state, never a hard error.

### 3. Manual entry — inline-edit (Approach A)

Clicking the ribbon **Grid** value turns it into an input in place. Enter commits, Esc
cancels. Commit calls a new `config_set_grid` command:

- Validate via the existing `validateGrid` / `normalizeGrid` (4- or 6-char Maidenhead).
- `write_config_atomic` the normalized grid; pin `source = Manual`.
- Refresh the ribbon (the existing `config_read` poll already feeds it).

### 4. Source chip (Approach A)

An amber `MANUAL` / outlined `GPS` chip beside the grid value. When `source == Manual`
and a usable fix exists, a subtle "● GPS ready — tap to switch" affordance appears.
Clicking the chip switches source explicitly. Clicking the value always edits (→ Manual).

## Ribbon states (Grid cell)

| Source | Fix | Display |
|---|---|---|
| Manual | (any) | `CN87` + `MANUAL` chip; "GPS ready" hint if a fix exists |
| Gps | usable | `CN87` (live, precision-reduced) + `GPS` chip |
| Gps | none | `CN87 · GPS no fix` (fallback to last grid) + obvious "set manually" |

The displayed grid is always the broadcast-precision form (4-char default).

## Config / data model

Add `position_source: PositionSource` (`Manual | Gps`, default `Gps`) alongside the
existing `PrivacyConfig.position_precision`. Schema-version bump + migration: existing
configs without the field default to `Gps`. `grid` continues to be stored at full
precision.

## CMS locator integration

`native_connect` builds the exchange locator from the arbiter's `broadcast_grid()` —
precision-reduced, source-correct — rather than directly from `config.identity.grid`.
This is the single point that closes the privacy gap above.

## Testing

- **Unit:** Maidenhead conversion (lat/lon ↔ grid, round-trip + known references);
  arbiter source contract (manual sticky, GPS no-op while manual, switch requires fix,
  broadcast reduction); fix-quality gate (reject no-fix/stale); TPV JSON parsing.
- **gpsfake (loopback):** replay an NMEA fixture with a known fix → assert the arbiter
  reports the expected grid and the CMS locator is reduced to the configured precision.
  No live RF, no network, no transmission.
- **Live LC29C:** the indoor "device alive, no fix" path validates the no-fix fallback +
  the "GPS ready"/"no fix" ribbon states against real gpsd output. Operator-run; no
  transmission.
- **Frontend:** inline-edit commit/cancel/validation; source-chip switching; ribbon
  state rendering per the table above.

## Out of scope

- Using gpsd's PPS for the application clock (the ribbon clock stays system time).
- "Set grid from map" — that is the future Geographica integration (tuxlink-0a2).
- Altitude, speed, course from GPS — position (grid) only for now.
- Changing gpsd's own configuration (`-G` all-interfaces bind, etc.) — pre-existing,
  owned by the host/Geographica.

## Defaults chosen (no further decision needed)

- gpsd staleness window: 30 s.
- Reconnect backoff: capped exponential (e.g. 1 s → 30 s).
- `position_source` default: `Gps` (per the "GPS on by default" convention).
- Fix acceptance: 2D or 3D; no HDOP threshold initially (mode + staleness suffice).
