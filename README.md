<p align="center">
  <img src="assets/tuxlink_icon.png" alt="Tuxlink logo" width="120" height="120">
</p>

# Tuxlink

A native Linux desktop Winlink client for amateur-radio emergency communications.
No Windows, no web UI to babysit — a Rust application that speaks the Winlink B2F
protocol directly.

<p align="center">
  <img src="docs/design/mockups/images/mock-b-principles-faithful.png" width="860"
       alt="The Tuxlink mailbox: dashboard ribbon, folder sidebar, message list, reading pane, and a live session log">
</p>
<p align="center"><sub>The Tuxlink mailbox — dashboard ribbon, folder sidebar, reading pane, and the live B2F session log.</sub></p>

## Status

**v0.2.0 — early release.** Tuxlink is a working native Winlink client: it connects to
the Winlink CMS over telnet, runs the B2F message exchange in Rust, and sends, receives,
and renders real Winlink messages. AX.25 1200-baud packet, GPS privacy controls, a
no-docs-required first-run wizard, and an ARDOP HF transport core (radio-free MVP) ship
in this release.

It is early. On-air RF paths (AX.25 over a real radio) and production CMS access require
operator validation — see [Maturity](#maturity--what-is-and-isnt-proven) before relying
on it for live emergency traffic.

## What it is

[Winlink](https://winlink.org/) is the de-facto amateur-radio email system used by
emergency-communications teams, CERT organizations, the Red Cross, and offshore cruisers.
On Linux today the practical options are a proprietary Windows client under WINE, or
Pat — a capable library with a web UI, not a desktop application.

Tuxlink is a proper native desktop Winlink client for Linux. The first-run experience
needs no README and no YouTube tutorial, and the Winlink CMS password never touches a
config file on disk.

## v0.2.0 features

- **Native Winlink engine** — the Winlink B2F protocol implemented directly in Rust.
  Connects to the CMS over telnet (TLS port 8773 / plaintext port 8772), proposes and
  accepts messages, and files them into the local mailbox. No external modem daemon or
  sidecar process for CMS.
- **Desktop GUI** — Tauri 2.x + React 18; native OS menu bar; AppShell with dashboard
  ribbon, folder sidebar (Inbox / Sent / Outbox / Archive), message list, reading pane,
  session-log strip, and status bar.
- **Onboarding wizard** — callsign + Winlink CMS password (stored in the OS keyring,
  never on disk) for CMS setup, or an offline / radio-only path. Mounts the mailbox on
  completion.
- **AX.25 1200-baud packet** — connected-mode AX.25 over a KISS TNC (USB serial or
  Bluetooth RFCOMM), carrying the same B2F exchange. Inline connection panel; no pop-up
  windows.
- **ARDOP HF transport (radio-free MVP)** — ARQ transport core built on top of an
  `ardopcf` daemon: wire codec, command/data sockets, ARQ connect/disconnect, byte-stream
  data path, and a `ManagedModem` process supervisor. Backend-only in v0.2; the Connect
  panel doesn't expose ARDOP as a selectable transport yet — a follow-up release wires
  the UI.
- **GPS privacy controls** — position broadcast is off, local-display-only, or broadcast
  at a chosen precision; the default reduces broadcast position to a 4-character
  Maidenhead grid (~1°). Position is never broadcast more precisely than you opt into.
- **Mailbox + Compose** — Inbox / Sent / Outbox / Archive; reading pane with reply;
  compose new messages and replies.
- **Session log** — both a human-readable projection of the CMS session and the raw B2F
  wire dialogue, toggleable.

## Interface

The first-run wizard takes a new operator from install to first message with no README
and no tutorial — pick a CMS-connected path or an offline / radio-only path:

<p align="center">
  <img src="docs/design/mockups/images/wizard-a-welcome.png" width="820"
       alt="Tuxlink first-run wizard welcome screen: choose a CMS-connected or an offline / radio-only path">
</p>

Tuxlink is a native desktop application — no browser, no WINE, no web UI to keep alive in
a tab. Here it is on an Ubuntu 24.04 desktop:

<p align="center">
  <img src="docs/design/mockups/images/in-situ-ubuntu-2404.png" width="820"
       alt="Tuxlink on an Ubuntu 24.04 desktop">
</p>

<sub>The images above reflect the approved v0.2.0 interface design, which the application
renders faithfully.</sub>

## Maturity — what is and isn't proven

Tuxlink is honest about its edges:

- **Validated:** native CMS connection over telnet, and real Winlink message
  receive/render, against the Winlink CMS test server.
- **Operator-pending (Part 97):** AX.25 has been validated over a TCP/KISS loopback, but
  **on-air RF validation over a real radio is the operator's to perform** — no
  transmission happens without explicit, per-invocation operator consent (see
  [Amateur radio / Part 97](#amateur-radio--part-97)).
- **Production CMS:** reaching the production Winlink CMS requires the tuxlink client to
  be registered with Winlink (in progress); until then, CMS connectivity targets the test
  server.

## Not in v0.2.0

- **VARA HF / VARA FM** — VARA is x86 Windows software. It runs under WINE on **x86
  Linux** (no Windows OS required) but **not on ARM** (e.g. Raspberry Pi). A clean-room
  native HF modem is planned for v0.5+.
- **Hamlib rig control** and USB rig autodetect.
- **Email attachments.**
- **ICS-213 / HTML form rendering** (Red Cross, SHARES).
- **Packaging** — `.deb`, `.rpm`, and Flatpak are not yet built; build from source for now
  (a prebuilt AppImage via CI is forthcoming).

## Architecture

Tuxlink is a **single Rust crate** (`src-tauri/`) using Tauri 2.x as the desktop
framework, with a **React 18 + TypeScript frontend** (`src/`) rendered via WebKitGTK 4.1.
The Winlink engine — CMS connection, the B2F exchange, the mailbox, and the AX.25 packet
path — is native Rust; there is no external modem or sidecar process for CMS.

The Winlink CMS password is stored in the **OS keyring** (secret-service on Linux,
Keychain on macOS, CredentialManager on Windows) and is never written to a config file on
disk.

A layered multi-crate workspace is the planned v0.5+ direction; the current v0.x series
deliberately ships as a single crate (see [ADR 0002](docs/adr/0002-tauri-react-single-crate.md)).

See [CLAUDE.md](CLAUDE.md) for the agent workflow, ethos, and safety rails this project
operates under.

## Install

See **[docs/install.md](docs/install.md)** for the full install and first-run guide.

**Build from source** is the path today — a prebuilt AppImage via CI is forthcoming. The
runtime requires a secret-service-compatible keyring daemon on Linux; see
[docs/development.md — Runtime prerequisites for end-users](docs/development.md#runtime-prerequisites-for-end-users).

**System dependency:** WebKitGTK 4.1 is required. Distros shipping only WebKitGTK 4.0
(older Debian stable, older RHEL/CentOS) cannot run tuxlink without a backport.

## Amateur radio / Part 97

Tuxlink transmits under the operator's amateur-radio callsign to real Winlink CMS
gateways. A valid amateur-radio license is required to use CMS-connected features. The
licensed operator is responsible for ensuring all transmissions comply with Part 97 of
the FCC rules (or the equivalent regulations in their jurisdiction).

Automated or agent-initiated transmissions without explicit, per-invocation operator
consent are prohibited. See [docs/live-cms-testing-policy.md](docs/live-cms-testing-policy.md).

## License

[MIT](LICENSE) — Copyright 2026 Cameron Zucker.

## Contributing / Development

Build prerequisites, toolchain setup, and the runtime keyring requirement are documented
in **[docs/development.md](docs/development.md)**.

Agent workflow, commit discipline, and project ethos are in **[CLAUDE.md](CLAUDE.md)**.
