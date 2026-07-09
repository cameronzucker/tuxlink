# Handoff — 2026-07-09 — `kestrel-butte-granite` — TX chain PROVEN via off-air self-decode; 33 on-air dials unanswered; suspect = channel-list data gaps (hours/bandwidth)

## Headline

**The Tuxlink VARA transmit chain is now proven end-to-end by a second VARA modem
decoding our signal off-air (13/13 ConReqs), and by FT8 CQs spotted >0 dB in Denver,
SLC, and the PNW — yet 33 on-air dial attempts across 11 gateways on BOTH frequency
conventions got zero answers.** Two product fixes landed on PR #1061 (CI green,
mergeable at `acbdcf01`). The operator's verdict: "we're missing something" — the
prime suspect is the channel catalog's data source (no per-channel operating HOURS,
no BANDWIDTH; `tuxlink-hmoz8`).

## What landed on PR #1061 today (both CI-green)

1. `86055ba1` **fix(rig): set_mode passband −1** — `M PKTUSB 0` engaged the Yaesu
   NARROW DSP filter (hamlib "backend default"), an uncommanded RX-width change
   that crushes VARA. Hardware-verified: `SH0`/`NA0` unchanged through a dial.
   (`tuxlink-ntzzk`, operator-observed.)
2. `acbdcf01` **fix(vara): 250 ms PTT tail-hold** — VARA raises `PTT OFF` when it
   finishes *writing* samples; Wine→PipeWire buffering left 65–170 ms (median 127 ms)
   of every frame in flight, so unkeying on the event amputated every ConReq tail
   (measured off-air; operator's scope confirmed both the defect and the fix).

## Root causes found & fixed this session (chronological)

| # | Defect | Fix/Status |
|---|--------|-----------|
| 1 | VARA.ini output name 34 chars; Wine/MME truncates at 31 → `MISSING SOUNDCARD`, every dial insta-rejected (also yesterday's "WRONG CALLSIGN") | Fixed on R2 (31-char name); `tuxlink-y14cb` for provisioning |
| 2 | FT-710 `DATA MOD SOURCE=AUTO` picks mod source by PTT method → CAT keying got NO rear audio (0 W, ALC 0) | Operator set REAR; `tuxlink-h874a` |
| 3 | Narrow DSP via hamlib passband 0 | Fixed (`86055ba1`) |
| 4 | TX overdrive: sink 86% → ALC pegged 135 (splatter). Tone-calibrated ALC-zero at 45%, but VARA's OFDM PAPR ≠ tone: **operative drive = sink 0.70** (ALC clean, ~40 W VARA peaks) | Set on R2 (restore-device persists) |
| 5 | PTT tail amputation | Fixed (`acbdcf01`) |
| 6 | No center→dial −1500 Hz conversion anywhere in Tuxlink (WLE: `VaraSession.cs:3390`; Trimode: "Center frequency is 1500 Hz higher than the USB dial") | `tuxlink-<P0 issue>` filed; A/B-tested on air, did NOT alone produce answers |
| 7 | UI dial: VARA panel needs two clicks (Start≠dial, WLE muscle-memory violation); prefill auto-opens sessions; Send/Receive errors swallowed to console.debug; session-log lines never reach jsonl; IPC-layer rejections invisible | `tuxlink-nvgjy`, `tuxlink-46hof`, `tuxlink-o1e9w`, +session-log-mirror issue — all [fable] P1 |
| 8 | Arm-authority state split across app instances (operator armed one instance, MCP served by another) + arm/disarm flakiness both directions | evidence added to `tuxlink-kw873` |
| 9 | Dial races VARA registration after modem restart (CONNECT 464 ms after MYCALL rejected) | `tuxlink-m9kcd` |

`bd list | grep '\[fable\]'` → 11 issues, all tagged with provenance notes.

## Proven-good (do NOT re-litigate)

- **UI dial path works**: operator's Send/Receive produced full ConReq cycles (the
  morning "UI doesn't work" was the two-click model + swallowed errors + rigctld
  missing in the diagnostic build).
- **TX waveform valid**: VARA2 on the G90/Digirig decoded 13/13 and ANSWERED calls
  addressed to its MYCALL → addressing/encoding correct.
- **Propagation**: FT8 "CQ N7CPZ DM33" spotted >0 dB Denver/SLC/PNW (operator pulled
  PSK Reporter via VPN; the site 503s readily — query ≥5 min apart, once).
- **RX chain**: jt9 decoded 40+ FT8 signals to −24 dB off the DRA.
- **Frequency conventions**: BOTH published and −1500 dialed at 11 gateways.
  23 A/B attempts + 10 earlier dials — silence. Retries tested at 15 and 30/30.

## The open mystery + next experiments (in order)

1. **Channel data gaps (`tuxlink-hmoz8`, top suspect)**: the ingested text listing
   lacks per-channel HOURS (Trimode scans each freq only in its UTC window) and
   BANDWIDTH (500 vs 2300). We may have called scheduled-off or BW-incompatible
   channels all afternoon. Fix: CMS channels API (Channels.dat equivalent, as Pat).
2. **WLE differential test** (operator): dial KD7ZDO from WLE on this same rig
   (DRA + CAT). Connect → diff wire behavior vs Tuxlink; fail → Tuxlink exonerated.
3. **500 Hz axis**: `config_set_vara bandwidth_hz=500` + redial one strong-path
   gateway both conventions (never run).
4. Evening retry: 20m channel windows may open after ~00:00–06:00Z.

## R2 machine state (READ before touching)

- **Running app = DIAGNOSTIC build**, `~/tuxlink-yrrjq-build` debug binary at
  `acbdcf01`-equivalent (both fixes + error-surfacing patches + click/entry probes),
  frontend served by a **Vite dev server on :1420 — if that process dies the app
  white-screens** ("Could not connect to localhost"). Production deb (0.87.0, no
  fixes) is still installed at /usr/bin/tuxlink; restore = kill debug app + launch
  /usr/bin/tuxlink, but you LOSE the passband+tail fixes until PR #1061 merges.
- **App restarts CLEAR the agent-send arm** (operator re-armed ~5×today; kw873).
- **VARA1**: `C:\VARA`, port 8300, `Retries=30` (was 15; WLE uses `RETRIES 10` live).
  **VARA2**: `C:\VARA2`, port 8400, Digirig audio, used as off-air decoder
  (see memory `g90-selfdecode-rig`). Both under `~/.wine-vara`, both running.
- **Wireplumber**: `~/.config/wireplumber/main.lua.d/51-disable-radio-internal-codec.lua`
  now pins by USB path `*usb-0:7.2*` (FT-710 internal codec) — name-matching broke
  the Digirig (same C-Media name). DRA = card 1 ("USB PnP"), duplex profile,
  default sink+source, sink 0.70 (≈ Speaker 70%, ALC-clean VARA drive).
  Digirig = card 4 at usb 3-1 (needed a C→C cable; A→C never enumerated);
  capture gain `Mic 4` + AGC off (default clips 28%).
- **G90**: dummy load, CAT via Digirig CP2102N serial, hamlib model 3088 @19200.
  FT-710: antenna, DATA-U, `DATA MOD SOURCE=REAR`, dial left at 14074.0 (FT8).
- Helper scripts: `/tmp/bruteforce_dial.py` (A/B campaign, 305 s cooldowns),
  `/tmp/ft8_cq.py` (slot-synced FT8 CQ), `~/mcp_call.py`, `~/vara_txmon.py`.
  Radio verified `TX0`, VARA session closed, campaign scripts killed.

## Worktrees / branches

- `worktrees/tuxlink-yrrjq` (PR #1061, `bd-tuxlink-yrrjq/vara-ptt-keying`): head
  `acbdcf01`, CI green, MERGEABLE. This handoff commits on it.
- Main checkout still on stale `bd-tuxlink-ant8s/ardop-connect-fixes` (operator state;
  read origin/main via `git show`). Pending disposals unchanged from prior handoff:
  `worktrees/tuxlink-graylinefix`, `worktrees/verify-087`.

## Antenna discipline (operator-mandated, 110 °F Phoenix)

≥5 min cooldown between dial attempts; ~86 s TX per 30-retry attempt. Batch
campaigns must keep ≤~20 % duty. PSK Reporter: one query per ≥5 min.

Agent: kestrel-butte-granite
