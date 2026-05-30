# 15. Modem integration and rig-control foundation

Date: 2026-05-27
Status: Accepted
Deciders: Cameron Zucker (operator), marten-finch-gorge (agent)

## Context

Tuxlink is gaining an ARDOP HF transport, and a clean-sheet first-party HF modem
is on the v0.5+ roadmap (ADR 0014). Both interact with external RF/audio
processes, both need rig control (PTT minimum, frequency/mode for single-pane
UX), and the sound card is a single contended resource (one radio, one audio
interface, one modem at a time). The first-party modem may eventually ship as a
**standalone open-source TCP daemon** usable by non-tuxlink clients
(Pat/ARIM/etc.), which would invert who owns rig control.

## Decision

1. **tuxlink launches and owns the modem lifecycle** (managed-spawn) — tuxlink is
   the single arbiter of the sound-card conflict. Lifecycle = spawn / supervise /
   SIGINT-clean-stop / confirm-audio-device-released-before-swap.
2. **The transport to any soundcard modem is a generic "external TCP modem"
   client**, NOT modem-special-cased. ardopcf / Dire Wolf / VARA / (future)
   first-party tuxmodem are all instances of one `ModemTransport` abstraction
   (drive a modem over its TCP host protocol + manage its process).
3. **Rig control is its own crate** (`tux-rig`: trait
   `Ptt/SetFreq/SetMode/ReadStatus` + Hamlib as the first backend) — NOT baked
   into client internals. Consumed by ARDOP-full and the future first-party
   modem; structured so a future standalone modem daemon and the client can both
   link the crate (build-once survives the spin-off).

## Consequences

- The ARDOP MVP can ship without `tux-rig` (modem keys PTT via RTS) — see
  `docs/superpowers/plans/2026-05-27-ardop-mvp-transport.md`.
- The full single-pane (tuxlink owns CAT freq + PTT) depends on tuxlink-5jb
  (rig-control plane research → `tux-rig` crate).
- `rigctld` becomes the third managed external process tuxlink supervises
  (alongside ardopcf and Dire Wolf), reusing the same spawn/SIGINT machinery.
- Modem spin-off vs. monolith remains an open packaging decision; (2) and (3)
  keep it open at near-zero extra cost.
- The ARDOP transport is built **synchronous + threads** (a `ByteLink`-style sync
  `Read + Write`), not Tokio: its concurrency is a fixed fan-out of three (cmd
  socket + data socket + child process), and its consumer — the shared
  synchronous blocking B2F engine `run_exchange` — would otherwise force a
  sync↔async seam. This is a principled fit, not mimicry of the AX.25 transport.

## Alternatives considered

- **Per-protocol special-casing (no `ModemTransport` abstraction):** rejected — forks
  ardopcf/Dire Wolf/VARA handling and forecloses the spin-off optionality.
- **Dial-only (operator runs the modem; tuxlink only opens TCP):** rejected — loses
  single-pane arbitration of the sound-card conflict, which is the core UX win.
- **Async (Tokio) ARDOP transport:** rejected — fan-out of three (cmd socket + data
  socket + child process) needs no async runtime, and the shared *synchronous*
  `run_exchange` B2F engine would force a sync↔async seam (a failure mode already hit
  here per Cargo.toml L21). Sync+threads is the principled fit; wl2k-go/Pat using Go
  goroutines is not evidence for Rust async.
- **Rig control baked into the client (not a crate):** rejected — the future standalone
  modem daemon needs the same PTT/CAT logic; a shared crate is build-once.

## Open (deferred)

- PTT/frequency sequencing for ARDOP-full (MVP vs. full single-pane).
- Host-protocol / clean-sheet line for the eventual standalone modem (the
  on-air protocol is clean-sheet per ADR 0014; the host-side control API is
  argued: NOT bound by clean-sheet — settle before the modem spec).
- Hamlib backend form (libhamlib FFI vs. managed `rigctld` subprocess vs.
  minimal own-CAT).

## Related

- ADR 0014 — Clean-sheet modem; no prior-art examination (the modem's on-air
  protocol).
- tuxlink-5jb — Frequency/rig control plane research.
- tuxlink-6aj — Add ARDOP HF transport (consumes decisions #1, #2 in MVP).
- docs/design/ardop-deployment-findings.md — Full findings (Locked decisions
  section + Forward-looking analysis).
