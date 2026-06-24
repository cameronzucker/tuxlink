# Two-radio ARDOP full-session SUCCESS — definitive working recipe

**Date:** 2026-06-23 (~22:20 local / 04:20Z 2026-06-24) · **Agent:** condor-butte-wren · **Operator:** N7CPZ (supervising; RADIO-1 suspended; dummy-load bench)

## HEADLINE — what was proven

A **complete ARDOP ARQ session ran end-to-end between two radios** under our control,
over real RF (both into dummy loads, 10 W, 80m... actually 7.108 MHz / 40m):

```
22:19:50  G90 (caller) ARQCALL N7CPZ-7
22:19:54  CONNECTED  N7CPZ ↔ N7CPZ-7  @ 500 Hz
            FT-710 decoded ConReq  Quality 91
            G90    decoded ConAck  Quality 86
22:20:07  PAYLOAD RECEIVED by answerer:  'TUXLINK TWO-RADIO ARDOP TEST'   (G90 → FT-710)
22:20:15  REVERSE reply received by caller:  'ACK FROM ANSWERER'          (FT-710 → G90)
22:20:21  clean DISCONNECT, both ends → DISCONNECTED
            session exit code 0
            Received-frame quality:  Avg 4FSK 83 (6 frames),  4PSK 57 (2 frames)
```

**Connect → bidirectional traffic → graceful teardown.** The FT-710 transmitted (ConAck +
DataACK/NAK turnarounds, 8 keyings) via the **close-serial CAT bridge with ZERO codec
crashes**. This is the "whole tamale" the operator asked for: a local dry-run that proves
the entire ARDOP transport pipeline before the real on-air CMS attempt (Field Day).

This **supersedes the pessimistic "Bottom line" in `ft710_conjunction_findings.md`** which
declared "ARDOP ARQ cannot run on the FT-710's single USB cable." That conclusion was
wrong — see "Why the earlier verdict was wrong" below.

---

## THE WORKING RECIPE (reproduce this)

### Hardware / device map (auto-detect by USB PID; survives ALSA/ttyUSB renumbering)
| Role | Radio | ALSA card | USB PID | PTT | ardopcf host port |
|---|---|---|---|---|---|
| CALLER | G90 + DigiRig | `Device` → `plughw:CARD=Device,DEV=0` | `0d8c:0012` | **RTS** on `/dev/ttyUSB2` (CP2102N) | 8515 (data 8516) |
| ANSWERER | FT-710 (internal C-Media) | `Device_1` → `plughw:CARD=Device_1,DEV=0` | `0d8c:0013` | **close-serial CAT bridge** on `/dev/ttyUSB0` (CP2105 if00) | 8525 (data 8526) |

Map card↔radio with `cat /proc/asound/card*/usbid` (DigiRig=0d8c:0012, FT-710=0d8c:0013).
Each ardopcf also wants a WebGUI port (default 8514) → pass **`-G 0`** to both so they don't collide.

### Radio settings (BOTH radios — must match)
- **Frequency: identical** (here 7.108 MHz). A mismatch is silent death: the receiver's
  S-meter stays at 0 and ardopcf never decodes — looks exactly like a dead audio path.
- **Mode: USB-data** — G90 `PKTUSB`, FT-710 `DATA-U` (CAT `MD0C`).
- **Power: 10 W** into dummy loads. Minimum power (G90 ~1 W) did NOT couple enough across
  the bench; the receiving S-meter read 0. 10 W gave clean decodes (Quality 80s).
- Verify over CAT before trusting it:
  - FT-710: `IF;` → `IF00000` + 11-digit Hz + ... ; `MD0;` → `MD0C;` (DATA-U).
  - G90 (hamlib model 3088): `rigctl -m 3088 -r /dev/ttyUSB2 -s 19200 f` and `... m`.

### The FT-710 PTT — close-serial CAT bridge (THE load-bearing detail)
The FT-710's CP2105 CAT serial and C-Media audio share one internal full-speed USB hub.
**Holding the serial open during audio resets the codec** (`No such device` → ardopcf
`snd_pcm_avail: Assertion 'pcm' failed` → crash). So RTS PTT (`-p`, which holds the tty
open) is FATAL for the FT-710. The fix is to key by **momentarily** opening the serial,
writing the CAT keystring, and closing it — the close-serial CAT bridge:

`catptt_bridge.py` (in this dir): listens on TCP 4532; on each keystring it opens
`/dev/ttyUSB0 @38400`, writes `TX1;`/`TX0;`, sleeps 70 ms, closes. Serial is shut while
audio streams. ardopcf connects to it with `-c TCP:4532 -k <hex TX1;> -u <hex TX0;>`.
- `TX1;` = hex `5458313B`  ·  `TX0;` = hex `5458303B`

The G90+DigiRig has no such contention (separate USB devices) → plain **RTS** (`-p /dev/ttyUSB2`).

### Exact commands (verbatim — these ran and connected)
```bash
# 1) FT-710 CAT bridge (background)
python3 catptt_bridge.py >catbridge.log 2>&1 &      # TCP:4532 → /dev/ttyUSB0, close-serial

# 2) CALLER = G90 (RTS PTT)
ardopcf -G 0 -p /dev/ttyUSB2 \
  -H "PROTOCOLMODE ARQ;DRIVELEVEL 70;MYCALL N7CPZ;ARQBW 500MAX" \
  8515 plughw:CARD=Device,DEV=0 plughw:CARD=Device,DEV=0 >ardopcf_caller.log 2>&1 &

# 3) ANSWERER = FT-710 (CAT bridge PTT)
ardopcf -G 0 -c TCP:4532 -k 5458313B -u 5458303B \
  -H "PROTOCOLMODE ARQ;DRIVELEVEL 70;MYCALL N7CPZ-7;ARQBW 500MAX" \
  8525 plughw:CARD=Device_1,DEV=0 plughw:CARD=Device_1,DEV=0 >ardopcf_answerer.log 2>&1 &

# 4) Drive the full session (connect → payload both ways → disconnect)
python3 two_radio_session.py \
  --caller-port 8515 --caller-call N7CPZ \
  --answerer-port 8525 --answerer-call N7CPZ-7 --arqbw 500MAX
```
Or just: `AUTO=1 DRIVELEVEL_CALLER=70 DRIVELEVEL_ANSWERER=70 ./two_radio_launch.sh`
(auto-detects devices, starts the bridge + both ardopcf, runs the session, force-unkeys both).

### Harness files (all in `dev/scratch/ft710-ardop-bringup/`)
- `two_radio_launch.sh` — primary launcher (G90 caller / FT-710 answerer). Modes: default
  (full session), `--rxo` (Rung-0 receive-only precheck), `--dryrun`. `AUTO=1` skips the
  warn-before-key prompt for unattended dummy-load runs.
- `two_radio_swapped.sh` — roles reversed (FT-710 caller / G90 answerer).
- `two_radio_session.py` — the session orchestrator. Connects to both ardopcf cmd+data
  sockets (with connection RETRY — ardopcf can take >2.5 s to bind its host port), sets
  ARQ/MYCALL/ARQBW, LISTEN on the answerer, ARQCALL on the caller, waits for `CONNECTED`,
  pushes a payload (2-byte big-endian length prefix + bytes on the data socket), confirms
  receipt, sends a reverse reply, DISCONNECTs, and ALWAYS disconnect+abort in `finally`.
- `catptt_bridge.py` — the FT-710 close-serial CAT PTT bridge (TCP 4532 → ttyUSB0).

### Safety (non-negotiable — learned the hard way today)
- **Never SIGKILL / TaskStop a process while it is keying.** It orphans the transmitter
  keyed. The launchers force-unkey in a trap: G90 = RTS low on ttyUSB2; FT-710 = send
  `TX0;` via CAT on ttyUSB0. The DigiRig RTS idles HIGH on close → must be driven low.
- `pkill -x ardopcf` (exact name) + `fuser -k <port>/tcp` to reap — NOT `pkill -f ardopcf`
  (matches the controlling shell's own cmdline and self-kills). For the bridge use
  `pkill -f "[c]atptt"` (bracket trick) or `fuser -k 4532/tcp`.

---

## THE DIAGNOSTIC JOURNEY (so nobody re-debugs these)

Every blocker hit today, and its resolution — in order:

1. **RTS PTT crashes the FT-710 (`ardopcf -p /dev/ttyUSB0`).** Holding the tty open resets
   the C-Media codec → `No such device` → `snd_pcm_avail: Assertion 'pcm' failed` → SIGABRT,
   transmitter potentially left keyed. **FIX: never RTS on the FT-710 single cable. Use the
   close-serial CAT bridge.** (This was the morning's whole saga; the bridge was already in
   our findings as test #1 — keys cleanly, codec reset = 0.)

2. **"Broken pipe" underruns at every turnaround are BENIGN, not fatal.** ardopcf source
   (`src/linux/ALSA.c:1380`, logged at VERBOSE): *"For some sound devices… this occurs at
   the start of each transmission."* `SoundFlush()` never `snd_pcm_drain()`s, so the
   playback ring underruns between frames; the next frame's first `snd_pcm_writei` hits
   `-EPIPE` → `snd_pcm_recover()` re-prepares it and `PackSamplesAndSend` **retries the same
   samples** — full frame goes out intact. The earlier verdict mistook this recovered log
   noise for the failure. It is not.

3. **Minimum power into dummy loads doesn't couple.** At G90 min (~1 W) the FT-710's
   receiver S-meter read `SM0000` even during the G90's full-drive two-tone. **FIX: 10 W
   both** → clean decodes. (Diagnosed over CAT via `SM0;`, not by guessing.)

4. **Wrong freq + mode on the FT-710 looks identical to a dead audio path.** With the FT-710
   off-frequency, S-meter ~0, no decode — easy to misread as "USB RX audio not routed"
   (it WAS routed; operator confirmed; CAT `MD0C` confirmed DATA-U). **FIX: match freq AND
   mode; verify over CAT; beacon the G90 (repeated `TWOTONETEST`) while tuning the FT-710.**

5. **Harness bug: `ConnectionRefusedError` connecting to ardopcf.** ardopcf needs several
   seconds to bind its host port after launch (audio + CAT init); the 2.5 s wait raced it.
   **FIX: `_connect_retry()` in `two_radio_session.py` retries for 15 s.**

### Symptom → cause quick map (for the next session)
- S-meter `SM0000` during the other radio's TX → no RF reaching the receiver: **wrong freq,
  or power too low**. Not an audio/USB problem.
- ardopcf assertion crash / `No such device` on the FT-710 at key-up → **RTS keying; switch
  to the CAT bridge**.
- `STATUS CONNECT TO X FAILED` with bridge KEY count 0 + no `ConReq` decode on the answerer
  → the answerer never heard the caller: **freq/mode/coupling**, work up from Rung 0/S-meter.
- `Broken pipe` in the logs → **ignore**, it's recovered.

---

## Why the earlier verdict ("single cable not viable") was wrong
`ft710_conjunction_findings.md` concluded ARQ couldn't run on the FT-710 single cable
because the close-serial bridge showed `broken_pipe` on turnaround frames. But (a) those
underruns are benign/recovered (item 2 above), and (b) no full connect had ever been
attempted — the test target (K7HTZ) never answered, so "underruns on turnarounds" was
assumed fatal without proof. Today the FT-710 ran a **complete ARQ session** (answerer
role: decoded ConReq Q91, sent ConAck Q86, exchanged DataACK/NAK over 8 keyings, clean
disconnect) on that exact single cable via the bridge. The single cable **is viable for
ARDOP** with close-serial CAT keying; what it cannot do is RTS/held-open keying.

(The hardware fixes A/B in the old doc — separate soundcard / CAT-3 jack — remain valid
*alternatives*, not requirements. The bridge makes the stock single cable work.)

---

## What this buys, and what it does NOT prove
**Proven:** the full ARDOP transport mechanics on this hardware — PTT timing, RX↔TX
turnaround, drive, ARQ reverse-link ACKs, data integrity both directions, clean teardown.
The morning's on-air failures were a non-answering station + uncalibrated drive, NOT the
modem.

**NOT yet proven (next rungs of the ladder):**
- **B2F / Winlink layer.** This was raw ardopcf ARQ, not a Winlink message exchange. Next:
  Pat ↔ Pat over these two ardopcf instances (Pat v1.0.0 is installed), then tuxlink (its
  ARDOP listener/answerer exists — `winlink_backend.rs:2841`, `winlink/listener/`).
- **Real CMS.** Prod rejects unregistered client SIDs; `bd-tuxlink-lu7t` = obtain the
  Tuxlink Winlink access key; dev uses `cms-z.winlink.org`.
- **Longer/larger transfers** and sustained many-turnaround sessions (this was one small
  payload each way).

## Next-session starting point
1. Re-confirm both radios: same freq, USB-data, 10 W, dummy loads. Verify over CAT.
2. `AUTO=1 DRIVELEVEL_CALLER=70 DRIVELEVEL_ANSWERER=70 ./two_radio_launch.sh` → expect a
   clean connect+payload+disconnect (exit 0).
3. Then climb to the B2F rung: Pat ↔ Pat over the two ardopcf, then tuxlink ↔ tuxlink/Pat.
