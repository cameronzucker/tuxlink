# Dockable surfaces — pop-out / dock-back shell capability

- **bd issue:** tuxlink-dmwte (Routines plan 6/6; parent epic tuxlink-03d39)
- **Status:** design approved in brainstorm (operator + agent sandbar-oriole-falcon, 2026-07-15); hardened by the full 5-round adversarial cycle (§11); pending operator review of the post-adrev amendments
- **Parent spec:** [2026-07-13-routines-design.md](2026-07-13-routines-design.md) §12 "Dockable surfaces (shell capability)" is the canonical behavioral contract. This document is the mechanism layer beneath it and does not restate it; where the two could be read to differ, parent §12 wins and this spec gets a corrective commit. Two parent-§12 letters are amended on this branch (AMD-1 missing-monitor, AMD-2 close semantics), both preserving the parent's underlying invariants; see §3.

## 1. Purpose

Operators run multi-monitor stations to place more information under simultaneous attention: a busy Winlink inbox on the main screen, APRS chat and the Tac Map on a second, Routines on either. A surface popped to its own OS window serves higher-volume workflows without obscuring the inbox. The single-laptop bag deployment remains the default; nothing about a docked-only station changes.

Three surfaces ship wired: **Routines**, **Tac Map**, **APRS Chat**. The mechanism is generic; the three consumers and the framework are one feature (ADR 0022, completeness doctrine).

### Design principle — the visual pathway (operator, 2026-07-15)

Every popped surface leaves a visible trace at its original placement, and that trace is the way back: the menu reads "Routines ↗" and focuses the window; the map toggle reads "Tac Map ↗ — in window" and focuses the window; the APRS dock tab renders a click-to-focus placeholder. A user who cannot remember where a window went can always follow the pathway from where the element used to live. No surface ever simply vanishes from the main window's vocabulary. Pathway affordances always carry their text label — never icon-only. This principle binds future dockable consumers, not just the first three.

### Design principle — transitions preserve the operator's work and attention (adrev round 4)

A dock transition moves the surface without destroying what the operator was doing in it (state travels — §7) and without commandeering what the operator is doing elsewhere (✕ never steals the main pane — §3). Mechanically clean transitions that cost the operator their canvas edits or their inbox focus are defects, not simplifications.

## 2. Decisions carried out of the brainstorm and the adversarial cycle

| Decision | Choice | Notes |
|---|---|---|
| Dock-state ownership | Backend (Rust) dock registry | Both windows and the next launch must agree on where a surface lives; frontend copies are views, never owners. Same posture as the radio arbiter. |
| Popped-window chrome | **Option B**: slim custom title bar + surface-scoped mini status strip | Fallback to Option A (bare surface, title bar only) if the strip feels heavy in real use; the strip is a self-contained component per surface, so the rollback is a removal, not a redesign. |
| ⇤ vs ✕ (adrev R4-F1) | Both set `Docked`; they differ in main-window presentation | ⇤ Dock back **foregrounds** the surface at its inline placement. ✕ returns it to **availability** — pathways revert to their normal forms — without stealing the main pane, reading pane, or active dock tab. Parent §12 AMD-2. |
| Surface-state continuity (adrev R4-F3) | Opaque per-surface context token travels with every transition | Routines carries its view + draft; chat and map ignore it (they have event/viewport continuity already). Eliminates both the designer reset and the need for any dirty-prompt. |
| Consent modal placement | On the window hosting the Routines surface; amber badge stays on main, click routes on dock state | Backend resolution is canonical; TS mirrors with a parity fixture (§6). Attention: desktop notification (cross-backend) + urgency hint (X11 polish). |
| Vacated-slot treatment | Focus pathways per the visual-pathway principle, plus a main-side dock-back action per surface | Mock-approved 2026-07-15 (`vacated-states.html`); main-side dock-back added by adrev R5-F12 (mid-session recovery). |
| From-launch buffers | Snapshot handshake (hardened) + backend own-send echo for chat | §7. |
| Session logout | Honest limitation: per-toplevel WM closes are indistinguishable from operator ✕; surfaces persist `Docked` | The popped layout is one ↗ per surface to rebuild. No exit-flag machinery ships (adrev R5-F2 killed it: its only necessary case has no detectable trigger). |

## 3. Backend — dock registry and window lifecycle

### The wire contract (adrev R5-F1 — normative)

One mapping table, stated once, binding every context:

| Rust variant | Wire string (invoke args, events, config keys) | TS type value | Window label | Route | Window title |
|---|---|---|---|---|---|
| `SurfaceId::Routines` | `"routines"` | `'routines'` | `pop-routines` | `/pop/routines` | `Routines — Tuxlink` |
| `SurfaceId::TacMap` | `"tac_map"` | `'tac_map'` | `pop-tacmap` | `/pop/tacmap` | `Tac Map — Tuxlink` |
| `SurfaceId::AprsChat` | `"aprs_chat"` | `'aprs_chat'` | `pop-aprschat` | `/pop/aprschat` | `APRS Chat — Tuxlink` |

The label/route forms drop the underscore (irregular — do not derive; read this table). Serialization is pinned with explicit `#[serde(rename_all = "snake_case")]` on `SurfaceId` and `DockMode` plus a shape test (the standing serde-enum rule). Titles are static — the Routines title does not change when the designer is open inside the window.

**`dock:changed` payload and `dock_state_get` return the same full snapshot** (never deltas — windows replace wholesale, which makes a missed event self-healing at the next one):

```json
{
  "surfaces": { "routines": "popped", "tac_map": "docked", "aprs_chat": "docked" },
  "context": { "routines": { "view": "designer", "routine": "morning-ics-cycle", "tab": "design", "draft": { } } }
}
```

`context` holds each surface's opaque continuity token (§7) from its most recent transition, `null` when none; the destination host consumes it at mount and the registry clears it on the next transition of that surface. The config `dock` section persists only the `surfaces` map, exactly as spelled above (context is runtime-only): this JSON literal is the v8 schema shape, and `CONFIG_SCHEMA_VERSION` bumps 7→8 with a `MigrateAdditive` path (`rig` v5 / `onboarding` v7 precedent). Window geometry is NOT stored here — `tauri-plugin-window-state` persists size and position per window label.

### Registry

A `DockRegistry` managed-state struct owns the snapshot above. **Runtime authority and persist failure (adrev R2-F1 / Codex-5):** the in-memory registry is authoritative while the app runs; the config write is write-through. A transition mutates the registry, then persists best-effort, then **always** emits `dock:changed` with the registry snapshot — a failed persist (full SD card, read-only FS) never blocks the emit and never lets two windows disagree; it is logged and surfaced as a session-log warning, and its only consequence is a stale layout on next launch. Pop-out mutates the registry only after the window spawn succeeds — a spawn failure changes nothing, emits nothing, and returns the error verbatim.

### Commands

| Command | Behavior |
|---|---|
| `surface_pop_out(surface, context?)` | Spawn (or focus, if live) the surface's window; on spawn success set `Popped`, store `context`, persist, emit `dock:changed`. |
| `surface_dock_back(surface, context?)` | Set `Docked`, store `context`, persist, emit `dock:changed`, then **destroy** the window — `destroy()`, never `close()`, which re-fires `CloseRequested` into this same route and loops (`compose_window.rs` documents the footgun). No-op without emit if already `Docked`. |
| `surface_focus(surface)` | Unminimize + raise + activate the popped window (§5, "Focus semantics"); no state change. Backs every visual-pathway affordance. |
| `dock_state_get()` | The full snapshot, for mounting webviews (listen first, then get — §5). |

Both mutating commands — and the crash path below — converge on one transition function (pure core, unit-tested) with a defined contract: registry mutation → best-effort persist → emit, exactly once per *effective* transition; no-op transitions (dock-back on `Docked`, pop-out on a live `Popped` window) emit nothing, which makes concurrent double dock-backs (✕ clicked as main invokes the command) safe by construction.

### Close handling (adrev R4-F1 / R4-F2 / R3-F4 / R3-F5 / R5-F2)

The backend `on_window_event` handler catches `CloseRequested` for `pop-*` labels, calls `prevent_close`, and runs the **close-intent round-trip**: emit a close-intent event to that window's webview; the webview flushes its continuity token by invoking `surface_dock_back(surface, context)` (✕ semantics — availability, not foreground); a bounded liveness timeout (1.5 s) falls through to `surface_dock_back(surface, None)` so a hung webview cannot wedge the close. This is backend-intercepted (the main window's proven close-to-tray pattern), NOT the compose window's frontend `onCloseRequested` pattern, which depends on a live webview and a registration race (adrev R3-F4) — the round-trip is an optimization for state carry on top of the backend intercept, and the timeout plus the crash signal below keep the dead-webview cases airtight.

**Session logout / WM close-all is accepted as lossy-to-Docked (adrev R5-F2, superseding R3-F5's exit flag).** From the backend, a logout delivering `CloseRequested` per toplevel is byte-identical to the operator clicking ✕ on each window, and no reliable session-end signal is plumbed through Tauri — the round-1–3 exit-flag design had no implementable setter for its only necessary case, and shipped as specified it would have re-created the very failure it targeted. Honest behavior instead: logout docks every surface back (`Docked` persists); next launch is fully functional with the surfaces inline, one ↗ per surface to rebuild the layout; runs die `interrupted` and launch recovery surfaces them (the graceful-quit prompt is an in-app-quit feature and is knowingly bypassed on logout — adrev R4-F11). In-app Quit and tray Quit call `app.exit(0)`, which bypasses `CloseRequested` entirely, so the popped layout persists and restores across normal quit/relaunch — the §8 promise holds on every path the app controls.

**Crash detection (adrev R2-F2 / R3-F1 / Codex-2).** A WebKitGTK WebProcess crash kills the content but not the OS window, and no Tauri window event fires — the close path alone is NOT a crash safety net. Each popped window connects the WebKitGTK `web-process-terminated` signal via `WebviewWindow::with_webview` (Linux) and routes it into the same dock-back transition (destroy window, set `Docked`, emit; context lost — edits since last save are lost, accepted and stated). The consent host (§6) therefore never points at a dead webview beyond signal delivery. **If implementation finds the signal unreachable through wry's current surface, that is a stop-and-escalate condition — not a branch a subagent takes alone** (adrev R5-F5: the previously sketched destroy-and-respawn fallback contradicted three normative statements and shipped un-reviewed design decisions).

### Window spawning — the shared helper

The four existing secondary windows (`compose_window.rs`, `help_window.rs`, `logging_window.rs`, `stations_window.rs`) carry a copy-pasted pattern: main-caller authorization guard, idempotent get-or-focus, `WebviewWindowBuilder` construction, `WindowLabelAlreadyExists` race guard. This work factors that pattern into one `open_secondary_window(app, caller, spec)` helper and the dock windows consume it. Migrating the four existing callers onto the helper is in scope (the helper is only proven general if the existing windows use it); their behavior must not change. The helper's window spec carries an explicit close policy (`CloseSelf` — help/logging/stations; `CommandRouted` — compose; `DockBack` — pop windows) so the migration cannot flatten windows with opposite ✕ semantics into one default (adrev R3-F7).

- Labels and routes: per the §3 wire table. Decorations off (custom slim title bar, house style — Help/Logging precedent).
- **First-spawn default sizes (adrev R4-F12):** Tac Map 1100×750, Routines 960×680, APRS Chat 440×640; thereafter `tauri-plugin-window-state` owns geometry per label. Placement is WM territory (§9).
- One capability file per label (no wildcard): `core:event` grants plus the `core:window` drag/resize/minimize/maximize grants for custom chrome. **Capabilities do not ACL custom commands in Tauri 2 (adrev Codex-8)** — the capability file gates bridge/event/window permissions only; per-surface restriction of app commands is enforced Rust-side by caller-label checks (the existing `caller_is_authorized` pattern, extended to admit `pop-*` labels only where the surface legitimately calls the command). Pop-window capability files are written fresh, never cloned from `help.json`/`logging.json`, which grant `core:window:allow-close` — the opposite of the pop windows' close semantics (adrev R3-F7).

### Launch restoration and the missing-monitor posture

Restoration is keyed on a new idempotent `shell_mounted` invoke, fired from AppShell's mount effect (adrev R2-F4: "after the shell mounts" was not previously a signal that exists — the first-paint emit fires under the wizard, and the wizard-completion write never fires on ordinary launches). Its first arrival triggers the registry to spawn a window for each surface persisted `Popped`; later arrivals are no-ops. A mid-session wizard exit mounts AppShell and restores then; while the wizard owns the screen, nothing spawns.

**The missing-monitor fallback is deleted, and parent §12 is amended — AMD-1 (adrev R3-F3 / Codex-1 / R2-F7).** The originally specified post-spawn geometry check is dead code on both display backends: on X11, `tauri-plugin-window-state` only restores a saved position that intersects a connected monitor (otherwise the WM places the window on-screen), so the check can never observe an off-screen window; on Wayland, a client can neither set nor query window position (`outer_position()` returns (0,0); the project's own `compose_window.rs` documents labwc ignoring `.position()`) and the compositor always places windows on live outputs — the hazard cannot exist and the check's inputs are fiction. A surface saved popped on a now-missing monitor therefore restores **popped, placed on a remaining monitor by the platform**. **This guarantee is launch-time (adrev R5-F12):** a mid-session monitor unplug on X11 can leave a live window at off-screen coordinates that focus alone cannot recover; the main-side dock-back actions (§5) are the designed recovery, so the surface stays reachable without relaunch.

## 4. Frontend — the popped window

`routing.ts` gains `parsePopRoute(pathname): SurfaceId | null` (same shape as `parseComposeRoute`); `App.tsx` gains one branch mounting `PoppedSurfaceHost`, lazy-loaded like every secondary surface. `parsePopRoute` joins a shared `isSecondaryWindow` predicate used by every main-window-only side effect — first-paint emission suppression and wizard probing included — so a restored pop window that loads before main cannot emit main's first-paint signal or run wizard probes from the wrong window (adrev Codex-9).

`PoppedSurfaceHost` renders, from a three-entry surface registry (`{ id, title, Component, StatusStrip, defaultSize }`):

1. **Slim title bar** — drag region, **⇤ Dock back** button (invokes `surface_dock_back` with the continuity token, foreground semantics), surface title (per the §3 wire table), and minimize/maximize/close controls. The ✕ control routes through the same close-intent path as a WM close (availability semantics, §3); minimize/maximize use the window grants. Window controls render at every size (standing project rule). **Keyboard and accessibility (adrev R4-F7):** the title-bar controls are tab-reachable buttons with accessible names ("Dock back into main window", …), and the popped webview binds **Ctrl+W → dock-back (✕ semantics)** — semantically honest, since close *is* dock-back.
2. **The surface component** — the same component the main shell mounts inline: `RoutinesSurface` (mounted on the continuity token's view when present — §7), `AprsPositionsMap` (with its hooks mounted in the host), and for chat `AprsChatPanel` **plus** `AprsConnectStrip`, composed by the host exactly as AppShell's dock header composes them today — the strip stays a separate component owned by the hosting container (adrev Codex-7: folding it into the panel would break the existing APRS ownership model; popping the panel without it would strand a chat window unable to start or stop its own connection).
3. **Mini status strip** (chrome option B) — a thin bottom strip of that surface's own vitals, never duplicating a vital the surface already shows in the same window (adrev R4-F8):
   - Routines: parked-consent count · running-run count · next scheduled fire
   - Tac Map: last-packet age, live-ticking (a frozen "2 min ago" misleads about channel liveness); plotted-station count only if implementation confirms the map's filter bar does not already show it
   - APRS Chat: last-heard station · unread count (NOT TX path state — `AprsConnectStrip` at the top of the same window already owns it, and two renderings of one vital will momentarily disagree during transitions)

**Theme propagation (adrev R5-F9):** the color scheme applies per window from shared `localStorage` at mount; a mid-session scheme change in main reaches long-lived popped windows via the cross-window `storage` event (plus the custom-theme token re-injection), so popped windows restyle live rather than staying stale until respawn.

Menu dispatch is main-window-only by design (the F7-recursion lesson), so every popped-window action is an `invoke`, never a menu-action-bus message.

## 5. Main window — vacated slots, pathways, and verbs

AppShell subscribes to `dock:changed` and renders per surface. Subscription order is mandatory: register the listener FIRST, then read `dock_state_get`, then reconcile (the `useRoutines` discipline, stated there in code) — the launch-restoration window is exactly where a get-then-subscribe gap loses a dock-back emit and strands a permanent pathway to a nonexistent window (adrev R2-F5).

**Focus semantics (adrev R4-F4 / R5-F19):** `surface_focus` unminimizes, raises, and activates. On Wayland, cross-toplevel activation requires an xdg-activation token; implementation designs the token path explicitly, and the §10 live pass verifies pathway-click → raise on labwc AND X11 — this is the single most load-bearing call in the feature and previously had zero coverage. The same-monitor affordance-feedback gap (operator on monitor 1 clicks a pathway; the change happens on monitor 2, nothing visible near the cursor) is **accepted for v1** and recorded here; a focus-flash cue is deliberate non-scope (§9).

Per-surface states while popped, each with BOTH a focus pathway and a dock-back action (the dock-back action is the mid-session recovery §3's launch-time guarantee doesn't cover — adrev R5-F12):

- **Routines popped:** panes return to the mailbox; the Routines menu shows "Routines ↗" (focuses) and a second item "Dock Routines back" (dock-back, foreground semantics). The amber consent badge and StatusBar item remain on main unconditionally; **their click routes on dock state (adrev R4-F5 / R5-F8):** hosting = main → the existing reopen signal; hosting = popped → `surface_focus('routines')`. **Menu verbs targeting a popped surface (adrev R4-F6):** "New Routine…" (and any future verb) focuses the window and forwards the intent as a surface-scoped event the popped host consumes — the generic rule for this class; a menu item never silently no-ops and never opens a second inline copy.
- **Tac Map popped:** the reading pane returns to messages; the map expand toggle reads "Tac Map ↗ — in window" (focuses) with an adjacent "⇤ dock back" action. ⇤ foregrounds: it sets the APRS dock open and the map expanded (the inline placement's two preconditions), restoring the inline map.
- **APRS Chat popped:** the dock keeps its other tabs; the APRS tab's placeholder ("APRS Chat ↗ — in its own window — click to focus") carries a "⇤ dock back" link. **AppShell flows that programmatically open the dock to reach the connect strip (first-run listening setup, connect-failure retry) become dock-state-aware and focus the popped window instead of escorting the operator to the placeholder (adrev R4-F9).**

**Dock-back presentation rule (adrev R4-F1 / R5-F6):** ⇤ (from either window) foregrounds the surface at its inline placement — Routines pane opens (on the token's view), map expands, APRS tab activates. ✕ / Ctrl+W / WM close returns the surface to availability only — every pathway reverts to its normal form, and the main pane, reading pane, and active dock tab are never commandeered. Parent §12's "closing it returns it inline" letter is amended accordingly — **AMD-2** — preserving its invariant (the surface is inline-available again, nothing vanished) while removing the pane-theft reading; the operator who closes a window to clear a monitor does not lose the inbox they were protecting.

Pop-out entry points: an **↗ Pop out** affordance (text-labeled) in each surface's existing header — Routines dashboard AND designer header (popping from the designer carries the designer view in the token), the Tac Map header controls, the APRS chat panel header.

## 6. Consent surfacing

`ConsentGate` splits along the seam it already has:

- **Data:** `useParkedRuns` runs in AppShell always — the amber MenuBar badge and StatusBar item never move, and their click routes on dock state (§5).
- **Modal:** renders on the window hosting the Routines surface, resolved by a pure function `consentHostWindow(dockState) -> 'main' | 'pop-routines'`; each window renders the modal iff the resolution equals its own label. Both windows compute from the same `dock:changed`-driven snapshot, so disagreement is bounded by event propagation, never by independent bookkeeping (adrev Codex-4). **The Rust-side resolution is canonical** — the backend must resolve the hosting window anyway to fire the notification — **and the TS function mirrors it against a shared cross-checked fixture** (adrev R5-F10; the PARITY-1 pitfall shape). The popped Routines host mounts the modal renderer with its own `useParkedRuns` instance (the hook is already window-agnostic and launch-recovery-safe). The modal's parked-duration display seeds from the run journal's park timestamp, not hook-instance learn-time, so changing host windows cannot reset a Part 97 surface's asserted duration (adrev R2-F8).
- **Attention is two-channel (adrev R3-F2 / Codex-6):** `request_user_attention` is a GTK urgency hint implemented by the X11 backend only — a literal no-op on labwc/Wayland, the default Pi session. It stays as free X11 polish. The guaranteed cross-backend channel is a desktop notification (`tauri-plugin-notification`, already registered) fired by the backend when a run parks while the hosting window is unfocused (focus evaluated at park time — accepted). **The notification path depends on a freedesktop notification daemon; the §10 live pass verifies presence and the no-daemon behavior on the reference image (adrev R4-F5)** — if the reference session lacks a daemon, the badge-click routing (§5) is the fallback discovery channel and `consentHostWindow` gains no visibility term until the live pass proves the need (adrev R4-F10).
- Docking back while a park is live moves the modal to main on the next render; no handoff protocol exists because the render is a pure function of (dock state, parked runs).
- **Quit prompt (adrev R4-F11):** the graceful-quit prompt names awaiting-consent runs distinctly ("1 routine waiting for transmit consent") so the operator knows a run is one Confirm from its purpose before choosing to kill it.

## 7. Data continuity across the move

- **The continuity token (adrev R4-F2 / R4-F3).** Every transition carries an optional opaque per-surface context blob, supplied by the vacating host and delivered to the destination host (§3 wire contract). Routines' token is its serializable view state — `{view, routine, tab}` plus the in-progress draft — so popping from the designer opens the popped window *in that designer view with the draft intact*, and dock-back returns it the same way. This makes the previously specified dirty-guard prompt unnecessary for dock transitions (nothing is discarded; the plan-5 designer ships no such guard — `RoutineDesigner.tsx` says "No modal" — and none is added: prompts on save-free transitions would contradict the parent spec's "save never blocks" doctrine). Crash-path token loss = edits since last save lost; accepted and stated (§3). Chat and map ignore the token — their continuity is below.
- **Outbound chat traffic becomes a backend event (adrev R2-F3 / R5-F7).** Today `useAprsChat` appends the operator's own sends locally, in the invoking window's instance only — the backend emits no echo, so a dock-back would permanently destroy the popped window's record of the operator's own transmitted traffic. The backend emits a new **`aprs-message:sent`** event at `aprs_send` acceptance (the same point the optimistic local append fires today), payload: msgid, recipient, text, backend-clock timestamp. The sending window keeps its optimistic append and dedupes the echo by msgid; every other window appends from the echo. The invariant is precise: **every window's feed is reconstructible from backend events alone** (the local append is a latency optimization, never a source of unique truth). Delivery-state events keep keying on msgid and now apply in every window.
- **Snapshot handshake, hardened:** `useAprsChat` and `useAprsPositions` gain the `snapshotRole: 'host' | 'client'` handshake `useEnvStations` already implements (request/answer over broadcast events), with one amendment: the client's snapshot request retries every 250 ms until answered, bounded at 3 s (adrev Codex-3 — the current single-shot request is a lost-request hole when the client mounts before the host listener registers; benign today only because both buffers are empty at process start, which stops being true the moment launch restoration spawns pop windows in parallel with the shell). Hosts answer idempotently. AppShell mounts host; pop-outs mount client and seed on mount.
- **Map transition:** popping the Tac Map is an unmount in main and a fresh mount in the pop window (Leaflet is per-mount; no live re-parent exists). `usePersistedViewport` restores center/zoom, the module-scope packs cache carries installed offline packs, and the `tile://` scheme plus GL environment are per-process — a popped map gets LAN tiles and the same rendering path with zero new wiring. One live map engine total, preserved structurally by move-not-clone — with one accepted transient: dock-back emits before the popped webview is destroyed, so two engine instances can coexist for the teardown interval (sub-second, self-healing); inverting the order would trade this for a blank-map gap, which is worse (adrev R2-F9).

## 8. Edge cases

| Case | Behavior |
|---|---|
| Quit with popped windows | In-app/tray Quit calls `app.exit(0)`, bypassing `CloseRequested`; dock state persists `Popped`; next launch restores. |
| WM close-all / session logout | Indistinguishable from operator ✕ per window (adrev R5-F2); surfaces dock back to availability, `Docked` persists, runs die `interrupted` and launch recovery surfaces them. Layout rebuilds at one ↗ per surface. |
| Popped webview crashes | `web-process-terminated` routes to the dock-back transition (§3); the surface returns inline (availability); context token lost — edits since last save lost, stated. Re-popping respawns fresh. |
| Monitor absent at launch | The platform places the window on a connected monitor; the surface stays popped and reachable (§3 — parent §12 AMD-1). |
| Monitor unplugged mid-session (X11) | The window may sit off-screen; focus alone cannot recover it — the main-side dock-back actions (§5) are the recovery. |
| Config persist fails mid-transition | Registry stays authoritative; all windows stay consistent; a warning surfaces in the session log; next launch sees the last successfully persisted layout (§3). |
| First-run wizard active | No restoration; the wizard owns the whole screen. Restoration runs on `shell_mounted` (§3). |
| Pop-out invoked while already popped | `surface_pop_out` is idempotent: focuses the existing live window. |
| Consent parks while popped window minimized/buried | Modal on the hosting window; desktop notification (+ X11 urgency) + main badge whose click focuses/unminimizes the hosting window (§5, §6). |
| Main hidden to tray, Routines docked, run parks | Modal renders in the hidden main window; the notification is the discovery channel; operator restores main from tray. Verified in the §10 live pass (adrev R4-F10). |
| Second instance of a pop route loaded manually | Same defense as existing windows: label collision focuses the existing window; the route without a registry entry renders nothing. |

## 9. Non-goals

- **Always-on-top, snap layouts, drag-to-dock:** window management belongs to the WM; the buttons are the whole interface.
- **A combined "second screen" shell window** hosting multiple surfaces: contradicts parent §12 (each surface gets its own OS window) and reintroduces window management inside a window.
- **Focus-flash / attention cue on pathway click:** the same-monitor feedback gap is accepted for v1 (§5); revisit only on operator report.
- **Popping surfaces beyond the three named:** the registry is the growth path; wiring a fourth surface is adding a registry entry plus its pathway affordances, but none ships in this plan.
- **Cross-window drag of content** (e.g., dragging a message onto the popped map): out of scope entirely.

## 10. Testing

- **Pure-function units (Rust):** dock-registry transition contract — mutation→persist→emit ordering, persist-failure surfacing (emit still fires; warning logged), no-op-transition emit suppression (double dock-back, re-pop on live window), spawn-failure leaves state untouched, context-token store/clear lifecycle; `shell_mounted` idempotence; `consentHostWindow` resolution (canonical) + the park-notification decision (dock-state resolution × focus × fire) (adrev R5-F17); `aprs-message:sent` emission (payload shape, msgid identity with delivery-state events) (adrev R5-F7); config v7→v8 migration (`detect_schema_action` classification + `config_schema_version_tracks_field_set`).
- **Wire-shape parity (adrev R5-F11 — testing-pitfalls §7, the k61j composed-seam class):** serialize the Rust `dock:changed`/`dock_state_get` types and command args; assert the JSON against TS-side fixtures in both directions, including the `SurfaceId`/`DockMode` rename shape test and the shared `consentHostWindow` parity fixture (§6).
- **Pure-function units (TS):** `parsePopRoute`, `consentHostWindow` (mirroring the shared fixture), the shared `isSecondaryWindow` predicate.
- **vitest:** `PoppedSurfaceHost` registry mounting and title-bar controls (⇤ carries the token with foreground semantics; ✕/Ctrl+W route close-intent, never `window.close()`; controls tab-reachable with accessible names); AppShell vacated-slot states for all three surfaces including the main-side dock-back actions and dock-state-aware badge click + dock-opening flows; ConsentGate modal/badge split across dock states; Routines token round-trip (pop from designer → popped host mounts designer view with draft; dock back → inline restores it); own-send echo (a send is present in a second window's feed from events alone; sender dedupes by msgid); snapshot-handshake client seeding including the retry path (client mounts before host listener; 250 ms/3 s bounds).
- **Render harness:** `?view=pop-routines | pop-tacmap | pop-aprschat` fixture families, the three vacated-slot main-shell states, AND the three docked-state headers with their ↗ affordances (adrev R5-F18 — the affordance is a WebKitGTK chrome/flex-crush candidate), smoked on real WebKitGTK before merge — the plan-5 lesson stands: this feature's defect class is invisible to jsdom.
- **Live multi-window pass:** on the Pi, pop all three surfaces; run a consent-parking dry-run routine; verify modal placement, badge-click routing in both dock states, desktop notification **including daemon presence/absence on the reference image** (urgency hint checked only on X11 — Wayland no-op, adrev R3-F2), **pathway-click focus/raise/unminimize on labwc AND X11 (adrev R4-F4 — the feature's most load-bearing call)**, dock-back mid-park, ⇤-vs-✕ presentation difference, quit/relaunch restoration, live theme change reaching a popped window, main-to-tray consent discovery (adrev R4-F10), and one full pop→dock→re-pop churn cycle per surface (the wry blank-webview regression canary; lockfile wry is post-fix but the lifecycle is exactly the reported class). Re-measure marginal window memory with a recreated harness — the parent spec's `dev/scratch` measurement script is gitignored and absent (adrev R3-F6); a popped Tac Map under llvmpipe will exceed the dashboard-grade ~30 MiB figure, and the user-docs note in parent §12 should carry the measured map number. Dry-run only; no transmission (RADIO-1 untouched — this feature changes nothing about consent semantics, only where the gate renders).

## 11. Adversarial-review record (2026-07-15, 5 rounds)

Five-round cycle per build-robust-features; raw transcripts local-only under `dev/adversarial/` (gitignored). Round 1: Codex (GPT-5.5 per ADR 0023), 9 findings. Round 2: Claude, concurrency/state lens, 9 findings. Round 3: Claude, platform lens (Tauri/WebKitGTK/Wayland), 7 findings. Round 4: Claude, operator-experience lens, 12 findings. Round 5: Claude, spec-completeness/subagent-readiness audit, 19 findings + ADR 0022 judgments + wire-walk table. Every P1/P2 is dispositioned as an inline-tagged amendment (`adrev R<round>-F<n>` / `Codex-<n>`); no finding was rejected without a stated reason in its disposition commit.

Headline design changes forced by the cycle: crash detection rebuilt on `web-process-terminated` (the close-path "safety net" was fiction); ⇤/✕ intent split with parent §12 AMD-2 (✕ was pane-theft); the per-surface continuity token (pop/dock previously demolished the designer); attention rebuilt two-channel with daemon verification (urgency hints are a Wayland no-op); the missing-monitor geometry check deleted as dead code with parent §12 AMD-1, plus main-side dock-back actions for the mid-session case; the exit-flag design deleted as unimplementable and logout honestly accepted as dock-to-availability; the full wire contract pinned (payload, serialization table, config literal); backend own-send echo for chat; badge-click routing on dock state; focus semantics defined and put under live-pass coverage.

## 12. Sequencing note for the implementation plan

The shared window helper + registry + one surface (Routines) prove the mechanism end to end; Tac Map and APRS Chat follow as registry entries plus their continuity work (snapshot handshakes, viewport restore verification, own-send echo). All three ship in this plan (ADR 0022); the ordering exists to front-load the risky mechanism, not to create deferral seams.
