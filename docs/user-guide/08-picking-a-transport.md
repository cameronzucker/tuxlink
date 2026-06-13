# Picking a transport

A *connection* in Tuxlink combines an operating mode with a transport and
endpoint. Pick the operating mode first - Winlink (CMS), Peer-to-peer,
Radio-only, Post Office, or Network Post Office - then pick the transport
that can reach the target for that mode. See
[Operating modes](33-operating-modes.md) for the routing semantics.

This topic covers the transport half of that choice: Telnet, Packet,
ARDOP, and VARA.

## Telnet (CMS over internet)

Telnet talks directly to the Winlink CMS over TCP. It needs internet, not
radio, and is the simplest transport for development, training, or
fall-back when HF is poor.

- **Bandwidth:** unrestricted (internet).
- **Latency:** seconds.
- **License gate:** none for the connection itself (Telnet is operator-
  to-server, not on air).
- **Use cases:** drafts, training, attachment-heavy mail, troubleshooting.

Configuration: the Telnet connection panel (Connections → Winlink (CMS) →
Telnet). The host / port default to the operator-registered CMS endpoint;
the wizard's default is the published Winlink telnet entry.

## Packet (1200-baud AX.25)

Packet uses AX.25 over a TNC or a soundcard modem (Dire Wolf with KISS
TCP). It carries short Winlink B2F sessions over VHF / UHF FM.

- **Bandwidth:** 1200 baud — small messages, no large attachments.
- **Latency:** seconds (line-of-sight VHF) to minutes (digipeated).
- **License gate:** any U.S. amateur class (no Morse requirement).
- **Use cases:** local emcomm, neighborhood nets, training, low-bandwidth
  HF when no HF transport is available.

The SSID picker in the dashboard ribbon attaches `-N` to the callsign for
each AX.25 session (`N7CPZ-7` is the convention for Winlink). Pick a SSID
the local network reserves for Winlink.

## ARDOP HF

ARDOP is a high-frequency robust digital protocol. Tuxlink drives a local
ARDOP daemon (`ardopcf`) over its command and data sockets; the daemon
generates the audio passed to the radio.

- **Bandwidth:** 200, 500, 1000, or 2000 Hz; operator-selected per band
  conditions.
- **Latency:** seconds to minutes per session.
- **License gate:** General or higher (HF digital modes).
- **Use cases:** long-range emcomm without internet, mountain net check-
  ins, regional gateway access.

Configuration: the ARDOP HF radio panel (Connections → ARDOP HF) —
capture / playback ALSA devices, optional PTT serial path, command port
(default 8515), and ARQ bandwidth.

## VARA HF

VARA is a separate HF protocol with a different waveform. Tuxlink does
not bundle a VARA modem — the operator runs VARA HF on a separate machine
(Wine on x86 Linux, or a Windows host) and Tuxlink connects to its TCP
command + data ports.

- **Bandwidth:** Standard (~2300 Hz) is operationally confirmed against
  RMS gateways; Tactical (~2750 Hz) and Narrow (~500 Hz) depend on the
  modem's licensed feature set.
- **Latency:** seconds to minutes per session, generally faster than
  ARDOP for the same SNR.
- **License gate:** General or higher (HF digital modes).
- **Use cases:** the same envelope as ARDOP, when the operator already
  has VARA running and wants the higher throughput.

Configuration lives in the VARA radio panel itself (not Settings) —
**Host** (default `127.0.0.1`), **Cmd Port** (default `8300`), **Data
Port** (default `8301`), and an optional fixed **Bandwidth** override.
Empty bandwidth means "leave at VARA's default."

## What "Connect" does

The selected radio panel's primary action - **Start**, **Connect**, or
**Connect & send N** depending on the mode - runs one B2F exchange on that
connection: send the selected or queued outbound messages, receive any
waiting mail, then close the session. Progress is in the radio panel's
session log; result (success / failure reason) lands back in the same log
when the session closes.

Multiple Connect calls in a row are safe — the second one waits for the
first to finish before starting.

## Starting a session in the alpha UI

Tuxlink's connection procedure starts in the left sidebar:

1. Open **Connections**.
2. Pick the operating mode: **Winlink (CMS)**, **Radio-only**,
   **Post Office**, **Peer-to-peer**, or **Network Post Office**.
3. Pick the built protocol under that mode. Disabled protocol rows are
   intentionally visible so the Winlink model is understandable even when a
   Winlink Express session type is not shipped in Tuxlink.
4. Confirm the right-hand radio panel title matches what you intended:
   `Telnet Winlink`, `Packet Winlink`, `Telnet Post Office`, and so on.
5. Fill the target fields, then use the panel's primary action.

For RF modes, **Find a gateway** and favorite **Connect** buttons only prefill
the target. They do not transmit. Transmission starts only when the operator
uses the panel's primary Connect action.

## Winlink (CMS) walkthroughs

### Telnet CMS

Use Telnet CMS for first tests, training, large messages, and any situation
where local internet is the intended path.

1. Select **Connections -> Winlink (CMS) -> Telnet**.
2. Confirm the CMS host and transport in the `Telnet Winlink` panel. Secure
   CMS is the normal path; plaintext Telnet is mainly for compatibility and
   diagnostics.
3. Confirm any Outbox messages are intentional.
4. Click **Start**.
5. Watch the session log for TCP connect, secure-login challenge, B2F
   proposals, `-> Sent`, `-> Inbox`, and clean close.

Telnet is not an RF transmission. It still changes the operator's Winlink
mailbox, so use the same Outbox discipline you would use on-air.

### Packet CMS

Use Packet CMS when a local VHF/UHF RMS is reachable and traffic is short.

1. Start Dire Wolf or the KISS TNC.
2. Select **Connections -> Winlink (CMS) -> Packet (AX.25)**.
3. Confirm KISS host/port, SSID, target RMS callsign, and any digipeater path.
4. If using the station finder, prefill the RMS from a gateway row, then review
   the target before transmitting.
5. Click **Connect** and watch both logs: Dire Wolf for the radio layer,
   Tuxlink for the B2F exchange.

### ARDOP and VARA CMS

Use ARDOP or VARA for HF RMS access.

1. Start the modem (`ardopcf`, VARA HF, or VARA FM) and confirm its audio path.
2. Select **Connections -> Winlink (CMS) -> ARDOP HF**, **VARA HF**, or
   **VARA FM**.
3. Use **Find a gateway** or enter the RMS callsign manually.
4. Match bandwidth to the gateway and conditions. For VARA FM, leave bandwidth
   on Auto so the VARA FM binary uses its own configured mode.
5. Run the RF pre-flight from [Choosing the right mode](17-choosing-the-right-mode.md#pre-flight-checklist).
6. Click **Connect**.

## Peer-to-peer walkthroughs

Peer-to-peer requires coordination. One station listens, the other connects,
and both stations must agree on transport, frequency/path, callsigns, and time.

For the listening station:

1. Select **Connections -> Peer-to-peer -> Telnet**, **Packet (AX.25)**,
   **VARA HF**, or **VARA FM**.
2. Open the **Listen** / **Listen (Accept Inbound)** section.
3. Configure allowed stations. Telnet can also allow IP patterns; RF
   transports use callsigns.
4. Arm Listen. The arming window is finite; when it expires, the station stops
   accepting inbound attempts until re-armed.

For the calling station:

1. Select the same Peer-to-peer transport.
2. Enter the peer callsign or host/path required by that transport.
3. Click **Connect**.

Peer-to-peer mail does not enter the global CMS mailbox automatically. Treat it
as direct station-to-station traffic unless your operating plan says how it
will be copied or forwarded later.

## Radio-only walkthrough

Radio-only is the Hybrid/RF-only routing pool, not merely "use a radio."
Tuxlink exposes Radio-only rows for ARDOP HF, VARA HF, and VARA FM. Telnet
and Packet rows are disabled for this operating mode.

1. Select **Connections -> Radio-only -> ARDOP HF**, **VARA HF**, or
   **VARA FM**.
2. Choose the Hybrid/Radio-only station named by the net plan or gateway data.
3. Confirm the routing intent in the panel title before transmitting.
4. Run the RF pre-flight.
5. Click **Connect**.

Local procedures matter here: the served-agency or net plan should tell
operators which Hybrid stations to use and where recipients retrieve mail.

## Post Office and Network Post Office walkthroughs

Both Post Office modes connect to an RMS Relay over Telnet, so they are TCP
sessions rather than RF transmissions. The routing difference is the login:
Post Office logs in as `CALL-L` for local-pool mail; Network Post Office logs
in as the full callsign for onward routing through the relay.

1. Select **Connections -> Post Office -> Telnet** for local-only relay mail,
   or **Connections -> Network Post Office -> Telnet** for onward routing
   through a relay.
2. Enter or choose the relay `host:port`.
3. Review the Outbox checklist. Select the messages this relay session should
   carry. Select none to run a receive-only pickup.
4. Click **Connect** or **Connect & send N**.
5. If pending inbound messages are offered, review the selection prompt before
   download.

The panes and send-selection controls are built in the alpha. The relay-dial
path still needs operator validation against real RMS Relay deployments before
you rely on it operationally.

## Reviewing pending incoming messages

When the remote side offers multiple inbound messages before download, Tuxlink
may show **Review Pending Messages**. Proposal metadata is limited: MID,
uncompressed size, and compressed size are available before download; sender
and subject are not.

The default is to download everything. Uncheck messages you do not want now,
then choose what happens to unchecked messages:

- **Hold** leaves them for a later session.
- **Delete** tells the remote side not to keep them for you.

If the operator does nothing, the prompt times out and submits the current
selection so the B2F session can continue.

## Aborting

The Abort button (visible only while a connect is in progress) shuts the
connecting socket so a slow TLS / login / exchange phase unblocks, then
the backend reports Cancelled in the session log. Abort is a soft stop —
it cannot pull a packet back off the air.

For an emergency RF stop, the right call is the radio's power switch.

## Where next

- [Operating modes](33-operating-modes.md) — CMS, P2P, Radio-only, and Post Office semantics.
- [The mailbox](18-the-mailbox.md) — Inbox, Outbox, Sent, Drafts, Archive.
- [The B2F protocol](06-the-b2f-protocol.md) — how a session exchanges proposals and messages.
- [Settings](27-settings.md) — identities, GPS & privacy, map tiles.
