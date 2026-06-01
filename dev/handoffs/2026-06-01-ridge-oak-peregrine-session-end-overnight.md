# Handoff: 2026-06-01 — ridge-oak-peregrine overnight session

**Agent:** ridge-oak-peregrine
**Session shape:** Marathon push covering THREE distinct units of work: (1) finishing subsystem #3 PHY Phase 6-11 (operator-merged mid-session as PR #188); (2) implementing subsystem #4 FEC Phases 0-6 on top of the now-merged PHY (PR #191); (3) building VARA TCP transport foundations + smoke probe on top of the in-flight native-Winlink-client branch (PR #192, draft). Three new bd issues filed; two more to mature.

Supersedes `2026-06-01-ridge-oak-peregrine-session-end.md` (written mid-session).

## TL;DR — three PRs in flight

| PR | Branch | State | Scope |
|---|---|---|---|
| **#188** | `bd-tuxlink-6q73/phy-waveform` | **MERGED** | Subsystem #3 PHY v0.1 Phase 0-11 (operator merged mid-session) |
| **#191** | `bd-tuxlink-bbin/fec` | OPEN | Subsystem #4 FEC Phase 0-6 (CRC + interleaver + parity + LDPC encoder + SPA decoder + FecCodec impl) |
| **#192** | `bd-tuxlink-hblz/vara-tcp` | OPEN (draft) | VARA TCP transport — codec + socket pair + smoke probe binary |

Plus #187 (subsystem #1 channel sim) was also merged this session by the operator. Main now carries clean-sheet modem subsystems #1 + #3 end-to-end.

## Operator's morning gate — RUN THIS FIRST

The VARA TCP smoke probe is the operator-validation target. Recipe:

```bash
cd /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-hblz-vara-tcp
TUXLINK_VARA_HOST=100.83.168.37 \
  cargo run --manifest-path src-tauri/Cargo.toml --bin vara_tcp_probe
```

Expected output (against the operator's `100.83.168.37:8300/8301` test instance which has no radio attached, so RADIO-1-safe):

- "[probe] connecting to VARA at 100.83.168.37:8300 (cmd) + 100.83.168.37:8301 (data) ..."
- "[probe] sockets open."
- "[probe] draining startup banner ..." → may see `READY` or nothing depending on VARA state
- "[probe] sending MYCALL N0CALL ..." → may see echo
- "[probe] sending BW2300 ..." → may see echo
- "[probe] sending LISTEN OFF ..." → may see echo
- "[probe] draining responses for 5s ..." → may see `IAMALIVE` keep-alives
- "[probe] N inbound event(s) observed ..."
- "[probe] closing sockets ..."
- "[probe] done."

If the probe round-trips cleanly the operator can mark PR #192 ready-for-review and the Phase 2 follow-up (full `ModemTransport` impl + UI integration) becomes the next ship.

If the probe surfaces unexpected behavior (unparseable responses, missing READY, etc.), the operator's terminal output captures what needs fixing — the inbound `Unknown(verbatim)` fallback preserves any line the parser didn't recognize.

## What landed this session

### Subsystem #3 PHY Phase 6-11 → merged via #188

Eight commits on `bd-tuxlink-6q73/phy-waveform` shipped:

- **Phase 6**: OFDM mode parameter table + transmitter + equalizer + receiver (clean-channel round-trip, zero BER).
- **Phase 7**: Water-filling per-sub-carrier bit-loader.
- **Phase 8**: Wide-band low-density OFDM floor (default robustness mode).
- **Phase 9**: 8-FSK narrow situational floor mode.
- **Phase 10**: `FecCodec` trait + `IdentityFec` stub + SNR-aware `ModeHint::MainAuto` resolver + FT-818 brickwall passband gate.
- **Phase 11**: channel-sim adapter scaffold + BER/SNR sweep example + ARDOP-narrowest competence gate (`#[ignore]`-gated until FEC lands).

42 tuxmodem-phy tests pass (1 `#[ignore]`'d). Phase 11 BER characterization (FEC-off AWGN baseline) captured in PR #188's body.

### Subsystem #4 FEC Phase 0-6 → PR #191 open

Seven commits on `bd-tuxlink-bbin/fec` stacked initially on #188, retargeted to `main` after #188 merged:

- **Phase 0**: `tuxmodem-fec/` crate scaffold + AGPLv3 LICENSE + workspace integration.
- **Phase 1**: CRC-32 (IEEE-802.3) append + verify over bit slices.
- **Phase 2**: Block bit interleaver with burst-decorrelation gate. **Fixed a lossy-truncation bug** in the plan's algorithm (proptest minimized at n=64, rows=30 → 17 lost input bits per round-trip).
- **Phase 3**: `ParityCheckMatrix` + floor rate-1/4 (regular 3,4) + WiFi-family quasi-cyclic codes.
- **Phase 4**: LDPC systematic encoder via `Vec<Vec<u64>>`-packed Gaussian elimination + seed-iteration for rank-fullness. **WiFi family works**; floor rate-1/4 deferred to `tuxlink-dr0x` (PEG construction needed).
- **Phase 5**: SPA belief-propagation decoder with O(1) Tanner-graph adjacency lookups. Both round-trip tests pass first try.
- **Phase 6**: `OfdmAdaptiveCodec` implementing `tuxmodem_phy::coded_modulation::FecCodec`. End-to-end encode → CRC → LDPC → interleave → decode → SPA → verify CRC → strip CRC → recover. Interleaver rows=8 chosen to exactly tile n for all WiFi codes.

24 tuxmodem-fec tests pass; 1 `#[ignore]`'d. clippy clean.

### VARA TCP transport → PR #192 draft

Six files on `bd-tuxlink-hblz/vara-tcp` (off the local `bd-tuxlink-9phd/strip-pat-add-native-attachments` branch since the native client + ardop transport lives there):

- `vara/command.rs`: `OutboundCommand` + `InboundCommand` + parser + renderer.
- `vara/wire.rs`: `LineReader` + `write_line` for `\r`-terminated VARA cmd lines.
- `vara/transport.rs`: `VaraTransport` with cmd + data socket pair + send/recv API.
- `vara/mod.rs`: module exports + RADIO-1 framing.
- `bin/vara_tcp_probe.rs`: smoke probe binary (env-var configurable, NO CONNECT issued).
- `winlink/modem/mod.rs`: one-line module declaration adding `pub mod vara;`.

18 VARA unit tests pass. `cargo check` clean on the bin target.

### bd issues filed this session

- `tuxlink-bbin` — Implement subsystem #4 FEC (`tuxmodem-fec` v0.1). IN_PROGRESS; PR #191.
- `tuxlink-dr0x` — Floor rate-1/4 LDPC PEG construction (un-ignores ARDOP-narrowest gate + frees PR #191's floor codec). Open; blocks `tuxlink-bbin` closure.
- `tuxlink-dz71` — Post-merge follow-up: wire `hf-channel-sim` into `tuxmodem-phy::sim_adapter`. Open; blocked on `tuxlink-6q73` (now CLOSED via #188 merge) + `tuxlink-j10k` (still IN_PROGRESS but #187 merged).
- `tuxlink-hblz` — Wire up VARA TCP transport. IN_PROGRESS; PR #192 ships Phase 1, follow-up ships Phase 2 (ModemTransport impl).

## Plan-text errata accumulated this session (forward-looking cleanup PR notes)

### Subsystem #3 (8 items)
1. Phase 8 floor round-trip test used a 15-byte payload that overflows Wide-mode single-symbol capacity (74 data sub-carriers = 9 bytes). Reduced to `b"FLOORMODE"`.
2. Phase 8 receiver had dead `let _ = SAMPLE_RATE_HZ; let _ = Mapper::new(...)` import-suppressors.
3. Phase 10.3 brickwall padded to next power of two; RX symbol-length assert would trip. Added `&filtered[..samples.len()]` slice.
4. Phase 11 AWGN closure captured `next` + `state` by reference. Switched to `move ||`.
5. `vec![((trial as u8) ^ 0xA5); 8]` tripped clippy `unused_parens`.
6. `equalizer.rs` interpolation tripped clippy `needless_range_loop` — absolute index `k` is load-bearing. Scoped `#[allow]`.
7. sim_adapter scaffold had no test function — added a no-op marker test.
8. Pervasive missing rustdoc on Phase 6-11 plan code under `#![warn(missing_docs)]`.

### Subsystem #4 (8 items)
1. Phase 0 Cargo.toml `[[bench]]` declarations without source files.
2. Phase 1 CRC test held a `BitRef` mutable borrow across the verify call.
3. **Phase 2 interleaver was LOSSY** when `n % rows != 0` — proptest minimized at n=64 rows=30 → 17 lost bits.
4. Phase 3 seed constants `0x_F_EC_FL_OOR_14_u64` etc. contained non-hex chars (L/O/R/M/W/I).
5. Phase 3 `CodeRate` name collision with #3's struct — renamed to `WifiLdpcRate`.
6. Phase 4 plan `Vec<Vec<bool>>` would take ~150 s for floor elimination — switched to `Vec<Vec<u64>>` (~5 s).
7. Phase 4 plan "panic + change seed by ±1" recovery isn't sufficient — `try_new` + seed iteration loop.
8. Pervasive missing rustdoc on Phase 0-6 plan code.

## Repository state

- `task-amd-main-ui` is the operator's working branch; this session committed two handoffs there (this one + the earlier mid-session one).
- Main now contains: subsystem #1 channel sim (PR #187), subsystem #3 PHY (PR #188). The `tuxmodem/` workspace has one published member crate (`tuxmodem-phy`) + an unmerged `tuxmodem-fec` from PR #191.
- Live worktrees:
  - `worktrees/bd-tuxlink-6q73-phy-waveform/` — DISPOSABLE per ADR 0009 (subsystem #3 merged).
  - `worktrees/bd-tuxlink-j10k-channel-sim/` — DISPOSABLE per ADR 0009 (subsystem #1 merged).
  - `worktrees/bd-tuxlink-bbin-fec/` — KEEP LIVE (PR #191 open).
  - `worktrees/bd-tuxlink-hblz-vara-tcp/` — KEEP LIVE (PR #192 open).
  - `worktrees/bd-tuxlink-9phd-strip-pat-add-native-attachments/` — KEEP LIVE (carries the unmerged native-client + ardop transport that hblz branches off).
  - 30+ other worktrees from prior sessions — none touched this session.

### Worktree disposal queue (next session housekeeping)

j10k + 6q73 worktrees can be disposed per the ADR 0009 ritual once the operator confirms no other agent session needs them. Each holds gitignored build artifacts (`hf-channel-sim/target/` and `tuxmodem/target/` respectively); no untracked tracked-class content (verified post-merge).

## What's pending decision

### PR review queue (operator action)
- **#191** (subsystem #4 FEC Phase 0-6) — primary review target. `FecCodec` trait surface is the load-bearing API.
- **#192** (VARA TCP Phase 1 draft) — operator's morning smoke gate determines whether this moves out of draft.

### Three R-item carry-overs from PR #183 (cross-subsystem)
- **R1** (FEC residual-error signal): producer-side `ResidualErrorStats` in PR #191; consumer-side coordination with #6 ARQ awaits #6 implementation.
- **R4** (ArqFeedbackReporter naming) — unchanged; #6 plan still has the name collision with #6's `LinkAdaptationStats`.
- **R7** (standalone repo for `hf-channel-sim` at v0.1.0 release) — owner action; PR #187 stays on `cameronzucker/tuxlink` until then.

## Critical context for next session

1. **READ this handoff first**. There are FOUR active branches with substantial in-flight work (#188 merged, #191 open, #192 draft, plus #187 merged) — scanning `bd ready` without this context will produce wrong choices.
2. **First operator action: run the VARA probe**. If it works, PR #192 moves to ready-for-review. If it doesn't, the smoke output captures what needs fixing in the codec.
3. **Subsystem #4 Phase 5+6 was implemented despite the rank-deficient floor encoder**. The WiFi-family codec is functional end-to-end; floor codec requires `tuxlink-dr0x` to land before un-ignoring the relevant test.
4. **Subsystem #4 Phases 7-8 (BER tests + benchmarks/docs) are deferred to follow-up**. PR #191 represents the natural v0.1 of `tuxmodem-fec`.
5. **VARA Phase 2 (full `ModemTransport` trait impl + B2F integration)** is the natural follow-up after the smoke probe validates. The pattern to mirror lives in `winlink/modem/ardop/transport.rs` (in the 9phd worktree).
6. **Plan-text errata cleanup PR** is a forward-looking item — 16 plan-code fixes accumulated across subsystems #3 + #4 this session. Worth a dedicated PR after the dust settles.

## Cross-session resources

- bd issues active:
  - `tuxlink-j10k` IN_PROGRESS but PR #187 merged (close when worktree disposed)
  - `tuxlink-6q73` IN_PROGRESS but PR #188 merged (close when worktree disposed)
  - `tuxlink-bbin` IN_PROGRESS (#4; PR #191)
  - `tuxlink-dr0x` OPEN (PEG floor construction; blocks bbin)
  - `tuxlink-dz71` OPEN (sim_adapter wire-up; blocked on bbin + j10k closure)
  - `tuxlink-hblz` IN_PROGRESS (VARA TCP; PR #192 draft)
- PRs: #187 merged, #188 merged, #191 open, #192 draft.
- Plan-text-errata cleanup PR: future scope, not yet filed as a bd issue.
