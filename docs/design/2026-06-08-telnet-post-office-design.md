# Telnet Post Office & Network Post Office — design spec

> **Status:** design, pending operator review → 5-round cross-provider adversarial review → `writing-plans`.
> **Date:** 2026-06-08 · **Agent:** sequoia-pika-maple · **bd:** `tuxlink-6c9y`.
> **Grounding:** [`2026-06-08-telnet-post-office-grounding.md`](2026-06-08-telnet-post-office-grounding.md) (primary-source, decompiled WLE).
> **Mock:** [`mockups/2026-06-08-post-office-mocks.html`](mockups/2026-06-08-post-office-mocks.html).
> **Operator decisions (2026-06-08):** both modes in scope, Telnet/IP only; AREDN auto-discovery out of scope;
> routing determined by connection with send-time message selection (no compose-time routing flag); divergences
> documented explicitly.

## 1. Goal

Wire the two Post Office session types — **Telnet RMS Post Office** and **Network Post Office** — onto the
Telnet/IP connection path, replacing their `soon` stubs in the connections accordion with working backends.
Deliver WLE-equivalent *capability* (exchange mail with an RMS Relay over TCP) while deliberately diverging
from WLE's compose-time routing model in favor of a connection-determined, select-at-send-time model. Ship an
operating-mode documentation page that explains both modes accurately, since that explanation exists nowhere
else.

Both modes are pure TCP/IP (internet, LAN, or AREDN-over-IP). Neither keys a transmitter, so the feature is
entirely outside RADIO-1.

### 1.1 The three session types at a glance

These two modes join the existing Winlink (CMS) mode as three **distinct** session types — separate sidebar
rows, panes, logins, and backends. They differ on two axes: *how the client connects* and *what the relay does
with the mail*. Network Post Office sits between the other two — it reaches the normal Winlink system like CMS,
but through a relay like the local post office, which is exactly why a one-word "global/local" label flattens
it.

| Session type | How it connects | What happens to the mail | When an operator reaches for it |
|---|---|---|---|
| **Winlink (CMS)** | direct to the CMS (internet or RF gateway) | enters the global Winlink system | the CMS is directly reachable |
| **Network Post Office** | to an RMS Relay on a LAN / AREDN mesh (full callsign, `C`/`MESH`) | normal Winlink routing — the relay forwards it onward, and can deliver to local mesh recipients | a relay is reachable on the network but the CMS is not directly reachable |
| **Telnet RMS Post Office** | to an RMS Relay, typically local (`CALL-L` login, `L`) | held in the relay's local pool for local pickup — never forwarded to the global system | local delivery that should stay off the global system |

Telnet RMS Post Office is the only mode whose mail is *local-only* (`L` pool). Network Post Office carries
*normal* (`C`) mail — it is distinct from CMS on the **transport** axis (a network relay path), not the routing
axis.

## 2. Scope

**In scope:**

- **Telnet RMS Post Office** — dials one operator-configured RMS Relay (default `127.0.0.1:8772`) over plain
  TCP, logs in as the base callsign with the `-L` suffix, and exchanges the *local* message pool: mail held at
  that relay for local pickup, never forwarded to the global Winlink CMS.
- **Network Post Office** — dials an operator-added RMS Relay on a LAN or AREDN mesh (manual `host:port`),
  logs in with the full callsign, and exchanges *normal* (`C`) Winlink mail. The relay routes it as it would
  any Winlink mail — forwarding onward toward the global system and/or delivering to local mesh recipients.
  Distinct from Winlink (CMS) on the **transport** axis: it reaches Winlink through a network relay, for
  operators who can reach a relay but not the CMS directly. It is *not* the local-only pool (§1.1).
- **Send-time Outbox selection** for both modes: the connection pane lists Outbox messages; the operator
  selects which to send in the session.
- **Inbound routing-source display** (minor): mail received from a local post office is tagged so its origin
  is visible.
- **Operating-mode documentation** page (ties to smoke-walk item 8).

**Out of scope:**

- **AREDN auto-discovery** (`sysinfo.json` node/service fetch). Omitted deliberately — see §7.2. Relays are
  added by manual `host:port`.
- **Packet / VARA / ARDOP variants** of these modes. `tuxlink-6c9y` covers the **Telnet** protocol cell only;
  the `post-office` row's `packet` protocol stays `soon`.
- **Hosting / being an RMS Relay.** tuxlink is a client of an existing relay; running a relay is a separate,
  far larger feature with no issue.
- **Compose-time routing flag / message pools.** Deliberately not built — see §3.
- **RF / RADIO-1.** Both modes are TCP/IP; no transmit.

## 3. Routing model — headline divergence from WLE

This is the load-bearing design decision and the most important thing the documentation must convey.

**Protocol fact (verified, decompiled WLE):** an RMS Relay decides whether a session is *local* or *global*
from the **login**, not from anything in the message. A Telnet RMS Post Office session logs in as `CALL-L`;
that `-L` suffix is the entire routing discriminator. The B2F proposal that carries a message has no routing
field (`msg_type` is the hardcoded `"EM"`), and the transmitted message bytes are identical regardless of
destination. WLE's compose-time `C`/`R`/`L` "message type" flag is **pure client-side bookkeeping** — it does
nothing on the wire. Its only purpose in WLE is to let WLE's *auto-send-the-entire-outbox* behavior filter
out-of-pool mail, plus Outbox display.

**tuxlink divergence:** tuxlink does **not** implement compose-time routing pools. Routing is determined by the
connection the operator opens; the operator selects which Outbox messages to send in that session.

- Composing a message carries no routing decision. The compose "Send as" control stays a disabled stub.
- Opening a **Telnet RMS Post Office** session and selecting messages sends them to the local relay pool
  (held locally). Opening a **Network Post Office** or **Winlink (CMS)** session sends selected messages to
  the global system.
- A message is never "stuck" because of a compose/connect mismatch, and there is no second decision to keep
  consistent with the connection choice.

**Rationale (operator, 2026-06-08):** compose-time pools do not serve a real-world deployment to a degree that
justifies the near-certain operator footgun and added complexity. The footgun is real and documented in WLE
(messages silently stranded in the Outbox unless manually typed "Post Office Message"), and tuxlink's
status-bar quick-Connect — which reuses the last modem — makes it sharper: composing a "Post Office" message
and then quick-Connecting (which silently reuses, e.g., CMS) would strand the message. Eliminating the
compose-time flag eliminates the failure mode entirely.

**This divergence is documented prominently** (§8): the feature works differently from WLE by design, and the
reasoning is recorded for operators who know the WLE model.

### 3.1 Wire compatibility with WLE clients on a shared relay (verified)

The compose-time divergence is a UX-layer change only; it does **not** alter the wire format, and tuxlink stays
**byte-identical to WLE and fully interoperable** with WLE clients on the same RMS Relay. Verified against
decompiled WLE:

- The relay distinguishes a *local* (`L`) session from a global session by the **login** (`CALL-L`), which
  tuxlink sends identically (`TelnetSession.cs:2004-2018`).
- WLE additionally serializes the routing flag into the transmitted message as an `X-RMS-Routing:` header, but
  **only for `R` (Radio-only) and `L` (Post Office)** mail (`Message.cs:1404-1411`); normal `C` mail carries no
  such header. A receiving client also classifies inbound mail by the **session** it arrives in
  (`B2Protocol.cs:1142-1153` stamps `L` for a PostOffice session regardless of header), so the header is
  belt-and-suspenders for the immediate recipient.

Because tuxlink knows the routing from the **connection**, it derives the flag at **send time** (not from a
compose-time choice) and emits the same header WLE would:

- **Telnet RMS Post Office** session → stamp `X-RMS-Routing: L` on each sent message.
- **Network Post Office** (`MESH` / `C`) session → no `X-RMS-Routing` header (matches WLE's `C` behavior).
- Radio-only (`R`) is out of scope.

Result: tuxlink's transmitted messages are byte-identical to WLE's, the relay routes them identically (by
login), and a WLE client picking them up from the shared relay classifies them identically. **The UX
divergence lives entirely above the wire.** This send-time header emission is a hard requirement — omitting it
would still likely function (inbound is session-stamped) but would forfeit byte-identical parity; tuxlink emits
it.

## 4. UX

### 4.1 Compose — unchanged

No new control. The deferred "Send as" field (`Compose.tsx:516-524`) stays disabled at "Winlink Message". A
composed message carries no routing attribute.

### 4.2 Connections sidebar

The `post-office` and `network-po` rows (`sessionTypes.ts:48-56`, `:74-82`) flip from `built:false` (rendered
with a `soon` badge, non-selectable) to live for the `telnet` protocol. The accordion structure is otherwise
unchanged. Labels remain **"Post Office"** and **"Network Post Office"** (operator-confirmed; WLE's literal
strings are "Telnet RMS Post Office" / "Network Post Office").

### 4.3 Telnet RMS Post Office pane

- A banner stating the session exchanges *local* mail — held at the relay, not sent to the global system.
- **Relay host** + **Port** fields (default `127.0.0.1` / `8772`), operator-editable, persisted in config.
- A read-only "Logs in as `CALL-L`" indicator (the `-L` suffix is appended automatically; no password field —
  the handshake password is the non-secret constant `CMSTelnet`).
- An **Outbox send-selection** checklist: each pending Outbox message with a checkbox (To, Subject, size).
- A **Connect & send N** action; only checked messages are proposed.
- A session log (same component class as the existing Telnet pane).

### 4.4 Network Post Office pane

- A banner stating the session exchanges *normal* Winlink mail through a LAN/mesh relay — the relay routes it
  onward (and can deliver to local mesh recipients); distinct from CMS in *how* it connects, not the routing.
- A **saved relay endpoints** list (favorites): label/callsign + `host:port`, with add/remove. No credential
  field — the handshake carries no secret; any future relay requiring auth routes through the OS keyring,
  never plaintext (WLE stores these in plaintext `.dat`; tuxlink does not).
- The same **Outbox send-selection** checklist as §4.3.
- Login uses the **full callsign** (no `-L`).
- A note that AREDN auto-discovery is intentionally omitted (relays added by `host:port`).

### 4.5 Reading pane — routing source (minor)

Mail received from a Telnet RMS Post Office session is tagged with its origin (e.g., a "Post Office" routing
chip), reusing the existing inbound `routing` field (`mailbox/types.ts:52`). Informational only, derived from
the receiving session; never a compose-time choice. Network Post Office and CMS mail is unmarked (normal).

## 5. Backend design

Touch-points (from the verified implementation map):

1. **Session-type registry** (`src/connections/sessionTypes.ts`): set `built:true` for `post-office.telnet`
   and `network-po.telnet`.
2. **Connection intent routing** (`src/radio/radioPanelVisibility.ts:33`, `src/shell/AppShell.tsx:723-761`):
   the current `sessionType` collapse to `cms|p2p` gains a `post-office` / `network-po` intent and a dispatch
   case for the new pane.
3. **New pane component** (`src/radio/modes/PostOfficeRadioPanel.tsx`, templated on `TelnetRadioPanel.tsx`):
   parameterized by mode (`local` vs `network`) to render the host:port field vs favorites list, the
   send-selection checklist, and the correct login indicator.
4. **New Tauri command** (e.g. `post_office_connect`): parameters `host`, `port`, `mode` (`local`/`network`),
   and the selected Outbox message IDs. It connects plain-TCP to `host:port`, sends the login
   (`base_callsign + "-L"` for `local`, full callsign for `network`) followed by the `CMSTelnet` password
   constant, then runs the existing B2F exchange against target call `WL2K`. **For `local` mode, each proposed
   message is stamped with an `X-RMS-Routing: L` header before transmission** (§3.1 — wire-parity with WLE);
   `network` mode adds no routing header (matches WLE's `C` behavior).
5. **Outbound proposal selection** (`src-tauri/src/winlink_backend.rs`): `build_outbound_proposals`
   (`:229-243`) gains a selection parameter so it proposes only the chosen message IDs rather than the whole
   Outbox. The two existing outbound-build paths (the `build_outbound_proposals` helper and the inline mirror
   in the native-telnet path at `~:1567-1581`) are reconciled so the Post Office command uses the
   selection-aware helper. The CMS path's behavior is unchanged by this feature.
6. **Favorites persistence** (Network Post Office): a new config collection of `{label, host, port}` records.
   No credentials are stored.
7. **Inbound routing source**: received mail is tagged with its origin based on the session mode, surfaced via
   the existing inbound `routing` field.

The B2F wire exchange is unchanged — the post-office modes are an endpoint-swap plus a login-suffix plus a
proposal-set selection atop the existing B2F path. No new wire protocol.

## 6. Data model

- **No new outbound message field.** Routing is not a message attribute.
- **Send selection** is a transient set of message IDs passed to the connect command, not persisted on any
  message.
- **Network Post Office favorites**: a new persisted config collection `{label, host, port}`; no secrets.

## 7. Divergences from WLE (all documented)

1. **No compose-time routing flag / message pools.** Routing follows the connection; send-time selection
   replaces the pools. Rationale in §3. *(Headline divergence.)*
2. **No AREDN auto-discovery.** WLE's `sysinfo.json?services=1` discovery rides the **OLSR** routing protocol,
   which AREDN is removing in favor of **Babel** (production release 4.26.1.0 drops OLSR entirely; third-party
   tools that scrape the OLSR-era data path already break on babel-only nodes — e.g. `mesh-info` #140). Dialing
   a relay by `host:port` is routing-protocol-agnostic and survives the transition. Manual entry is the durable
   approach; the omission is documented, not silent.
3. **Keyring, never plaintext, for any relay credential.** The handshake itself has no secret (`CMSTelnet` is a
   protocol constant in code). WLE stores favorite/relay passwords in plaintext `.dat`/INI; tuxlink does not.
4. **No misleading banner.** WLE's "Only post office messages will be sent" banner misrepresents the Network
   Post Office (`MESH`) path, which carries normal mail; tuxlink does not reproduce it.
5. **No tuxlink-added safeguards.** No airtime caps, no extra confirmation modals; mirror WLE's *suppression*
   of the RMS-Relay warning modal for these sessions. The feature is pure TCP, outside RADIO-1.

## 8. Documentation deliverable

A new operating-mode documentation page (help system) covering:

- What an RMS Relay "post office" is, and the two modes it backs.
- The local-vs-global distinction: Telnet RMS Post Office holds mail locally for pickup; Network Post Office
  forwards into the global Winlink system via a LAN/mesh relay.
- tuxlink's routing model: routing follows the connection, with send-time message selection — and the explicit
  divergence from WLE's compose-time pools, including the footgun rationale (§3).
- Why AREDN auto-discovery is omitted (the OLSR→Babel transition, §7.2).
- When an operator would use each mode (e.g., a self-contained local emergency net vs a mesh-reachable gateway).

The page targets the "explain each operating mode in detail" gap (smoke-walk item 8) and is authored to be
accurate against the decompiled behavior, since this explanation is not otherwise available.

## 9. Testing strategy

Two-tier, no RF, no real relay required for the gate:

- **Tier A — protocol-shape smoke (CI gate):** a local TCP listener fixture emulates the RMS Relay login. It
  asserts the client sends `base_callsign-L` (local mode) or the full callsign (network mode), then the
  `CMSTelnet` password, then runs a canned B2F exchange. The send-selection is verified by asserting only the
  selected message IDs are proposed. **Wire-parity is verified by asserting the transmitted message carries
  `X-RMS-Routing: L` in local mode and no `X-RMS-Routing` header in network mode** (§3.1). No transmitter, no
  real relay.
- **Tier B — real-relay smoke (operator-run, optional):** the operator stands up an RMS Relay listening on
  `127.0.0.1:8772` and dials it from tuxlink. Verifies live parity. No RF.

Unit coverage: the `build_outbound_proposals` selection filter; the login-suffix logic (`-L` vs full
callsign); the Network Post Office favorites store; the new connection-intent routing. Frontend: pane render,
checklist selection state, connect wiring, the inbound routing chip.

## 10. Open questions

None blocking. The routing model, scope, AREDN disposition, and labels are operator-confirmed. Remaining
choices (exact command signature, favorites storage shape, reconciling the two outbound-build paths) are
implementation details resolved during the adversarial review and `writing-plans`.
