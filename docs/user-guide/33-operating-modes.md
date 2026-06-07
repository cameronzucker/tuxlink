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

Post Office mode is local store-and-forward through RMS Relay. Instead of
routing normal CMS mail, a local RMS Relay system acts as a post office
for a served area or exercise. Operators deposit and retrieve Post Office
messages from that local server.

Use it when:

- A local RMS Relay server is the planned hub for an exercise.
- The served area wants mail to stay local.
- Operators are told to send "Post Office messages," not ordinary Winlink
  messages.
- A hub station is managing pickup and forwarding.

Important details:

- Post Office message type and Post Office session type must match.
- A Post Office session should not be treated as a normal CMS sync.
- Gateway paths may look similar to Packet or Telnet paths, but the
  message pool is different.
- Tuxlink currently exposes this as a planned operating mode, not a fully
  shipped alpha path.

## Network Post Office

Network Post Office is related to Post Office mode, but it is about a
network of local Post Office servers that synchronize with each other. It
is commonly discussed around LAN or AREDN mesh deployments, where several
servers may share messages without relying on the public internet.

Use it when:

- A local mesh/LAN plan provides one or more Post Office servers.
- Operators are told to connect to a Network Post Office service.
- A served agency needs local mailbox continuity across several sites.

Important details:

- Network Post Office and RMS Post Office are close enough to confuse
  experienced users.
- Some Network Post Office deployments may accept conventional Winlink
  messages as well as Post Office messages, depending on server policy.
- Tuxlink currently exposes this as a planned operating mode, not a fully
  shipped alpha path.

## What Tuxlink exposes today

The Connections sidebar is organized by operating mode first, then
transport. Alpha readiness is intentionally visible: built entries open a
real panel; planned entries are shown so operators can understand the
Winlink model without assuming every WLE cell is already shipped.

| Operating mode | Tuxlink alpha status | Built transports shown today |
|---|---|---|
| Winlink (CMS) | Available | Telnet, Packet, ARDOP HF, VARA HF, VARA FM |
| Radio-only | Available for current alpha RF panels | ARDOP HF, VARA HF, VARA FM |
| Peer-to-peer | Available for current alpha panels | Telnet, Packet, VARA HF, VARA FM |
| Post Office | Planned | None yet |
| Network Post Office | Planned | None yet |

Availability means the UI/backend path exists in the alpha build. It does
not replace the operator's normal validation: on-air RF paths still need
operator-run testing under the live-radio policy.

## Message pools and the Outbox

The operating mode also controls which queued messages should leave the
Outbox. Winlink Express distinguishes normal CMS messages, Radio-only
messages, Post Office messages, and direct peer-to-peer traffic. The
session type you open determines which pool is eligible to move.

That separation prevents an operator from accidentally sending ordinary
CMS mail during a local Post Office exercise, or from trying to push a
Radio-only message through a normal CMS session.

Tuxlink's alpha UI is still catching up to that full per-message routing
model. When a non-CMS mode is exposed but Tuxlink has not yet tagged
outbound drafts for that mode, the backend errs on the conservative side
and avoids draining ordinary CMS Outbox mail through the wrong path.

## Practical rule

Pick the operating mode first:

1. **Need normal Winlink/email routing?** Use Winlink (CMS).
2. **Need direct station-to-station with no CMS?** Use Peer-to-peer.
3. **Need Hybrid/RF-only continuity?** Use Radio-only.
4. **Need a local RMS Relay mailbox for an exercise?** Use Post Office.
5. **Need LAN/AREDN synchronized post offices?** Use Network Post Office.

Then pick the transport that can actually reach the target station today:
Telnet for local internet, Packet for local VHF/UHF, ARDOP or VARA for
HF, and the transport your net plan names when operating under a served
agency procedure.

## Where next

- [Picking a transport](08-picking-a-transport.md) - after the operating
  mode is clear, choose the pipe.
- [Choosing the right mode](17-choosing-the-right-mode.md) - transport
  tradeoffs under pressure.
- [The mailbox model](07-mailbox-model.md) - how Inbox, Outbox, Sent, and
  Drafts behave.
- [Glossary](30-glossary.md) - short definitions for CMS, RMS, P2P, and
  related terms.
