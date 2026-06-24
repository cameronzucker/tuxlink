# Handoff — VARA HF on the Pi under emulation: PTT is the last mile (morning pickup)

**Agent:** oak-magpie-savanna · **Date:** 2026-06-24 (~03:15 local) · **For:** next session, daytime
**Operator plan:** "I'll connect you to the FT-710 on a **dummy load** and you can try permutations until VARA is firing. We are close. This can be done."

## TL;DR — what's done and what's left
**DONE (proven + committed):** VARA HF v4.9.0 runs on a Raspberry Pi 5 under emulation (box64 + wow64
wine), stable, TCP interface live, **licensed**; its real-time **receive DSP keeps up with ~3×+
headroom** (~25–34% of one core). The **Pat → VARA software chain works end-to-end**: Pat connects to
VARA, VARA goes green, accepts the dial command, and sits in Listen ready to transmit.

**THE ONE BLOCKER:** VARA **never keys the radio** (stuck in Listen). It's NOT the codec bug (operator
confirmed — totally different behavior; the radio is never keyed at all). It's that **VARA's PTT can't
work through wine's serial emulation.**

**THE FIX (researched, high-confidence) — let PAT do PTT via Hamlib, not VARA.** Confirmed by the
dl1gkk "Winlink on Raspberry Pi 5 with VARA & Pat" guide: *"serial ports managed entirely through
Hamlib/rigctld rather than Wine's serial emulation."* So:
```jsonc
// ~/.config/pat/config.json
"varahf": { "addr": "localhost:8300", "bandwidth": 500, "rig": "<FT710-rig-name>", "ptt_ctrl": true }
```
```bash
# rigctld keys the radio; Pat watches VARA's PTT request and drives this
rigctld -m <FT710_hamlib_model> -r /dev/ttyUSB0 -s 38400 --set-conf=dtr_state=OFF,rts_state=OFF &
```
Pat's `rig` + `ptt_ctrl:true` makes Pat key the rig via rigctld when VARA asks to transmit. VARA's own
wine-PTT gap stops mattering.

## ★ UPDATE (same night, ~04:00) — PTT SOLVED; VARA's flaky CONNECT is the real wall
Ran the whole chain on the dummy load. Three corrections to the plan above:

1. **`rigctld`'s own PTT does NOT key the FT-710.** `rigctl T 1` returns success (`RPRT 0`, `t`→1) but
   produces **no RF** — confirmed against the operator watching the radio. (Oddly, rigctld's *open-time*
   line pulse DID key it once — nondeterministic.) The radio keys reliably **only** via the proven
   close-serial `TX1;` CAT command (operator confirmed it fired, twice).
2. **So PTT is solved with a shim, not rigctld:** `dev/scratch/ft710-ardop-bringup/closeserial_rigctl_shim.py`
   — a rigctl-protocol server (port 4532) that answers hamlib's handshake (replays the captured
   `hs_dumpstate.raw`/`hs_chkvfo.raw`/`hs_powerstat.raw` in that dir) but keys via **close-serial `TX1;`/`TX0;`**
   (momentary opens = codec-safe). Verified: a hamlib client + Pat's `\set_ptt 1` both drive a real key
   through it. Pat config already points `hamlib_rigs.FT710 → localhost:4532`. **Start the shim BEFORE Pat;
   do NOT run rigctld (it can't key this radio + holds the port).**
3. **The "yellow Listen state" was a red herring.** Per EA5HVK's command ref, `LISTEN ON` is part of the
   *normal* connect sequence (`MYCALL → LISTEN ON → CONNECT → CONNECTED`) — it does NOT block transmit.

**The actual wall: VARA's `CONNECT` is flaky.** `CONNECT N7CPZ N0DAJ` (the documented form) returned, across
runs: `WRONG` (twice), then silent-accept-but-no-`PTT ON`, then total silence (deaf even to `BW2300`). VARA
emits PTT via `PTT ON`/`PTT OFF` on 8300 — we **never** got a single `PTT ON`, so VARA never tried to key.
Operator's read (trust it): **VARA is just unstable in general, on Windows too** — not an emulation artifact.
Registration is **async** (`REGISTERED N7CPZ` lands seconds after `MYCALL`); wait for it before `CONNECT`.

**Morning path (PTT is done; only VARA's flake remains):** fresh single VARA → wait for `REGISTERED` →
`BW2300` → start shim → `pat connect varahf:///<target>` (or drive 8300 directly), and **retry the connect
until VARA emits `PTT ON`**. The instant it does, the shim keys the radio (proven). Consider also: a 2nd
VARA instance on 8400 as a known-good P2P target to prove a full ARQ without depending on a live gateway.

## Morning permutation plan (FT-710 on a DUMMY LOAD — clear channel so the busy detector lets it key)
1. **Find the FT-710 hamlib model** (`rigctl --list | grep -i 710` / FTDX10 family; the FT-710 may be a
   recent model number). Start `rigctld -m <model> -r /dev/ttyUSB0 -s 38400 --set-conf=dtr_state=OFF,rts_state=OFF`.
2. Add `rig` + `ptt_ctrl:true` to Pat's `varahf` block (a Pat `hamlib_rigs` entry pointing at the rigctld
   may also be needed — check `pat configure`/the rig field semantics).
3. Relaunch VARA clean (ONE instance — see "gotchas"), Pat connect `varahf:///<target>` on a **clear**
   freq, watch the FT-710 key.
4. **FT-710 codec caveat:** rigctld holds `ttyUSB0` open → the single-cable C-Media may still reset on
   key. Two outs: (a) route Pat/rigctld PTT through the **close-serial CAT bridge**
   (`dev/scratch/ft710-ardop-bringup/catptt_bridge.py`, proven codec-safe for ardopcf) instead of a
   held-open rigctld; (b) switch to the **G90+DigiRig** (separate audio/CAT, zero contention) — the
   genuinely clean rig. On a dummy load, try (a) first since the operator wants the 710.
5. If VARA *crashes* on launch under wine: dl1gkk's fix —
   `wine reg add "HKCU\Software\Wine\DllOverrides" /v pdh /t REG_SZ /d "" /f` (disables a pdh.dll ARM
   sensor incompat). We didn't hit this, but keep it in pocket.

## Known gotchas (cost us hours tonight — don't repeat)
- **VARA "red TCP" = no host connected.** It's red until a host (Pat) holds 8300/8301. Green ≠ bound,
  it means a client is attached. My transient probe scripts left it red and confused everything.
- **Only ONE VARA instance.** Sloppy relaunches left a **stale wineserver holding 8300/8301** while a new
  VARA couldn't bind → red. Always `wineserver -k` + `pkill -x VARA.exe wineserver`, verify ports free,
  then launch one.
- **Wine audio under box64 is finicky.** `winealsa` ("capture slave is not defined", under/overruns) is
  flaky; VARA intermittently reports `MISSING SOUNDCARD`. It DID work (demodulated a loopback at 27% CPU)
  — but bring-up is fragile. The soundcard is a GUI selection VARA won't auto-make.
- **VARA busy detector has NO GUI toggle / host command.** It won't key a busy channel — use a clear freq
  or dummy load. (This is correct Part-97 behavior.)
- **VARA is half-duplex** → a single instance can't self-test a full ARQ; needs a 2nd instance or a real
  station. VARA's TCP port IS configurable (`fraTCPPorts`) so a 2nd instance on 8400 is possible.
- **`pkill -f` self-matches** the controlling shell — use `pkill -x` / `fuser -k <port>/tcp` / bracket trick.

## Current machine state
- **Env:** Pi 5 (16GB), Debian 13 trixie, kernel `6.18.34-rpi-v8` **4k pages** (`getconf PAGESIZE`=4096 —
  do NOT revert to the 16k 2712 kernel; `kernel=kernel8.img` is set in `/boot/firmware/config.txt`).
- **box64 v0.4.3** built from source, installed to `/usr/local/bin/box64`, and `/usr/bin/box64` symlinked
  to it (binfmt). Apt's 0.3.4 is too old (can't load wow64 kernel32) — keep the source build.
- **Staged in `~/vara-box64-work/`** (deliberately OUTSIDE the VS Code workspace — a runaway VS Code
  ripgrep pegged 3 cores when the wine prefix was inside it):
  - `wine-x86/` = Kron4ek wine-11.11-amd64-wow64 (x86_64, run under box64)
  - `vara.wine/` = the prefix: VARA HF v4.9.0 installed + **licensed (N7CPZ)**, VB6 runtime (`vb6run`),
    OCXs registered (MSWINSCK/MSCOMCTL/MSCHRT20/...), audio driver = alsa, `dosdevices/com1→ttyUSB0`,
    `com2→ttyUSB1`
  - `x86wine.env` = `source` this for WINEPREFIX/WINELOADER/PATH/BOX64_LD_LIBRARY_PATH
  - logs: `vara_*.log`, `pat_n0daj*.log`
- **Pat** `~/.config/pat/config.json` has a `varahf` block (addr localhost:8300, bw 500) — **needs `rig` +
  `ptt_ctrl:true` added** (the morning's change).
- **Relaunch VARA:** `cd ~/vara-box64-work && source x86wine.env && box64 "$WINELOADER" "vara.wine/drive_c/VARA HF/VARA.exe" &`
- **FT-710:** ttyUSB0/1 = CP2105 CAT; C-Media `0d8c:0013` = ALSA card "Device". (G90+DigiRig =
  ttyUSB2/CP2102N + its own C-Media when plugged.)

## Durable docs written this session
- **`docs/vara-hf-on-raspberry-pi-5-via-box64-emulation.md`** (committed/pushed) — the full reproducible
  recipe + symptom→cause map. The publishable artifact.
- `dev/scratch/vara-box64/FINDINGS-vara-on-arm-emulation.md` (local) — the execution log.
- This handoff.

## What this unlocks if PTT lands
**VARA + radio + Pat/tuxlink all on one 8W Pi** — no x86 box, no data island. The DSP already proved it
keeps up. PTT-via-Pat-Hamlib is the last mile.

## Sources
- dl1gkk — Winlink on Raspberry Pi 5 with VARA & Pat: https://dl1gkk.com/winlink-raspberry-pi-5-vara-pat-ipad-guide/
- VARA + Pat Hamlib PTT (pat-users): https://groups.google.com/g/pat-users/c/F8qXAtRL1Aw
- varanny (VARA launcher/PTT helper): https://github.com/islandmagic/varanny
