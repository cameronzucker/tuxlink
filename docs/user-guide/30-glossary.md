# Glossary

Every acronym and project-specific term used elsewhere in this guide is
defined here. Reference page; not meant to be read top-to-bottom.

## A

ACCEPTLIST
:   Winlink's allow-list mechanism for internet-origin mail. Operators can
    restrict which non-Winlink senders may deliver to their Winlink address.
    Tuxlink does not yet expose a dedicated ACCEPTLIST helper flow.

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

Channel data
:   The station/channel directory that lets a client show usable RMS
    gateways, frequencies, modes, and other dial details. In Tuxlink, RMS
    gateway discovery is handled through catalog/listing flows and the
    connection panel's station picker rather than a Winlink Express-style
    channel selector.

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

Hybrid network
:   Winlink's RF-forwarding network used by Radio-only operation. Hybrid
    stations can store mail locally, forward by radio, or route toward a
    recipient's pickup station when ordinary CMS backhaul is not the plan.
    See [Operating modes](33-operating-modes.md).

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

Iridium GO
:   A satellite hotspot used by some Winlink Express workflows for offshore
    or remote-area internet access. Tuxlink does not ship an Iridium GO
    transport.

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

MPS
:   Message pickup station. In Radio-only/Hybrid operation, the station
    where a recipient retrieves held mail. See
    [Operating modes](33-operating-modes.md).

## N

NCS
:   Net Control Station. The operator running a Winlink net. See
    [Net check-ins](25-net-check-ins.md).

Network Post Office
:   A network of local Post Office servers, often on LAN or AREDN-style
    deployments, that synchronize messages between sites. See
    [Operating modes](33-operating-modes.md).

NMEA
:   National Marine Electronics Association. The serial protocol most GPS
    receivers emit. Tuxlink reads NMEA via gpsd.

NVIS
:   Near Vertical Incidence Skywave. A propagation mode where HF signals
    reflect off the ionosphere nearly straight up and come down within a
    few hundred miles. Useful for regional emcomm.

## O

Operating mode
:   The Winlink session type or routing intent: Winlink (CMS), Peer-to-
    peer, Radio-only, Post Office, or Network Post Office. Distinct from
    the transport used to carry it. See
    [Operating modes](33-operating-modes.md).

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
    [Moving from other Winlink clients](32-from-express-or-pat.md).

Peer-to-peer / P2P
:   Direct station-to-station Winlink B2F with no CMS in the path. One
    station listens and the other connects. See
    [Operating modes](33-operating-modes.md).

Post Office
:   Local RMS Relay store-and-forward operation for a served area or
    exercise. Post Office messages use a different message pool from
    ordinary CMS mail. See [Operating modes](33-operating-modes.md).

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

Radio-only
:   The Winlink Hybrid-network operating mode for RF-only or RF-forwarded
    message movement. Distinct from normal CMS-over-RF. See
    [Operating modes](33-operating-modes.md).

`rigctld`
:   The Hamlib rig-control daemon. See
    [CAT and rigctld](12-cat-and-rigctld.md).

RMS
:   Radio Mail Server. A volunteer-operated gateway between RF and the
    CMS. See [CMS and RMS gateways](05-cms-and-rms.md).

RMS Relay
:   Winlink gateway/server software that can support Radio-only and Post
    Office operation, including local store-and-forward behavior. See
    [Operating modes](33-operating-modes.md).

RMS Packet
:   Winlink gateway software for AX.25 packet gateways. A client reaches
    it through packet radio; the gateway forwards mail to or from the CMS.

RMS Trimode
:   Winlink gateway software for HF RMS operation, commonly associated
    with PACTOR, ARDOP, and VARA-capable gateways.

Robust Packet
:   A specialized packet mode used with SCS hardware and Winlink Express
    Robust Packet sessions. Tuxlink does not ship a Robust Packet
    transport.

RPR
:   Robust Packet Radio. See Robust Packet.

Routing intent
:   Tuxlink's implementation term for the selected operating mode. It
    controls which message pool a session should exchange. See
    [Operating modes](33-operating-modes.md).

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

Session type
:   Winlink Express's operator-facing term for operating mode. See
    [Operating modes](33-operating-modes.md).

SNR
:   Signal-to-Noise Ratio. The fundamental measure of HF channel quality.
    ARDOP and VARA both report SNR estimates during a session.

SSID
:   AX.25 callsign suffix (`-N`). The Winlink convention is `-7` for the
    operator's mailbox endpoint. See
    [Packet on AX.25](14-packet-on-ax25.md).

## T

Tactical address
:   A role or incident address used in some Winlink operations instead of a
    personal callsign, such as an EOC desk or served-agency function.
    Treat it as an operational identity: it needs to be planned, assigned,
    and understood by the net before traffic depends on it.

Telnet
:   The internet-only Winlink transport. TCP to the CMS, no radio. See
    [Picking a transport](08-picking-a-transport.md).

TLS
:   Transport Layer Security. The encryption layer some CMS endpoints
    require for Telnet sessions.

TNC
:   Terminal Node Controller. The device or software that handles the
    packet modem layer. Dire Wolf is a software TNC. See KISS.

Transport
:   The pipe used to carry a Winlink session: Telnet, Packet, ARDOP,
    VARA, PACTOR, and similar paths. Distinct from the operating mode.
    See [Picking a transport](08-picking-a-transport.md).

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
    [Moving from other Winlink clients](32-from-express-or-pat.md).

WL2K
:   "Winlink 2000." Historical brand name; current rebranding is just
    "Winlink." The handshake banner some servers emit still reads
    `[WL2K-5.0-B2FWIHJM$]` for backward compatibility.

## Where next

- [Credits](31-credits.md) — the projects and communities this guide draws on.
- [Troubleshooting](29-troubleshooting.md) — for diagnostic walks referencing terms above.
- [The Winlink ecosystem](04-the-winlink-ecosystem.md) — for the overall framing.
