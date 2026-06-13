# Credits and acknowledgments

Tuxlink does not exist in isolation. The broader amateur radio community,
the Linux ecosystem, and several specific projects make tuxlink possible.
This page acknowledges them.

## The Winlink system

The Winlink network — CMS infrastructure, RMS gateways, the protocol
specification, the Express client — is operated by the **Amateur Radio
Safety Foundation** (ARSF, https://winlink.org). Tuxlink is an
independent Linux client that speaks the published B2F protocol and
connects to the public Winlink network. ARSF runs the global infrastructure
that makes any Winlink session possible.

## Standards and references

- **B2F protocol.** Documented in the Winlink technical documents on
  winlink.org. The framing and message format that every Winlink session
  uses.
- **AX.25 protocol.** ARRL / TAPR documentation. The packet layer
  underneath Winlink Packet.
- **AREDN documentation** (https://docs.arednmesh.org). The quality bar
  this guide aims for. AREDN is an amateur radio mesh network project;
  its documentation is the operator-facing reference standard the
  amateur radio community knows it can produce.

## Open-source projects tuxlink depends on

### Direct runtime dependencies

- **Dire Wolf** (https://github.com/wb2osz/direwolf). The software TNC
  tuxlink talks to for Packet operation. Written by John Langner WB2OSZ.
  Without Dire Wolf, native Linux Packet would not exist.
- **ardopcf** (the community port of ARDOP). The HF data mode tuxlink
  speaks for ARDOP sessions. Maintained by a community of contributors
  who keep the open-source ARDOP modem alive and working.
- **Hamlib** (https://hamlib.github.io). The CAT-control abstraction
  layer that makes per-radio frequency / mode control possible from a
  single tuxlink build. Hamlib is the reason new radio support comes
  from a model-number change rather than a code change.

### Architecture / runtime libraries

- **Tauri** (https://tauri.app). The cross-platform desktop application
  framework tuxlink runs on. Lets the Rust backend and the TypeScript /
  React frontend share one binary.
- **React** + **TypeScript** + **Vite**. Frontend stack.
- **Tokio** + **async-trait**. Async Rust runtime + ergonomics.
- **DOMPurify**. The HTML sanitizer used to render this very help window
  safely.
- **`marked`** + **Mermaid**. The markdown and diagram libraries
  rendering the user guide.

### Prior-art clients

- **Winlink Express.** The reference Windows client. The B2F protocol's
  reference implementation; the user-surface conventions Winlink Express
  established are what most Winlink operators expect. Tuxlink's surfaces
  are deliberately divergent where the Linux desktop convention
  differs (see [Migration](32-from-express-or-pat.md)).
- **Pat** (https://github.com/la5nta/pat). The open-source Go Winlink
  client. La5nta and the Pat contributors did the work of reverse-
  engineering Winlink's protocols for the open-source ecosystem.
  Tuxlink's B2F implementation references wl2k-go (Pat's protocol
  library) for cross-checking wire-format behaviour.

### Radio-control protocol decoding

- **benlink** (https://github.com/khusmann/benlink) by **Kyle Husmann
  (KC3SLD)**. The open-source decode of the **Benshi/Vero** Bluetooth
  control protocol used by the BTECH UV-Pro and related radios. Tuxlink's
  native UV-Pro device control (channel / frequency / mode / status over
  Bluetooth) derives its protocol from benlink. Apache-2.0.
- **HTCommander** (https://github.com/Ylianst/HTCommander) by **Ylian
  Saint-Hilaire**. A full control client for the same radio family, built
  on benlink's work; tuxlink cross-validated the command set and
  channel-selection mechanism against it. Apache-2.0.

  Tuxlink reimplements the protocol independently in Rust (no source code
  copied). A full attribution + license-compliance review lives in the
  developer reference docs (`docs/reference/uvpro-benshi-protocol-attribution.md`).

### Modem / mode references

- **VARA HF** by EA5HVK. The closed-source HF modem tuxlink can drive
  over its published TCP socket interface. Not a tuxlink dependency in
  the strict sense — VARA runs separately — but the integration would
  not work without VARA's documented interface.

## The amateur radio community

Beyond named projects, the amateur radio community at large — the
operators running RMS gateways, the net controllers running EmComm nets,
the YouTube and forum educators explaining Winlink to new operators, the
ARES and SHARES coordinators keeping emcomm preparedness alive — is what
makes a Winlink client useful. Tuxlink would be operating into a vacuum
without them.

This guide will, over time, accumulate specific creator acknowledgements
as their framings of particular concepts shape specific topics. Credits
for those will appear here with the creator's callsign, channel link,
and the topics their work informed.

## Tuxlink itself

Tuxlink is developed by Cameron Zucker. The repo lives at
https://github.com/cameronzucker/tuxlink. Contributions welcome through
the usual GitHub mechanisms.

## Reporting an attribution issue

If an attribution above is incorrect, incomplete, or missing — or if a
project should be credited that isn't — please open an issue on the
GitHub repo. The credits page is meant to be living documentation, not a
static snapshot.

## Where next

- [What is tuxlink](01-what-is-tuxlink.md) — the project's framing.
- [The Winlink ecosystem](04-the-winlink-ecosystem.md) — the system tuxlink interoperates with.
- [Moving from other Winlink clients](32-from-express-or-pat.md) — the prior-art clients tuxlink takes operating conventions from.
