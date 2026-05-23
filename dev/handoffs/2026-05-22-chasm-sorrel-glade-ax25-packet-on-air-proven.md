# Handoff — 2026-05-22 — chasm-sorrel-glade — AX.25 packet: PROVEN on-air, now fix the TX-timing bug

## Headline

**Connected-mode AX.25 packet transmitted on-air over the BTECH UV-Pro on Linux and
got a two-way response** — the question this whole feature hung on is answered: the
KISS/AX.25 transport carries connected-mode over this radio. The dial to gateway
`W7MOT-6` keyed the radio, sent a SABM, and the gateway answered with a **DM**
(refused at the app layer). **Next session's FIRST job:** fix `tuxlink-uhc` (the
double-key / AX.25-params bug) so a single clean SABM goes out, then the operator
rebuilds + retries on-air. The double-key may itself be causing the DM.

## RADIO-1 / on-air gate (read first)

This is live RF. The **agent fixes code + makes it runnable; the OPERATOR keys the
radio.** Never run the dial in the agent shell. Pairing/binding/SDP queries are OS
setup (fine); transmitting is operator-only.

## What completed this session (all pushed)

- **`orj` — live packet status feed** → merged to `main` via PR #115 (ribbon shows
  Listening/Connecting/Connected · Packet 1200, derived from the live `backend_status` poll).
- **libudev CI fix** (`Release build` was failing: `serialport`→`libudev-sys` needs
  `libudev-dev`) → merged via #115.
- **release-please fixed** — root cause was the repo setting *"Allow GitHub Actions to
  create and approve pull requests"* being OFF (`can_approve_pull_request_reviews:false`,
  user-owned repo so no org gate). Operator enabled it; re-run created **PR #118
  "chore(main): release 0.1.0"** (0.1.0 is correct per `bump-minor-pre-major`).
- **`tuxlink-nj1` — clean serial/BT Stop** (AbortableByteLink routes the existing
  `aborting` flag into the serial link) → **PR #119, ready, NOT merged.** 311 lib tests green.
- **`tuxlink-jvp` — BT setup help text** → branch `bd-tuxlink-jvp/uvpro-setup` pushed,
  **NO PR, and the help text is now KNOWN-WRONG** (it hardcodes "rfcomm bind … 1"; the
  RFCOMM slot is dynamic — see findings). Needs revision before any PR; arguably superseded
  by the deferred in-app RFCOMM-socket transport.

## On-air session log (the proof + the open failure)

```
Connecting to W7MOT-6 over packet… → Opening KISS link… → Connecting to W7MOT…
→ Packet connect failed: AX.25 connect: peer refused the connection (DM)
```
Radio **fired** (TX'd the SABM); gateway replied **DM**. Earlier attempts: `Broken pipe`
(wrong RFCOMM slot) — fixed by binding the correct slot. Operator also observed the radio
**double-key in fast succession** and that **AX.25 params aren't respected**.

## Bug backlog (all filed in bd)

- **`tuxlink-uhc` (P2) — FIX FIRST.** Radio double-keys on connect; configured
  T1/TXdelay/persistence/slot not respected. Investigate (do NOT assume): config→`Ax25Params`
  mapping (`PacketConfig::into_params`), `kiss_param` correctness (verified command-nibble
  1–5 = config, not data — so params don't key the radio), and the `connect()` T1 retransmit
  loop in `datalink.rs` (~L84-115: `for _ in 0..=n2_retries { send SABM; wait T1 }`).
  **Hypothesis (unconfirmed):** the double-key may be CAUSING the DM — two SABMs close
  together → gateway sees a garbled/duplicate connect → DM. Fixing → single clean SABM →
  retry → may get UA. This is the agent-doable next step.
- **`tuxlink-sox` (P2)** — packet panel transport segment resets to USB Serial instead of
  persisting Bluetooth (USB + BT both map to `linkKind:'Serial'`, so the panel can't tell
  them apart on reload → defaults to USB). Also: `/dev/rfcomm0` did NOT appear in the BT
  device picker (`packet_list_serial_devices` discovery gap) — operator typed the path.
- **C/R-response bug (P2, filed)** — `frame.rs Path::encode` hardcodes dest C-bit=1/src
  C-bit=0 ("refined in P2" — never done). Correct for COMMANDS (SABM), but RESPONSES
  (UA when answering; RR/RNR/REJ as responses) are mis-framed per v2.2. Independent-decoder-
  visible. **NOT** the SABM-DM cause (SABM is correctly a command).
- **RFCOMM-socket in-app transport (P3, DEFERRED)** — the real BT-UX fix: open an RFCOMM
  **socket** (non-root, confirmed creatable as uid 1000), read the SPP slot from SDP, no
  `rfcomm bind`/root/shell. Deferred by operator until connected-mode B2F is proven (we have
  a DM, not yet a clean message transfer).

## Key findings (durable — also in bd memory `uv-pro-kiss-tnc-transport`)

- UV-Pro KISS-TNC over Bluetooth = **classic RFCOMM/SPP**, not BLE-GATT. Spec §4.1
  `/dev/rfcommN` serial model is correct; no in-app BlueZ needed for the byte-pipe.
- **The RFCOMM slot is DYNAMIC** — observed 4 then 5 on unit `38:D2:00:01:55:5C` (DC6AP's
  VR-N76 was 1). Read it from SDP each time: `sdptool records 38:D2:00:01:55:5C | grep -A1 "SPP Dev"`.
  A hardcoded/stale slot → `Broken pipe`. (Terminology: this is an RFCOMM *slot/service number*
  on the Bluetooth link — NOT an RF channel/frequency.)
- The **audio/headset class is a RED HERRING** — `input.conf Disable=Headset` did NOT
  matter, PipeWire did not grab audio; the `audio-headset` class coexists with SPP (on
  Windows it's an audio device AND a COM port). Pairing's `ConnectionAttemptFailed` was transient (retry fixed it).
- **Encoder verified spec-correct** for the SABM (frame address shift, SSID reserved/ext
  bits, control 0x3F, C/R command bits, KISS port-0 wrap, no-FCS-for-KISS). So the DM is
  app-refusal OR the timing-garble (`uhc`) — not a static frame malformation.

## State

- **Worktree** `worktrees/bd-tuxlink-7fr-ax25-packet` is on branch
  `bd-tuxlink-jvp/uvpro-setup` (clean, pushed). Its 7fr work merged via #115.
- **Branches on origin:** `bd-tuxlink-jvp/uvpro-setup` (jvp, no PR, help-text needs fix),
  `bd-tuxlink-nj1/serial-stop` (PR #119, ready). `main` has orj + libudev + the merged packet feature.
- **PRs open:** #118 (release 0.1.0), #119 (nj1).
- **OS state (operator machine):** UV-Pro `38:D2:00:01:55:5C` paired+trusted; `/dev/rfcomm0`
  bound (slot may be STALE — it rotates; re-read via sdptool before relying on it); radio's
  KISS TNC enabled (Menu → General Settings → KISS TNC).
- **Operator's running `tauri dev`** = the jvp build (orj + help text). It has **no**
  dial-behavior fix — retrying as-is reproduces the double-key + DM.
- bd state is durable in Dolt; `.beads/issues.jsonl` export sits staged in the main checkout
  (operator state — do not commit from a worktree).

## Next session — DO FIRST

1. Read this handoff + `bd show tuxlink-uhc`.
2. Fix `tuxlink-uhc` on a new branch `bd-tuxlink-uhc/...` off `main` (Rust backend:
   `datalink.rs` connect loop, `params.rs`, packet config `into_params`). Goal: one clean
   SABM with the configured TXdelay/T1 respected; no rapid double-key. TDD against the param
   path + connect timing (hardware-free; the on-air retry is the operator's).
3. Surface a rebuild+relaunch command so the operator retests on-air (RADIO-1). If it still
   DMs with a verified-single clean SABM, the cause is app-layer (callsign registration /
   connect SSID) — and the definitive evidence is a `btmon` capture of the exchange.

Agent: chasm-sorrel-glade
