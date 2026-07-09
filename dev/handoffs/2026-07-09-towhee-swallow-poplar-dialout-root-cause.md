# Handoff — 2026-07-09 — `towhee-swallow-poplar`

## Headline

**"No agent can drive Tuxlink" root cause found and proven: the R2 app user is not in the `dialout` group.** It cannot open the serial ports the radio's CAT and PTT ride on. This is the single unifying cause of the agent's rig failures, the "can't read CAT," and manual VARA not transmitting. Also merged the gray-line fix (PR #1059) earlier this session.

## Proven root cause (not a hypothesis)

Opening the ports as the app's own user (`administrator`, pid was 9610) on R2:

```
/dev/ttyUSB0 OPEN FAILED -> 13 Permission denied   (PTT port)
/dev/ttyUSB1 OPEN FAILED -> 13 Permission denied   (CAT port, 38400)
```

- Ports are `crw-rw---- root dialout`, `other=---`, **no udev ACL** (getfacl confirmed).
- `administrator` groups: `adm cdrom sudo audio dip plugdev users lpadmin ollama` — **no `dialout`**.
- Chain to the opaque error: `rig_tune` → `ardop_tune_rig()` → `tux_rig::ManagedRig::spawn()` launches bundled `tuxlink-rigctld` (present at `/usr/bin/tuxlink-rigctld`) → rigctld can't open the serial (EACCES) and dies → `ManagedRig`'s timed read to the dead daemon returns `EAGAIN` → `"rig I/O error: Resource temporarily unavailable (os error 11)"` reaching both the Elmer agent and the operator UI.
- **Same cause breaks VARA transmit:** PTT is `ptt_method: cat_command` on `/dev/ttyUSB0` (`TX1;`/`TX0;`). No dialout → can't key → VARA cannot transmit.

**Tuxlink already detected this and hid it:** the serial env-probe logs `"in_dialout_group": false` at every `first_paint`, but nothing surfaces it; the rig tool returns an opaque `os error 11` instead. That is the real end-to-end product gap.

## Operator fix (run on R2, then re-test)

```bash
sudo usermod -aG dialout administrator
# LOG OUT and back in (group applies to new sessions only), or reboot
# relaunch Tuxlink → retry manual VARA + an agent rig_tune
```

## Audio was a false alarm

`cards_count:0` in the logs is a **diagnostic artifact**: `env_probes/audio.rs` shells `pactl`, which is **not installed on R2** (PipeWire-only). The hardware is fully present — two C-Media USB cards in ALSA; `arecord -L`/`aplay -L` list `hw:CARD=Device`; the agent's `ardop_list_audio_devices` tool (which parses those) works. No audio blocker to driving.

## Filed issues

- **tuxlink-82rrf (P1, bug)** — Serial/PTT **preflight**: detect dialout + can't-open-port and surface an *actionable* error to operator UI AND agent tool result (instead of `os error 11`). This is the fix that makes the class stop happening.
- **tuxlink-27jtd (P2, feature)** — `rig_status` should read **live CAT** (VFO/mode) via the gated session; today it is config-only, so the agent has no true CAT read at all.
- **tuxlink-ga3sg (P3, bug)** — audio probe should not depend on `pactl` (use `pw-cli` or parse `/proc/asound/cards`); the false `cards_count:0` actively misled debugging.
- **tuxlink-0523m (P3, task)** — rename `ardop_list_audio_devices` MCP tool to modem-agnostic (audio enum is generic; the `ardop_` prefix mis-steers agents doing VARA/packet setup).

## State

- **Branch (main checkout):** `bd-tuxlink-ant8s/ardop-connect-fixes` (HEAD 81259bfb) — pre-existing, unrelated to today's work; this handoff commits here.
- **PR #1059** (gray-line `flex: 0 0 auto` fix) — **merged to main**, verified live on R2. bd tuxlink-rionf closed.
- **Working tree:** many untracked dev docs/pngs (pre-existing, per session-start git status).

### Worktrees needing disposal (ADR 0009 ritual, NOT done — deferred for context)

- `worktrees/tuxlink-graylinefix` — branch `bd-graylinefix/elmer-card-flex-shrink`, **merged via #1059 → dead**. Dispose.
- `worktrees/verify-087` — render-harness scratch (`dist/` fixtures), detached. Dispose.
- Any `tuxlink-h5azu` / `tuxlink-jc6st` worktrees from earlier streaming/run-limits work — inventory before disposing.

## Pending / next

1. **Blocking next step:** operator runs the `usermod` + re-login, then reports *exactly* how manual VARA fails now (connect never keys? keys but no decode? engine won't launch?). dialout definitively unblocks CAT/PTT; whether audio-routing/engine layers remain is unverified — chase with evidence, do not assume.
2. Then implement **tuxlink-82rrf** (the preflight) — highest-value code fix; turns this whole class into a 30-second guided fix.
3. Deferred backlog (unchanged): run-limits increment 2 (configurable per-response timeout UI); voice-Elmer internal trial (`dev/scratch/voice-trial/PRESTAGE.md`); DGX Spark model bake-off (Nemotron stays; TrueNAS cold-storage + agent SMB ACL); revoke Spark passwordless sudo when done.

## Process note for next session

Three times this session I asserted machine state from memory instead of verifying (R2 not running tuxlink; 0.87.0 installed; "no live radio") and each was wrong. Memory `feedback_verify_operator_machine_state_never_assume` written. **Verify live or take the operator's word; never assert remembered state.** The dialout root cause was only found by opening the ports as the actual user — evidence, not theory.

Agent: towhee-swallow-poplar
