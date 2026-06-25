# Handoff — FT-710 codec defect RESOLVED (DRA-100 bypass) · VARA TX blocked under box64 · NEXT: ARDOP/VARA on-air

**Agent:** peregrine-moss-hawk · **Date:** 2026-06-25 · **Host:** pandora · **Branch:** `bd-tuxlink-xygm/recover-handoffs`

## TL;DR
Started on VARA-on-Pi-emulation PTT. Two big outcomes: (1) **VARA's transmit path is blocked under box64** (RX/DSP fine, rejects every TX command), and (2) root-caused the FT-710's "won't do digital" to a **defective internal USB codec** and **RESOLVED it** with an external USB sound card (the **DRA-100**) on the rear DATA jack — radio transmits clean again, for $0, with gear the operator already owned.

**Next session (operator's ask): validate VARA AND ARDOP on the air, and find out why we've never raised a single station on ARDOP — operator suspects a VERSION issue.**

## Read these FIRST — do NOT re-derive
1. **FT-710 codec is a defective unit, but RESOLVED** → memory `project_ft710_usb_audio_rfi_reset_on_tx`. Internal C-Media codec fails on keyed-TX-with-audio (serial-open + audio contention), cross-platform (Windows + Linux), not RF/cable/Pi. Fix = external USB sound card on the rear 6-pin DATA jack + `MOD SOURCE=REAR` + CAT over the 710 USB. **Do not re-debug the internal codec.**
2. **VARA TX is blocked under box64** → memory `project_vara_tx_blocked_under_box64`. VARA receives/registers fine but rejects ALL transmit commands (`CONNECT`/`CQFRAME` → `WRONG`), hardware-independent, confirmed with the reference Pat client. **Pi-box64 VARA cannot transmit.** On-air VARA must run on a real x86/Windows host (the laptop), with tuxlink talking to it over TCP.
3. **ARDOP (native ARM `ardopcf`) CAN run + TX on the Pi** (proven 2026-06-23 two-radio success). This is the on-air-testable mode on the Pi itself.

## Current machine state
- **FT-710**: on DUMMY LOAD, RX-idle. Working digital base = **DRA-100 (ALSA `hw:3`, C-Media `0d8c:013a`, usb 3-2) for audio** + **CAT over `ttyUSB0` (CP2105) for control/PTT**. `MOD SOURCE=REAR`. The 710's internal codec (`hw:4`, `0d8c:0013`) is idle/unused.
- **DRA-100** (MastersCommunications DRA-100-DIN6): proven this session — RX capture clean (peak ~2751), keyed tone played to it → 710 MOD-REAR → TX into dummy load with `aplay rc=0`, **no `File descriptor in bad state`, no codec reset**. Its OWN GPIO PTT (hidraw0 / CM119A) did **not** key the 710 (tried GPIO 1-8) — likely the DIN cable lacks a PTT conductor or the DRA is VOX-configured. **Use CAT PTT** (works).
- **PTT**: CAT close-serial (`TX1;`/`TX0;`) proven. NOTE: with audio now off the internal codec, the contention class is gone — held CAT / RTS PTT should also work now (untested, expected).
- **VARA**: staged `~/vara-box64-work` (box64 + wow64 wine, licensed N7CPZ). Relaunch recipe in `docs/vara-hf-on-raspberry-pi-5-via-box64-emulation.md`. RX works, TX blocked (box64).
- **ardopcf**: native `/usr/local/bin/ardopcf` (develop build, commit cb2c4c1). Tooling in `dev/scratch/ft710-ardop-bringup/` — `catptt_bridge.py` (close-serial CAT PTT, TCP:4532), `ardop_call.py`, the 2026-06-23 two-radio recipe.
- **G90+DigiRig**: other proven radio (ttyUSB2 + own C-Media). Operator's portable unit (DigiRig stays with it).
- **pipewire**: I freed the USB-codec + Loopback ACP profiles (`wpctl set-profile <id> 0`) for clean exclusive audio during VARA tests. To restore normal system audio routing later: `wpctl set-profile <id> <on-index>`.
- **Working tree**: 73 uncommitted files (mostly prior untracked handoffs in `dev/handoffs/` — a cleanup item; the branch name "recover-handoffs" implies that's the intended work — plus `M README.md`). This handoff commits only itself.

## NEXT SESSION FOCUS
### A. ARDOP on-air — why has it never raised a station? (operator suspects version)
Test in this order:
1. **Retest now that audio is clean.** The 2026-06-23 ARDOP attempts had ALSA `Broken pipe` underruns on the 710's internal codec that corrupted ConReq frames → no connect. The DRA-100 bypass should kill those underruns. **First test: `ardopcf` on FT-710 + DRA-100 (hw:3) + CAT PTT → ARDOP connect to a real RMS gateway → does it raise a station now?** This may simply work.
2. **Version/protocol compat** (operator's hypothesis): bundled `ardopcf` cb2c4c1 vs the ARDOP version the target gateways run (ARDOP 2.0.x). The two-radio success was ardopcf↔ardopcf (same version); raising a real gateway needs gateway-protocol compat. Confirm target-gateway ARDOP versions + whether cb2c4c1 interoperates; consider testing against a known ARDOP 2.0.3.2.1 station.
3. Gateway/propagation: Find-a-Station + strong-prop gateway, clear freq. Real on-air (antenna) is OPERATOR-RUN.
- Related bd: `tuxlink-5xxq` (ARDOP failed-handshake self-terminate), `tuxlink-wu0k` (FT-710 CAT-PTT/contention — **now resolved via DRA-100**), new issue filed (see below).

### B. VARA on-air
- Pi-box64 VARA can't TX. So VARA on-air = VARA on the **Windows laptop** (reinstall — it was bricked during the FT-710 codec attempts) + a radio (G90+DigiRig, or move the DRA-100 to the Windows box). tuxlink → VARA over TCP.
- Related bd: `tuxlink-3ij0v` (VARA dial gaps), `tuxlink-p6iq` (Find-a-Station VARA flow).

## Safety
RADIO-1: on-air TX under the callsign is operator-run, per-invocation consent. Agent makes the test runnable + observable; the operator keys. Dummy-load FT-710 keying this session was authorized + supervised.

## Durable records updated this session
- memory `project_ft710_usb_audio_rfi_reset_on_tx` (defect + DRA-100 resolution), `project_vara_tx_blocked_under_box64` (TX-block).
- `dev/scratch/vara-box64/FINDINGS-vara-on-arm-emulation.md` (local/gitignored — full execution log: the all-TX-commands-WRONG isolation, Pat reference confirm, RMS-Express-under-box64).
- bd epic `tuxlink-u3m0g` (PSKReporter-trued propagation engine + 5 children) — future feature, NOT next-session.
- Design doc `~/.gstack/projects/cameronzucker-tuxlink/administrator-bd-tuxlink-xygm-recover-handoffs-design-20260625-pskreporter-prop-truing.md`.
