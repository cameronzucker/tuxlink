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

Configuration: Tools → Settings → Connection (when wired). The host /
port default to the operator-registered CMS endpoint; the wizard's
default is the published Winlink telnet entry.

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

Configuration: Tools → Settings → ARDOP HF — capture / playback ALSA
devices, optional PTT serial path, command port (default 8515), and ARQ
bandwidth.

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

The Connect button at the top right of the dashboard ribbon runs ONE CMS
exchange on the currently-selected transport: send everything queued in
the Outbox, receive any waiting mail, close the session. Progress is in
the radio panel's session log; result (success / failure reason) lands
back in the same log when the session closes.

Multiple Connect calls in a row are safe — the second one waits for the
first to finish before starting.

## Aborting

The Abort button (visible only while a connect is in progress) shuts the
connecting socket so a slow TLS / login / exchange phase unblocks, then
the backend reports Cancelled in the session log. Abort is a soft stop —
it cannot pull a packet back off the air.

For an emergency RF stop, the right call is the radio's power switch.

## Where next

- [Operating modes](33-operating-modes.md) — CMS, P2P, Radio-only, and Post Office semantics.
- [The mailbox](18-the-mailbox.md) — Inbox, Outbox, Sent, Drafts, Archive.
- [Settings](27-settings.md) — GPS, privacy, ARDOP, connection.
