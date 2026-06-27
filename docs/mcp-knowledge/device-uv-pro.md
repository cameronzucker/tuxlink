# Device setup: UV-Pro (Benshi)

The UV-Pro is a Benshi-platform handheld that Tuxlink supports for both APRS
and Winlink work over its Bluetooth data interface. This guide covers the
operating setup an operator needs.

## APRS and Winlink: dual-mode, but not simultaneously

The UV-Pro can do both APRS and Winlink Packet with Tuxlink, but **not at the
same time over Bluetooth.** The radio accepts a single Bluetooth connection,
and its data path is either:

- **KISS** — the packet path that carries Winlink Packet sessions and
  APRS-over-KISS frames, or
- **native control** — Tuxlink's UV-Pro control link for reading and setting
  channel, frequency, and mode.

These are mutually exclusive. Starting one while the other holds the radio is
rejected: attempting native control while a KISS session is active surfaces
as a "radio in use" condition. Plan the session: connect for control to set
the channel and frequency, disconnect control, then start the KISS-based
packet or APRS session, or vice versa.

To switch tasks, end the active session first so the single Bluetooth link is
free for the next one.

## KISS versus a tuned frequency

There are two distinct things to get right:

- **The KISS data path** is how frames move between Tuxlink and the radio
  over Bluetooth. Tuxlink (or Dire Wolf, depending on the setup) speaks KISS
  to the radio's data interface. This is the transport plumbing.
- **The tuned frequency** is what the radio is physically listening to and
  transmitting on. KISS does not set the frequency; the radio must already be
  on the correct simplex frequency or channel for the packet gateway, APRS
  network, or peer you are working.

Use Tuxlink's native UV-Pro control to select the channel or set the
frequency before you start a KISS session, since the KISS path itself does not
tune the radio. Common APRS work is on the regional APRS frequency; Winlink
Packet work is on the target gateway's published frequency.

## Bluetooth pairing notes

- Pair the UV-Pro with the host over Bluetooth at the OS level first, then
  point Tuxlink's packet/Bluetooth configuration at the radio's MAC address.
- Tuxlink connects over RFCOMM; the configured Bluetooth MAC is the default
  the control link uses, so set it once in the packet configuration.
- Only one host connection is accepted at a time. If pairing or connecting
  fails, confirm no other device or app already holds the radio's Bluetooth
  link.
- On link loss there is no automatic reconnect: the connection state returns
  to disconnected and the operator reconnects deliberately. Dropping the
  socket is also how a control session is ended.
