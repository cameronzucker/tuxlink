<p align="center">
  <img src="assets/tuxlink_icon.png" alt="Tuxlink logo" width="120" height="120">
</p>

# Tuxlink — native Linux Winlink client for amateur radio emergency communications

Tuxlink is a native Linux desktop [Winlink](https://winlink.org/) client for
amateur radio (ham radio) emergency communications. It implements the Winlink
B2F protocol directly in Rust and presents the mailbox, compose pane, and live
session log inside one desktop window. No Windows, no WINE, no browser tab, no
external CMS sidecar.

Beyond Winlink, Tuxlink fuses strategic and tactical emergency communications in
one workspace: long-haul Winlink email over HF, and tactical APRS messaging over
VHF and UHF with native control of the Benshi UV-Pro handheld.

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPL%20v3-blue.svg" alt="License: GPL v3"></a>
  <a href="https://github.com/cameronzucker/tuxlink/releases/latest"><img src="https://img.shields.io/badge/release-latest-blue.svg" alt="Latest release"></a>
  <a href="https://github.com/cameronzucker/tuxlink/releases/latest"><img src="https://img.shields.io/badge/downloads-deb%20%7C%20rpm%20%7C%20AppImage-success" alt="Downloads: deb, rpm, AppImage"></a>
  <a href="https://github.com/cameronzucker/tuxlink/actions/workflows/release.yml"><img src="https://img.shields.io/github/actions/workflow/status/cameronzucker/tuxlink/release.yml?label=build" alt="Build status"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.75+-orange.svg?logo=rust" alt="Rust 1.75+"></a>
  <a href="https://www.kernel.org"><img src="https://img.shields.io/badge/platform-linux%20(x86__64%20%7C%20arm64)-lightgrey.svg?logo=linux&logoColor=white" alt="Platform: Linux x86_64 and arm64"></a>
</p>

> [!TIP]
> **Coming from Winlink Express or Pat?** Start with
> [Moving from other Winlink clients](docs/user-guide/32-from-express-or-pat.md):
> settings mapping, conceptual differences, current parity gaps, and a
> recommended migration sequence (including how to carry mailbox history across
> by copying the `native-mbox/` directory).

<p align="center">
  <img src="docs/readme/images/tuxlink-mailbox.png" width="860"
       alt="Running Tuxlink mailbox showing the dashboard ribbon, folder sidebar, message list, reading pane, and status bar">
</p>
<p align="center"><sub>The running Tuxlink mailbox with privacy-safe sample data: dashboard ribbon, folder sidebar, message list, reading pane, and status bar.</sub></p>

<p align="center">
  <img src="docs/readme/images/tuxlink-demo.webp" width="760"
       alt="Tuxlink in action: opening a Winlink message into the reading pane, bringing up the APRS tactical-chat dock with live traffic, then cycling the color schemes from dark to night-tactical red to daylight">
</p>
<p align="center"><sub>Open a Winlink message, bring up APRS tactical chat alongside it, and re-skin the whole interface for the lighting at hand.</sub></p>

## What Tuxlink is

[Winlink](https://winlink.org/) is the de-facto amateur radio email system used
by emergency-communications (emcomm) teams, ARES and CERT organizations, the
Red Cross, and offshore cruisers. It moves email-style messages over radio when
the internet is down.

Two clients reach the Winlink network on Linux today.
[Winlink Express](https://winlink.org/WinlinkExpress), the proprietary Windows
reference client, runs under WINE. [Pat](https://getpat.io/) is an open-source Go
client with broad transport support, pairing a command-line tool with an optional
browser-served web UI.

| | Winlink Express | Pat | Tuxlink |
|---|---|---|---|
| Native Linux, no WINE | No (Windows) | Yes | Yes |
| ARM / Raspberry Pi | No | Yes | Yes |
| Native desktop GUI | Windows only | No (CLI + web UI) | Yes |
| Winlink Standard Forms catalog | Yes | No | Yes |
| Credential storage | Local file | Config file | OS keyring |
| Native UV-Pro Bluetooth control | No | No | Yes |

Tuxlink takes a third path: it implements the Winlink B2F protocol itself,
natively in Rust. The mailbox, the CMS connection, and the wire-protocol exchange
are one application, not a desktop GUI wrapped around a separate modem daemon or a
Windows binary under emulation. The same build-it-natively approach reaches the
radio link: Tuxlink is developing Sonde, a clean-room HF modem, rather than
depending on closed Windows modem software.

On that native engine it ships a single [Tauri](https://tauri.app/) desktop
application. The complete Winlink Express Standard Forms catalog, an address book,
station finding, location-aware request workflows, full-text search, and an
offline map all ship in the box, and first run needs no README and no video
tutorial. The OS keyring holds the Winlink CMS password; Tuxlink never writes it
to a config file on disk. The mailbox, compose pane, address book, and session
log all render inside one desktop window.

Tuxlink unifies two layers of emergency communication that operators have
historically run on separate devices. The strategic layer carries Winlink email
over HF to wherever propagation reaches. The tactical layer carries APRS position
and text over VHF and UHF to stations in local range, with native control of the
Benshi UV-Pro handheld. Both run from one workspace on a mains-powered Linux
station, instead of a Windows laptop for Winlink alongside a battery handheld for
APRS.

> [!NOTE]
> **Tuxlink is in alpha and looking for testers.** It installs from `.deb`,
> `.rpm`, and `.AppImage` artifacts on every release. It is not yet ready for
> field deployment. Install it, run it through real workflows, and
> [file an issue](https://github.com/cameronzucker/tuxlink/issues) with a clear
> repro and the exported logs (Help → Logging → Export logs).
>
> Specifically, Tuxlink needs validation with a wide variety of radios. It's
> currently tested against a Digirig, Bluetooth KISS, and the Benshi UV Pro
> protocol. Please report hardware successes/failures with specific radios
> and interfaces using the Help menu in Tuxlink.
>
> Version tags are generated automatically from conventional-commit activity by
> [release-please](https://github.com/googleapis/release-please) and track
> repository velocity, not release readiness. The
> [Maturity](#maturity-what-is-and-is-not-proven) section covers which paths
> are validated and which are operator-verified.

## Features

Tuxlink ships the following on Linux for x86_64 and arm64:

### Winlink engine

<img src="docs/readme/images/tuxlink-ardop-hf.png" width="340" align="right"
     alt="ARDOP HF radio panel: Find-a-Gateway, Favorites/Recent station tabs, ALSA capture/playback and PTT selectors, ARQ bandwidth, a live quality meter, and an ARDOP frame ribbon" />

- **Native B2F engine.** The Winlink B2F protocol is implemented directly in
  Rust: CMS over telnet (TLS or plaintext), the full propose / accept message
  exchange, and on-disk mailbox persistence. No external modem daemon or
  sidecar process handles CMS.
- **Telnet to CMS.** Operator-to-CMS sessions over the internet for
  development, training, and fall-back when HF propagation is poor.
- **AX.25 1200-baud packet.** Connected-mode AX.25 over a KISS TNC: USB serial,
  Bluetooth RFCOMM, or KISS-over-TCP to a soundcard modem such as
  [Dire Wolf](https://github.com/wb2osz/direwolf). Inline radio panel with an
  SSID picker and a digipeater relay path.
- **ARDOP HF** *(pictured)*. A complete panel for the ARDOP transport, driving a
  local `ardopcf` over its command and data sockets: **Find a Gateway** sorts
  RMS stations by distance from your grid; favorites and recent stations are
  saved per transport; ALSA capture/playback and the PTT serial line are picked
  inline; ARQ bandwidth is selectable; and a live quality meter, an ARDOP frame
  ribbon, and the session log track the link as it runs — freeze-free, with a
  working abort.
- **VARA HF / VARA FM.** A connection panel manages the TCP link to an
  operator-supplied VARA instance, surfaces connect and error state, and edits
  the persisted VARA configuration. Over-the-air peer sessions are pending.

<br clear="right" />

### Tactical and local operations

- **APRS tactical chat.** Per-callsign message threads over APRS on VHF and UHF
  with delivery-acknowledgement tracking, presented inline beside the address
  book. A fixed, mains-powered station carries local tactical traffic without
  draining a handheld.
- **Native Benshi UV-Pro support.** Tuxlink drives the Benshi UV-Pro handheld
  directly over Bluetooth, through its RFCOMM / GAIA control link, with no cable,
  no sound card, and no separate TNC. A control strip surfaces the radio, and the
  same Bluetooth link carries a KISS data path for APRS tactical chat, AX.25
  packet, and Winlink. A modern handheld and a Linux machine make a complete
  strategic-and-tactical station.
- **FULL and tactical identities.** A licensed FULL identity for Winlink and
  tactical identities for local operation, managed under Settings → Identities.

**Simultaneous HF/VHF workspace.** Strategic and tactical layers run at the same
time in one window: an HF Winlink session — launched from the status-bar Connect
control with saved session details — alongside a live VHF APRS tactical-chat
channel over sustained Bluetooth KISS. One operator works store-and-forward email
on HF and real-time tactical chat on VHF without the two contending.

<p align="center">
  <img src="docs/readme/images/tuxlink-workspace.png" width="100%"
       alt="Tuxlink showing an HF Winlink ICS-213 in the reading pane while the APRS tactical-chat dock on the right carries live VHF traffic from K4ARC, WX4MTL, and N4SAR">
</p>
<p align="center"><sub>HF Winlink and VHF APRS tactical chat in one workspace: an ICS-213 in the reading pane while the APRS channel carries live tactical traffic.</sub></p>

### Mailbox and messaging

- **Mailbox.** Inbox, Outbox, Sent, Drafts, Archive, plus operator-created
  nested user folders. A selection-aware context menu performs bulk Archive and
  Move across multiple messages.
- **Compose.** New message, Reply, Reply All, and Forward. Cc is carried end to
  end through the native B2F path. Drafts auto-save to a local store keyed by a
  stable draft id and reopen exactly as left.
- **Address book.** Contacts and distribution groups with an inline editor.
  Recipient fields autocomplete from contacts and expand groups to their
  members at send time.
- **HTML Forms, full Winlink Express catalog.** The complete Winlink Express
  Standard Forms snapshot (251 templates) ships bundled. Compose or view any
  catalog form through a hierarchical browser; native React composers cover the
  highest-volume forms (ICS-213, Bulletin), and the long tail renders through
  Tuxlink-skinned child webviews. Received form-tagged messages render their
  viewer template inline. Drop a `.html` file into the custom-forms directory
  and it appears in the browser on next launch, for club-specific forms or
  templates released after the bundled snapshot.
- **Find Messages.** Token-driven full-text search across folders
  (`FOLDER:`, `FROM:`, `SUBJECT:`, `BEFORE:`, `AFTER:`, `UNREAD:`, `HAS:`),
  plus saved and recent searches.

### Stations, requests, and position

- **Find a Gateway.** A location-aware station finder polls Winlink RMS gateway
  lists by mode and sorts results by distance from the operator's grid square.
  Star a gateway to save it as a favorite; the radio panels surface saved
  favorites and recent connections per transport.
- **Request Center.** A request-first workspace resolves location-aware catalog
  requests (state and marine forecasts, propagation, solar-terrestrial, aurora,
  public gateway lists) from the operator's grid square, runs a catalog-wide
  search, and composes Saildocs GRIB requests. Selected items collect in a
  unified basket and dispatch per rail.
- **Offline map.** A position and station map renders from a configurable tile
  source, with tile-source provenance status and a validated-precision gate for
  fine zoom.
- **GPS privacy controls.** Position broadcast defaults to off. Operators may
  switch to local-display-only or broadcast at a chosen precision. The default
  reduces a broadcast position to a 4-character Maidenhead grid (about one
  degree). Higher precision is opt-in.

### Interface and operations

- **Native desktop GUI.** [Tauri](https://tauri.app/) 2.x with a
  React 18 + TypeScript frontend rendered by WebKitGTK 4.1. Custom title bar
  and native-style menu bar, a dashboard ribbon (callsign, grid, time,
  connection, Connect), the folder sidebar, the message list with search
  highlighting, the reading pane, and a mode-aware radio panel.
- **Onboarding wizard.** A first-run wizard takes a new operator from install
  to first message: callsign, grid, default transport, and an optional test
  send. It offers a CMS-connected path and an offline / radio-only path.
- **Session log.** A per-mode surface inside the radio panel renders both the
  human-readable projection of the CMS session and the raw B2F wire dialogue.
- **Color schemes.** Six bundled presets (Default dark, Daylight, High contrast
  light, Paper, Night / tactical red, Grayscale) plus an inline Theme Designer
  for custom palettes, for outdoor and bright-sun LCD readability.
- **Diagnostic logging.** Structured logging exports to a single `.tar.zst`
  archive via Help → Logging → Export logs, or attaches automatically through
  Help → Report Issue. Environment probes capture keyring, audio, serial,
  modem-process, network, and display state at startup and on errors.
- **OS keyring credentials.** The OS keyring (secret-service on Linux) holds
  the Winlink CMS password. Tuxlink never persists it to a config file on disk.

Emergency operating happens in a tent at noon and an EOC at 3 a.m. Every color
scheme re-skins the whole interface — ribbon, folders, message list, reading
pane — not just an accent. **Night / tactical (red)** preserves night vision in
a darkened shelter; **Daylight** is a light, high-contrast palette for reading
an LCD in direct sun. The same mailbox, two lighting modes:

<p align="center">
  <img src="docs/readme/images/tuxlink-color-night-red.png" width="49%"
       alt="Tuxlink in the Night / tactical red color scheme: red-on-black across the ribbon, folder sidebar, message list, and reading pane" />
  <img src="docs/readme/images/tuxlink-color-daylight.png" width="49%"
       alt="Tuxlink in the Daylight color scheme: a light, high-contrast palette across the ribbon, folder sidebar, message list, and reading pane" />
</p>

## Install

Download the `.deb`, `.rpm`, or `.AppImage` for your distribution and
architecture (`x86_64` or `arm64`, with `SHA256SUMS`) from the
**[latest release](https://github.com/cameronzucker/tuxlink/releases/latest)**.

Install, first-run, uninstall, and build-from-source steps live in
**[docs/install.md](docs/install.md)**. Tuxlink requires WebKitGTK 4.1 and a
secret-service keyring daemon.

## Interface

The first-run wizard takes a new operator from install to first message with no
README and no tutorial, on a CMS-connected path or an offline / radio-only
path:

<p align="center">
  <img src="docs/readme/images/tuxlink-first-run-wizard.png" width="820"
       alt="Tuxlink first-run wizard connection-choice step offering CMS-connected and radio-only paths">
</p>

The Request Center resolves location-aware catalog requests, searches the
Winlink catalog, and collects selected items in a unified send basket:

<p align="center">
  <img src="docs/readme/images/tuxlink-request-center.png" width="820"
       alt="Tuxlink Request Center dialog showing weather, propagation, gateway list, catalog search, and GRIB request cards">
</p>

<sub>Images are generated from the current frontend in WebKitGTK using privacy-safe sample data.</sub>

## Maturity: what is and is not proven

Where each path stands:

- **On-air validated (RF path end-to-end):** three modes are vetted on the air —
  AX.25 1200-baud packet, ARDOP HF, and APRS tactical chat over Bluetooth KISS.
  Packet and ARDOP both connect over a real radio, and the transmit path to the
  Winlink network is proven end-to-end — a production Winlink CMS protocol response
  was received over the air. That response was a rejection pending the client
  registration noted below, which is precisely what confirms the chain (transmit,
  RF link, gateway, CMS) is intact; the gap is an account, not a path. Peer-to-peer
  sessions on both modes work today. APRS tactical chat has run continuously over a
  sustained Bluetooth KISS link **at the same time as** an HF Winlink session — the
  simultaneous HF/VHF workspace is functional today. Transmission always requires
  explicit, per-invocation operator consent (see
  [Amateur radio and Part 97](#amateur-radio-and-part-97)).
- **Validated (internet):** native CMS connection over telnet and real Winlink
  message receive and render, against the Winlink CMS test server.
- **Operator-pending (Part 97):** APRS beacon transmit (own-position beaconing)
  is built and pending operator on-air verification; the APRS receive and chat
  paths and native UV-Pro Bluetooth control are vetted on the air (above). On-air
  validation of any new transmit path, including clean abort and de-key, is the
  operator's to perform.
- **Production CMS registration:** message exchange with the production Winlink
  CMS requires Winlink's prior registration of the Tuxlink client. The
  over-the-air rejection above confirms the RF path already reaches the production
  CMS; message exchange begins once registration completes. Until then, CMS
  message exchange targets the test server.

### Pending

- **VARA over-the-air peer sessions.** The VARA panel manages the TCP transport
  to an operator's VARA instance; the RF connect-to-peer session lifecycle is
  pending.
- **Hamlib rig control** and USB rig autodetect.
- **Native HF modem.** VARA is x86 Windows software that runs under WINE on x86
  Linux but not on ARM. Tuxlink targets a clean-room native HF modem (Sonde,
  developed as a separate project) rather than bundling VARA; VARA-TCP wire
  compatibility serves operators who bring their own VARA install.

## Architecture

Tuxlink's desktop application lives in `src-tauri/` (Tauri 2.x with a React 18 +
TypeScript frontend in `src/` rendered by WebKitGTK 4.1). The Winlink engine, the
CMS connection, the B2F exchange, the mailbox, and the AX.25 packet path are
native Rust in `src-tauri/`; no external modem or sidecar process intervenes for
CMS. The desktop app ships as a single crate in the `v0.x` series, per
[ADR 0002](docs/adr/0002-tauri-react-single-crate.md).

**Sonde**, the clean-room native HF modem, is developed as a separate clean-room
project in its own repository (per [ADR 0019](docs/adr/0019-sonde-rebrand-and-extraction.md)).
Tuxlink will consume it as an external modem backend; it is not yet wired into
the desktop app's Winlink session lifecycle.

[CLAUDE.md](CLAUDE.md) documents the agent workflow, commit discipline, ethos,
and safety rails this project operates under.

## Amateur radio and Part 97

Tuxlink transmits under the operator's amateur radio callsign to real Winlink
CMS gateways. CMS-connected features require a valid amateur radio license. The
licensed operator bears responsibility for ensuring every transmission complies
with Part 97 of the FCC rules, or the equivalent regulations in the operator's
jurisdiction.

Tuxlink prohibits automated or agent-initiated transmissions absent explicit,
per-invocation operator consent. See
[docs/live-cms-testing-policy.md](docs/live-cms-testing-policy.md).

## Documentation

In-app documentation lives at **Help → Documentation**; bundled topics cover the
wizard, every transport, the mailbox, composing, HTML forms, operating modes,
search, settings, color schemes, keyboard shortcuts, and troubleshooting. The
source markdown resides in [`docs/user-guide/`](docs/user-guide/) for reading
outside the app. **Help → About Tuxlink** shows the running build's version,
license, and source-repository links.

## License

[GNU GPL v3 or later](LICENSE). Copyright 2026 Cameron Zucker.

## Contributing and development

[docs/development.md](docs/development.md) documents the build prerequisites,
toolchain setup, and runtime keyring requirement.
[CLAUDE.md](CLAUDE.md) holds the agent workflow, commit discipline, and project
ethos.
