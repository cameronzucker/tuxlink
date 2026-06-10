# Operating modes

Winlink has two different ideas that are easy to mix up:

- **Operating mode**: where the message is meant to go, and which message
  pool it belongs to. Winlink Express also calls this a session type.
- **Transport**: how bits move for this session: Telnet, Packet, ARDOP,
  VARA, PACTOR, and so on.

The operating mode is the more important choice. It decides whether a
message is normal CMS mail, direct peer-to-peer mail, Radio-only Hybrid
network mail, or Post Office mail. The transport is the pipe used to carry
that choice.

## The four axes

Every connection can be read as four choices:

| Axis | Examples | What it controls |
|---|---|---|
| Operating mode | Winlink (CMS), Radio-only, Post Office, Peer-to-peer, Network Post Office | Routing semantics and message pool |
| Transport | Telnet, Packet, ARDOP HF, VARA HF/FM | Link technology |
| Role | Connect, Listen | Who initiates the session |
| Endpoint | CMS, RMS gateway, peer station, RMS Relay, local post office server | The station or server on the other end |

Example: **Winlink (CMS) over ARDOP HF** means "send normal CMS-routed
mail through an RMS gateway, using ARDOP as the RF transport." **Peer-to-
peer over VARA HF** means "exchange direct station-to-station mail, using
VARA HF as the RF transport, with no CMS in the path."

## Winlink (CMS)

This is the ordinary Winlink mode most operators mean when they say
"send a Winlink message." The client authenticates as your callsign,
talks to a CMS directly over Telnet or indirectly through an RMS gateway,
and syncs your global Winlink mailbox.

Use it when:

- You want to send to a normal email address.
- You want your message to route through the worldwide Winlink system.
- You want waiting CMS mail to appear in your Inbox.
- You are practicing with the same path most nets and forms workflows use.

Important details:

- Telnet reaches the CMS directly and needs local internet.
- Packet, ARDOP, and VARA reach an RMS gateway over RF; the gateway needs
  a working path to CMS.
- CMS sessions use your Winlink account credentials.
- Message flow is mailbox-style: Outbox sends, Inbox receives, Sent keeps
  delivered copies.

## Peer-to-peer

Peer-to-peer is direct station-to-station B2F. There is no CMS and no RMS
gateway in the middle. Both stations must arrange the session: one side
listens, the other connects, and both stations must be reachable on the
chosen transport.

Use it when:

- You need a direct tactical exchange with another station.
- The internet or CMS path is not available.
- You know the peer callsign, frequency/path, and schedule.
- You do not need the message to appear in the global CMS mailbox.

Important details:

- Peer-to-peer does not deliver to ordinary internet email addresses.
- A message exchanged peer-to-peer is not automatically synced later to
  CMS.
- Listen mode matters: if the peer is not listening, the connect goes
  nowhere.
- Password behavior is transport-specific; Telnet P2P can use a station
  password, while RF transports rely on their own session setup.

## Radio-only

Radio-only is Winlink's Hybrid network path. Instead of assuming an
internet-connected CMS gateway, the message can be held or forwarded by
Radio-only/Hybrid network stations. A Hybrid station may store messages
locally, forward them by HF to another Hybrid station, or route them
toward the recipient's message pickup station.

Use it when:

- The ordinary CMS path may be unavailable.
- An exercise or emergency plan uses the Hybrid network.
- Operators are expected to retrieve mail from a designated pickup station.
- You need RF-only continuity rather than "RF to a gateway with internet."

Important details:

- Radio-only is not just "use a radio." Normal CMS-over-RF still depends
  on the RMS gateway's internet backhaul.
- Radio-only messages belong to a separate routing pool from ordinary CMS
  messages.
- Hybrid/RMS Relay stations may warn the operator that mail will be held
  or forwarded by radio rather than sent directly to CMS.
- Local procedures matter. Nets should publish which Hybrid stations to
  use and where recipients should pick up mail.

## Post Office

Post Office mode is local store-and-forward through an RMS Relay. An RMS
Relay is a "post office": a station that accepts mail and holds or
forwards it on behalf of operators in a served area. Telnet RMS Post
Office mode connects to that relay over Telnet and deposits mail into the
relay's **local pool** for local pickup. Mail in the local pool stays at
the relay; it is never forwarded onto the global Winlink system.

The relay decides whether a session is local or global from the login
alone. A Telnet RMS Post Office session logs in as **`CALLSIGN-L`** — the
base callsign with a `-L` suffix. That `-L` suffix is the sole routing
discriminator: it tells the relay "deposit this in the local pool." A
session that logs in with the bare base callsign instead gets normal
routing (see Network Post Office, below).

Use it when:

- A local RMS Relay server is the planned hub for an exercise.
- The served area wants mail to stay local rather than reach the global
  Winlink system.
- Operators are told to send "Post Office messages," not ordinary Winlink
  messages.
- A hub station is managing pickup and forwarding.

Important details:

- Local-pool mail is local only. A message deposited through Telnet RMS
  Post Office is held at the relay for local pickup and is not forwarded
  globally. Network Post Office is the mode for mail that should route
  onward (below).
- A Post Office session is not a normal CMS sync. The relay is the
  endpoint, not the worldwide Winlink system.
- The `-L` login is automatic. Tuxlink extracts your base callsign and
  appends `-L`; the pane shows the resulting login. The Telnet handshake
  uses a fixed, non-secret value, so the pane has no password field.
- Tuxlink exposes Telnet RMS Post Office as a built operating mode,
  selectable in the Connections sidebar. The pane provides relay
  `host:port` fields, a send-time Outbox selection checklist, and an
  inbound message-selection prompt. The relay-dial path still needs
  operator-run validation before operational reliance (see "What Tuxlink
  exposes," below).

## Network Post Office

Network Post Office also connects to an RMS Relay, but the relay does
**normal routing** rather than local-pool holding. Where Telnet RMS Post
Office logs in as `CALLSIGN-L` to deposit local-only mail, Network Post
Office logs in with the **full base callsign**, and the relay forwards the
mail onward — and can also deliver to recipients reachable on the local
mesh. This mode targets LAN and AREDN mesh deployments, where a relay is
reachable but the global CMS is not.

The distinction is the routing axis, not the transport axis. Network Post
Office mail is ordinary, onward-routed Winlink mail; it differs from
Winlink (CMS) in that it reaches a mesh-local relay instead of the CMS,
not in how it is routed once the relay has it.

Use it when:

- A local mesh/LAN plan provides one or more relays for forwarding.
- A relay is reachable but the global CMS path is not.
- A served agency needs mailbox continuity across several mesh sites.

Important details:

- Network Post Office carries normal mail, not local-pool mail. The full
  base callsign login is what tells the relay to route onward; this is the
  one detail that separates it from Telnet RMS Post Office.
- The two modes are close enough to confuse experienced users. The local
  pool (`-L`) holds mail at the relay; normal routing (full callsign)
  forwards it.
- Tuxlink exposes Network Post Office as a built operating mode,
  selectable in the Connections sidebar. The pane provides a saved list of
  relay `host:port` endpoints — the Network Post Office favorites, which the
  operator names and reuses — plus the same send-time Outbox selection and
  inbound message-selection controls as Telnet RMS Post Office.
- Tuxlink locates relays by **manual `host:port`** rather than mesh
  auto-discovery. Winlink Express auto-discovers mesh relays through a
  mechanism that rides the OLSR mesh routing protocol. AREDN is replacing
  OLSR with Babel, and OLSR-based discovery already breaks on Babel-only
  nodes, so auto-discovery is not durable. Manual `host:port` with saved
  favorites is explicit and stays correct across that transition.
- As with Telnet RMS Post Office, the relay-dial path still needs
  operator-run validation before operational reliance.

## How Tuxlink decides routing

Routing in Tuxlink is **determined by the connection you open**, and you
**select which Outbox messages to send** when you connect. The operating
mode you pick in the Connections sidebar is the routing decision: opening
Telnet RMS Post Office means "deposit to the local pool"; opening Network
Post Office means "route onward through a mesh relay"; opening Winlink
(CMS) means "send through the global Winlink system." Composing a message
carries no routing attribute — the compose "Send as" control is a disabled
stub, present only to signal where Winlink Express puts the choice.

This diverges deliberately from Winlink Express. Winlink Express tags each
message with a routing pool at **compose time**, then matches that tag
against the session type at send time. That model is a footgun: a message
composed for one pool sits silently in the Outbox when the operator opens
a session for a different pool, and nothing leaves. Tuxlink removes the
compose-time tag entirely. You open the connection that means what you
want, then pick which queued messages go on that connection. Nothing is
silently stranded because nothing is pre-committed to a pool it can only
leave through one specific session.

The practical consequence: at connect time, each Post Office and Network
Post Office pane shows a checklist of Outbox messages with select-all and
select-none. You choose what to send. Connecting with nothing selected is
valid and supported — receive-only is a primary use, pulling waiting local
mail without sending anything.

## Winlink Express session names in Tuxlink

Winlink Express names many session windows directly after the transport. Tuxlink
separates the routing decision from the transport, so the same capability often
appears as an operating mode plus a protocol row in the Connections sidebar.

| Winlink Express term | Tuxlink equivalent or status |
|---|---|
| Telnet Winlink / Telnet CMS | **Winlink (CMS) -> Telnet**. Direct TCP session to CMS. |
| Packet Winlink / Packet RMS | **Winlink (CMS) -> Packet (AX.25)**. KISS/Dire Wolf path to a Packet RMS. |
| ARDOP Winlink | **Winlink (CMS) -> ARDOP HF**. `ardopcf` modem path to an HF RMS. |
| VARA HF Winlink / VARA FM Winlink | **Winlink (CMS) -> VARA HF** or **VARA FM**. External VARA modem path to an RMS. |
| Telnet P2P | **Peer-to-peer -> Telnet**. One station listens on TCP, one station connects. |
| Packet P2P | **Peer-to-peer -> Packet (AX.25)**. Direct AX.25 station-to-station B2F. |
| VARA P2P | **Peer-to-peer -> VARA HF** or **VARA FM**. Direct VARA station-to-station B2F. |
| Radio-only / Hybrid client sessions | **Radio-only -> VARA HF / VARA FM** for the cleanest current alpha path. **Radio-only -> ARDOP HF** is visible, but the ARDOP panel still needs intent-aware validation before operational reliance. Telnet and Packet are disabled for this operating mode in Tuxlink. |
| Telnet Radio-only / Use RMS Relay | Split in Tuxlink between **Radio-only** for Hybrid/RF-only routing and the Post Office modes below. Use the mode named by the event plan. |
| Telnet RMS Post Office | **Post Office -> Telnet**. Client-to-relay local-pool mail using `CALL-L` login; this is not "be the hub." |
| AREDN Mesh / Network Post Office | **Network Post Office -> Telnet**. Client-to-relay onward routing using full-callsign login. Tuxlink uses manual/saved `host:port`, not OLSR mesh auto-discovery. |
| Pending incoming / Review Pending Messages | Tuxlink shows an inline **Review Pending Messages** prompt when the remote side offers proposals before download. |
| Channel Selection / update channel data | Use the gateway finder, saved favorites, and [Catalog requests](23-catalog-requests.md) to refresh RMS data. |
| HF Auto Connect / scheduled polling | Not shipped. Tuxlink is attended-operation-first; each session starts from an operator action. |
| PACTOR | Not supported. Keep Winlink Express for SCS PACTOR hardware sessions. |
| Robust Packet / RPR | Not supported. Use Packet, ARDOP, or VARA instead. |
| Iridium GO | Not supported as a dedicated session type. If the satellite link presents ordinary internet to Linux, Tuxlink can still use normal Telnet CMS over that IP connection, but it has no Iridium-specific setup surface. |

Project-facing parity rationale lives in
`docs/design/2026-06-02-wle-client-parity-closure-plan.md` and
`docs/design/2026-06-08-telnet-post-office-design.md`; they are referenced
here as maintainer breadcrumbs rather than help-window links because the
in-app guide only links within the user-guide bundle.

## What Tuxlink exposes

The Connections sidebar is organized by operating mode first, then
transport. Alpha readiness is visible: built entries open a real pane;
planned entries are shown so operators can understand the Winlink model
without assuming every Winlink Express cell is already shipped.

| Operating mode | Tuxlink alpha status | Built transports |
|---|---|---|
| Winlink (CMS) | Built | Telnet, Packet, ARDOP HF, VARA HF, VARA FM |
| Radio-only | Built for the alpha RF panes | ARDOP HF, VARA HF, VARA FM |
| Peer-to-peer | Built for the alpha panes | Telnet, Packet, VARA HF, VARA FM |
| Post Office | Built (pane; relay dial unvalidated) | Telnet |
| Network Post Office | Built (pane; relay dial unvalidated) | Telnet |

"Built" means the pane and its backend path exist in the alpha build. It
does not replace the operator's normal validation. On-air RF paths need
operator-run testing under the live-radio policy. The Post Office and
Network Post Office panes are selectable and their send-selection controls
work; the relay-dial path that reaches an RMS Relay over Telnet needs
operator-run validation before operational reliance.

## Message pools and the Outbox

Winlink Express separates the Outbox into pools — normal CMS messages,
Radio-only messages, Post Office messages, and direct peer-to-peer traffic
— and matches a message's pool against the session type at send time. The
separation prevents sending ordinary CMS mail during a local Post Office
exercise, or pushing a Radio-only message through a normal CMS session.
The cost is that a message tagged for one pool is invisible to every other
session type, and an operator who opens the wrong session sees nothing
leave.

Tuxlink reaches the same goal through connection-determined routing and
send-time selection rather than per-message pools. The session you open is
the pool, and the checklist at connect time is the choice of which
messages move. An operator cannot drain CMS mail through a Post Office
session by accident, because the Post Office pane only sends the messages
checked in that session. The Outbox holds one undifferentiated set of
queued messages; the routing decision lives at the connection, not on the
message.

## Practical rule

Pick the operating mode first:

1. **Need normal Winlink/email routing?** Use Winlink (CMS).
2. **Need direct station-to-station with no CMS?** Use Peer-to-peer.
3. **Need Hybrid/RF-only continuity?** Use Radio-only.
4. **Need local-only mail held at an RMS Relay for an exercise?** Use
   Telnet RMS Post Office.
5. **Need onward routing through a LAN/AREDN mesh relay when the CMS is
   unreachable?** Use Network Post Office.

Then pick the transport that can reach the target station: Telnet for
local internet, Packet for local VHF/UHF, ARDOP or VARA for HF, and the
transport your net plan names when operating under a served agency
procedure.

## Where next

- [Picking a transport](08-picking-a-transport.md) - after the operating
  mode is clear, choose the pipe.
- [Choosing the right mode](17-choosing-the-right-mode.md) - transport
  tradeoffs under pressure.
- [The mailbox model](07-mailbox-model.md) - how Inbox, Outbox, Sent, and
  Drafts behave.
- [Glossary](30-glossary.md) - short definitions for CMS, RMS, P2P, and
  related terms.
