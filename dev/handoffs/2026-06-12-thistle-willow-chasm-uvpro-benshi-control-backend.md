# Handoff — native UV-Pro Benshi control backend (Phase 2, tuxlink-nx95)

**Session:** thistle-willow-chasm · **Date:** 2026-06-12
**Branch:** `bd-tuxlink-nx95/uvpro-benshi-control` (off `origin/main`) · **Draft PR:** #647
**bd:** tuxlink-nx95 (in_progress, depends-on tuxlink-2f2n)

## What this session did

Built the **backend** for native on-screen control of the BTECH UV-Pro over its
own Bluetooth link (RFCOMM + GAIA) — Layer 2 of the APRS tactical chat epic
(tuxlink-2f2n). Deliverable per the operator's task = backend + a documented
Tauri command/event API a parallel frontend session wires to. Full
build-robust-features pipeline: grounding → spec → adrev → plan → TDD.

### Protocol grounding (sanctioned RE)
Reverse-engineered from two prior-art implementations that cross-validate 1:1
(winlink-RE-authoritative-sources; the OPPOSITE of the clean-sheet VARA rule):
- **benlink** (Python; the protocol-documentation project) — the spec-in-code.
- **HTCommander** (C# reference client) — confirms command ids + connect flow.

Cloned to `dev/scratch/benshi-re/` (gitignored). Grounded facts in
`dev/scratch/benshi-GROUNDING-FINDINGS.md`. **Golden byte vectors** derived
offline from benlink's pure encoder (no radio) are committed at
`docs/design/uvpro-benshi-golden-vectors.md` and pin every codec test.

### Shipped (all committed + pushed on the branch)
- **Spec:** `docs/design/2026-06-12-uvpro-benshi-control-phase2-design.md`
- **Plan:** `docs/plans/2026-06-12-uvpro-benshi-control-phase2-plan.md`
- **Backend module** `src-tauri/src/winlink/ax25/uvpro/`:
  - `bits.rs` big-endian bit codec · `gaia.rs` frame wrap + resilient deframer
    (multi-frame / split / resync / RX-checksum / buffer-cap) · `message.rs`
    header + request encoders + reply/event decode · `rf_ch.rs` 25-byte channel
    round-trip · `model.rs` camelCase DTOs · `settings.rs` channel-select patch
  - `session.rs` `Driver` (connect/hydrate/serialized req-reply/event apply/
    set_frequency/set_mode/set_channel/no-reconnect) + `UvproLinkLock`
    single-Bluetooth-host owner-lock + `UvproSession` wrapper
  - `commands.rs` 7 `uvpro_*` Tauri commands + `uvpro:status` broadcaster
    (+ bounded battery poll)
  - wired into `lib.rs` (manage state, register commands, spawn broadcaster)
- **Frontend API contract:** `docs/design/uvpro-control-api.md`

### Key decisions (all source-grounded)
- Transport = **RFCOMM + GAIA** (the on-air-proven UV-Pro link), not BLE-GATT.
- `set_channel` → `WRITE_SETTINGS` (active channel = `Settings.channel_a/_b`,
  per `Radio.cs`), patched in place (offsets pinned by a benlink diff).
- Single-Bluetooth-host arbitration: native side holds an owner-lock, fails
  `uvpro_connect` fast with `LinkBusy` if held.
- **RADIO-1 / ADR 0018:** non-transmitting by construction (no TX command);
  abort = drop the socket; no auto-reconnect. Agent never transmits.

## State at handoff
- **CI:** all 4 jobs (verify + build-linux, arm64 + amd64) PASS on `bb567de8`
  (the full backend incl. session driver + commands + wiring). The battery-poll
  follow-up first failed clippy `incompatible_msrv` (`u32::is_multiple_of` is
  Rust 1.87, newer than the project MSRV) and was corrected to modulo; the
  corrected commit's CI is the final gate — **re-verify `gh pr checks 647`.**
- **Working tree:** clean on the branch after the battery-fix commit.
- **Worktree:** `worktrees/bd-tuxlink-nx95-uvpro-benshi-control/`. Untracked/
  gitignored on disk: `node_modules/` (pnpm-installed; needed for the lint:docs
  pre-push hook), `dev/scratch/benshi-re/` (RE clones + venv refs),
  `dev/adversarial/` (codex stub). None are at-risk project content — all
  regenerable. Dispose via the ADR-0009 ritual once the PR merges.

## Status: PR #647 marked READY (2026-06-12, CI-green, all 4 jobs)

Operator decision 2026-06-12: **not gating on Codex** (can't wait for the quota
reset), so #647 was marked ready on CI-green alone.

Remaining (post-merge / non-blocking):
1. **Operator on-air smoke** — RADIO-1 operator-only: connect to the real UV-Pro,
   read live status into a UI, set a frequency and watch the radio retune, confirm
   disconnect works + `isTx` never asserts from control. Agent cannot run this.
2. **Optional** Codex cross-provider adrev (tuxlink-bv0b) — downgraded to a
   non-blocking post-merge review; run if/when Codex quota returns. Self-adrev
   rounds already folded into the spec.

## Follow-up bd issues filed (all depend-on nx95)
- **tuxlink-bv0b** (P2) — the deferred Codex adrev round.
- **tuxlink-mn9y** (P3) — KISS/packet path should consult the owner-lock so the
  conflict-from-KISS direction surfaces `LinkBusy`, not a raw socket error.
- **tuxlink-mjlh** (P3) — APRS messaging over the native `HT_SEND_DATA` path
  (collapses control + data onto one BT link; depends on Phase 1a / PR #642).
