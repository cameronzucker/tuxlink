# Tuxlink

A Linux desktop Winlink client for amateur-radio emergency communications — wraps
[Pat](https://getpat.io/) so operators never have to touch it directly.

## Status

**v0.0.1 in active development.** The milestone: a non-Cameron user can install
tuxlink, complete the onboarding wizard, send a Winlink CMS message, receive a
reply, and never invoke Pat directly. Code exists; the build works; the UI is
functional. See the [v0.0.1 plan](docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md)
for full scope details.

## What it is

[Winlink](https://winlink.org/) is the de-facto amateur-radio email system used by
emergency communications teams, CERT organizations, Red Cross, and offshore
cruisers. On Linux today, the options are a proprietary Windows client under WINE,
or Pat — a capable library with a web UI, not a desktop application.

Tuxlink is a proper desktop mail client for Winlink on Linux. The first-run
experience does not require reading a README, does not require a YouTube setup
tutorial, and the Winlink CMS password never touches a config file on disk.

## v0.0.1 features

- **Desktop GUI** — Tauri 2.x + React 18 frontend; native OS menu bar and system
  tray; AppShell with dashboard ribbon, folder sidebar (Inbox / Sent / Outbox /
  Archive), message list, reading pane, session log strip, and status bar.
- **Onboarding wizard** — guides the operator through CMS-connected setup (callsign
  + Winlink CMS password → stored in the OS keyring, never on disk) or offline /
  radio-only setup. Optional test-send to verify the round-trip before entering the
  mailbox.
- **Mailbox** — Inbox and Sent views backed by Pat's HTTP API. Message reading pane
  with reply actions.
- **Compose** — compose new messages or replies in a separate floating window.
- **Live session log** — human-readable projection of the CMS session as it happens.
- **CMS over TLS** (port 8773, default) or plain Telnet (port 8772, fallback).
- **Bundled Pat sidecar** — tuxlink spawns and manages the
  [tuxlink-pat](https://github.com/cameronzucker/tuxlink-pat) fork (itself a fork
  of [la5nta/pat](https://github.com/la5nta/pat)) as a child process. Operators
  never run Pat directly.

## Not in v0.0.1 (deferred to v0.1+)

The following are explicitly out of scope for v0.0.1:

- VARA HF / VARA FM (requires WINE bridge in v0.1; clean-room replacement planned
  for v0.5+)
- AX.25 / packet radio
- Hamlib rig control and USB autodetect
- Email attachments
- ICS-213 / HTML form rendering (Red Cross, SHARES)
- Position reports
- Flatpak, `.deb`, and `.rpm` packaging — v0.0.1 is AppImage only. A prebuilt
  AppImage via CI is tracked as a separate task (not yet available; build from
  source for now).

## Architecture

Tuxlink v0.0.1 is a **single Rust crate** (`src-tauri/`) using Tauri 2.x as the
desktop framework, with a **React 18 + TypeScript frontend** (`src/`) rendered via
WebKitGTK 4.1. Pat — via the
[tuxlink-pat](https://github.com/cameronzucker/tuxlink-pat) fork — runs as a
managed HTTP sidecar: tuxlink spawns it on launch, talks to its HTTP API for
mailbox operations, and terminates it on quit.

The Winlink CMS password is stored in the **OS keyring** (secret-service on Linux,
Keychain on macOS, CredentialManager on Windows) and never written to a config file.
This is enforced at the engine layer via the tuxlink-pat fork (see
[ADR 0011](docs/adr/0011-fork-pat-for-tuxlink.md)).

**Post-v0.0.1 direction:** a layered multi-crate workspace (`tuxlink-core`,
`tuxlink-protocol-native`, etc.) is the planned v0.5+ architecture, but it is NOT
what v0.0.1 ships. See [ADR 0002](docs/adr/0002-tauri-react-single-crate.md) for
the rationale behind the single-crate decision.

See [CLAUDE.md](CLAUDE.md) for the agent workflow, ethos, and safety rails this
project operates under.

## Install

See **[docs/install.md](docs/install.md)** for the full install and first-run guide.

**Build from source** is the path today — a prebuilt AppImage via CI is forthcoming.
The runtime requires a secret-service-compatible keyring daemon on Linux; see
[docs/development.md — Runtime prerequisites for end-users](docs/development.md#runtime-prerequisites-for-end-users)
for the short list of what desktops need action and what do not.

**System dependency:** WebKitGTK 4.1 is required. Distros shipping only WebKitGTK
4.0 (older Debian stable, older RHEL/CentOS) cannot run v0.0.1 without a backport.

## Amateur radio / Part 97

Tuxlink transmits under the operator's amateur-radio callsign to real Winlink CMS
gateways. A valid amateur-radio license is required to use CMS-connected features.
The licensed operator is responsible for ensuring all transmissions comply with
Part 97 of the FCC rules (or the equivalent regulations in their jurisdiction).

Automated or agent-initiated transmissions without explicit, per-invocation operator
consent are prohibited. See [docs/live-cms-testing-policy.md](docs/live-cms-testing-policy.md).

## License

[MIT](LICENSE) — Copyright 2026 Cameron Zucker.

## Contributing / Development

Build prerequisites, toolchain setup, and the runtime keyring requirement are
documented in **[docs/development.md](docs/development.md)**.

Agent workflow, commit discipline, and project ethos are in **[CLAUDE.md](CLAUDE.md)**.
Contributions are welcome once the v0.0.1 milestone lands.
