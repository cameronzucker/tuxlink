# GPS / Location setup — position confirmation, unconditional diagnostics, one-click fix

> **Status:** DRAFT — pending operator review (2026-06-13).
>
> **Author:** swallow-hemlock-fox (Claude Opus 4.8) + Cameron Zucker.
> **Mode:** Builder.
> **Amends:** [`docs/design/2026-06-05-gps-setup-ux-design.md`](2026-06-05-gps-setup-ux-design.md) (the parent GPS UX design; APPROVED 2026-06-05). This doc does **not** supersede it — it extends slice 1 (`tuxlink-9xy1`, shipped) and pulls slice 2 (`tuxlink-m9ej`, the pkexec one-click) forward into the same delivery.
> **Mockups:** `.superpowers/brainstorm/3430082-1781348910/content/location-fullscreen-v4.html` (full-screen layout, operator-approved 2026-06-13).

---

## Problem statement

The wizard's Location step (`StepLocation` → shared `GpsSourcePicker`) shipped in v0.59.0 and is reachable from every onboarding path. But it has you **select a source on faith**: there is no map, no readout of the resulting position, and no way to tell whether what GPS produced is actually *where you are*. Three concrete gaps, each confirmed against the shipped code:

1. **No position display or confirmation.** Picking a GPS source fires `position_set_source('Gps')` fire-and-forget (`useLocationConfig`); the picker never reads back the fix. The operator sees a source label, never a position. "Is this right?" is unanswerable. The approved parent design contains **no map** at all.

2. **The Linux GPS diagnostics are gated on device-presence — backwards.** `classifyGpsSources` only emits triage cards when `hasSerial` is true (a serial device is already enumerated *and* blocked). But on Linux the device frequently **does not appear precisely because the system is misconfigured** (ModemManager grabbing the port; not in `dialout`; gpsd down). So the debugging the operator most needs is invisible exactly when it is needed. With no device plugged in, the step renders only a blank grid input. This is both a divergence from the parent design's stated intent (its bd-1 acceptance lists a "no-device" diagnosis) and the operator's primary complaint: the carefully-built triage was effectively unreachable.

3. **Manual location-setting is underspecified.** The step offers a grid text field only. There is no map-based "drop a pin / drag to my QTH" path, even though the offline map subsystem (`src/map/`) already ships exactly that component.

This is the **"set up my location so I can use Tuxlink"** dialog, not "the GPS dialog." GPS is one input method; manual map/grid entry is co-equal; diagnosing why GPS is dark is a first-class job.

## Decisions (this design)

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | **Reuse the shipped offline map as a confirmation surface.** Render `GridMapPicker` (pin mode) + `BaseMap` inside the location component; show the live fix as a pin + highlighted grid square + grid readout. | The map subsystem (`BaseMap`, `GridMapPicker`, `MaidenheadOverlay`, `useTileSource`) already ships, is offline, and is built for exactly this. No new map code beyond drag. |
| D2 | **Precise pin — plumb raw lat/lon from the GPS fix (local display only).** | A 4-char grid square is ~110 km wide; a square is a weak "correct?" signal. A pin on the operator's actual spot is the confirmation. Operator chose precise. |
| D3 | **Diagnostics run unconditionally — gate on "no working fix," not on device-presence.** dialout + ModemManager + gpsd checks surface whenever GPS is not producing a fix, regardless of whether a device enumerated. | Operator pushback: the device often won't appear *because* Linux is broken, which *requires* the diagnostics. Corrects the shipped `hasSerial` gate. |
| D4 | **Build the one-click "Fix it for me" (pkexec) now** — `tuxlink-m9ej`, per its existing spec. | Operator chose to fold slice 2 into this delivery ("Dave's magic button"). m9ej is already fully designed; this doc references it, does not restate it. |
| D5 | **Manual = click the map to drop a pin AND drag the marker to fine-tune**, alongside the grid text field. | Operator asked for grid *or* pin drag-and-drop. `GridMapPicker` pin mode already does click-to-place; drag is a small add. |
| D6 | **Full-screen wizard layout** (big map + ~400px control rail), not a constrained column. | The first-run wizard is a full-window takeover; the ~700px reading-pane constraint is a main-shell rule, not a wizard rule. Operator-approved layout v4. |
| D7 | **The map + diagnostics live in the SHARED component**, so Settings → Location (`LocationSettings`) gets the same experience. | Parent design premise #1: one component, two chromes. Avoids re-doing it in Settings. |
| D8 | **Live readout = grid + pin + acquiring→fixed state. No satellite count / fix-quality.** | Sat count needs gpsd SKY-report parsing for a cosmetic detail; out of scope. gpsd TPV already carries everything D2 needs. |

## Scope of this delivery

In: D1–D8 across the shared location component, its two chromes (wizard `StepLocation`, `LocationSettings`), the offline map integration, the lat/lon backend plumbing, the unconditional-diagnostics rework, and the full `tuxlink-m9ej` pkexec helper + PolicyKit policy + spawner + post-fix UX.

Out (unchanged from parent design): native NMEA reader (`tuxlink-ley0`), live background detection monitoring (`tuxlink-gnws`), Bluetooth NMEA, satellite-count/fix-quality readout, finer-than-4-char broadcast precision (separate opt-in, `tuxlink-dyop`).

---

## Architecture

### Component shape

The location experience is one shared presentational component (the current `GpsSourcePicker`, reworked — name TBD during implementation; "LocationSetup" is clearer but renaming is optional) consumed by two chromes via the shared `useLocationConfig` hook:

- **Wizard:** `StepLocation` — full-screen, big map + right rail, "Set later" / "Continue" footer.
- **Settings:** `LocationSettings` — same component inside the inline Settings overlay.

The component's internal layout is responsive to its container: full-screen two-pane in the wizard; map-above-controls (stacked) when the container is narrow (Settings overlay). Both render the **same** map, source/diagnostic cards, and manual entry — only the outer chrome (stepper/footer vs. overlay) differs.

### Three render regions

1. **Map (D1, D5, D6).** `GridMapPicker mode="pin"`:
   - `grid` prop = the current effective UI grid (live fix grid when source=Gps + fresh; else manual/config grid).
   - **Pin position:** when a fresh GPS fix exists, the marker sits at the *exact* fix lat/lon (D2). Otherwise it sits at the grid-square center (`gridToLatLon(grid)`).
   - Click the map → `latLonToGrid(lat,lon)` → `onGridChange` (pins Manual via `config_set_grid`).
   - **New:** the marker is draggable; drag-end → `latLonToGrid` → `onGridChange` (D5). This is the one net-new map behavior; everything else reuses `GridMapPicker` as-is.
   - A live readout chip over the map shows the grid + source ("from GPS · u-blox 7 · live" / "set manually").

2. **Position source / diagnostics (D3).** Driven by detection probes + the live fix:
   - **Working fix:** a green source card ("GPS receiver — u-blox 7 · live fix → EM75km", "✓ In use"). No diagnostics shown — no nagging.
   - **No working fix:** the diagnostics region renders **unconditionally**, listing every detected blocker independent of device presence:
     - `dialout` not a member → warn + `Show command` (copy) + **`Fix it for me`** (D4).
     - ModemManager active → warn + `Show command` (copy) + **`Fix it for me`** (D4).
     - gpsd reachable → green "ready to relay a fix once a receiver is connected."
     - gpsd unreachable → info "gpsd not running; native serial reading arrives in a later release" (no fix-it; that's `tuxlink-ley0`).
     - No serial device detected → "No receiver detected yet — plug in your USB/serial GPS, then Rescan."
   - `Rescan` re-runs all probes.

3. **Manual entry (D5).** Grid text field (validated via `validateGrid`, pins Manual on commit) + the "…or click/drag the pin on the map" affordance. Always present, always first-class (Mike's path).

### Detection-classification rework (D3)

`classifyGpsSources` (in `gpsProbes.ts`) currently emits triage only when `hasSerial`. Rework so the classifier returns, independent of `hasSerial`:

- `dialout` triage whenever `!dialout.member` (device-independent; you'll need it the moment a device appears).
- ModemManager triage whenever `modemManager.active` (device-independent; it will grab the port on plug-in).
- A `noDevice` state when no serial device and gpsd unreachable.
- Source cards still require an actually-usable source (gpsd reachable, or serial present + in dialout).

The component shows the diagnostics region whenever there is **no live fix and no working source**, i.e. gated on "GPS isn't working," not on "a device is present." When a working source/fix exists, diagnostics are suppressed.

### Backend — lat/lon plumbing (D2)

The on-air privacy boundary is unchanged: only `broadcast_grid` (grid, precision-reduced) ever goes on air. The new lat/lon is **local-display-only** — the operator's own exact position, shown on their own map during setup, never transmitted.

- `position::Fix` gains `lat: f64, lon: f64` (in addition to `grid`, `received`).
- `gpsd::parse_tpv` retains the parsed `lat`/`lon` it already extracts (today it computes the grid and drops them).
- `PositionArbiter` exposes `fresh_fix_latlon() -> Option<(f64, f64)>` — `Some` only when `last_fix` is fresh (reuses the `FIX_STALENESS` check), else `None`.
- `PositionStatusDto` gains `fix_lat: Option<f64>` and `fix_lon: Option<f64>`, populated from `fresh_fix_latlon()` only when `gps_ready` and `gps_state != Off`. The component polls `position_status` (already polled at 2 s) while the step is open; an arriving fix moves the pin and flips "acquiring…" → "fix acquired."

No change to `broadcast_grid` / `ui_grid` / the precision-reduction path.

### Live readout state machine (D8)

While the Location step / Settings → Location is open and source=Gps:

- `gps_ready === false` → "Acquiring GPS fix…" (cold start can take 30–90 s); map shows no GPS pin (manual/config grid square only if set).
- `gps_ready === true` → "Fix acquired" + grid + pin at `(fix_lat, fix_lon)`; map recenters on the fix once.

### One-click "Fix it for me" (D4)

Built per `tuxlink-m9ej` — see `bd show tuxlink-m9ej` and the parent design's [bd-2 section](2026-06-05-gps-setup-ux-design.md) for the canonical spec. Not restated here (propagation contract). Summary of what lands:

- Helper binary `/usr/libexec/tuxlink-gps-fix` with fixed action enum (`add-dialout`, `mask-modemmanager`, `unmask-modemmanager`); refuses unknown actions; requires `$PKEXEC_UID`; no arbitrary execution.
- PolicyKit policy `com.tuxlink.app.policy` (`auth_admin`, annotated exec path).
- `.deb`/`.rpm` bundle wiring for binary + policy; **AppImage graceful degradation** — buttons hide → "Show command" only, since an AppImage cannot register a system PolicyKit policy.
- Tauri spawner: `pkexec /usr/libexec/tuxlink-gps-fix <action>`, exit-code handling (0 ok / 1 failed / 126 auth-dismissed / 127 pkexec-absent), re-runs the relevant probe.
- Post-fix dialout UX: "log out and back in" notice + optional `loginctl terminate-session` (user-session op, no sudo). ModemManager mask is reversible from Settings → Location.

This design's only addition over m9ej's text: the fix-it buttons live in the **unconditional** diagnostics region (D3), so they are reachable before a device is plugged in.

---

## Data flow

```
gpsd TPV ──parse_tpv──▶ Fix{grid,lat,lon,received} ──apply_gps_fix──▶ Arbiter.last_fix
                                                                          │
   position_status (poll 2s) ◀── fix_lat/fix_lon (if fresh) + ui_grid ───┘
                                                                          │
   LocationSetup component ◀── readout + map pin (exact lat/lon) ─────────┘

   map click / pin drag ──latLonToGrid──▶ onGridChange ──config_set_grid──▶ pin Manual
   source card click ─────────────────── onSelectSource ──position_set_source──▶ arbiter
   Fix-it button ──▶ pkexec helper ──▶ re-run probe ──▶ diagnostics refresh
```

## Error handling / edge cases

- **Probe failure** (detection throws) → "Couldn't scan for GPS sources; you can still set your grid manually/on the map." (existing behavior, retained).
- **Invalid mid-typed grid** → inline error, not persisted (existing `validateGrid` gate).
- **GPS fix goes stale while on the step** (`fix_lat`/`fix_lon` become `None`) → readout returns to "Acquiring…"; the last manual/config grid square remains on the map. No transmit implications (broadcast path unchanged).
- **pkexec absent** (exit 127 / minimal install) → fix-it buttons hidden, "Show command" remains (m9ej).
- **AppImage** → fix-it buttons hidden with explanation (m9ej).
- **Map click while source=Gps** → switches to Manual (pins the clicked grid). Consistent with the sticky-Manual config contract; a later GPS fix does not silently override it.
- **Wizard non-blocking invariant** preserved: grid is optional everywhere in onboarding; "Continue" / "Set later" is always available regardless of GPS state.

## Testing

- **`parse_tpv`** retains lat/lon (extend existing unit test asserting the `Fix` carries the input coords).
- **Arbiter** `fresh_fix_latlon()` — `Some` when fresh, `None` when stale/absent (unit).
- **`position_status`** populates `fix_lat`/`fix_lon` only when `gps_ready && gps_state != Off`; `None` when source=Manual or gps_state=Off (command test).
- **`classifyGpsSources`** rework — table test across {dialout member?, MM active?, serial present?, gpsd reachable?}: dialout/MM triage emit independent of `hasSerial`; `noDevice` state correct; source cards only for usable sources (pure unit).
- **Component** — persona paths via mocked `invoke`/poll:
  - Bob: gpsd reachable → green source card, no diagnostics, pin on fix.
  - Dave (no device, broken): dialout + MM triage both visible with fix-it buttons; map+manual usable; Continue available.
  - Mike: manual grid + map pin work with GPS never present.
  - Acquiring → fixed transition moves the pin (poll-driven).
- **Map** — pin drag fires `onGridChange` with the dropped grid (shape test; real drag verified via grim per the map subsystem's existing convention).
- **m9ej** — helper stdout contract, polkit syntax (`pkaction`), spawner exit-code mapping (per m9ej).
- **Reachability (wire-walk):** from a clean install with NO GPS device, the wizard Location step shows the Linux diagnostics (not a blank grid box). This is the regression that motivated the redesign — assert it explicitly.

## Out of scope

Native NMEA reader (`tuxlink-ley0`), live background monitoring + dashboard events (`tuxlink-gnws`), Bluetooth NMEA, satellite-count/fix-quality, sub-4-char broadcast precision (`tuxlink-dyop`), any change to the on-air broadcast/precision path.

## Open questions

None blocking. One polish question deferred: whether the "acquiring fix…" state should show an indeterminate progress affordance or just text — decide during implementation against the real cold-start timing.

## Propagation

Canonical source for this design is this doc + `bd show tuxlink-m9ej`. No CLAUDE.md / AGENTS.md rule changes. New bd issue(s) to be filed at plan time for the confirmation + unconditional-diagnostics work, depending on `tuxlink-9xy1`; `tuxlink-m9ej` is pulled into the same delivery.
