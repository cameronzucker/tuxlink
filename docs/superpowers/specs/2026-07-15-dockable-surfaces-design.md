# Dockable surfaces — pop-out / dock-back shell capability

- **bd issue:** tuxlink-dmwte (Routines plan 6/6; parent epic tuxlink-03d39)
- **Status:** design approved in brainstorm (operator + agent sandbar-oriole-falcon, 2026-07-15); pending written-spec review and adversarial rounds
- **Parent spec:** [2026-07-13-routines-design.md](2026-07-13-routines-design.md) §12 "Dockable surfaces (shell capability)" is the canonical behavioral contract. This document is the mechanism layer beneath it and does not restate it; where the two could be read to differ, §12 wins and this spec gets a corrective commit.

## 1. Purpose

Operators run multi-monitor stations to place more information under simultaneous attention: a busy Winlink inbox on the main screen, APRS chat and the Tac Map on a second, Routines on either. A surface popped to its own OS window serves higher-volume workflows without obscuring the inbox. The single-laptop bag deployment remains the default; nothing about a docked-only station changes.

Three surfaces ship wired: **Routines**, **Tac Map**, **APRS Chat**. The mechanism is generic; the three consumers and the framework are one feature (ADR 0022, completeness doctrine).

### Design principle — the visual pathway (operator, 2026-07-15)

Every popped surface leaves a visible trace at its original placement, and that trace is the way back: the menu reads "Routines ↗" and focuses the window; the map toggle reads "Tac Map ↗ — in window" and focuses the window; the APRS dock tab renders a click-to-focus placeholder. A user who cannot remember where a window went can always follow the pathway from where the element used to live. No surface ever simply vanishes from the main window's vocabulary. This principle binds future dockable consumers, not just the first three.

## 2. Decisions carried out of the brainstorm

| Decision | Choice | Notes |
|---|---|---|
| Dock-state ownership | Backend (Rust) dock registry | Both windows and the next launch must agree on where a surface lives; frontend copies are views, never owners. Same posture as the radio arbiter. |
| Popped-window chrome | **Option B**: slim custom title bar + surface-scoped mini status strip | Fallback to Option A (bare surface, title bar only) if the strip feels heavy in real use; the strip is a self-contained component per surface, so the rollback is a removal, not a redesign. |
| Consent modal placement | On the window hosting the Routines surface; amber badge stays on main | Resolved by a pure function of dock state. `request_user_attention` fires on the hosting window when a park arrives. |
| Vacated-slot treatment | Focus pathways per the visual-pathway principle | Mock-approved 2026-07-15 (companion screens `vacated-states.html`). |
| From-launch buffers | Snapshot handshake extended to chat + positions | `useEnvStations` host/client model copied verbatim; AppShell is always host. |

## 3. Backend — dock registry and window lifecycle

### Registry

A `DockRegistry` managed-state struct owns `DockMode` (`Docked | Popped`) per surface id (`routines`, `tac_map`, `aprs_chat`). Persistence is a new `dock` section in the app config: `#[serde(default)]`, always serialized, `CONFIG_SCHEMA_VERSION` bumped 7→8 with a `MigrateAdditive` path (the `rig` v5 / `onboarding` v7 precedent). Window geometry is NOT stored here — `tauri-plugin-window-state` already persists size and position per window label.

### Commands

| Command | Behavior |
|---|---|
| `surface_pop_out(surface)` | Spawn (or focus) the surface's window, set `Popped`, persist, emit `dock:changed`. |
| `surface_dock_back(surface)` | Set `Docked`, persist, emit `dock:changed`, then close the window. |
| `surface_focus(surface)` | Focus the popped window; no state change. Backs every visual-pathway affordance. |
| `dock_state_get()` | Snapshot read for mounting webviews. |

Both mutating commands — and the launch-restoration fallback that docks a stranded window back (§3, "Launch restoration") — converge on one transition function (pure, unit-tested) so persist-then-emit ordering cannot diverge per call site.

**Close is dock-back.** A popped window's close request routes to `surface_dock_back` (§12: closing returns the surface inline). This is also the crash safety net: a webview that dies flips its surface to `Docked` rather than stranding it popped-but-invisible.

### Window spawning — the shared helper

The four existing secondary windows (`compose_window.rs`, `help_window.rs`, `logging_window.rs`, `stations_window.rs`) carry a copy-pasted pattern: main-caller authorization guard, idempotent get-or-focus, `WebviewWindowBuilder` construction, `WindowLabelAlreadyExists` race guard. This work factors that pattern into one `open_secondary_window(app, caller, spec)` helper and the dock windows consume it. Migrating the four existing callers onto the helper is in scope (the helper is only proven general if the existing windows use it); their behavior must not change.

- Labels: `pop-routines`, `pop-tacmap`, `pop-aprschat`. Routes: `/pop/routines`, `/pop/tacmap`, `/pop/aprschat`.
- Decorations off (custom slim title bar, house style — Help/Logging precedent).
- One capability file per label (no wildcard): `core:event` grants, the `core:window` drag/resize/minimize/maximize grants for custom chrome, and the invoke grants the hosted surface's hooks actually call (enumerated per surface at planning; least-privilege per the stations-window precedent). Close is NOT granted to the webview directly; it routes through `surface_dock_back` (compose-window precedent for self-close discipline).

### Launch restoration and the missing-monitor fallback

After the main shell mounts (never during the first-run wizard), the registry spawns a window for each surface persisted `Popped`. Post-spawn, a pure geometry check compares the restored outer frame against connected monitors: a window intersecting no monitor is docked back immediately (§12's safety net — never a window stranded off-screen). The check is a pure function of (window rect, monitor rects) and is the unit-test target; the plugin's restore behavior is not otherwise second-guessed.

## 4. Frontend — the popped window

`routing.ts` gains `parsePopRoute(pathname): SurfaceId | null` (same shape as `parseComposeRoute`); `App.tsx` gains one branch mounting `PoppedSurfaceHost`, lazy-loaded like every secondary surface.

`PoppedSurfaceHost` renders, from a three-entry surface registry (`{ id, title, Component, StatusStrip }`):

1. **Slim title bar** — drag region, **⇤ Dock back** button (invokes `surface_dock_back`), surface title ("Routines — Tuxlink"), and minimize/maximize/close controls. The ✕ control also invokes `surface_dock_back` (close is dock-back, §3); minimize/maximize use the window grants. Window controls render at every size (standing project rule).
2. **The surface component** — the same component the main shell mounts inline: `RoutinesSurface`, `AprsPositionsMap` (with its hooks mounted in the host), `AprsChatPanel` (including its connect strip — the strip is part of the surface, not chrome).
3. **Mini status strip** (chrome option B) — a thin bottom strip of that surface's own vitals:
   - Routines: parked-consent count · running-run count · next scheduled fire
   - Tac Map: plotted-station count · last-packet age
   - APRS Chat: TX path state · last-heard station

The strip renders from the same hooks the surface already uses; it introduces no new backend channels.

Menu dispatch is main-window-only by design (the F7-recursion lesson), so every popped-window action is an `invoke`, never a menu-action-bus message.

## 5. Main window — vacated slots and pathways

AppShell subscribes to `dock:changed` (plus an initial `dock_state_get`) and renders per surface:

- **Routines popped:** the panes return to the mailbox master-detail; the Routines menu item reads "Routines ↗" and dispatches `surface_focus` instead of swapping the pane. The amber consent badge and StatusBar item remain on main unconditionally.
- **Tac Map popped:** the reading pane returns to messages; the map expand toggle reads "Tac Map ↗ — in window" and focuses. The inline expansion and the popped window are the same surface — never both (move, not clone).
- **APRS Chat popped:** the dock keeps its other tabs; the APRS tab's content is a placeholder ("APRS Chat ↗ — in its own window — click to focus").

Pop-out entry points: an **↗ Pop out** affordance in each surface's existing header (Routines dashboard/designer header, the Tac Map header controls, the APRS chat panel header).

## 6. Consent surfacing

`ConsentGate` splits along the seam it already has:

- **Data:** `useParkedRuns` runs in AppShell always — the amber MenuBar badge and StatusBar item never move, regardless of dock state.
- **Modal:** renders on the window hosting the Routines surface, resolved by a pure function `consentHostWindow(dockState) -> 'main' | 'pop-routines'`. The popped Routines host mounts the modal renderer with its own `useParkedRuns` instance (the hook is already window-agnostic and launch-recovery-safe).
- **Attention:** when a run parks, the backend calls `request_user_attention` on the hosting window — the taskbar/WM urgency hint that covers "prompt on a powered-off second monitor" beyond the badge.
- Docking back while a park is live moves the modal to main on the next render; no handoff protocol exists because the render is a pure function of (dock state, parked runs).

## 7. Data continuity across the move

- **Snapshot handshake:** `useAprsChat` and `useAprsPositions` gain the `snapshotRole: 'host' | 'client'` handshake `useEnvStations` already implements (request/answer over broadcast events). AppShell mounts host; pop-outs mount client and seed on mount. Without this, a popped chat window opens empty and fills only from new traffic — the defect class the env panel already solved.
- **Map transition:** popping the Tac Map is an unmount in main and a fresh mount in the pop window (Leaflet is per-mount; no live re-parent exists). `usePersistedViewport` restores center/zoom, the module-scope packs cache carries installed offline packs, and the `tile://` scheme plus GL environment are per-process — a popped map gets LAN tiles and the same rendering path with zero new wiring. One live map engine total, preserved structurally by move-not-clone.
- **Designer dirty state:** pop-out and dock-back of Routines route through the designer's dirty-state guard (save or confirm) so a move cannot discard canvas edits. Whether plan 5 shipped such a guard is verified at planning; if absent, it is added as part of this work, not deferred.

## 8. Edge cases

| Case | Behavior |
|---|---|
| Quit with popped windows | Windows close with the app; dock state persists `Popped`; next launch restores. |
| Popped webview crashes | Close path fires → surface docks back. Recoverable by re-popping. |
| Monitor absent at launch | Post-spawn geometry check docks the surface back (§3). |
| First-run wizard active | No restoration; the wizard owns the whole screen. Restoration runs when the shell mounts. |
| Pop-out invoked while already popped | `surface_pop_out` is idempotent: focuses the existing window. |
| Consent parks while popped window minimized | Modal is on the popped window; urgency hint + main-window badge cover discovery. |
| Second instance of a pop route loaded manually | Same defense as existing windows: label collision focuses the existing window; the route without a registry entry renders nothing. |

## 9. Non-goals

- **Always-on-top, snap layouts, drag-to-dock:** window management belongs to the WM; the buttons are the whole interface.
- **A combined "second screen" shell window** hosting multiple surfaces: contradicts §12 (each surface gets its own OS window) and reintroduces window management inside a window.
- **Popping surfaces beyond the three named:** the registry is the growth path; wiring a fourth surface is adding a registry entry plus its pathway affordances, but none ships in this plan.
- **Cross-window drag of content** (e.g., dragging a message onto the popped map): out of scope entirely.

## 10. Testing

- **Pure-function units (Rust):** dock-registry transition function (persist-then-emit ordering, idempotent pop), missing-monitor geometry check, config v7→v8 migration (`detect_schema_action` classification + `config_schema_version_tracks_field_set`).
- **Pure-function units (TS):** `parsePopRoute`, `consentHostWindow`.
- **vitest:** `PoppedSurfaceHost` registry mounting and title-bar controls; AppShell vacated-slot states for all three surfaces; ConsentGate modal/badge split across dock states; snapshot-handshake client seeding for chat and positions (env-panel test shape).
- **Render harness:** `?view=pop-routines | pop-tacmap | pop-aprschat` fixture families plus the three vacated-slot main-shell states, smoked on real WebKitGTK before merge — the plan-5 lesson stands: this feature's defect class (clipping, flex-crush, font metrics, window chrome) is invisible to jsdom.
- **Live multi-window pass:** on the Pi, pop all three surfaces, run a consent-parking dry-run routine, verify modal placement, badge, urgency hint, dock-back mid-park, and quit/relaunch restoration. Dry-run only; no transmission (RADIO-1 untouched — this feature changes nothing about consent semantics, only where the gate renders).

## 11. Sequencing note for the implementation plan

The shared window helper + registry + one surface (Routines) prove the mechanism end to end; Tac Map and APRS Chat follow as registry entries plus their continuity work (snapshot handshakes, viewport restore verification). All three ship in this plan (ADR 0022); the ordering exists to front-load the risky mechanism, not to create deferral seams.
