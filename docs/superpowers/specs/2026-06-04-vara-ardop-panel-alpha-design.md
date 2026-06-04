# VARA + ARDOP panel alpha-polish — design

> **Date:** 2026-06-04 · **Author:** willow-mesa-mink (brainstorm with operator) · **Status:** DRAFT
>
> **Scope:** ARDOP HF + VARA HF + VARA FM panels, designed together for consistency. Three intents (cms, p2p, radio-only) × three protocols = 9 alpha-target sidebar combos. Closes operator-reported gaps from the converged-build smoke after PR #348: no coherent outbound flow, fragile listener UX, in-panel intent toggle was a category error.
>
> **bd:** umbrella for tuxlink-fzl7 (VARA Phase 3 — outbound RF dial) + tuxlink-tccc (arming label fix supersession) + a new ARDOP rewire issue. PR #348's backend SessionIntent::P2p caller capability stays; PR #348's frontend "Dial as" toggle gets rolled back.
>
> **Alpha framing:** per memory `alpha-is-vettedness-not-built-ness`, every surface listed below ships fully built or stays excluded entirely. No half-built features. Per memory `no-tuxlink-added-safeguards`, the panel mirrors legacy WLE client behavior — no tuxlink-added bounded-airtime caps, TOT timers, or extra confirmation modals. ADRs are agent-internal and must not inform user-facing behavior.

## 1. The disparity we're addressing

Legacy WLE binds session lifecycle to the **session window**. Operator picks a session type from the main dropdown; a session window opens; for P2P, the inbound listener is configured and armed immediately when the window appears. Within the open window, the operator can also dial outbound. Stop closes the window.

Tuxlink doesn't spawn windows — everything inline per project memory `inline-ui-no-window-clutter`. So tuxlink needs a **session lifecycle button** as the equivalent of "operator opens session window." That button is the operator's "I am now in this mode" intent: it opens the transport, arms the listener (for intents that have one), and unlocks outbound dial. Closing the session tears everything down.

We build the WLE-parity shape first. Once it's walkable, we revisit whether the radio-dock surface benefits from any divergence.

## 2. Architecture decisions

**One panel component, intent-aware.** ARDOP HF + VARA HF + VARA FM share the same React component shell. It reads `intent` from `RadioPanelMode` props (sidebar-driven, not local state) and renders the right surface. No in-panel "Dial as" toggle.

**Sidebar entry IS the intent picker.** `(sessionType, protocol)` from the existing sidebar (`src/connections/sessionTypes.ts`) is the operator's mode choice. `radio-only` flips to `built: true` for ardop-hf, vara-hf, vara-fm.

**Session lifecycle button.** "Open session" / "Close session" at the top of the panel. Open transitions the panel from `closed` → `open`. The button is the operator's explicit consent for everything that happens inside.

**Auto-managed transport.** No separate Open/Close-transport affordance. The session-lifecycle button drives transport open/close as a side effect (TCP open for VARA, ardopcf spawn for ARDOP).

**Listener auto-arms with session open** for `intent ∈ {p2p, radio-only}`. No separate Arm/Disarm button. Closing the session disarms. Allowlist editor lives inside the open-session view and is live-editable; edits apply to subsequent inbound, not the active exchange.

**Outbound dial is within-session.** Once session is open, operator types target + bandwidth and clicks "Connect" to dial out. Connect does: CONNECT → CONNECTED → B2F → DISCONNECT. Returns to "session open / idle" (listener stays armed for p2p/radio-only). Operator can re-dial or close session.

**Mutually exclusive in time.** Modem can only run one ARQ session at once. Connect disabled while an inbound exchange is mid-flight; modem-busy state spans both directions.

**No tuxlink-added safeguards.** Drop the `CONNECT_DEADLINE = 120s` constant in `modem_commands.rs`. Drop the `ConsentModal` component + all "RADIO-1 SAFETY" comment/identifier surface. Drop intent-prefixed modal copy. The modem's own timeouts (VARA's connect timeout, ARDOP's ARQTIMEOUT) stay — those are modem-native. The internal backend token-mint-consume guard can stay as defense against frontend bug-loops, but it doesn't get a UI surface.

**Radio-only listener divergence.** Tuxlink intentionally extends WLE's R-pool client semantics to include accepting inbound R-pool peer sessions. Allowlisted peer connects → B2F runs with `SessionIntent::RadioOnly` → message routing flag tagged `R`. Documented in this spec as the one deliberate divergence.

## 3. Capability matrix

| Intent | Outbound | Listener | Target shape | Creds | Routing flag |
|---|---|---|---|---|---|
| **cms** | Yes | — | RMS callsign (`WB7VYH-10`) | keyring password for `;PQ` | `C` (drains only C-tagged) |
| **p2p** | Yes | Yes (any peer) | Peer callsign (`K7LED-7`) | none | none (unflagged) |
| **radio-only** | Yes | Yes (R-pool peer · tuxlink divergence) | R-pool RMS callsign (`K6RR-10`) | none | `R` (drains only R-tagged) |

Maps to backend `SessionIntent` enum (already exists per `src-tauri/src/winlink/session.rs:109-136`):
- `cms` → `SessionIntent::Cms`
- `p2p` → `SessionIntent::P2p`
- `radio-only` → `SessionIntent::RadioOnly`

`PostOffice` and `Mesh` exist in the backend but are out of scope for alpha (`built: false` in `sessionTypes.ts` stays).

## 4. Panel surface (all states)

```
┌─────────────────────────────────────────────────┐
│ Protocol title · intent pill · status pill      │  ← Header
├─────────────────────────────────────────────────┤
│ [Open session]   (when closed)                  │  ← Session control
│ [Close session]  (when open)                    │
├─────────────────────────────────────────────────┤
│ ⮟ Outbound (only when open)                     │  ← Outbound form
│   Target: ___________ Bandwidth: ▾              │
│   [Connect]                                     │
├─────────────────────────────────────────────────┤
│ ⮟ Listen (only when open AND p2p|radio-only)    │  ← Allowlist editor
│   ☑ Allow any peer                              │
│   K7LED-7 [×]   + callsign                      │
├─────────────────────────────────────────────────┤
│ Session log (always)                            │  ← Rolling event stream
│                                                 │
├─────────────────────────────────────────────────┤
│ ▸ Modem settings (collapsed)                    │  ← Per-protocol settings
└─────────────────────────────────────────────────┘
```

**Header status pill**: `closed` (gray) · `open · idle` (green) · `open · dialing` (yellow) · `open · exchange` (orange) · `open · inbound-exchange` (orange) · `open · error` (red).

**Modem settings expander** (per-protocol content):
- ARDOP: capture device, playback device, PTT serial path, cmd_port, binary path, WebGUI port. Editable while session is closed; locked while session is open (changes need restart).
- VARA: host, cmd_port, data_port, default bandwidth. Same lock-while-open semantics.

**Session log** is the existing `SessionLogSection` (rolling event stream from backend, no changes needed).

## 5. State machine

```
              ┌──────────────┐
              │   closed     │  ← initial state after sidebar nav
              │              │  ← operator clicks "Open session"
              └──────┬───────┘
                     │
        ┌─Open session─┐
        │              ▼
        │     [auto-open transport]
        │     [send LISTEN ON if p2p|radio-only]
        │              │
        │              ▼
        │     ┌──────────────┐
        │     │  open · idle │ ←──────────────────────────┐
        │     └──────┬───────┘                            │
        │            │                                    │
        │            ├─ operator types target + Connect ──┐│
        │            │                                    ▼│
        │            │              ┌──────────────────┐  │
        │            │              │ open · dialing   │  │
        │            │              └──────┬───────────┘  │
        │            │                     │              │
        │            │                     ▼              │
        │            │              ┌──────────────────┐  │
        │            │              │ open · exchange  │──┘
        │            │              └──────────────────┘
        │            │
        │            ├─ inbound peer CONNECTED + allowlist ──┐
        │            │   (p2p/radio-only only)              ▼
        │            │              ┌──────────────────────────┐
        │            │              │ open · inbound-exchange  │──┐
        │            │              └──────────────────────────┘  │
        │            │                                            │
        │            └──── operator clicks Close session ──┐      │
        │                                                  ▼      │
        └────────────────────────────────────► closed ◄───────────┘
                              [send LISTEN OFF, terminate, close transport]
```

Transitions:
- `closed → open · idle`: open transport, arm listener if applicable, render outbound form
- `open · idle → open · dialing → open · exchange → open · idle`: outbound flow
- `open · idle → open · inbound-exchange → open · idle`: inbound flow (only when listener armed)
- `open · {anything} → closed`: tear down

## 6. Implementation plan

### 6.1 Frontend

- **Component shape: shared `RadioSessionPanel`.** Replace `ArdopRadioPanel.tsx` + `VaraRadioPanel.tsx` with a single `RadioSessionPanel` component parameterized by `RadioPanelMode`. Rationale: brainstorm signal was design-them-together-for-consistency, and the surface (header, session control, outbound form, listener section, log, modem expander) is identical across the three protocols modulo per-protocol settings expander content. Per-protocol divergence lives in a small adapter (`ardopAdapter`, `varaHfAdapter`, `varaFmAdapter`) that supplies the Tauri command names + the settings-expander render function. Alternative considered + rejected: keep separate panels with shared sub-components. Rejected because the inconsistency we're fixing comes from divergent panel shapes; sharing the shell is what enforces consistency at compile time.
- The component reads `intent` from props and renders the per-intent surface (no useState for intent).
- Session lifecycle state held locally: `'closed' | 'open-idle' | 'dialing' | 'exchange' | 'inbound-exchange' | 'closing'`.
- "Open session" Tauri command per protocol: `ardop_open_session`, `vara_open_session`. Internally these drive transport-open + listener-arm in one call.
- "Connect" Tauri command per protocol per intent: extends the existing `modem_ardop_b2f_exchange` / new `modem_vara_b2f_exchange` to do connect + exchange + disconnect inside the open session (no separate Connect command — the existing all-in-one exchange path already does CONNECT + B2F + DISCONNECT).
- Allowlist editor: reuse the `AllowedStationsEditor` component shipped in PR #348.
- Remove the existing `ConsentModal` component + all sites that consume it (currently ARDOP's Connect + Send/Receive flows). Operator's click on Connect IS the consent.
- Roll back the in-panel "Dial as" toggle in `ArdopRadioPanel` (introduced in PR #348). Backend `parse_b2f_intent` widens to accept `cms | p2p | radio-only`.
- Widen `RadioPanelMode` types: `kind: 'ardop-hf' | 'vara-hf' | 'vara-fm'; intent: 'cms' | 'p2p' | 'radio-only'`.
- Update `src/radio/radioPanelVisibility.ts:33` mapping to carry `radio-only` (currently narrows everything-not-p2p to `cms`).
- Flip `sessionTypes.ts` `radio-only` to `built: true` for ardop-hf, vara-hf, vara-fm.

### 6.2 Backend

- Drop `CONNECT_DEADLINE` constant in `modem_commands.rs:25` and any code paths that enforce it.
- Drop `ARQ_TIMEOUT_SECS` if it diverges from ARDOP's stated default; audit.
- Drop `ConsentModal`-specific code paths (token-mint-from-frontend isn't gone, but the modal stays out of UI). Internal token-mint-consume guard stays in backend Tauri commands as defense against frontend bug-loops.
- Drop "RADIO-1 SAFETY" code comments + identifier surface. Rephrase as needed (e.g., "operator-click consent gate" or just drop the comment if the logic is self-evident).
- `vara_open_session` Tauri command: opens transport if not already open, arms listener if intent in {p2p, radio-only}.
- `vara_close_session` Tauri command: aborts in-flight, disarms listener, closes transport.
- ARDOP equivalents: `ardop_open_session` / `ardop_close_session`.
- VARA outbound dial: new `modem_vara_b2f_exchange` Tauri command mirroring ARDOP's. Takes target + intent + (no consent_token — internal backend guard handles the dup-call protection without the user-side modal pattern).
- VARA disarm during active exchange (tuxlink-12sc): add the ABORT side-channel so closing the session interrupts B2F immediately. This was already filed as a P2 follow-up; for alpha it must land.

### 6.3 PR-disposition cleanup

- PR #350 (tuxlink-tccc, Arming label fix): supersede. The `disabled` prop on `ListenArmButton` is still useful, but the entire `Arm/Disarm` button surface in `VaraRadioPanel` goes away with auto-arming. Close PR #350 without merging if it hasn't merged yet; if merged, the disabled-prop addition stays useful for any future ListenArmButton consumer.
- PR #348 (tuxlink-9ls2): backend stays, frontend "Dial as" toggle rolls back. New PR rewires the frontend per this spec.

## 7. Out of alpha scope

Per the alpha-vettedness filter (ship fully built or exclude entirely):

- Frequency display (no CAT integration today; not building one now)
- Favorite Channels combo (would be a UX win but post-alpha)
- Signal-quality / bearing / distance toolbar readouts (defer to ARDOP's existing signal panel pattern; not adding for VARA)
- Polling sessions
- Per-target persistence (last-used target per intent)
- Address book / channel database
- Auto-connect schedules (tuxlink-hfft separately)

## 8. Build-walk-revise loop

Per operator framing: build to WLE parity now. Once the operator can walk through it in the dev build, revise based on what doesn't work for tuxlink's radio dock surface (vs WLE's separate-window pattern). The spec captures the parity target; the walk identifies divergences.

Implementation log entry should call out the planned walk + revise step so it doesn't get skipped after merge.

## 9. Watched failure modes

- **Auto-arm of listener could surprise**. Operator opens a P2P session intending to dial out; doesn't realize they're also accepting inbound. Mitigation: session-open log entry explicitly says "listener armed" and the status pill shows it.
- **Modem-busy collisions**. Operator dials out while inbound exchange is mid-flight. Connect button must be disabled with a clear hint, OR queue the outbound for after inbound completes. Pick: disable + hint. Queue adds state we don't need.
- **ARDOP spawn failures auto-bundled into Open Session**. ardopcf can fail at audio-device init, PTT serial, binary-missing. These failures need clean error surfacing on the Open Session response, not buried in the session log after the fact.
- **VARA disarm during active B2F (tuxlink-12sc)**. Must ship the ABORT side-channel before alpha — operator's Close Session click can't take 30s to actually stop transmission.

## 10. Reviewer concerns (self-review)

Spec self-review surfaced one item, resolved inline above:
- **Shared component vs separate components.** §6.1 originally said "replace with a shared RadioSessionPanel" without justifying the choice. Now explicitly: shared component is the decision, with the per-protocol adapter pattern handling the divergence. Alternative considered + rejected.

No unresolved concerns. Spec ready for operator review.
