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
| Consent modal placement | On the window hosting the Routines surface; amber badge stays on main | Resolved by a pure function of dock state. Attention is two-channel: desktop notification (cross-backend) + urgency hint (X11 polish) — §6. |
| Vacated-slot treatment | Focus pathways per the visual-pathway principle | Mock-approved 2026-07-15 (companion screens `vacated-states.html`). |
| From-launch buffers | Snapshot handshake extended to chat + positions | `useEnvStations` host/client model copied verbatim; AppShell is always host. |

## 3. Backend — dock registry and window lifecycle

### Registry

A `DockRegistry` managed-state struct owns `DockMode` (`Docked | Popped`) per surface id (`routines`, `tac_map`, `aprs_chat`). Persistence is a new `dock` section in the app config: `#[serde(default)]`, always serialized, `CONFIG_SCHEMA_VERSION` bumped 7→8 with a `MigrateAdditive` path (the `rig` v5 / `onboarding` v7 precedent). Window geometry is NOT stored here — `tauri-plugin-window-state` already persists size and position per window label.

**Runtime authority and persist failure (adrev R2-F1 / Codex-5).** The in-memory registry is authoritative while the app runs; the config write is write-through. A transition mutates the registry, then persists best-effort, then **always** emits `dock:changed` with the registry state — a failed persist (full SD card, read-only FS) never blocks the emit and never lets two windows disagree; it is logged and surfaced as a session-log warning, and its only consequence is a stale layout on next launch. Pop-out mutates the registry only after the window spawn succeeds — a spawn failure changes nothing, emits nothing, and returns the error verbatim.

### Commands

| Command | Behavior |
|---|---|
| `surface_pop_out(surface)` | Spawn (or focus, if live) the surface's window; on spawn success set `Popped`, persist, emit `dock:changed`. |
| `surface_dock_back(surface)` | Set `Docked`, persist, emit `dock:changed`, then **destroy** the window — `destroy()`, never `close()`, which re-fires `CloseRequested` into this same route and loops (`compose_window.rs` documents the footgun). No-op without emit if already `Docked`. |
| `surface_focus(surface)` | Focus the popped window; no state change. Backs every visual-pathway affordance. |
| `dock_state_get()` | Snapshot read for mounting webviews. |

Both mutating commands — and the crash and exit paths below — converge on one transition function (pure core, unit-tested) with a defined contract: registry mutation → best-effort persist → emit, exactly once per *effective* transition; no-op transitions (dock-back on `Docked`, pop-out on a live `Popped` window) emit nothing, which makes concurrent double dock-backs (✕ clicked as main invokes the command) safe by construction.

**Close is dock-back — intercepted in the backend.** The backend `on_window_event` handler catches `CloseRequested` for `pop-*` labels, calls `prevent_close`, and routes to `surface_dock_back` (§12: closing returns the surface inline). This is the main window's proven close-to-tray pattern, NOT the compose window's frontend `onCloseRequested` pattern — the frontend variant depends on a live webview and a registration race, and reintroduces the crash blind spot below (adrev R3-F4). **Exit passthrough (adrev R3-F5):** when the app is exiting (in-app Quit; a WM close-all or session logout delivering `CloseRequested` per toplevel), the handler passes the close through without a transition — otherwise logout would persist every surface `Docked` and silently destroy the popped layout §8 promises to restore.

**Crash detection (adrev R2-F2 / R3-F1 / Codex-2).** A WebKitGTK WebProcess crash kills the content but not the OS window, and no Tauri window event fires — the close path alone is NOT a crash safety net. Each popped window connects the WebKitGTK `web-process-terminated` signal via `WebviewWindow::with_webview` (Linux) and routes it into the same dock-back transition (destroy window, set `Docked`, emit). The consent host (§6) therefore never points at a dead webview beyond signal delivery. If implementation finds the signal unreachable through wry's current surface, the fallback design is liveness-qualified pop-out — `surface_pop_out` on an existing window destroys and respawns instead of focusing — but the signal is the primary design.

### Window spawning — the shared helper

The four existing secondary windows (`compose_window.rs`, `help_window.rs`, `logging_window.rs`, `stations_window.rs`) carry a copy-pasted pattern: main-caller authorization guard, idempotent get-or-focus, `WebviewWindowBuilder` construction, `WindowLabelAlreadyExists` race guard. This work factors that pattern into one `open_secondary_window(app, caller, spec)` helper and the dock windows consume it. Migrating the four existing callers onto the helper is in scope (the helper is only proven general if the existing windows use it); their behavior must not change. The helper's window spec carries an explicit close policy (`CloseSelf` — help/logging/stations; `CommandRouted` — compose; `DockBack` — pop windows) so the migration cannot flatten windows with opposite ✕ semantics into one default (adrev R3-F7).

- Labels: `pop-routines`, `pop-tacmap`, `pop-aprschat`. Routes: `/pop/routines`, `/pop/tacmap`, `/pop/aprschat`.
- Decorations off (custom slim title bar, house style — Help/Logging precedent).
- One capability file per label (no wildcard): `core:event` grants plus the `core:window` drag/resize/minimize/maximize grants for custom chrome. **Capabilities do not ACL custom commands in Tauri 2 (adrev Codex-8)** — the capability file gates bridge/event/window permissions only; per-surface restriction of app commands is enforced Rust-side by caller-label checks (the existing `caller_is_authorized` pattern, extended to admit `pop-*` labels only where the surface legitimately calls the command). Pop-window capability files are written fresh, never cloned from `help.json`/`logging.json`, which grant `core:window:allow-close` — the opposite of the pop windows' close semantics (adrev R3-F7). Close is NOT granted to the webview directly; it routes through `surface_dock_back` (compose-window precedent for self-close discipline).

### Launch restoration and the missing-monitor posture

Restoration is keyed on a new idempotent `shell_mounted` invoke, fired from AppShell's mount effect (adrev R2-F4: "after the shell mounts" was not previously a signal that exists — the first-paint emit fires under the wizard, and the wizard-completion write never fires on ordinary launches). Its first arrival triggers the registry to spawn a window for each surface persisted `Popped`; later arrivals are no-ops. A mid-session wizard exit mounts AppShell and restores then; while the wizard owns the screen, nothing spawns.

**The missing-monitor fallback is deleted, and parent §12 is amended (adrev R3-F3 / Codex-1 / R2-F7).** The originally specified post-spawn geometry check is dead code on both display backends: on X11, `tauri-plugin-window-state` only restores a saved position that intersects a connected monitor (otherwise the WM places the window on-screen), so the check can never observe an off-screen window; on Wayland, a client can neither set nor query window position (`outer_position()` returns (0,0); the project's own `compose_window.rs` documents labwc ignoring `.position()`) and the compositor always places windows on live outputs — the hazard cannot exist and the check's inputs are fiction. A surface saved popped on a now-missing monitor therefore restores **popped, placed on a remaining monitor by the platform**. This preserves §12's actual invariant — never a window stranded off-screen, never an unreachable surface (the visual pathways still focus it) — while dropping its "falls back to docked" letter. The corrective amendment to the parent spec ships on this branch, per this spec's preamble rule.

## 4. Frontend — the popped window

`routing.ts` gains `parsePopRoute(pathname): SurfaceId | null` (same shape as `parseComposeRoute`); `App.tsx` gains one branch mounting `PoppedSurfaceHost`, lazy-loaded like every secondary surface. `parsePopRoute` joins a shared `isSecondaryWindow` predicate used by every main-window-only side effect — first-paint emission suppression and wizard probing included — so a restored pop window that loads before main cannot emit main's first-paint signal or run wizard probes from the wrong window (adrev Codex-9).

`PoppedSurfaceHost` renders, from a three-entry surface registry (`{ id, title, Component, StatusStrip }`):

1. **Slim title bar** — drag region, **⇤ Dock back** button (invokes `surface_dock_back`), surface title ("Routines — Tuxlink"), and minimize/maximize/close controls. The ✕ control also invokes `surface_dock_back` (close is dock-back, §3); minimize/maximize use the window grants. Window controls render at every size (standing project rule).
2. **The surface component** — the same component the main shell mounts inline: `RoutinesSurface`, `AprsPositionsMap` (with its hooks mounted in the host), and for chat `AprsChatPanel` **plus** `AprsConnectStrip`, composed by the host exactly as AppShell's dock header composes them today — the strip stays a separate component owned by the hosting container (adrev Codex-7: folding it into the panel would break the existing APRS ownership model; popping the panel without it would strand a chat window unable to start or stop its own connection).
3. **Mini status strip** (chrome option B) — a thin bottom strip of that surface's own vitals:
   - Routines: parked-consent count · running-run count · next scheduled fire
   - Tac Map: plotted-station count · last-packet age
   - APRS Chat: TX path state · last-heard station

The strip renders from the same hooks the surface already uses; it introduces no new backend channels.

Menu dispatch is main-window-only by design (the F7-recursion lesson), so every popped-window action is an `invoke`, never a menu-action-bus message.

## 5. Main window — vacated slots and pathways

AppShell subscribes to `dock:changed` and renders per surface. Subscription order is mandatory: register the listener FIRST, then read `dock_state_get`, then reconcile (the `useRoutines` discipline, stated there in code) — the launch-restoration window is exactly where a get-then-subscribe gap loses a dock-back emit and strands a permanent pathway to a nonexistent window (adrev R2-F5).

- **Routines popped:** the panes return to the mailbox master-detail; the Routines menu item reads "Routines ↗" and dispatches `surface_focus` instead of swapping the pane. The amber consent badge and StatusBar item remain on main unconditionally.
- **Tac Map popped:** the reading pane returns to messages; the map expand toggle reads "Tac Map ↗ — in window" and focuses. The inline expansion and the popped window are the same surface — never both (move, not clone).
- **APRS Chat popped:** the dock keeps its other tabs; the APRS tab's content is a placeholder ("APRS Chat ↗ — in its own window — click to focus").

Pop-out entry points: an **↗ Pop out** affordance in each surface's existing header (Routines dashboard/designer header, the Tac Map header controls, the APRS chat panel header).

## 6. Consent surfacing

`ConsentGate` splits along the seam it already has:

- **Data:** `useParkedRuns` runs in AppShell always — the amber MenuBar badge and StatusBar item never move, regardless of dock state.
- **Modal:** renders on the window hosting the Routines surface, resolved by a pure function `consentHostWindow(dockState) -> 'main' | 'pop-routines'`; each window renders the modal iff the resolution equals its own label. Both windows compute from the same `dock:changed`-driven snapshot, so disagreement is bounded by event propagation, never by independent bookkeeping (adrev Codex-4). The popped Routines host mounts the modal renderer with its own `useParkedRuns` instance (the hook is already window-agnostic and launch-recovery-safe). The modal's parked-duration display seeds from the run journal's park timestamp, not hook-instance learn-time, so changing host windows cannot reset a Part 97 surface's asserted duration (adrev R2-F8).
- **Attention is two-channel (adrev R3-F2 / Codex-6):** `request_user_attention` is a GTK urgency hint implemented by the X11 backend only — a literal no-op on labwc/Wayland, the default Pi session. It stays as free X11 polish. The guaranteed cross-backend channel is a desktop notification (`tauri-plugin-notification`, already registered) fired by the backend when a run parks while the hosting window is unfocused; the main-window amber badge remains the always-on in-app indicator.
- Docking back while a park is live moves the modal to main on the next render; no handoff protocol exists because the render is a pure function of (dock state, parked runs).

## 7. Data continuity across the move

- **Outbound chat traffic becomes a backend event (adrev R2-F3).** Today `useAprsChat` appends the operator's own sends locally, in the invoking window's instance only — the backend emits no echo for them, and delivery-state events no-op in windows that lack the message. A dock-back would therefore permanently destroy the popped window's record of the operator's own transmitted traffic. The backend gains an own-send echo emission (same channel family as the inbound message event, carrying the msgid the delivery-state events already reference); every window's feed becomes a pure accumulation of backend events. This restores the precondition the snapshot handshake depends on: the host buffer is always a superset.
- **Snapshot handshake, hardened:** `useAprsChat` and `useAprsPositions` gain the `snapshotRole: 'host' | 'client'` handshake `useEnvStations` already implements (request/answer over broadcast events), with one amendment: the client's snapshot request retries on a short backoff until answered or a bounded timeout — the current single-shot request is a lost-request hole when the client mounts before the host listener registers (adrev Codex-3), benign today only because both buffers are empty at process start, which stops being true the moment launch restoration spawns pop windows in parallel with the shell. Hosts answer idempotently. AppShell mounts host; pop-outs mount client and seed on mount.
- **Map transition:** popping the Tac Map is an unmount in main and a fresh mount in the pop window (Leaflet is per-mount; no live re-parent exists). `usePersistedViewport` restores center/zoom, the module-scope packs cache carries installed offline packs, and the `tile://` scheme plus GL environment are per-process — a popped map gets LAN tiles and the same rendering path with zero new wiring. One live map engine total, preserved structurally by move-not-clone — with one accepted transient: dock-back emits before the popped webview is destroyed, so two engine instances can coexist for the teardown interval (sub-second, self-healing); inverting the order would trade this for a blank-map gap, which is worse (adrev R2-F9).
- **Designer dirty state:** pop-out and dock-back of Routines route through the designer's dirty-state guard (save or confirm) so a move cannot discard canvas edits. Whether plan 5 shipped such a guard is verified at planning; if absent, it is added as part of this work, not deferred.

## 8. Edge cases

| Case | Behavior |
|---|---|
| Quit with popped windows | In-app Quit bypasses `CloseRequested`; a WM close-all/logout passes through via the exit flag (§3). Dock state persists `Popped`; next launch restores. |
| Popped webview crashes | `web-process-terminated` routes to the dock-back transition (§3); the surface returns inline. Re-popping respawns fresh. |
| Monitor absent at launch | The platform places the window on a connected monitor; the surface stays popped and reachable via its pathway (§3 — §12 amended). |
| Config persist fails mid-transition | Registry stays authoritative; all windows stay consistent; a warning surfaces in the session log; next launch sees the last successfully persisted layout (§3). |
| First-run wizard active | No restoration; the wizard owns the whole screen. Restoration runs on `shell_mounted` (§3). |
| Pop-out invoked while already popped | `surface_pop_out` is idempotent: focuses the existing live window. |
| Consent parks while popped window minimized | Modal is on the popped window; desktop notification (+ X11 urgency) + main-window badge cover discovery (§6). |
| Second instance of a pop route loaded manually | Same defense as existing windows: label collision focuses the existing window; the route without a registry entry renders nothing. |

## 9. Non-goals

- **Always-on-top, snap layouts, drag-to-dock:** window management belongs to the WM; the buttons are the whole interface.
- **A combined "second screen" shell window** hosting multiple surfaces: contradicts §12 (each surface gets its own OS window) and reintroduces window management inside a window.
- **Popping surfaces beyond the three named:** the registry is the growth path; wiring a fourth surface is adding a registry entry plus its pathway affordances, but none ships in this plan.
- **Cross-window drag of content** (e.g., dragging a message onto the popped map): out of scope entirely.

## 10. Testing

- **Pure-function units (Rust):** dock-registry transition contract — mutation→persist→emit ordering, persist-failure surfacing (emit still fires; warning logged), no-op-transition emit suppression (double dock-back, re-pop on live window), exit-flag close passthrough, spawn-failure leaves state untouched; `shell_mounted` idempotence; config v7→v8 migration (`detect_schema_action` classification + `config_schema_version_tracks_field_set`).
- **Pure-function units (TS):** `parsePopRoute`, `consentHostWindow`, the shared `isSecondaryWindow` predicate.
- **vitest:** `PoppedSurfaceHost` registry mounting and title-bar controls (✕ routes to dock-back, never `window.close()`); AppShell vacated-slot states for all three surfaces; ConsentGate modal/badge split across dock states; own-send echo accumulation (a send in one window's instance is present after remount from events alone); snapshot-handshake client seeding including the retry path (client mounts before host listener).
- **Render harness:** `?view=pop-routines | pop-tacmap | pop-aprschat` fixture families plus the three vacated-slot main-shell states, smoked on real WebKitGTK before merge — the plan-5 lesson stands: this feature's defect class (clipping, flex-crush, font metrics, window chrome) is invisible to jsdom.
- **Live multi-window pass:** on the Pi, pop all three surfaces; run a consent-parking dry-run routine; verify modal placement, badge, desktop notification (urgency hint checked only on X11 — it is a Wayland no-op, adrev R3-F2), dock-back mid-park, quit/relaunch restoration, and one full pop→dock→re-pop churn cycle per surface (the wry blank-webview regression canary; lockfile wry is post-fix but the lifecycle is exactly the reported class). Re-measure marginal window memory with a recreated harness — the parent spec's `dev/scratch` measurement script is gitignored and absent (adrev R3-F6); a popped Tac Map under llvmpipe will exceed the dashboard-grade ~30 MiB figure, and the user-docs note in §12 should carry the measured map number. Dry-run only; no transmission (RADIO-1 untouched — this feature changes nothing about consent semantics, only where the gate renders).

## 11. Adversarial-review record (2026-07-15, rounds 1–3 of 5)

Five-round cycle per build-robust-features; raw transcripts local-only under `dev/adversarial/` (gitignored). Round 1: Codex (GPT-5.5 per ADR 0023). Rounds 2–3: independent Claude reviewers (concurrency lens; platform lens). Every P1/P2 finding is dispositioned as an amendment above, tagged inline (`adrev R2-F1` = round 2 finding 1; `Codex-N` = round 1 finding N). Headline changes forced by the cycle: crash detection rebuilt on `web-process-terminated` (the close-path "safety net" was fiction); attention rebuilt two-channel (urgency hints are a Wayland no-op); the missing-monitor geometry check deleted as dead code on both backends, with the parent §12 letter amended; a backend own-send echo added to chat (dock-back destroyed the operator's own transmitted traffic); the transition function given explicit failure/reentrancy semantics; capability least-privilege corrected to Rust-side caller-label checks. Rounds 4–5 (operator-UX adversary; spec-completeness) ran against the amended spec — record appended below.

## 12. Sequencing note for the implementation plan

The shared window helper + registry + one surface (Routines) prove the mechanism end to end; Tac Map and APRS Chat follow as registry entries plus their continuity work (snapshot handshakes, viewport restore verification). All three ship in this plan (ADR 0022); the ordering exists to front-load the risky mechanism, not to create deferral seams.
