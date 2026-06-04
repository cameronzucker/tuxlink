# Glossary

Every acronym and project-specific term used elsewhere in this guide is
defined here. Reference page; not meant to be read top-to-bottom.

## A

AFSK
:   Audio Frequency Shift Keying. The modulation scheme used by 1200-baud
    AX.25 packet. Two audio tones (1200 Hz and 2200 Hz) encode bits as
    mark / space.

ALSA
:   Advanced Linux Sound Architecture. The low-level audio framework that
    Linux audio devices appear under. Tuxlink's per-mode panels list ALSA
    cards by name when selecting input + output.

ARDOP
:   Amateur Radio Digital Open Protocol. The open-source HF data mode
    tuxlink supports natively via ardopcf. See
    [ARDOP deep dive](15-ardop-deep-dive.md).

ARES
:   Amateur Radio Emergency Service. The ARRL's emergency communications
    organisation in the US.

ARQ
:   Automatic Repeat reQuest. The protocol family in which receivers
    acknowledge frames and senders retransmit on missed ack. ARDOP, VARA,
    and AX.25 connected mode all use ARQ.

ARRL
:   American Radio Relay League. The US national amateur radio
    association.

ARSF
:   Amateur Radio Safety Foundation. Operates the Winlink CMS.

AX.25
:   The packet protocol used by Winlink Packet over VHF/UHF (and
    occasionally HF). See [Packet on AX.25](14-packet-on-ax25.md).

## B

B2F
:   Block Forwarding v2. The application protocol carrying every Winlink
    message exchange. See [The B2F protocol](06-the-b2f-protocol.md).

Bandwidth
:   For digital modes, the on-air RF bandwidth of the signal. ARDOP
    offers 200/500/1000/2000 Hz; VARA HF offers 500 Hz (Narrow), 2300 Hz
    (Standard), and 2750 Hz (Tactical / Wide); Packet 1200 baud occupies
    ~3 kHz.

BB6PRO
:   A "Black Box" all-in-one radio-to-PC interface (audio + CAT + PTT in
    one unit). Similar role to DigiRig. See
    [SignaLink and other soundcards](11-signalink-and-others.md).

## C

CAT
:   Computer Aided Tuning. The serial protocol most modern radios expose
    for PC-driven frequency and mode control. See
    [CAT and rigctld](12-cat-and-rigctld.md).

Catalog request
:   A Winlink message asking the CMS for catalog data (RMS gateway list,
    weather, position reports, etc.). See
    [Catalog requests](23-catalog-requests.md).

CMS
:   Common Message Server. The central Winlink server cluster operated by
    ARSF. See [CMS and RMS gateways](05-cms-and-rms.md).

CRC
:   Cyclic Redundancy Check. A checksum scheme used in packet framing to
    detect bit errors.

CW
:   Continuous Wave — Morse code. Mentioned here only because some HF
    bands are partitioned with CW-only sub-bands; data modes are not
    permitted in those segments.

## D

DigiRig
:   A small purpose-built radio-to-PC interface (USB audio + CAT
    passthrough + hardware PTT). The canonical interface for tuxlink. See
    [DigiRig setup](10-digirig.md).

Dire Wolf
:   The open-source software TNC used for Winlink Packet on Linux. Runs
    as a daemon on a KISS port. See
    [Packet on AX.25](14-packet-on-ax25.md).

DOMPurify
:   The HTML-sanitization library tuxlink uses to render help-window
    markdown safely. Implementation detail; not operator-visible.

## E

EmComm
:   Emergency Communications. The operating context most Winlink work
    serves. See [Emcomm and ICS](24-emcomm-and-ics.md).

EOC
:   Emergency Operations Center. A fixed-site EmComm activation venue.

## F

FBB
:   The original packet bulletin board software from which Winlink
    inherited B2F framing. Historical reference; FBB itself is rare today.

FCC
:   US Federal Communications Commission. Promulgates Part 97, the rules
    governing amateur radio operation in the US.

FCS
:   Frame Check Sequence. The CRC trailer on AX.25 frames.

FFT
:   Fast Fourier Transform. The algorithm modems use to decode multi-tone
    digital signals. Implementation detail.

FT-818 / FT-991A
:   Yaesu HF transceivers. See
    [Radio-specific notes](13-radio-specific-notes.md).

FTS5
:   The full-text search extension built into SQLite. Tuxlink uses FTS5
    for both the help-window topic search and the mailbox archive search.

## G

G90
:   Xiegu G90. Small QRP HF transceiver popular for portable operating.
    Operationally confirmed with tuxlink + VARA HF Standard.

Grid square
:   See Maidenhead.

## H

Hamlib
:   The radio control library that abstracts dozens of rig CAT protocols
    behind one API. See [CAT and rigctld](12-cat-and-rigctld.md).

HF
:   High Frequency — 3 to 30 MHz amateur radio bands. The bands where
    long-range data modes (ARDOP, VARA, HF packet) live.

HTML Forms
:   The Winlink standardised forms catalog (ICS-213, ICS-205, SHARES
    check-ins, etc.) rendered as HTML in compliant clients. See
    [HTML forms](20-html-forms.md).

## I

IC-7300 / IC-705
:   Icom HF transceivers. See
    [Radio-specific notes](13-radio-specific-notes.md).

ICS
:   Incident Command System. The US emergency management framework that
    structures multi-agency emergency operations. See
    [Emcomm and ICS](24-emcomm-and-ics.md).

ICS-213 / ICS-205 / ICS-309
:   Standard ICS forms. ICS-213 = general message; ICS-205 =
    communications plan; ICS-309 = communications log.

## K

KISS
:   Keep It Simple, Stupid. The host-to-TNC protocol Dire Wolf uses to
    receive frames from tuxlink and emit them to the radio. See
    [Packet on AX.25](14-packet-on-ax25.md).

## L

LZHUF
:   The compression scheme used by B2F format `C`. Inherited from the
    original FBB-era Winlink. Tuxlink ships its own LZHUF implementation.

## M

Maidenhead
:   The grid square system encoding position into 2 / 4 / 6 / 8 (or 10)
    character locator strings. Each pair adds resolution: a 4-character
    grid is the size of a small county or two; 6-character is a town;
    8-character is a few blocks. Tuxlink's broadcast default is the
    4-character grid. See
    [Position and privacy](26-position-and-privacy.md).

`marked`
:   The markdown rendering library tuxlink's help window uses (with
    extensions). Implementation detail.

Mermaid
:   The diagram rendering library tuxlink's help window uses for
    sequence, flow, and state diagrams. Operator-visible: diagrams
    in this guide are Mermaid output.

MID
:   Message ID. A 10–12 character opaque string identifying a Winlink
    message uniquely. See [Mailbox model](07-mailbox-model.md).

## N

NCS
:   Net Control Station. The operator running a Winlink net. See
    [Net check-ins](25-net-check-ins.md).

NMEA
:   National Marine Electronics Association. The serial protocol most GPS
    receivers emit. Tuxlink reads NMEA via gpsd.

NVIS
:   Near Vertical Incidence Skywave. A propagation mode where HF signals
    reflect off the ionosphere nearly straight up and come down within a
    few hundred miles. Useful for regional emcomm.

## P

PACTOR
:   A licensed proprietary HF data mode. Older than VARA, requires a
    hardware modem. Tuxlink does not support PACTOR — the protocol
    requires the SCS-licensed modem hardware. Mentioned here because some
    legacy Winlink stations still run PACTOR.

Packet
:   Short for "packet radio." See AX.25.

Part 97
:   The US FCC rules governing amateur radio. See
    [Emcomm and ICS](24-emcomm-and-ics.md) §Tuxlink and Part 97.

Pat
:   The open-source Go Winlink client (cross-platform). Tuxlink's prior
    art for Linux Winlink. See
    [Migration from Express or Pat](32-from-express-or-pat.md).

PR_
:   Position Report. A Winlink catalog query returning recently-reported
    positions for stations in a region. See
    [Catalog requests](23-catalog-requests.md).

PTT
:   Push-To-Talk. The signal that switches a radio from receive to
    transmit. See [PTT methods overview](09-ptt-overview.md).

PSK
:   Phase Shift Keying. A digital modulation scheme. ARDOP's higher-rate
    modes use PSK.

## Q

QRM
:   Man-made interference (often from other radio signals).

QRN
:   Natural interference (often atmospheric).

QRP
:   Low-power operating — typically ≤5 W (HF) or ≤10 W (other bands). The
    Xiegu G90 is a QRP rig.

QSB
:   Signal fading. HF signals often QSB over seconds-to-minutes as
    propagation paths shift.

## R

`rigctld`
:   The Hamlib rig-control daemon. See
    [CAT and rigctld](12-cat-and-rigctld.md).

RMS
:   Radio Mail Server. A volunteer-operated gateway between RF and the
    CMS. See [CMS and RMS gateways](05-cms-and-rms.md).

RTS / DTR
:   Serial port control lines (Request To Send / Data Terminal Ready).
    Used as the PTT signalling line on most hardware-PTT setups.

## S

SHARES
:   Shared Resources. The US federal emergency-management amateur radio
    program. See [Net check-ins](25-net-check-ins.md).

SignaLink
:   Tigertronics radio interface (USB audio + optional VOX or hardware
    PTT). See
    [SignaLink and other soundcards](11-signalink-and-others.md).

SNR
:   Signal-to-Noise Ratio. The fundamental measure of HF channel quality.
    ARDOP and VARA both report SNR estimates during a session.

SSID
:   AX.25 callsign suffix (`-N`). The Winlink convention is `-7` for the
    operator's mailbox endpoint. See
    [Packet on AX.25](14-packet-on-ax25.md).

## T

Telnet
:   The internet-only Winlink transport. TCP to the CMS, no radio. See
    [Picking a transport](08-picking-a-transport.md).

TLS
:   Transport Layer Security. The encryption layer some CMS endpoints
    require for Telnet sessions.

TNC
:   Terminal Node Controller. The device or software that handles the
    packet modem layer. Dire Wolf is a software TNC. See KISS.

TS-590
:   Kenwood HF transceiver. See
    [Radio-specific notes](13-radio-specific-notes.md).

## V

VARA HF
:   A proprietary HF data mode developed by EA5HVK. Three on-air
    bandwidths — Narrow (500 Hz), Standard (2300 Hz), and Tactical /
    Wide (2750 Hz) — with licensing tiers that gate which bandwidths the
    operator can transmit at. Tuxlink connects to a separately running
    VARA installation over TCP. See
    [VARA HF deep dive](16-vara-hf-deep-dive.md).

VHF / UHF
:   Very High Frequency (30–300 MHz) / Ultra High Frequency (300 MHz–3
    GHz). The bands where 1200-baud FM packet typically lives.

VOX
:   Voice Operated Transmit. PTT triggered by audio threshold instead of
    a control line. See [PTT methods overview](09-ptt-overview.md).

## W

Wine
:   The compatibility layer that runs Windows binaries on Linux. Required
    for VARA HF on Linux. See [VARA HF deep dive](16-vara-hf-deep-dive.md).

Winlink
:   The amateur radio email system. See
    [The Winlink ecosystem](04-the-winlink-ecosystem.md).

Winlink Express
:   The official Windows Winlink client. Reference for protocol
    behaviour. See
    [Migration from Express or Pat](32-from-express-or-pat.md).

WL2K
:   "Winlink 2000." Historical brand name; current rebranding is just
    "Winlink." The handshake banner some servers emit still reads
    `[WL2K-5.0-B2FWIHJM$]` for backward compatibility.

## Where next

- [Credits](31-credits.md) — the projects and communities this guide draws on.
- [Troubleshooting](29-troubleshooting.md) — for diagnostic walks referencing terms above.
- [The Winlink ecosystem](04-the-winlink-ecosystem.md) — for the overall framing.
