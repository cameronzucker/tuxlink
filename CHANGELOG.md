# Changelog

All notable changes to Tuxlink are documented here.

This project adheres to [Semantic Versioning](https://semver.org) with project-specific rules described in [VERSIONING.md](VERSIONING.md). Entries from `v0.0.2` onward are generated automatically by [`release-please`](https://github.com/googleapis/release-please) from [Conventional Commits](https://www.conventionalcommits.org).

## 0.0.1 (2026-05-21)

First tagged release. Tuxlink is a Linux-native desktop Winlink client for amateur-radio
email — a proper mail application for [Winlink](https://winlink.org/), where the prior
Linux options were a Windows client under WINE or [Pat](https://getpat.io/)'s web UI. The
milestone for this release: a new operator can install Tuxlink, complete the onboarding
wizard, send a Winlink CMS message, receive a reply, and never invoke Pat directly.

Built on Tauri 2 (Rust backend) with a React 18 / TypeScript frontend, distributed as a
Linux AppImage.

### Highlights

- **Onboarding wizard** — first-run setup for CMS-connected operation (callsign + Winlink
  CMS password) or an offline / radio-only identity. The CMS password is stored in the OS
  keyring (Secret Service) and never written to a config file on disk. An optional
  test-send verifies the round-trip before entering the mailbox.
- **Native Winlink CMS client** — a from-scratch Rust implementation of the Winlink
  session: telnet and TLS-wrapped transports, secure-login challenge/response, the FBB B2
  forwarding protocol with lzhuf compression, framed block transfer, and B2F message
  exchange — validated against the live Winlink CMS, backed by a Pat-independent on-disk
  message store.
- **Connect** — a one-click CMS exchange from the dashboard ribbon, with fail-fast connect
  timeouts, live per-step progress in the session log, and an Abort control for an
  in-flight connection.
- **Mailbox** — folder sidebar (Inbox / Sent / Outbox / Archive), a virtualized message
  list, a reading pane with RFC 5322 parsing, and read/unread tracking.
- **Compose** — author new messages and replies in a separate window with draft
  persistence.
- **Live session log** — a human-readable projection of the CMS session as it happens,
  plus a raw view.
- **Desktop integration** — custom dark application chrome (titlebar + menu bar) with
  keyboard accelerators, a system tray with close-to-tray, and selectable color schemes
  (night / tactical / grayscale).
- **Bundled Pat sidecar** — Tuxlink spawns and supervises the
  [tuxlink-pat](https://github.com/cameronzucker/tuxlink-pat) fork as a managed child
  process, so operators never run Pat directly.

### Not in this release

VARA HF / VARA FM, AX.25 / packet radio, and Hamlib rig control are deferred to v0.1+. See
[VERSIONING.md](VERSIONING.md) and the README roadmap for the full scope.
