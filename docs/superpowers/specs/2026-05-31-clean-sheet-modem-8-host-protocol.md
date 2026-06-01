# Subsystem #8 — Host protocol / control plane

> **Status: Canonical.** Subordinate to
> [2026-05-31-clean-sheet-modem-overview.md](2026-05-31-clean-sheet-modem-overview.md).
> Incorporates overview §5.A.3 (TCP host protocol via the existing
> `ModemTransport` abstraction — pattern settled; vocabulary remains to
> design at this subsystem's level), §5.A.4 (AGPLv3-only — the host
> protocol itself doesn't constrain license, but the daemon that exposes
> it does).
>
> **Architectural note:** ADR 0015 originally defers this subsystem's
> design with
> the note "**must be settled before the modem spec**." This STUB captures the
> open questions but does NOT settle the deferred choice — that's an
> operator-decision moment in the brainstorm.

## §1. Role

The host protocol subsystem defines the **API between the modem and the
client.** When tuxmodem is embedded in tuxlink, the host protocol is the
seam between tuxlink's transport layer (`ModemTransport` per ADR 0015) and
tuxmodem's internal state. When tuxmodem ships as a standalone TCP daemon
(per subsystem #10), the host protocol is the *only* interface other
software (Pat, ARIM, etc.) has to tuxmodem.

Per ADR 0015's deferred question: the on-air protocol (subsystems #3-#7)
is bound by ADR 0014's clean-sheet rule, but **the host-side control API
is not** — because it's a software interface design choice, not a
transmitted waveform.

## §2. What the subsystem is NOT

- **Not the on-air protocol.** Subsystems #3-#7 define what goes over
  the air; #8 defines what the client and modem say to each other on
  the host side.
- **Not the rig control plane.** PTT, frequency, mode-set, audio gain
  are handled by the `tux-rig` crate per ADR 0015. Host protocol may
  expose rig-status queries (informational) but doesn't own rig
  control.
- **Not the client-application protocol.** B2F (Winlink), HTML Forms,
  etc., live above the host protocol in the consuming client.

## §3. Forcing functions

1. **Transport choice.** Local Unix-domain socket, TCP, stdio, shared
   memory, or D-Bus. Each has tradeoffs: Unix socket is cleanest for
   same-host integration; TCP allows network-remote control (Pat
   already uses TCP for ardopcf); stdio is simplest but constrains the
   process model.
2. **Protocol style.** Text-based command/response (KISS-style,
   AT-style, line-oriented JSON) vs. binary framed.
3. **Standardization against prior art.** ARDOP's host interface
   (documented in `pflarue/ardop/docs/refs/Host_Interface_Spec_for_WL2K
   _supported_Protocols_TNCs_20171109.pdf` — open-access) is the
   established Linux-amateur-radio convention. Direwolf's KISS interface
   is another reference. Either of those could be adopted (subsystem
   gets interop with existing client tools for free) or tuxmodem could
   define a novel protocol (more flexible, no existing client support).
4. **Versioning + capability negotiation.** Required from day one.
   Client and modem must be able to agree on which protocol version is
   in use; missing features must be discoverable without trying them.
5. **State-change semantics.** Some commands are synchronous (set audio
   gain → confirm); some are asynchronous (initiate connection → events
   stream during the connection lifecycle). Protocol must handle both
   cleanly.
6. **Security posture.** A network-listening tuxmodem TCP port could be
   reached from off-host. Either bind to localhost only (default), or
   support authentication (TLS, token-based). Default-localhost-only is
   safer; authentication is required if remote-control is a use case.
7. **Performance.** Per-frame latency matters less than for the on-air
   PHY but still has limits — operator interactive UI can tolerate
   tens of ms; tight modem control loops less so.

## §4. Open design questions (THE BIG ONE that must settle before subsystem #5/#6 freeze)

| # | Question | Notes |
|---|---|---|
| §8.Q1 | Transport — Unix socket, TCP, stdio, D-Bus? | Foundation choice. TCP is the established Linux-amateur convention (ardopcf, Direwolf, hamlib's rigctld); Unix socket is cleaner for tuxlink-internal use. |
| §8.Q2 | Standardize against ARDOP host interface, KISS, or define new? | Interop vs. flexibility. **This is the explicit ADR 0015 deferred question.** |
| §8.Q3 | Text vs. binary protocol? | Text is debuggable; binary is efficient. ardopcf uses text. |
| §8.Q4 | Versioning scheme — semver, capability bits, or both? | Required. |
| §8.Q5 | Sync vs. async command model? | Both required; question is the protocol-level distinction. |
| §8.Q6 | Authentication / authorization model — localhost-only default, token-based, TLS? | Security posture. |
| §8.Q7 | Two-port (cmd + data, ardopcf-style) or single-port multiplexed? | Tradeoff: two ports is conceptually clean but more setup; single multiplexed is more compact. |
| §8.Q8 | Stable API commitment — when does the protocol freeze? | Before the standalone-daemon spin-off ships. After tuxmodem-in-tuxlink is operationally validated. |

## §5. Citations from foundation doc

- §6.2 (ARDOP): Host_Interface_Spec PDF in ardopcf/docs/refs — the
  established Linux-amateur HF-data host protocol convention.
- Direwolf: KISS-protocol implementation reference.
- General Internet RFCs: TCP, telnet, line-protocol conventions
  (foundational).

## §6. Dependencies

- **Upstream:** subsystems #5 (MAC operations are what host commands
  invoke), #6 (ARQ connection state must be addressable from the API),
  #7 (link-adaptation state + override exposure).
- **Downstream:** subsystem #9 (tuxlink integration uses the host
  protocol), subsystem #10 (standalone daemon exposes the host protocol
  to external clients).

## §7. No-implementation-choice markers

No specific transport, protocol style, command set, versioning scheme,
or authentication model designated.

## §8. Watched failure modes

- **Premature commitment.** Settling the host protocol form before the
  on-air protocol stabilizes can force breaking-change cycles. Per ADR
  0015's framing: host protocol must be settled **before subsystems
  #5/#6 freeze**, but not earlier than necessary.
- **Interop trap.** Adopting ARDOP host interface for "free interop"
  with Pat/ARIM means inheriting ARDOP's design choices (and bugs).
  Could be net positive (most adoption friction is the on-air protocol,
  not the host interface) or net negative (locks tuxmodem out of
  novel host-side features).
- **Network exposure.** TCP listening modems can be reached from
  off-host. Default to localhost-only; require explicit opt-in for
  network exposure; document the security model in the standalone-
  daemon README.
- **Capability-negotiation drift.** Without explicit versioning + cap
  bits, downstream clients (Pat, ARIM) hard-code assumptions and
  silently break when tuxmodem evolves. Versioning is required from
  day one.

Agent: mink-swallow-kite
