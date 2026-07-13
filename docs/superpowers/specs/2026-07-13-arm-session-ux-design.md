# Design — ARDOP/VARA arm, dial, and session UX (one-click Connect, scoped Stop)

Date: 2026-07-13
Author: dune-willow-clover
bd: tuxlink-r788i (primary) · tuxlink-nvgjy (VARA two-click) · tuxlink-kw873 (P0: disarm doesn't stick) · tuxlink-4u43s (P0: disarm doesn't abort in-flight TX)
Status: DRAFT — pending operator design + render approval. NO transport code until approved (operator instruction on tuxlink-r788i).

## Problem

Four defects share one root: the radio panels conflate three distinct
operator intents — "accept inbound" (arm), "dial out" (connect+exchange),
and "make it stop" — into a two-button footer keyed on the wrong predicate.

1. **Arming looks like a session** (tuxlink-r788i a). `ardop_listen` spawns
   ardopcf when stopped, and `ArdopRadioPanel`'s footer keys ONLY on
   `isStopped` (ArdopRadioPanel.tsx:743, 1494), so arming the listener flips
   the footer to the live-session Send/Receive + Stop layout.
2. **Stop tears down the armed listener's modem while the arm record stays
   armed** (tuxlink-r788i b, tuxlink-kw873): `onStopClick` calls
   `modem_ardop_disconnect`; the listener believes it is armed but its modem
   is gone. Conversely the arm record re-enables without operator
   interaction. Arm state does not track operator intent.
3. **The two-step Start → Send/Receive encodes RADIO-1 agent-consent framing
   into product UX** (tuxlink-r788i c, tuxlink-nvgjy). VaraRadioPanel.tsx:214
   says it outright: "the operator's later Send/Receive click is the Part 97
   consent gate." RADIO-1 governs *agent* behavior, never product UX
   (memory `radio1-scope`). WLE operators read Start as the dial. Worse,
   ARDOP's Start (`modem_ardop_connect`) performs the full ON-AIR ARQ
   connect and then idles holding the channel until Send/Receive; VARA's
   Start only opens the local TCP transport. Packet and Telnet dial in one
   action. Two transports have a consent-theater split; two do not.
4. **Disarm is not a kill switch** (tuxlink-4u43s): an operator DISARM does
   not abort an in-flight agent transmit session. The operator's mental
   model — confirmed live on the R2 2026-07-12 — is that disarming the
   station stops the station.

A latent fifth: VARA's Send/Receive is *disabled* while the listener is
armed ("Disarm the listener first — it owns the VARA transport",
VaraRadioPanel.tsx:1013). An armed VARA station cannot dial out at all;
arm and outbound are mutually exclusive in the UI even though the arm
record is meant to persist.

## Grounding — what exists today (origin/main a858291d)

- Footers: ArdopRadioPanel.tsx §radio-panel-act (~1494): `isStopped ?
  [Start] : [Send/Receive, Stop]` (+ Open WebGUI). VaraRadioPanel.tsx
  (~975): `!isOpen ? [Start] : [Send / Receive, Stop]`. Deliberately
  mirrored per tuxlink-n95sr #3 ("no UX divergence between ARDOP and
  VARA") — this design keeps that mirror, with a corrected state machine.
- The one-click chain ALREADY EXISTS: `connectFor`
  (src/connections/connectDispatch.ts:184) runs ARDOP
  `modem_ardop_connect → modem_ardop_b2f_exchange` and VARA
  `vara_open_session → modem_vara_b2f_exchange` as one action for the
  ribbon, with honest-outcome recording. Only the panels split it.
- `ListenArmButton` + allowlist section is a shared, working arm surface;
  it stays. Intent auto-arm (`SessionIntent::auto_arms_listener`,
  WLE-grounded) stays.
- Backend connect flow invariants (consent token, bounded airtime, abort
  writer, PTT pump) are specified in
  docs/superpowers/specs/2026-07-09-vara-connect-flow-design.md and
  implemented for the agent path; this design changes which *frontend*
  actions drive them, not the invariants.

## Goals

- One click dials: Connect = connect + B2F exchange + disconnect, all
  transports identical (WLE parity; Packet/Telnet already behave this way).
- The footer states match reality: stopped, listener-armed, and
  session-active are visually and behaviorally distinct.
- Stop is scoped to the thing it stops: Disarm ends the armed window;
  Abort ends a session. Disarm during a session is the kill switch
  (aborts, then disarms).
- Arm state is durable operator intent: it survives an outbound session
  and never silently re-enables or silently dies.

## Non-goals

- No change to the backend RADIO-1 consent-token architecture for agents.
- No change to intent auto-arm semantics (spec'd, WLE-grounded).
- No rig/CAT/frequency work (tuxlink-9pzaj, tuxlink-ntzzk are separate).
- Not this session: ANY code. This document + renders gate first.

## Approaches considered

**A. Minimal predicate fix** — keep Start → Send/Receive, key the footer on
armed vs session instead of `isStopped`. Fixes the "arming looks like a
session" symptom only. Rejected: leaves the never-approved two-step in
place (the operator's core complaint), leaves ARDOP's channel-holding idle
connect, leaves disarm semantics broken.

**B. One-click Connect; panels adopt the ribbon chain; tri-state footer;
scoped Stop.** (RECOMMENDED — detailed below.) Unifies all four transports
on one dial semantic, removes the consent-theater split, and gives arm its
own lifecycle. Largest UI delta, but every piece lands on an
already-proven backend path (`connectFor`'s chain).

**C. Panels stop dialing; the ribbon becomes the only dial surface.**
Panels become config + monitor + arm surfaces. Rejected: the panel is
where target entry, QSY candidates, and per-mode fidelity live; the ribbon
deliberately reuses the panel's persisted target. Removing the panel dial
orphans target-entry UX and diverges from WLE, where the session window
dials.

## Design (Approach B)

### Footer state machine

Four states, derived per transport; exactly one footer layout each:

| State | Predicate (ARDOP / VARA) | Footer |
|---|---|---|
| **Stopped** | modem stopped / transport closed, not armed | `[Connect]` (primary; disabled without target) |
| **Armed** | listener armed, no session in flight | armed status line + `[Connect]` `[Disarm]` |
| **Outbound session** | connect/exchange chain in flight (panel- or ribbon- or agent-initiated) | progress line + `[Abort]` |
| **Inbound session** | armed listener accepted a peer; session active | peer line + `[Abort]` |

State is derived from backend status (`transportOwner`, `listenerArmed`,
session state), not from which button the panel clicked — a ribbon- or
agent-initiated session renders the same footer (single source of truth,
same as C3's status-event discipline).

### Connect (one click)

- `Connect` runs the same chain `connectFor` runs, with the panel's target
  and QSY candidates: spawn/open (if needed) → connect → B2F → disconnect
  → restore prior state. No intermediate operator action. Button shows the
  live phase (`Connecting…` → `Exchanging…`) and the footer is in
  Outbound-session state throughout, with `[Abort]`.
- The Start button is REMOVED from both panels. There is no "open the
  transport and wait" operator action: ARDOP's version holds an on-air
  channel idle (unacceptable); VARA's version is an implementation detail
  the chain performs itself. tuxlink-n95sr #3's ARDOP⇄VARA mirror is
  preserved — both panels change identically.
- RADIO-1 comment correction: consent = the Connect click (one click, one
  bounded, abortable session — memory `radio1-scope`). The
  "Send/Receive is the Part 97 consent" comments in VaraRadioPanel.tsx are
  rewritten to say exactly that.

### Connect while armed

Connect is ENABLED in the Armed state. The chain suspends the listener,
runs the session, and RE-ARMS on completion (success or failure). The arm
record — operator intent — never changes; only the listener's modem
attachment cycles. The armed status line shows `paused for outbound
session…` during the suspension. This removes VARA's armed-station-
cannot-dial dead end and matches the durable-intent rule below.
(Backend note: VARA's listener owns the transport while armed; the
suspend/re-arm sequencing belongs to the give-back path
`VaraSession::take_transport` already models.)

### Disarm and Abort (scoped Stop)

- **Armed state → `[Disarm]`**: ends the armed window (arm record cleared —
  operator intent, sticky per tuxlink-kw873), tears down the listener's
  modem if nothing else holds it. Nothing else is affected.
- **Session states → `[Abort]`**: aborts the in-flight session via the
  existing bounded-abort path; if the session started from an armed
  listener (inbound) or suspended one (outbound), the listener re-arms —
  aborting a session does not change operator arm intent.
- **Kill switch (tuxlink-4u43s)**: disarming from ANY surface (panel
  ListenArmButton, MCP disarm, TTL expiry is excluded — see below) while a
  session is in flight ABORTS the session first, then disarms. Disarm
  means "stop the station," not "edit a flag." TTL expiry is the one
  exception: a TTL that lapses mid-session lets the session finish, then
  disarms (a timer must not cut off a live exchange; the operator's
  explicit Disarm still can).
- The `Stop` label disappears. Post-change there is no button whose scope
  the operator has to guess.

### Arm durability (tuxlink-kw873)

The arm record changes ONLY on: operator arm/disarm, agent arm/disarm
(consent-gated), TTL expiry, or intent auto-arm per spec. Modem lifecycle
events (session start/end, modem crash, Stop-era teardowns) never mutate
it. If the modem dies while armed, the footer shows the Armed state with
an error badge (`armed — modem down, re-arming…` / actionable error), not
a silent flip to Stopped.

### Secondary consequences

- **Open WebGUI (ARDOP)** stays; enabled whenever ardopcf runs (Armed +
  both session states), disabled when Stopped. Unchanged behavior, now
  honestly labeled by state.
- **Config sections** (audio/PTT device pickers) keep their current
  stopped-only editability; the Armed state counts as running (it is).
- **Error surfacing** stays in the footer error line; tuxlink-46hof
  (swallowed catches) is adjacent but separate.
- **MCP/agent surface** is unchanged by this design: agents already run
  the one-click chain (`*_b2f_exchange`) under consent tokens. The panels
  converge on the agents' semantics, not the reverse. (tuxlink-39o6z
  listener-arm MCP tools remain open, separate.)

### Backend deltas the approved design will require

Enumerated so the implementation plan is honest about scope (built whole,
ADR 0018 — nothing here is deferrable once approved):

1. Panel command for the one-click chain (either invoke `connectFor`'s
   sequence from the panel or a shared frontend helper; no new backend
   command strictly required for outbound).
2. Scoped teardown commands: disarm (with abort-if-in-flight) vs abort
   (session only, preserve arm). Today `modem_ardop_disconnect` /
   `vara_disconnect` conflate them.
3. Listener suspend/re-arm around an outbound session (VARA transport
   give-back; ARDOP equivalent).
4. Arm-record durability rules above (kw873) — arm mutations only from
   intent-bearing actions.
5. Status payloads must expose the four-state derivation (ARDOP needs a
   `listenerArmed`/owner equivalent of VARA's; VARA already has
   `transportOwner`).

## Mocks (WebKitGTK renders for approval)

One standalone mock page,
`docs/design/mockups/2026-07-13-arm-session-ux/arm-session-footer-v1.html`,
full-fidelity dark theme, showing the footer + status region in all four
states (Stopped / Armed / Outbound session / Inbound session) for the
ARDOP panel, plus the Armed-paused-for-outbound variant and the
armed-modem-down error badge. VARA's footer is pixel-identical by design
(tuxlink-n95sr #3); one VARA state is included to show the mirror.
Rendered via `dev/render-harness/snapshot.py` (WebKitGTK, software GL);
PNGs are the approval artifact (memory `high-fidelity-mocks`).

## Testing (for the eventual implementation plan)

- Footer state-machine unit tests: each backend status permutation →
  exactly one of the four layouts; ribbon/agent-initiated sessions render
  identically to panel-initiated.
- Disarm kill-switch: disarm mid-session → abort called, then arm record
  cleared; TTL expiry mid-session → session completes, then disarm.
- Arm durability: session end, modem crash, abort → arm record unchanged.
- Connect-while-armed: suspend → session → re-arm on success AND failure.
- CSS/regression: no `isStopped`-keyed footer render remains.

## Operator review points

Decisions are made above (no option menus); flagging the two with the most
judgment in them, in case the operator wants to override at review:

1. **Connect enabled while armed** (auto-suspend + re-arm) — alternative
   was keeping VARA's "disarm first" refusal.
2. **TTL expiry mid-session lets the session finish**; explicit Disarm
   aborts. Alternative: TTL also hard-aborts.
