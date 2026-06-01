# Handoff: 2026-06-01 — session end — ridge-oak-peregrine

**Agent:** ridge-oak-peregrine
**Session shape:** Single-focus push that resumed subsystem #3 PHY at Phase 6 and shipped all remaining phases (6 → 11) end-to-end on the live `bd-tuxlink-6q73-phy-waveform` worktree. Six new commits land on top of `bee9f92`'s prior Phase 0-5 work; PR #188 now carries the full subsystem #3 v0.1 scope. No handoff PR'd (per `feedback_no_pr_for_handoffs`).

## TL;DR

PR #188 (subsystem #3 PHY) is now Phase 0-11 complete and stacked on PR #187 (subsystem #1 channel sim). 42 tuxmodem-phy tests pass (1 `#[ignore]`-gated awaiting subsystem #4 FEC); workspace clippy clean under `-D warnings`; release build clean. Two follow-up bd issues filed for the strict consequences of #3 closing.

## What landed this session

Six commits on `bd-tuxlink-6q73/phy-waveform` (oldest to newest):

| SHA | Phase | Scope | New tests |
|---|---|---|---|
| `7531188` | 6.1 | OFDM mode parameter table (Narrow/Mid/Wide) + `ofdm_main/` stubs | 2 |
| `bee9f92` | 6.2 | OFDM transmitter (one-symbol modulate) | 1 |
| `926de8f` | 6.3 | OFDM equalizer + receiver (clean-channel round-trip, zero BER) | 1 |
| `76c5c1a` | 7 | Water-filling per-sub-carrier bit-loader | 3 |
| `262fc1f` | 8 | Wide-band low-density OFDM floor (default robustness mode) | 3 |
| `a950860` | 9 | 8-FSK narrow situational floor mode | 2 |
| `87ee200` | 10 | `FecCodec` trait + `IdentityFec` stub + SNR-aware mode router + FT-818 brickwall gate | 6 |
| `fd0c422` | 11 | `sim_adapter` scaffold + BER/SNR sweep example + ARDOP-narrowest gate (`#[ignore]` until FEC) | 1 |

Phase 6 is the DSP heart. The audio-channel `.re` simplification halves all loaded sub-carriers identically — TX pilots at `+1+0j` observe as `+0.5+0j` after the matching `1/sqrt(N)`-scaled forward FFT, so the pilot-aided single-tap zero-forcing equalizer cancels the halving exactly. Clean-channel round-trip is lossless; the 3 dB SNR cost surfaces only in Phase 11's AWGN sweep.

## Phase 11 BER characterization (FEC OFF baseline)

Captured from `cargo run --release --example ber_vs_snr_sweep`:

```
mode,snr_db,ber
ofdm-Narrow,-5,0.0031 ... 0..30 dB,0
ofdm-Mid,-5,0.0117 ... 0..30 dB,0
ofdm-Wide,-5,0.0699  ofdm-Wide,0,0.0057 ... 5..30 dB,0
floor-wblo,-15,0.3578  -10,0.1430  -5,0.0039 ... 0..10 dB,0
```

The floor mode bottoms out at ~ -5 dB AWGN WITHOUT FEC. The ARDOP-narrowest competence gate targets -8 dB BER < 0.01, which lands once subsystem #4's rate-1/4 LDPC FEC is wired in. This is the un-ignore trigger for `wideband_floor_vs_ardop_target.rs`.

## Plan-text errata caught + fixed inline this session

1. **Phase 8 test payload overflowed Wide-mode single-symbol capacity**: plan used `b"FLOOR-MODE-TEST"` (15 bytes) but Wide mode at BPSK packs 9 bytes per symbol. Reduced to `b"FLOORMODE"`. Plan numbers predated Task 6.1's parameter pin.
2. **Phase 8 receiver had dead import-suppressors**: `let _ = SAMPLE_RATE_HZ; let _ = Mapper::new(...)` plus unused imports — vestigial from a prior draft. Dropped.
3. **Phase 10.3 brickwall padded to next power of two**: RX symbol-length assert would trip on the longer filtered buffer. Added `&filtered[..samples.len()]` slice before demod.
4. **Phase 11 AWGN closure captured `next` + `state` by reference**: switched to `move ||` per call so closure compiles cleanly.
5. **Plan's `vec![((trial as u8) ^ 0xA5); 8]`**: clippy `unused_parens`. Removed outer parens.
6. **Plan's equalizer interpolation tripped clippy `needless_range_loop`**: absolute index `k` is load-bearing for `t = (k-a)/span`. Scoped `#[allow]` with rationale rather than rewriting as iterator chain.
7. **Plan's sim_adapter scaffold had no test function**: would silently bit-rot under future feature changes. Added a no-op marker test.
8. **Pervasive missing rustdoc on Phase 6-11 plan code**: same pattern caught in Phases 0-5. Documented inline; the lib carries `#![warn(missing_docs)]`.

## Repository state

- Local `task-amd-main-ui` is the operator's working branch (no agent commits this session).
- `bd-tuxlink-j10k/channel-sim` branch live (PR #187 open).
- `bd-tuxlink-6q73/phy-waveform` branch live, all this session's commits pushed (PR #188 open, stacked on j10k).
- Operator has not merged #187 or #188 yet.

### In-flight worktrees (per ADR 0009)

**`worktrees/bd-tuxlink-j10k-channel-sim/`** (subsystem #1, PR #187):
- Tracked dirty: none
- Untracked: none
- Gitignored-stateful: `hf-channel-sim/target/` build artifacts
- Stashes: none worktree-scoped
- Disposition after PR #187 merge: dispose via ADR 0009 ritual

**`worktrees/bd-tuxlink-6q73-phy-waveform/`** (subsystem #3, PR #188):
- Tracked dirty: none (verified post-Phase-11 commit)
- Untracked: none
- Gitignored-stateful: `tuxmodem/target/` build artifacts (release + debug)
- Stashes: none worktree-scoped
- Disposition after PR #188 merge: dispose via ADR 0009 ritual

## What's pending decision

### Operator review of PR #188

- `PhyTransport` trait surface — last chance before #5 link-MAC and #7 link adaptation start integrating against it.
- `FecCodec` trait surface — the inter-crate boundary with subsystem #4 (now filed as `tuxlink-bbin`).
- `ModeTable::resolve` SNR thresholds (-3 / 0 / 10 / 20 dB) — these get re-pegged against Watterson sweeps in a follow-up.
- Phase 11 BER sweep numbers above and the floor mode's -5 dB AWGN BER-knee.
- Whether to continue PR #188 stacked on #187 or rebase against `main` after #187 merges.

### Filed follow-up bd issues this session

- **`tuxlink-bbin`** (P2 feature): Implement subsystem #4 FEC (`tuxmodem-fec` v0.1). Depends on `tuxlink-6q73`. Implements `FecCodec` against the trait surface this session landed; un-ignores the ARDOP-narrowest gate test once wired.
- **`tuxlink-dz71`** (P3 task): Wire `hf-channel-sim` into `tuxmodem-phy::sim_adapter` (post-merge follow-up). Depends on `tuxlink-6q73` + `tuxlink-j10k`. Decides workspace topology (path-dep vs published-crate), wires the actual Channel/ChannelCondition imports, re-runs the BER characterization under Watterson "moderate"+"poor", re-pegs Phase 7's threshold table against the Watterson data.

### R-item carry-overs from PR #183 (cross-subsystem)

R1 (FEC residual-error signal), R4 (ArqFeedbackReporter naming), R7 (standalone repo for `hf-channel-sim` at v0.1.0 release) — same disposition as opossum-pine-spruce's handoff. R2/R3/R5/R6 fully baked in by this session.

### Plan-text errata cleanup PR (forward-looking)

A future plans-cleanup PR is the natural home for fixing the plan markdown verbatim-code blocks against the 8 errata-fix patterns logged in PR #188's body. Not blocking; just hygiene.

## Critical context for next session

1. **READ this handoff first** before scanning `bd ready`. PR #188 is feature-complete and pending operator review; nothing in the visible `bd ready` set is unlocked-by-#3-completion in a way that warrants the same worktree.
2. **Subsystem #3 is `bd ready`-unlocking for `tuxlink-bbin` (subsystem #4 FEC)** but `tuxlink-bbin`'s `depends_on` is `tuxlink-6q73`, so it stays blocked until #6q73 closes (i.e., PR #188 merges). Operator may want to merge #187 + #188 first; then `bd ready` surfaces `tuxlink-bbin` for fresh-session subsystem #4 work.
3. **If the operator wants to start subsystem #4 immediately** (without waiting for #6q73 to close), the agent can claim `tuxlink-bbin` early and start a new worktree off `bd-tuxlink-6q73/phy-waveform`. This stacks PRs three deep (#187 ← #188 ← new), which works but increases rebase risk if any of the upstream gets squashed (it shouldn't per ADR 0010, but the cost is real).
4. **The Phase 11 BER sweep numbers** are baseline characterization (AWGN-only, FEC-off). Watterson + FEC results will look very different; that's the subsystem-#4 + sim_adapter wire-up work to characterize.

## Cross-session resources

- bd issues:
  - `tuxlink-yiyi` CLOSED (planning sprint)
  - `tuxlink-j10k` IN_PROGRESS (#1; PR #187)
  - `tuxlink-6q73` IN_PROGRESS (#3; PR #188) — this session pushed Phase 6-11
  - `tuxlink-bbin` OPEN (#4 FEC; filed this session)
  - `tuxlink-dz71` OPEN (sim_adapter wire-up; filed this session)
- Worktrees: 2 live (j10k + 6q73)
- PRs: #187 + #188 open, pending operator review/merge
