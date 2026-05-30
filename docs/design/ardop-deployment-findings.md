# ARDOP deployment & configuration — research findings (pre-plan)

> **Status:** research pass only — *no plan written yet*. Gathered 2026-05-24
> (agent marten-finch-gorge) to determine pertinent real-world ARDOP
> configuration facts before any ARDOP-support plan is drafted.
> **Decisions made by operator:** (1) add ARDOP support (ARDOP is in active use
> by real operators; tuxlink should not cut them off); (2) **tuxlink LAUNCHES and
> OWNS the modem lifecycle** (managed-spawn, not dial-only) — so tuxlink is the
> single arbiter of the one-sound-card conflict: on a VHF↔HF switch it spins the
> running modem down and the other up. Single-pane; uncomplicates a notoriously
> frustrating topic for new operators. Since tuxlink is the native client (Pat
> retired), tuxlink — not Pat — manages BOTH Dire Wolf (VHF packet) and ardopcf
> (HF). Implementation looks tractable — it is another local-TNC-over-TCP transport.
> **Sources** (full text cached in `dev/scratch/ardop-research/`): see bottom.

## Locked decisions (2026-05-24, operator)

**LOCKED:**
1. **Add ARDOP support** — serves real operators; tuxlink must not cut them off.
2. **tuxlink launches & owns the modem lifecycle** (managed-spawn) — tuxlink is the
   single arbiter of the one-sound-card conflict (spin Dire Wolf down / ARDOP up;
   SIGINT clean-stop; confirm the device released before the swap).
3. **ARDOP transport = generic "external TCP modem" client** — NOT ARDOP-special-cased.
   ardopcf / Dire Wolf / VARA / (future) tuxmodem are all instances of one transport
   abstraction (drive a modem over its TCP host protocol + manage its process).
4. **Rig control = its own crate from the start** (`tux-rig`: trait
   `Ptt/SetFreq/SetMode/ReadStatus` + Hamlib as the first backend) — NOT baked into
   client internals; consumed by ARDOP-full and the future first-party modem.

(3) + (4) are the optionality keystones: they keep modem spin-off-vs-monolith a
packaging decision, not a rewrite.

**STILL OPEN (deliberately):**
- **PTT/frequency sequencing** — ARDOP MVP (modem `-p` RTS PTT + manual freq, no rig
  crate) vs full single-pane (tuxlink owns Hamlib CAT+PTT). Depends on modem roadmap
  timing; MVP can ship without the crate.
- **Host-protocol / clean-sheet line** (← settle BEFORE the modem spec) — standalone
  modem adopts/echoes an existing host protocol (interop) vs clean-new; does clean-sheet
  bind the host control API (argued: NO — on-air only).
- **Modem spin-off vs monolith** — deferred; the two locks keep it open.
- **Hamlib backend form** — libhamlib FFI vs managed `rigctld` subprocess vs minimal own-CAT.

Canonical home for the locked architecture = a dedicated **modem ADR** (written when we
leave research mode); this doc + the bd issues are the interim lock.

## Bottom line

ARDOP is deployed exactly like the Dire Wolf / KISS path in *shape* — a local
soundcard-modem daemon you reach over loopback TCP — but the wire protocol,
ports, PTT ownership, and band/use-case differ. For tuxlink it is **a new host
transport: open the ARDOP control+data TCP sockets, issue an ARQ connect, then
stream the existing B2F session over the data socket.** The modem owns the
connection state machine, so tuxlink does *less* link-layer work than the
hand-rolled AX.25 path — but must speak ARDOP's command protocol and solve the
audio/PTT plumbing.

## 1. Which implementation to target

- **`ardopcf`** (Peter LaRue's fork of John Wiseman G8BPQ's `ardopc`) is the
  current, actively-maintained, multi-platform implementation. Target this.
- `ardopc` v1 / `piardopc` (Wiseman) is the older binary; KM4ACK's tooling still
  launches `piardopc`. Same protocol family as ardopcf.
- **ARDOP v2 is abandoned** (per Pat maintainers) — do *not* target v2.
- **Not in Debian apt.** It is a single static binary: download a prebuilt
  arm/amd release from GitHub or build from source, `chmod +x`, drop in
  `/usr/local/bin` or `$HOME/bin`. (Operator-side install; like Dire Wolf, the
  modem is external to tuxlink.)

## 2. TCP interface (what tuxlink connects to)

- **Control/command port: 8515** (default). ARDOP's own line-oriented command
  protocol (`MYCALL`, `ARQBW`, `DRIVELEVEL`, `FREQUENCY`, state events…).
- **Data port: 8516** — *control port + 1* (hardcoded convention; confirmed in
  wl2k-go `transport/ardop/tnc.go` and the protocol). The B2F byte stream rides
  here.
- **WebGui port: 8514** (optional, `-G 8514`) — ardopcf's own browser UI for
  audio-level tuning. Separate from the host interface.
- Reference client implementation to mirror: wl2k-go `transport/ardop/` (~10 Go
  files, on disk in `dev/scratch/ax25-prior-art/wl2k-go/`). ARDOP is an **open,
  published protocol** (spec PDFs also in that clone) — *not* under the
  clean-room constraint that governs the v0.5+ VARA work.

## 3. How the modem is launched / configured

Positional args after options: **`ardopcf [opts] <cmdport> <capture> <playback>`**

```
ardopcf --logdir ~/ardopc_logs -p /dev/ttyUSB1 -G 8514 \
        --hostcommands "DRIVELEVEL 90" 8515 plughw:1,0 plughw:1,0
```

- ardopcf is **not designed to run always-on / auto-start at boot** (per its
  author). It is started per operating session. *(Pat's .deb ships an optional
  `ardop@USER` systemd unit, but even Pat flags always-on as "for users with a
  functioning setup… makes debugging harder." Mild source divergence — treat
  ardopcf as start/stop-per-session, like KM4ACK's patmenu does.)*
- patmenu2 pattern: a single **"ARDOP Command"** config field holds the whole
  `ardopcf …` invocation. → tuxlink could own the launch the same way (compose +
  spawn the modem), or assume the operator runs it and tuxlink just dials the
  TCP ports. **Design fork — see §7.**
- `--hostcommands "MYCALL …;DRIVELEVEL …"` applies startup commands; tuxlink (as
  host) would set MYCALL itself over the control socket anyway.

## 4. Audio (the #1 practical gotcha)

- ARDOP uses a **12 kHz sample rate that most sound cards do NOT support
  natively.** You must let ALSA resample: use **`plughw:N,M`** (not `hw:N,M`),
  or define a `pcm.ARDOP` resampling device in `~/.asoundrc`, or use `pulse`.
- The radio interface is a **USB audio device** (Digirig / SignaLink / CM108).
  ardopcf prints the ALSA device list on startup (run with no args to enumerate
  cards → pick the capture/playback `plughw:` numbers).
- On a Pi, the onboard **HDMI audio throws `Error -524`** (same class as the
  headless Dire Wolf 524 from the gully handoff) — ignore it; use the USB card.

### Virtual / sound-server audio (does it help? mostly no)

- **Virtual audio devices do NOT solve the RF conflict.** The constraint is one
  *physical radio* on one *physical audio interface*; a virtual device cannot
  conjure a second radio link. tuxlink's modem arbitration is still required.
- **For ARQ, talk ALSA `plughw:` DIRECTLY to the radio's USB codec.** ARDOP is a
  high-duty-cycle, timing-sensitive ARQ protocol; routing it through a sound
  server (PipeWire/PulseAudio) adds latency/jitter/auto-processing risk. ardopcf
  is ALSA-first; its `pulse` support is "works but author hasn't fully explored
  it." **Pi gotcha:** Bookworm defaults to **PipeWire**, which can grab/manage the
  USB card — the robust path keeps the modem on direct ALSA `plughw:` and may
  require preventing PipeWire from claiming the device.
- `plughw:N,M` is itself the lightweight "virtual" layer you DO want — it
  resamples the card to ARDOP's 12 kHz over the real hardware. That is the
  sanctioned use of an ALSA plugin here.
- A **true** virtual device (ALSA `snd-aloop` / PipeWire null sink) is only
  useful for **no-RF loopback testing** of the host-protocol wiring (spawn
  ardopcf, loop its audio). Per the operator's RF-validation stance, that proves
  plumbing only and is near-pointless for the actual on-air ARQ question — dev
  convenience, not validation.
- Radios with a **built-in USB codec** (Xiegu G90 via Digirig, IC-7300, …) need
  no virtual device — the codec *is* the sound card.

## 5. PTT / CAT — the central design fork

Two mutually-exclusive ownership models (**never let both drive the same serial
device**):

| Model | How | Notes |
|---|---|---|
| **Modem keys PTT** | ardopcf `-p /dev/ttyUSBn` (RTS, simplest) or `-c … --keystring/--unkeystring HEX` (CAT) | Author's default; CM108/GPIO "may work" but unverified by author |
| **Host keys PTT+CAT** | host (Pat) does PTT via Hamlib; set `ptt_ctrl: true` + `rig:` in the host's ardop config; modem does audio only | Lets the host also set dial frequency via CAT |

- **Xiegu G90 (operator's radio) is documented in the ardopcf manual:**
  RTS `-p /dev/ttyUSB1` works and is simplest; CAT alternative
  `-c /dev/ttyUSB1 --keystring FEFE88E01C0001FD --unkeystring FEFE88E01C0000FD`.
  G90 also needs **per-band audio level** tuning (its power setting engages ALC
  at lower audio).
- Practitioner consensus (The Modern Ham): use a **real PTT line, not VOX** —
  VOX is unreliable for ARQ (may not key/unkey fast enough).

## 6. Connection model & on-air config

- Pat dials `ardop:///CALLSIGN?freq=NNNN` — target callsign + optional dial freq
  (for rig control). tuxlink's equivalent: set MYCALL + ARQ bandwidth on the
  control socket, command the ARQ connect to the target, stream B2F on the data
  socket.
- **ARQ bandwidths: 200 / 500 / 1000 / 2000 Hz.** RMS list reports e.g.
  "ARDOP 2000". Bandwidth must match/agree with the gateway.
- Channel selection is propagation- + grid-square-driven (HF): the operator
  picks a frequency/gateway with a decent path. **Dial vs center freq** matters
  (rmslist reports both).
- Inbound P2P needs a listen mode (`pat --listen=ardop`).

## 7. Deployment interlocks & open questions for the PLAN

- **One sound card = one modem at a time.** KM4ACK's launch script *refuses to
  start ARDOP if Dire Wolf is running* ("Stop all modems and try again"), and
  the pat-users list confirms ARDOP + Dire Wolf contend for a single sound card.
  → tuxlink's transport selector should treat **ARDOP (HF) and packet/Dire Wolf
  (VHF) as mutually exclusive** when they share an audio device.
- ~~Does tuxlink launch+manage `ardopcf`, or assume the operator runs it?~~
  **RESOLVED: tuxlink launches + owns the modem lifecycle** (managed-spawn). The
  swap invariant: on band switch, send the running modem **SIGINT** (ardopcf
  ignores SIGHUP; SIGINT is its clean stop), **confirm the process exited and the
  audio device is released**, *then* spawn the other modem. tuxlink consequently
  owns the audio-device-selection + drive-level + port config surface (the
  single-pane payoff). Bundling ardopcf (and direwolf) as **Tauri sidecars**
  (`src-tauri/sidecars/` already exists) gives the "just works" install story —
  confirm per-arch triples (aarch64 Pi / x86_64 desktop) + the ardopcf license.
- **PTT + frequency is now THE primary fork** (managed-spawn settled #1). ardopcf
  does PTT only — *no general CAT* (it cannot set frequency). And two processes
  must not open the same serial device. So:
  - **MVP:** tuxlink spawns ardopcf with `-p /dev/ttyUSBn` (modem keys PTT via
    RTS — G90-proven, simplest); operator sets the HF dial frequency manually.
    Minimal code, no rig-control dependency.
  - **Full single-pane:** tuxlink owns Hamlib → sets the HF dial frequency *and*
    keys PTT; ardopcf runs audio-only (no `-p`/`-c`). More "magical," but pulls
    in a rig-control component, and PTT *must* move to tuxlink (can't split
    PTT-on-modem / CAT-on-tuxlink across the *same* serial device).
  - (A radio exposing two serial ports could split PTT/CAT across them; a
    single-cable interface like Digirig cannot — tuxlink would own that one port.)
- **Audio-level UX:** ARDOP needs TX drive-level + RX-level tuning (ALC/AGC).
  ardopcf's own WebGui (:8514) already does this well — tuxlink could simply
  point the operator at it rather than rebuild level meters.
- B2F-over-data-socket: tuxlink's existing B2F session layer wants a reliable
  byte stream, which the ARDOP data socket provides — slots in beside
  telnet/packet transports.

## Forward-looking: rig control is SHARED FOUNDATION, not ARDOP-specific cost

Confirmed 2026-05-24: tuxlink has **no rig control today** (no Hamlib/rigctl/PTT;
only `serialport` for KISS bytes — the KISS TNC/UV-Pro keys its own PTT). So a
rig-control layer is greenfield. Key strategic point (operator, this session):

- The **clean-sheet first-party modem (v0.5+) forces rig control regardless** of
  ARDOP. Unlike the ARDOP MVP — which can offload PTT to the external `ardopcf`
  (`-p` RTS) — the first-party modem *is* tuxlink's code, so **tuxlink must key
  PTT itself** (non-negotiable to transmit), plus freq/mode CAT for single-pane
  channel selection + any frequency agility. Same PTT+freq+mode core as ARDOP
  "full." Holds under the current posture (drives a conventional SSB rig; G90 =
  primary target). Caveat: a direct-SDR-transceiver path would NOT use Hamlib CAT.
- **Move:** build ONE tuxlink rig-control abstraction (`Ptt/SetFreq/SetMode/
  ReadStatus`) with **Hamlib as the first backend**. ARDOP full-single-pane =
  first consumer / proving ground; the first-party modem inherits it. Keep it
  tuxlink's own interface (NOT Hamlib welded into the modem) so the modem can add
  SDR/custom/tight-timing control later.
- **Architectural through-line:** tuxlink is becoming a manager of external RF
  helper processes — ardopcf, Dire Wolf, and plausibly **`rigctld`** all fit the
  same spawn/supervise/SIGINT mold. Running Hamlib as a managed `rigctld`
  subprocess (vs `libhamlib` FFI) reuses that machinery + keeps the C dep at
  arm's length. (Implementation fork for later.)
- **Sequencing:** not either/or, and the Hamlib work is never wasted. Ship ARDOP
  MVP (modem-RTS-PTT + manual freq) fast, THEN land rig-control (upgrades ARDOP
  to full AND seeds the modem); OR build rig-control first if the modem is
  near-term. The only real cost of "full" is *when* we stand up a component
  needed anyway.
- Caveat: modem is v0.5+/unspecced — "needs rig control" is a strong posture
  prior, confirm at modem-spec time; its needs may exceed Hamlib's scope.

### Open strategic Q (deferred, surfaced 2026-05-24): spin the modem off as a standalone TCP modem?

If the clean-sheet modem ships as a **separate open-source TCP-daemon** (usable by
non-tuxlink clients — Pat/ARIM/etc., like ardopcf/VARA), it **revises the rig-control
home above**:

- To anyone, tuxmodem becomes **just another external TCP modem** — same shape as
  ardopcf/Dire Wolf/VARA. tuxlink-the-client collapses to ONE abstraction: "drive a
  modem over its TCP host protocol + manage its process." ARDOP is the proving ground
  for exactly that pattern. *Unifying, not complicating.*
- **Rig control relocates INTO the modem** (a standalone modem must do its own PTT),
  but the client still needs freq-setting for modems that punt CAT (ardopcf). Resolve
  by factoring rig control as a **shared crate** (`tux-rig`: trait + Hamlib backend)
  consumed by both the modem daemon and the client — "build once" survives the split.
- The modem's **TCP host protocol becomes a public, versioned API**. Fork: adopt/echo
  an existing host protocol (Pat/ARIM work unmodified — adoption lever) vs. clean-new
  (purer, slower uptake). **Clean-sheet rule almost certainly applies to the ON-AIR
  protocol only, NOT this host-side control API** (it's a local API, free to be
  interop-friendly) — but make that line explicit; it's easy to blur.
- **Optionality is cheap to preserve NOW:** (1) build the ARDOP transport as a generic
  "external TCP modem" client (not ARDOP-special-cased); (2) factor rig control as its
  own crate. Do both and spin-off-vs-monolith stays a packaging decision, not a rewrite.
- Costs: a public host protocol = maintenance/versioning/back-compat commitment; two
  repos. Decide the host-protocol/clean-sheet line (above) BEFORE the modem spec.
- **This belongs in the eventual modem ADR/spec, not just here** — recorded so it shapes
  that spec rather than evaporating.

## 8. Reliability notes (amateur-radio sources are unreliable — what's verified)

- **Verified across ≥2 independent sources:** local-daemon-over-TCP model;
  ports 8515 (cmd) / 8516 (data) / 8514 (webgui); ardopcf = current maintained;
  12 kHz → `plughw:` resampling requirement; PTT modem-vs-host fork; one-soundcard
  mutual exclusion with Dire Wolf; ARQ bandwidths 200/500/1000/2000.
- **Confirm at implementation time from primary source** (`ardopcf` `docs/` +
  `Host_Interface_Commands.md`, not recall): the exact current command set and
  state-event grammar on the control socket — these drift across versions.
- **Source divergence flagged:** always-on/systemd (Pat) vs per-session start
  (ardopcf author). Lean per-session.

## Sources

- ardopcf Linux usage (primary): https://github.com/pflarue/ardop/blob/master/docs/USAGE_linux.md
- ardopcf repo / releases: https://github.com/pflarue/ardop
- Pat ARDOP wiki: https://github.com/la5nta/pat/wiki/ARDOP
- The Modern Ham — Winlink with ARDOP (Billy Penley): https://themodernham.com/winlink-with-ardop-e-mail-over-ham-radio/
- KM4ACK patmenu `start-pat-ardop` (real launch script): https://github.com/km4ack/patmenu/blob/master/start-pat-ardop
- John Wiseman — Running ARDOPC: https://www.cantab.net/users/john.wiseman/Documents/ARDOPC.html
- Winlink — ARDOP overview: https://winlink.org/content/ardop_overview
- On-disk ground truth: `dev/scratch/ax25-prior-art/wl2k-go/transport/ardop/`
