# Handoff — 2026-07-09 — `opossum-badger-gulch` — VARA PTT keying shipped; live-fire surfaced 3 bugs; on-air QSO NOT yet closed

## Headline

**Root cause of "VARA never works" found and FIXED: Tuxlink parsed VARA's `PTT ON`/`PTT OFF` events and discarded them everywhere — VARA is a soundcard modem with no PTT of its own, so nothing ever keyed the radio.** Wrote the missing host-side keyer. **PR [#1061](https://github.com/cameronzucker/tuxlink/pull/1061) — CI GREEN on `3bf93e1e`, mergeable.** Live-fire on R2 proved the keying works (real FT-710 keyed via CAT, ~40s of ConReqs) but did **not** complete a QSO — blocked by audio-provisioning gaps + VARA state I destabilized. Next session = **Option A**: one clean-slate VARA restart → lock audio route → one monitored dial.

## CRITICAL SAFETY STATE (read first)

- **Radio is UNKEYED** (`TX;` → `TX0;` confirmed at handoff). Safe.
- **Agent send authority is still ARMED** (~4h window from the operator's arm, ticking down). Two P0 bugs mean **the arm control is not trustworthy**:
  - `tuxlink-kw873` (P0): operator's DISARM does not stick / arm re-enables without interaction — `server_info` still showed `armed:true` after the operator disarmed.
  - `tuxlink-4u43s` (P0): DISARM does not abort an in-flight transmit (the guard only gates NEW ops).
  - **The ONLY reliable transmit brake is `vara_stop_session` (ungated AbortPort) or GUI Stop.** Keep it on a hair trigger during any dial. The shipped `UnkeyGuard` DID hold — radio never stuck keyed across 4 dials.

## What shipped (PR #1061, branch `bd-tuxlink-yrrjq/vara-ptt-keying`, worktree `worktrees/tuxlink-yrrjq`, off `origin/main`)

Commits `3f3f0572` → `3bf93e1e` (CI green, verified on R2 too: `cargo test` + `clippy -D warnings` clean on 1.96):
1. **`src-tauri/src/winlink/modem/vara/ptt.rs`** (new) — `PttSink`/`VaraPtt` keyer, resolved **fail-closed** before any CONNECT; `cat_command`→close-serial `TX1;`/`TX0;`, `serial_rts`→held-RTS, `vox`→loud no-op. `UnkeyGuard` unkeys on every exit incl. panic.
2. Dial path (`vara/commands.rs`): `wait_for_connected` + `vara_dial_disconnect` service PTT; concurrent **PTT pump** owns the cmd socket through the B2F exchange (`run_vara_b2f_exchange_io`); listener/answer path mirrored.
3. **`vara_open_session` MCP tool** (`tuxlink-cgna5`, CLOSED) — agents could dial but not open the session it requires; added across all 4 MCP layers, egress-gated.
4. Wire-walk gate PASSED (operator flow: "send outbox messages over VARA HF to any station" traced `file:line`, recorded on PR).

## Live-fire results (R2, PR build installed as 0.87.0)

- **Dial 1 (16:23):** `CONNECT N0DAJ` → `PTT ON` → **Tuxlink keyed the FT-710 via CAT, cycled PTT ~40s.** THE KEYING FIX WORKS ON REAL HARDWARE. But **zero power/audio out** → VARA gave up (`DISCONNECTED`). Cause = TX audio not reaching the DRA-100 (see gaps).
- Dials 2–3: pre-air bail ("session not open" — a prior `ConnectFailed` correctly reset session to Closed; must re-`vara_open_session` before each dial).
- **Dial 4 (16:42):** VARA rejected instantly with `WRONG CALLSIGN`, no keying. NEW — VARA's own state degraded after my ~6 restarts. **Ask the operator to check the VARA GUI (visible on VNC :1) for its registration/callsign state.**

## Bugs filed this session (the real deliverables beyond the keyer)

- **`tuxlink-0nfe2` (P1)** — Radio setup is entirely manual; Tuxlink must OWN VARA/audio/CAT provisioning. Getting one dial out required 5 hand-configs invisible to any user: dialout group, CAT-port verification, VARA.ini soundcard names, PipeWire profile, ALSA mixer. The product provisions+enumerates but never wires. **This is the "why so much plumbing" answer — extends `tuxlink-82rrf` from error-surfacing to provisioning.**
- **`tuxlink-kw873` (P0)** — arm doesn't reflect operator intent (above).
- **`tuxlink-4u43s` (P0)** — disarm doesn't abort in-flight TX (above).
- **`tuxlink-n8fpg` (P2)** — PTT-method config UI only in the ARDOP panel; VARA users can't find it.
- **`tuxlink-39o6z` (P3)** — no VARA/ARDOP listener-arm MCP tool (`packet_set_listen` exists; agents can't arm inbound P2P).

## R2 hand-wiring state (what I changed outside the repo — durable, needs product-ification per 0nfe2)

- `~/.config/tuxlink/config.json`: `cat_serial_path` `/dev/ttyUSB1`→**`/dev/ttyUSB0`** (CAT verified answering `FA;` on ttyUSB0 @38400 only; ttyUSB1 silent). Backup `config.json.bak-opossum2-*`. (The earlier wrong `cat_baud:4800` was already reverted to 38400 from `config.json.bak-opossum-20260709T121151Z`.)
- `~/.wine-vara/drive_c/VARA/VARA.ini`: soundcard Output was aimed at the radio's INTERNAL codec ("USB Audio Device") — dead-air by config; changed to the DRA ("USB PnP Sound Device Analog Stereo"). Backup `VARA.ini.bak-opossum`. **This + PipeWire is the audio fight; not fully nailed.**
- PipeWire: DRA device profile toggled (pro-audio ↔ analog-stereo+mono duplex); wireplumber `restore-stream` VARA entries purged once (regenerated); mixer `Speaker` 40%→86%. A `pw-loopback` compat-source (`dra_mono_compat`) may still be running (`~/start-dra-compat.sh`). **All of this reverts on VARA/wireplumber restart — that's the instability.**
- Helper scripts left on R2: `~/mcp_call.py` (minimal MCP UDS client, socket `/run/user/1000/tuxlink/mcp.sock`), `~/vara_txmon.py` (TX-audio monitor), `~/tuxlink-pr1061.deb` (the PR build).
- R2 is a **fast compile host** (memory `r2-is-tuxlink-compile-host`): rustup 1.96 at `~/.cargo/bin`; build dir `~/tuxlink-yrrjq-build`. Install tuxlink debs with **`apt`, not dpkg** (memory `apt-not-dpkg-for-tuxlink-debs`).

## Option A plan for next session (the operator chose this)

1. Confirm radio unkeyed; note arm is still on (untrustworthy — brake = `vara_stop_session`).
2. **Clean-slate VARA:** `pkill VARA.exe; wineserver -k`; relaunch on `DISPLAY=:1` `WINEPREFIX=~/.wine-vara`; **wait for FULL registration** (operator eyeballs the VARA GUI — the `WRONG CALLSIGN` is likely my-restart damage) before any dial.
3. **Lock audio DEFINITIVELY:** the failure mode is VARA opening its TX output stream at key-time and wireplumber restoring it to the internal codec. Options: set VARA's output to the DRA in VARA's own GUI + purge restore-stream + verify a manual `pw-play`→DRA reaches the radio; OR a wireplumber rule pinning `application.name=VARA HF Modem` output to the DRA sink. Prove with `~/vara_txmon.py` (records the DRA sink monitor during a dial) BEFORE trusting a dial.
4. Re-`vara_open_session` (armed) → ONE monitored `vara_b2f_exchange` to **N0DAJ 7103.0** (61 km, radio pre-tuned; fallbacks KM7N 7106.0, K7HTZ 14097.8). Watch: session log PTT, `~/vara_txmon.py` audio verdict, operator's power meter. On CONNECT → outbox (2 msgs) drains → Sent. **NO mid-flight VARA restarts** — that's what broke it.

## Git / worktree state
- Worktree `worktrees/tuxlink-yrrjq` on `bd-tuxlink-yrrjq/vara-ptt-keying` (PR #1061, off `origin/main`). All code pushed through `3bf93e1e`; this handoff on `78f...`→ new commit.
- Main checkout is on the **stale** `bd-tuxlink-ant8s/ardop-connect-fixes` (2985 behind main — the whole "read origin/main not the checkout" lesson; the ADR-0018/spec commits from earlier this session landed there and are effectively orphaned scratch — the REAL work is PR #1061).
- Still-pending disposal from the prior handoff: `worktrees/tuxlink-graylinefix`, `worktrees/verify-087`, plus now `worktrees/tuxlink-yrrjq` (keep until #1061 merges).

Agent: opossum-badger-gulch
