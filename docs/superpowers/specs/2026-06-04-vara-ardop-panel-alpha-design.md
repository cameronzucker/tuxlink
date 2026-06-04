# VARA + ARDOP panel alpha-polish — design

> **Date:** 2026-06-04 · **Author:** willow-mesa-mink (brainstorm) → mink-harrier-cardinal (plan + Codex round 1) → cypress-glade-peregrine (operator-decisions + revision) · **Status:** REVISED (post-Codex round 1)
>
> **Revision history:**
> - 2026-06-04 v1 — willow-mesa-mink brainstorm with operator. PR #360 merged spec + plan + mocks.
> - 2026-06-04 v2 — mink-harrier-cardinal ran Codex Round 1 of the 5-round cross-provider adversarial review; 6 P1 + 5 P2 findings filed (bd tuxlink-8gq3 / qtgg / d8bq / fl6e).
> - 2026-06-04 v3 — cypress-glade-peregrine: operator answered the three shape decisions (drop backend consent gate; modem-native airtime bound; ship radio-only listener as designed), this revision applies Codex Round 1's plan-fix bundle. Codex rounds 2-5 pending.
>
> **Scope:** ARDOP HF + VARA HF + VARA FM panels, designed together for consistency. Three intents (cms, p2p, radio-only) × three protocols = 9 alpha-target sidebar combos. Closes operator-reported gaps from the converged-build smoke after PR #348: no coherent outbound flow, fragile listener UX, in-panel intent toggle was a category error.
>
> **bd:** umbrella for tuxlink-fzl7 (VARA Phase 3 — outbound RF dial) + tuxlink-tccc (arming label fix supersession) + a new ARDOP rewire issue. PR #348's backend SessionIntent::P2p caller capability stays; PR #348's frontend "Dial as" toggle gets rolled back.
>
> **Alpha framing:** per memory `alpha-is-vettedness-not-built-ness`, every surface listed below ships fully built or stays excluded entirely. No half-built features. Per memory `no-tuxlink-added-safeguards`, the panel mirrors legacy WLE client behavior — no tuxlink-added bounded-airtime caps, TOT timers, or extra confirmation modals. ADRs are agent-internal and must not inform user-facing behavior.

## 1. The disparity we're addressing

Legacy WLE binds session lifecycle to a **session window**. Operator picks a session type from the main dropdown; a session window opens; within the open window the operator dials outbound and, for some session types, accepts inbound. Stop closes the window. The exact inbound-listener behavior varies by transport and intent — per the repo's WLE-parity correction, non-Telnet transports (AX.25/ARDOP/VARA) receive inbound sessions through the TNC/modem layer, not a uniform application-level listener bound to the window. CMS, P2P, and radio-only windows differ further in how (and whether) they accept inbound at all. The WLE-evidence-backed claim is "the session window is the operator's mode boundary" — it is NOT "P2P auto-arms the listener at window-open across all transports."

Tuxlink doesn't spawn windows — everything inline per project memory `inline-ui-no-window-clutter`. So tuxlink needs a **session lifecycle button** as the equivalent of "operator opens session window." That button is the operator's "I am now in this mode" intent: it opens the transport, unlocks outbound dial, and — for intents that have an inbound side (p2p, radio-only) — arms the listener.

**Auto-arming the listener on Open Session is a tuxlink design choice**, not a position derived from decompiled WLE evidence for every (transport, intent) pair. The brainstorm signal was "operator picks a mode, the panel reflects that mode" — auto-arm executes that signal without making the operator click two buttons to enter the mode they already picked from the sidebar. The build-walk-revise loop (§8) is the gate for revisiting this if walking the dev build signals a different shape.

We build a WLE-shaped lifecycle first. Once it's walkable, we revisit whether the radio-dock surface benefits from divergence.

## 2. Architecture decisions

**One panel component, intent-aware.** ARDOP HF + VARA HF + VARA FM share the same React component shell. It reads `intent` from `RadioPanelMode` props (sidebar-driven, not local state) and renders the right surface. No in-panel "Dial as" toggle.

**Sidebar entry IS the intent picker.** `(sessionType, protocol)` from the existing sidebar (`src/connections/sessionTypes.ts`) is the operator's mode choice. `radio-only` flips to `built: true` for ardop-hf, vara-hf, vara-fm.

**Session lifecycle button.** "Open session" / "Close session" at the top of the panel. Open transitions the panel from `closed` → `open`. The button is the operator's explicit consent for everything that happens inside.

**Auto-managed transport.** No separate Open/Close-transport affordance. The session-lifecycle button drives transport open/close as a side effect. For VARA this means **TCP open only** — the cmd-port is connected so the modem will receive commands, but no `CONNECT <target>` is sent. For ARDOP this means **ardopcf spawn only** — the modem process is brought up and the TCP cmd-port is open, but no `ARQCALL` is sent. The keyed ARQ link is established lazily by the Connect step, not by Open Session.

**Listener auto-arms with session open** for `intent ∈ {p2p, radio-only}`. No separate Arm/Disarm button. Closing the session disarms. Allowlist editor lives inside the open-session view and is live-editable; edits apply to subsequent inbound, not the active exchange.

**Outbound dial is within-session.** Once session is open, operator types target + bandwidth and clicks "Connect" to dial out. Connect does: CONNECT (ARQCALL for ARDOP / `CONNECT <call>` for VARA) → CONNECTED → B2F → DISCONNECT. Returns to "session open / idle" (listener stays armed for p2p/radio-only). Operator can re-dial or close session. Both the existing `modem_ardop_b2f_exchange` and the new `modem_vara_b2f_exchange` perform `connect_arq` (or the VARA equivalent) inside the same command — open-session does not pre-connect.

**Backend session arbiter.** Listener auto-arm and within-session outbound dial both contend for the modem's single transport. The current ARDOP/VARA listener consumers take ownership of the transport for the entire armed window via `take_transport` / `return_transport`. Open Session must NOT race the listener consumer; outbound dial must be able to interrupt the armed window briefly to take the transport, run the exchange, then return it. The arbiter (a small backend coordinator inside `ModemSession` / `VaraSession`) sequences this as: armed-idle → outbound-takes-transport → exchange → outbound-returns-transport → armed-idle. There is no concurrent listener-and-outbound — the arbiter enforces strict mutual exclusion in time. Inbound exchange in-flight blocks outbound dial with "modem busy"; outbound exchange in-flight pauses the listener (in the sense that the consumer doesn't have the transport) without disarming it. This arbiter is a **prerequisite** for the shared-panel work, not an emergent property of it.

**Mutually exclusive in time.** Modem can only run one ARQ session at once. Connect disabled while an inbound exchange is mid-flight; modem-busy state spans both directions. The session arbiter (above) is what enforces this.

**No tuxlink-added safeguards.** Drop the `CONNECT_DEADLINE = 120s` constant in `modem_commands.rs` and do NOT replace it with any tuxlink-side wall-clock cap (no 600s "TCP-wedge guard"). The bound on keyed airtime is the operator's ABORT-button click plus the modem's own internal timeouts (VARA's connect timeout, ARDOP's `ARQTIMEOUT`) — those are modem-native. This requires the VARA ABORT side-channel (§6.2, formerly tuxlink-12sc) to ship as a hard precondition: ABORT must reliably halt TX within ~2s when clicked. Drop the `ConsentModal` component + the entire backend `mint_consent_token` / `consume_consent_token` token-mint-consume guard + all "RADIO-1 SAFETY" comment/identifier surface + intent-prefixed modal copy. The operator's click on the Connect / Open Session / Send/Receive button IS the Part 97 consent (per memory `radio1-governs-tx-not-ui`); no in-process token round-trip is required. Operator decisions 2026-06-04 (bd tuxlink-8gq3 + tuxlink-qtgg) ratify this posture against Codex Round 1's defense-in-depth arguments.

**Radio-only listener divergence.** Tuxlink intentionally extends WLE's R-pool client semantics to include accepting inbound R-pool peer sessions. Allowlisted peer connects → B2F runs with `SessionIntent::RadioOnly` → message routing flag tagged `R`. **This is a tuxlink design choice, not WLE-evidence-backed.** Decompiled WLE evidence frames Radio-only as a client/RMS-Relay routing mode (`RoutingFlag::R`), not as an end-user peer listener. Allowing inbound R-pool peer answers turns tuxlink into a leaf-relay-shaped node that can tag messages into the WLE Hybrid pool in ways unmodified WLE peers may not expect. The allowlist (default `allow_all=true` per project memory `allowed-stations-default-true`; per-callsign restrictions opt-in) keeps the operational surface explicit. Operator decision 2026-06-04 (bd tuxlink-d8bq) is to ship this divergence as designed in alpha, with the divergence labeled in the panel UI ("Accept inbound R-pool peer · tuxlink divergence") and the build-walk-revise loop as the gate for revisiting if real-world operation shows interop friction.

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

- **Component shape: shared `RadioSessionPanel`.** Replace `ArdopRadioPanel.tsx` + `VaraRadioPanel.tsx` with a single `RadioSessionPanel` component parameterized by `RadioPanelMode`. Rationale: brainstorm signal was design-them-together-for-consistency, and the surface (header, session control, outbound form, listener section, log, modem expander) is identical across the three protocols modulo per-protocol settings expander content. Per-protocol divergence lives in a small adapter (`ardopAdapter`, `varaHfAdapter`, `varaFmAdapter`) that supplies the Tauri command names, the per-protocol `transportKind` value, and the settings-expander render function. Alternative considered + rejected: keep separate panels with shared sub-components. Rejected because the inconsistency we're fixing comes from divergent panel shapes; sharing the shell is what enforces consistency at compile time.
- **Per-protocol adapters carry `transportKind`.** Each adapter (`ardopAdapter`, `varaHfAdapter`, `varaFmAdapter`) supplies a `transportKind: TransportKind` value matching the backend `TransportKind` enum (`Ardop` / `VaraHf` / `VaraFm`). This value is passed to every Tauri command that touches the listener layer (`vara_open_session`, `vara_close_session`, listener-arm RPCs) so VARA FM sessions are not silently logged + gated as VARA HF.
- The component reads `intent` from props and renders the per-intent surface (no useState for intent).
- **Lifecycle state is backend-driven, not local.** The shared panel subscribes to backend status events (`modem_get_status` for ARDOP, `vara_status` for VARA) and derives the lifecycle state from the live backend snapshot. Local `useState` is allowed only for transient UI affordances (pending-button-spinner, in-flight form values) — NEVER as the source of truth for "is the session open." This means hot reload, window remount, ardopcf crash, VARA socket drop, and rapid Open/Close sequences all recover to the right UI without a manual refresh. Open/Close buttons disable while opening/closing; the lifecycle state machine includes `crash-recovery` / `socket-lost` terminal states distinct from `closed`.
- "Open session" Tauri command per protocol: `ardop_open_session`, `vara_open_session`. Internally these drive transport-open + listener-arm in one call (transport-open meaning TCP connect for VARA, ardopcf spawn + cmd-port open for ARDOP — NOT `CONNECT` / `ARQCALL`). The arbiter in §2 sequences listener-arm and outbound dial within the open window.
- "Connect" Tauri command (outbound dial) does CONNECT + B2F + DISCONNECT inside the open session. For ARDOP: extends `modem_ardop_b2f_exchange` to perform `connect_arq(target, ...)` BEFORE the B2F handshake, so the ARQ link is brought up by Connect, not by Open Session. (Codex Round 1 P1 #1 corrects an earlier framing that read Open Session as also performing `connect_arq`.) For VARA: new `modem_vara_b2f_exchange` Tauri command mirroring the same shape — send `CONNECT <target>` on cmd-port, wait for connected, run B2F, send `DISCONNECT`.
- Allowlist editor: reuse the `AllowedStationsEditor` component shipped in PR #348.
- Remove the existing `ConsentModal` component + all sites that consume it (currently ARDOP's Connect + Send/Receive flows). Operator's click on Connect IS the consent.
- Roll back the in-panel "Dial as" toggle in `ArdopRadioPanel` (introduced in PR #348). Backend `parse_b2f_intent` widens to accept `cms | p2p | radio-only`.
- Widen `RadioPanelMode` types: `kind: 'ardop-hf' | 'vara-hf' | 'vara-fm'; intent: 'cms' | 'p2p' | 'radio-only'`.
- Update `src/radio/radioPanelVisibility.ts:33` mapping to carry `radio-only` (currently narrows everything-not-p2p to `cms`).
- Flip `sessionTypes.ts` `radio-only` to `built: true` for ardop-hf, vara-hf, vara-fm.

### 6.2 Backend

- Drop `CONNECT_DEADLINE` constant in `modem_commands.rs:25` and any code paths that enforce it. **Do not introduce a replacement wall-clock cap.** The bound on keyed airtime is the operator's ABORT-button click plus the modem's native timeouts (ARDOP `ARQTIMEOUT`, VARA's connect timeout). Operator decision 2026-06-04 (bd tuxlink-qtgg).
- Drop `ARQ_TIMEOUT_SECS` if it diverges from ARDOP's stated default; audit.
- Drop the **entire** `ConsentModal` UI surface AND the backend `mint_consent_token` / `consume_consent_token` token-mint-consume guard. The frontend `useConsent` hook + the backend `ModemSession::{mint,consume}_consent_token` pair + the `modem_mint_consent` Tauri command + the `consent_token: String` parameter on `modem_ardop_connect` and `modem_ardop_b2f_exchange` all go away. The operator's click on Connect / Open Session / Send/Receive IS the Part 97 consent — no in-process token round-trip is required. Frontend bug-loops (the original concern motivating the backend guard) are now defended by the in-process busy-bit (a session can only run one connect / one exchange at a time) plus the backend status snapshot the frontend reads from. Operator decision 2026-06-04 (bd tuxlink-8gq3).
- Drop "RADIO-1 SAFETY" code comments + identifier surface. Drop intent-prefixed modal copy. Rephrase as needed (or just drop the comment if the logic is self-evident).
- **Extend the existing `SessionIntent` enum** at `src-tauri/src/winlink/session.rs:109-136` (variants `Cms` / `RadioOnly` / `PostOffice` / `Mesh` / `P2p`). Add serde derives if missing. Add an `auto_arms_listener(&self) -> bool` method returning true for `RadioOnly` and `P2p`. Add or preserve `routing_flag(&self) -> Option<RoutingFlag>` for `R`-tagging on RadioOnly. Do **NOT** create a new `session_intent.rs` file with a duplicate enum. (Codex Round 1 P1 #6 corrects an earlier framing that assumed the enum was absent.)
- `vara_open_session(intent, transport_kind)` Tauri command: opens TCP cmd-port if not already open, arms listener via the arbiter if intent in {p2p, radio-only}. Does NOT send `CONNECT`.
- `vara_close_session()` Tauri command: aborts in-flight via the ABORT side-channel, disarms listener, closes TCP cmd-port.
- ARDOP equivalents: `ardop_open_session(intent)` / `ardop_close_session()`. Open spawns ardopcf + opens cmd-port; does NOT send `ARQCALL`.
- VARA outbound dial: new `modem_vara_b2f_exchange(target, intent, transport_kind)` Tauri command mirroring ARDOP's. Sends `CONNECT <target>` on cmd-port, awaits connected, runs B2F, sends `DISCONNECT`. No `consent_token` parameter.
- **VARA ABORT side-channel** (tuxlink-12sc, amended by Codex Round 1 P1 #4): the VARA command codec already models `Abort` as `ABORT` (hard tear-down — interrupts in-flight TX) distinct from `Disconnect` as `DISCONNECT` (graceful — waits for current burst). The `abort_in_flight` writer MUST send `ABORT\r`, not `DISCONNECT\r`. The spec's "must interrupt within ~2s" requirement is satisfied by ABORT; DISCONNECT can wait for the current burst and miss the deadline. Optionally follow ABORT with DISCONNECT to release the slot cleanly; if so, ABORT is the FIRST cmd on the wire. Test must capture ABORT as the first command.
- **Backend session arbiter** (Codex Round 1 P1 #5): add a per-modem coordinator inside `ModemSession` / `VaraSession` that serializes transport ownership across the listener consumer task and outbound exchange. The arbiter's contract: at any moment, exactly ONE of {listener-consumer, outbound-exchange} owns the transport; transitions are atomic; the listener consumer yields the transport to outbound on Connect and reclaims it after the exchange returns. This is a **prerequisite** for Phase 5 shared-panel work — without it, auto-arm + outbound dial race and either lose inbound while dialing or starve outbound while armed. See plan Phase 4 (new) for the implementation tasks.

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
- **Transport ownership race during outbound dial** (Codex Round 1 P1 #5). The ARDOP and VARA listener consumer tasks take ownership of the transport for the entire armed window via `take_transport` / `return_transport`. If outbound dial naively grabs the transport, it either finds it absent (consumer holds it) or pulls it out from under the consumer (disarming the listener mid-window). Mitigation: backend session arbiter (§2 + §6.2) sequences listener-armed-idle → outbound-takes-transport → exchange → outbound-returns-transport → listener-armed-idle as a single atomic state transition. Outbound and inbound are mutually exclusive in time within the armed window; the arbiter enforces this.
- **ABORT cmd correctness** (Codex Round 1 P1 #4). Sending VARA `DISCONNECT\r` instead of `ABORT\r` for the safety side-channel can wait for the current burst (~30s on slow modes) and miss the "~2s interrupt" requirement. The VARA command codec already distinguishes Abort (hard) from Disconnect (graceful). Tests must capture ABORT as the first wire byte sequence on Close Session.
- **Lifecycle desync from local React state** (Codex Round 1 P2 #9). Storing `'closed' | 'open-idle' | ...` purely in React `useState` leaves the UI lying after hot reload, ardopcf crash, VARA socket drop, or rapid Open/Close sequences. Mitigation: lifecycle derived from backend status (`modem_get_status`, `vara_status`) via subscription; local state used only for transient form/affordance state.
- **VARA FM vs HF transport-kind confusion** (Codex Round 1 P2 #10). Backend listener layer distinguishes `TransportKind::VaraHf` from `TransportKind::VaraFm`; if the shared adapter sends only `{ intent }` without `transportKind`, VARA FM sessions are silently logged + gated as VARA HF. Mitigation: per-protocol adapter carries `transportKind`; Tauri command signatures accept it; tests cover both HF and FM paths.
- **ARDOP spawn failures auto-bundled into Open Session**. ardopcf can fail at audio-device init, PTT serial, binary-missing. These failures need clean error surfacing on the Open Session response, not buried in the session log after the fact.
- **VARA disarm during active B2F (tuxlink-12sc)**. Must ship the ABORT side-channel before alpha — operator's Close Session click can't take 30s to actually stop transmission. (Same root as the ABORT-cmd-correctness item above; called out separately because the original tuxlink-12sc framing landed before Codex's correction.)
- **Radio-only listener WLE-interop friction** (Codex Round 1 P2 #8). Accepting inbound R-pool peer sessions is a tuxlink design choice (operator decision bd tuxlink-d8bq, ship as designed). Real-world WLE peers may not expect a leaf-relay-shaped node tagging messages into the Hybrid pool. Mitigation: divergence is explicitly labeled in UI ("tuxlink divergence"); allowlist defaults to allow-all (per project memory `allowed-stations-default-true`) but the operator surface for tightening it is visible; build-walk-revise loop is the gate for revisiting if interop friction shows up on-air.

## 10. Reviewer concerns (self-review + Codex Round 1 + operator decisions)

Spec self-review surfaced one item, resolved inline above:
- **Shared component vs separate components.** §6.1 originally said "replace with a shared RadioSessionPanel" without justifying the choice. Now explicitly: shared component is the decision, with the per-protocol adapter pattern handling the divergence. Alternative considered + rejected.

**Codex Round 1 cross-provider adversarial review (2026-06-04)** surfaced 6 P1 + 5 P2 findings against an earlier draft of this spec and its companion plan. Resolutions:

| Finding | Severity | Where | Resolution |
|---|---|---|---|
| ARDOP Connect-step location ambiguity | P1 | spec §6.1, plan Task 3.5/3.6 | §6.1 rewritten: Connect button performs `connect_arq` + B2F + DISCONNECT; Open Session spawns ardopcf only. |
| Backend consent gate dropped without replacement | P1 | spec §2, plan Phase 1 | Operator decision bd tuxlink-8gq3: drop backend `consume_consent_token` entirely. Operator click IS Part 97 consent (per memory `radio1-governs-tx-not-ui`). |
| 600s airtime cap replaces 120s | P1 | spec §2, plan Task 1.5 | Operator decision bd tuxlink-qtgg: no tuxlink-side cap. Bound is ABORT button + modem-native timeouts. ABORT must ship reliably as precondition. |
| VARA DISCONNECT used as ABORT | P1 | spec §6.2, plan Task 4.1 | Rewritten: send `ABORT\r` (hard tear-down). VARA codec already models Abort vs Disconnect separately. Test captures ABORT as first wire cmd. |
| Auto-arm vs transport ownership race | P1 | spec §2, plan Phase 5 | New "Backend session arbiter" added to §2 + §6.2. New plan Phase (transport arbitration) precedes shared-panel work. |
| Duplicate `SessionIntent` enum | P1 | plan Task 3.1 | Plan Task 3.1 rewritten: extend the existing enum at `src-tauri/src/winlink/session.rs`, add serde derives + methods. No new file. |
| Auto-arm-as-WLE-parity framing | P2 | spec §1 | §1 rewritten to label auto-arm as a tuxlink design choice; per-transport nuance acknowledged. |
| Radio-only listener WLE-Hybrid-pool concern | P2 | spec §2 | Operator decision bd tuxlink-d8bq: ship as designed with divergence labeled in UI + spec. Build-walk-revise loop is the revisit gate. |
| React-local lifecycle state | P2 | spec §6.1, plan Phase 5 | §6.1 rewritten: lifecycle derived from backend status subscription. |
| varaHfAdapter / varaFmAdapter command name collision | P2 | spec §6.1, plan Phase 5 | §6.1 rewritten: adapter carries `transportKind`; commands accept it. Tests cover both HF and FM. |
| Phase 1 pushes interim commits with broken tests | P2 | plan front matter | Plan reordered: each phase ends green; "expect broken tests" framing removed; safeguard-strip happens after replacement code lands. |

No unresolved concerns from Round 1. Spec ready for plan revision (tuxlink-fl6e) → Codex rounds 2-5 → subagent-driven implementation.
