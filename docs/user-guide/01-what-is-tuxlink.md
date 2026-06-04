# What is tuxlink

Tuxlink is a native Linux desktop Winlink client. It sends and receives Winlink
email over Telnet, AX.25 packet, ARDOP, and VARA HF. It runs as a normal
desktop application — not a Wine wrapper around Winlink Express, not a web
front-end to Pat — with a folder mailbox, a compose window, search across the
local archive, and a per-transport connection panel that surfaces session
state while a connect is running.

The audience is a licensed amateur who wants Winlink without leaving Linux. The
two reader profiles this guide assumes are (1) a General-class-or-higher ham
comfortable with HF, antenna chains, audio routing, PTT, and soundcards, and
(2) a current Winlink Express or Pat operator looking to migrate without
losing operational fluency. Brand-new hams who have never operated a digital
mode are not the audience; this guide assumes a license, a working radio, and
familiarity with reading schematic-level documentation.

## What tuxlink is not

Tuxlink does not teach RF basics, antenna theory, license study, repeater
operating, or Part 97 fundamentals. It does not bundle a VARA modem — VARA HF
runs separately and tuxlink connects to its command and data ports. It does
not pretend to be a Winlink Express clone; the surfaces have been redesigned
to read as native Linux software, and some Express conventions have been
deliberately reshaped (see the [Migration topic](32-from-express-or-pat.md)
for the mapping).

## Where next

- [First-launch wizard](02-first-launch-wizard.md) — three steps to a working identity.
- [Sending your first message](03-sending-your-first.md) — Compose → Outbox → Connect.
- [The Winlink ecosystem](04-the-winlink-ecosystem.md) — CMS, RMS gateways, and where tuxlink fits.
- [Picking a transport](08-picking-a-transport.md) — Telnet vs Packet vs ARDOP vs VARA HF.
