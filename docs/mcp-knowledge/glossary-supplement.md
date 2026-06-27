# Glossary supplement

Terms an agent assisting a Tuxlink operator encounters often, beyond the
in-app glossary. Operator-level definitions, not protocol specifications.

## Control strip

The control strip is Tuxlink's compact, always-present status-and-action bar.
It surfaces the live connection state (which transport, connected or idle,
the active operating mode) and the primary connect/abort actions for the
current surface. Connection status lives in the control strip rather than
inside a panel, so the operator can see and stop an in-progress session from
anywhere in the app. It is the operator's at-a-glance answer to "what is the
radio doing right now, and how do I stop it."

## SSID

An SSID is the numeric sub-identifier appended to an AX.25 callsign as `-N`
(for example `W1AW-7`). One licensed callsign can run several distinct
stations or services by giving each a different SSID: `-7` is the Winlink
convention for an operator's mailbox endpoint, while other SSIDs may identify
a node, a digipeater, or an APRS station. The SSID is part of the addressing,
so a packet connection must target the exact callsign-plus-SSID the
destination is listening on.

## KISS

KISS ("Keep It Simple, Stupid") is the host-to-TNC protocol that carries
raw frames between Tuxlink and a Terminal Node Controller. Tuxlink speaks
KISS over a TCP socket to a software TNC such as Dire Wolf, which handles the
packet modem layer and drives the radio. KISS itself carries no addressing or
retry logic; it is a thin framing protocol, and the AX.25 layer above it does
the connected-mode work. A "KISS port" is the TCP port the TNC listens on
(commonly 8001).

## NVIS

NVIS (Near Vertical Incidence Skywave) is an HF propagation technique for
short-to-medium range coverage. Signals are radiated nearly straight up,
reflect off the ionosphere, and come back down over a roughly 0–400 mile
radius with no skip zone. It fills the gap between line-of-sight VHF and
long-haul HF, which makes it valuable for regional emergency communications
where a single station must reach everyone in a county or region. NVIS favors
lower HF bands (40m and 80m), low antenna heights, and is generally a
nighttime path on 80m and a daytime-into-evening path on 40m.
