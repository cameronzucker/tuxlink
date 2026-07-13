# Winlink Express (third-party client — not Tuxlink)

Reference for **Winlink Express** (WLE, formerly RMS Express), a different Winlink
client. Use this when helping an operator who is running Winlink Express. It is not
documentation of Tuxlink's own behavior. For how Winlink Express compares to Tuxlink,
see the user-guide topic `32-from-express-or-pat`.

Winlink Express is the official Windows reference client and has the broadest Winlink
feature surface. It runs on Windows, and on Linux/macOS under Wine or a VM.

## The session model

Winlink Express is **session-oriented**. The operator picks a session type from the
dropdown in the top-right of the main window, presses **Open Session**, and a separate
session window appears. All connecting happens in that session window, not in the main
window.

| Session type | Path |
|---|---|
| **Telnet Winlink** | Internet to a CMS. No radio. |
| **Packet Winlink** | VHF/UHF packet (AX.25) to an RMS gateway. |
| **VARA HF Winlink** | VARA HF modem. |
| **VARA FM Winlink** | VARA FM modem. |
| **ARDOP Winlink** | ARDOP modem. |
| **Pactor Winlink** | SCS PTC modems. |
| **Winlink Express P2P** variants | Peer-to-peer to another client, not a gateway. |
| **Radio-only / Post Office** | Store-and-forward without internet. |

## Connecting through a digipeater (Packet session)

This is the Winlink Express analogue of Pat's `ax25:///DIGI/TARGET`.

In the **Packet Winlink** session window:

1. Set the **Connection Type** dropdown from **Direct** to **Digipeater**.
2. Enter the **target RMS gateway callsign and SSID** in the box beside that dropdown
   (e.g. `W4XYZ-10`).
3. In the **Via** box, enter the **digipeater callsign** (with SSID if it uses one).
4. A **second** digipeater may be entered in the rightmost box.
5. Press **Start**.

**Order matters** — enter the digipeaters in the order they will be used.

**Winlink Express supports at most two digipeaters** (there are two Via boxes). This
is a real difference from Pat, which accepts an arbitrary number of slash-separated
hops in the connect URL. If an operator needs more than two hops, Winlink Express
cannot express it.

**Same-frequency rule:** the digipeater and the target station must be on the same
frequency. A digipeater does not bridge frequencies.

## Channel selection

Radio session windows have a **Channel Selection** button opening a channel list of
known gateways with frequency, mode, and distance/bearing from the operator's grid.
Selecting a channel fills in the frequency and target callsign. The list is refreshed
from Winlink's published gateway data (the operator needs to have updated it at least
once while online).

## Account and password

The callsign is the account. The Winlink password is set on the operator's Winlink
account and entered in Winlink Express settings. A **password recovery email address**
should be registered on the account; without it a lost password cannot be recovered.
Winlink passwords are case-sensitive.

## Forms

Winlink Express has the original Winlink Standard Templates / HTML forms system
(ICS-213, Winlink Check-In, etc.). Forms are selected when composing, filled in a
browser window, and the completed form is attached to the message.

## When the answer is "that's a Tuxlink thing"

Winlink Express is a Windows application with its own settings dialogs and its own
credential storage. Do not describe Tuxlink's behavior (OS keyring, Linux-native
panels, Tuxlink's own UI) as though it were Winlink Express's. If an operator asks
where Winlink Express stores something and it is not covered here, say so rather than
guessing.
