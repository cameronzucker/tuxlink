<p align="center">
  <img src="assets/tuxlink_icon.png" alt="Tuxlink logo" width="120" height="120">
</p>

# Tuxlink

A native Linux desktop Winlink client for amateur-radio emergency communications.
No Windows, no web UI to babysit — a Rust application that speaks the Winlink B2F
protocol directly.

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-green.svg" alt="License: MIT"></a>
  <a href="CHANGELOG.md"><img src="https://img.shields.io/github/v/release/cameronzucker/tuxlink?label=release" alt="Latest release"></a>
  <a href="https://github.com/cameronzucker/tuxlink/actions/workflows/release.yml"><img src="https://img.shields.io/github/actions/workflow/status/cameronzucker/tuxlink/release.yml?label=build" alt="Build status"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.75+-orange.svg?logo=rust" alt="Rust 1.75+"></a>
  <a href="https://www.kernel.org"><img src="https://img.shields.io/badge/platform-linux-lightgrey.svg?logo=linux&logoColor=white" alt="Platform: Linux"></a>
</p>

> [!WARNING]
> **🚧 Tuxlink is pre-alpha software under active construction. It is not a working product.**
>
> Version tags on this repository are produced automatically by
> [release-please](https://github.com/googleapis/release-please) from conventional-commit
> activity — they reflect repository velocity, not release readiness. Anything below
> `v1.0` is incomplete, breakable, and unsuitable for production or emergency
> communications use.
>
> There are no installable artifacts. Running Tuxlink today requires building from source
> in a Tauri development environment on a developer machine with the full toolchain. The
> UI is still being shaped; surfaces shown in screenshots and mockups below may be
> out-of-date, partially-implemented, or actively being redesigned.
>
> Feature claims describe what has working code paths — they do not imply the integrated
> product is ready to operate. **Do not rely on Tuxlink for any emcomm deployment or
> as the only client for a real Winlink workflow.** Watch the repository for the first
> tagged release that does not carry this banner.

<p align="center">
  <img src="docs/design/mockups/images/mock-b-principles-faithful.png" width="860"
       alt="The Tuxlink mailbox: dashboard ribbon, folder sidebar, message list, reading pane, and a live session log">
</p>
<p align="center"><sub>The Tuxlink mailbox — dashboard ribbon, folder sidebar, reading pane, and the live B2F session log.</sub></p>

## Status

**Pre-alpha — see the banner above.** Working code paths exist for: CMS-over-telnet
sessions, B2F message exchange in Rust, AX.25 1200-baud packet plumbing, an ARDOP HF
transport core (radio-free MVP), GPS privacy controls, and a first-run wizard. Several
surfaces are being actively rebuilt against the locked UX spec
(`docs/design/v0.0.1-ux-mockups.md`) and may misbehave or look unfinished — most
visibly, the radio-mode connection panels.

On-air RF paths and production CMS access remain operator-verified-only. **Do not rely
on Tuxlink for live emergency traffic.** See
[Maturity](#maturity--what-is-and-isnt-proven) for the verified-vs-aspirational breakdown.

## What it is

[Winlink](https://winlink.org/) is the de-facto amateur-radio email system used by
emergency-communications teams, CERT organizations, the Red Cross, and offshore cruisers.
On Linux today the practical option is a proprietary Windows client under WINE; there
is no full native desktop Winlink application.

Tuxlink is a proper native desktop Winlink client for Linux. The first-run experience
needs no README and no YouTube tutorial, and the Winlink CMS password never touches a
config file on disk.

## Current features

The shipped surface area as of the latest pre-alpha build:

- **Native Winlink engine.** B2F implemented directly in Rust. CMS over
  telnet (TLS or plaintext), full propose / accept exchange, mailbox
  persistence. No external modem daemon or sidecar process for CMS.
- **Desktop GUI.** Tauri 2.x + React 18 rendered via WebKitGTK 4.1.
  Custom title bar + native-style menu bar; dashboard ribbon (callsign,
  grid, time, connection, Connect button); folder sidebar
  (Inbox / Outbox / Sent / Drafts) with per-mode connection entries;
  message list with search highlighting; reading pane; mode-aware radio
  panel; mailbox bar.
- **Onboarding wizard.** Callsign, grid, default transport, optional test
  send. CMS credentials live in the OS keyring (secret-service on Linux),
  never in a config file on disk.
- **Telnet.** Operator-to-CMS-over-internet sessions for development,
  training, or fall-back when HF is poor.
- **AX.25 1200-baud packet.** Connected-mode AX.25 over a KISS TNC
  (USB serial, Bluetooth RFCOMM, or KISS-TCP to a soundcard modem like
  Dire Wolf). Inline radio panel with SSID picker.
- **ARDOP HF.** Full UI for the ARDOP transport — pre-flight, dial,
  abort, quality scoring, session log. Driven by a local `ardopcf`
  daemon over its command + data sockets.
- **VARA TCP transport** (early — backend codec + smoke probe ship; the
  UI integration is in flight).
- **HTML Forms.** Position report, ICS-213, ICS-309, Bulletin, Damage
  Assessment compose + render. Catalog refresh path is in progress.
- **Compose.** New message / Reply / Reply All / Forward; Cc carried
  end-to-end via the native B2F path; drafts auto-save to a local store
  keyed by stable draft id; form-based composition shares the same
  window.
- **Find Messages.** Token-driven full-text search across folders
  (`FOLDER:`, `FROM:`, `SUBJECT:`, `BEFORE:`, `AFTER:`, `UNREAD:`,
  `HAS:`) plus saved + recent searches.
- **GPS privacy controls.** Position broadcast is off, local-display-
  only, or broadcast at a chosen precision; the default reduces broadcast
  to a 4-character Maidenhead grid (~1°). Higher precision is opt-in.
- **Color schemes.** Six bundled presets (Default dark, Daylight, High
  contrast (light), Paper, Night/tactical red, Grayscale) plus an inline
  Theme Designer for custom palettes — motivated by outdoor / bright-sun
  LCD readability.
- **Session log.** Per-mode session-log surface inside the radio panel —
  both the human-readable projection of the CMS session and the raw B2F
  wire dialogue.

## User guide

In-app documentation lives at **Help → Documentation** — bundled topics
cover the wizard, every transport, the mailbox, composing, HTML forms,
search, settings, color schemes, keyboard shortcuts, and troubleshooting.
The source markdown is in [`docs/user-guide/`](docs/user-guide/) for
reading outside the app.

**Help → About Tuxlink** shows the running build's version, license, and
links to the source repository.

**Help → Report Issue** opens the project's GitHub issue tracker in your
default browser.

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

## Not yet shipped

- **Hamlib rig control** and USB rig autodetect.
- **VARA HF / VARA FM as third-party binary.** VARA is x86 Windows software
  that runs under WINE on x86 Linux but not on ARM. The Tuxlink position is
  to ship a clean-room native HF modem (v0.5+) instead of bundling VARA;
  early VARA-TCP wire compatibility is landing for operators who want to
  bring their own VARA install.
- **Packaging.** `.deb` / `.rpm` / AppImage / Flatpak are not yet built;
  the install path is build-from-source.
- **Trash / Deleted folder behavior.** The Deleted folder is a UI
  placeholder; delete-from-mailbox semantics are pending.

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

**Build from source** is the path today — a prebuilt AppImage via CI is forthcoming. No Go toolchain required; Rust only. The runtime requires a secret-service-compatible keyring daemon on Linux; see
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
