# Tuxlink

A Linux-native, full-capability Winlink client for amateur radio
emergency communications.

## Status

**Pre-implementation.** Project framing and v0.1 scope were established
on 2026-04-22 via the `/office-hours` skill. No code yet.

The approved v0.1 plan is Approach B (layered Rust workspace) from the
design doc. See the plan in full:

> `~/.gstack/projects/cameronzucker-tuxlink/cameronzucker-main-design-20260422-200809.md`

## What it is

Winlink is the de-facto amateur-radio email system used by emergency
communications, CERT teams, Red Cross, and offshore cruisers. Today's
options on Linux are a proprietary Windows client under WINE, or
[Pat](https://getpat.io/) — a library with a web UI, not a desktop app.

Tuxlink is a proper desktop mail client for Winlink on Linux, with a
first-run experience that doesn't require reading a README, doesn't
require a YouTube setup tutorial, and (from v0.5+) doesn't require
WINE.

## Roadmap (from the design doc)

- **v0.0.1 — Weekend demo.** Telnet-only Tauri app over Pat-as-daemon.
  Thesis: a Linux user can install tuxlink, enter callsign credentials,
  send a Winlink telnet message, receive mail, and not touch Pat directly.
- **v0.1 — First shippable release.** Telnet, AX.25, VARA HF / VARA FM
  via Pat-Vara + WINE, hamlib autoconfig, ICS-213 + Red Cross forms,
  Flatpak + `.deb` + `.rpm` + RaspPi image. First-run wizard covers
  zero-reading install-to-first-message in under 30 minutes.
- **v0.5 — Protocol independence.** Native B2F + native VARA
  (clean-room). No WINE. Dual-backend (Pat vs native) until interop
  tested.
- **v1.0 — Institutional-adoption release.** Full Winlink Express
  feature parity. CERT / Red Cross fleet replacement story.

## Architecture

Rust workspace with clean layer separation from day one:

- `tuxlink-core` — message model, mailbox, form model, protocol traits
- `tuxlink-protocol-pat` — Pat-as-daemon backend (v0.1)
- `tuxlink-protocol-native` — native B2F + VARA (v0.5+, stubbed in v0.1)
- `tuxlink-radio` — hamlib, USB autodetect, rig profile management
- `tuxlink-forms` — ICS-213, SHARES, Red Cross, HTML renderer
- `tuxlink-session` — session orchestration, scheduler, retry policy
- `tuxlink-app` — Tauri desktop binary + SvelteKit/React UI

See [CLAUDE.md](CLAUDE.md) for the agent workflow, ethos, and safety
rails this project operates under.

## License

GPL-3.0-or-later (planned, consistent with Pat / vARIM and the emcomm
community norm).

## Contributing

Too early. Tuxlink is in the design-doc stage. The first code commit
will be the v0.0.1 weekend demo. Watch this repo or follow Cameron's
other work on [geographica](https://github.com/cameronzucker/geographica)
(the sister project and one of Pandora's planned services).
