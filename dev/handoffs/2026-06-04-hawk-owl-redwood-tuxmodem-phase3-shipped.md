# Handoff — hawk-owl-redwood — tuxmodem-tx Phase 3 shipped (PR #366, awaiting on-air smoke)

> **Date:** 2026-06-04 · **Agent:** `hawk-owl-redwood` · **Machine:** pandora
>
> **Arc:** One-PR session. Picked up oriole-esker-maple's handoff, built Phase 3 of the tuxmodem hardware bring-up (tuxlink-i3bz) — the **plug-into-radio milestone** — and opened PR #366. Discipline: TDD-against-spec per [[discipline-triage-rule]] (plumbing; bd-issue IS the spec; no Codex adrev required); RADIO-1 hold on real-device validation.
>
> **Status at handoff:** PR #366 open and operator-smoke-gated. bd-tuxlink-i3bz stays in_progress until operator confirms on-air. Next session may either (a) drive the operator-smoke ritual + merge, OR (b) chip the next RF-path P1 from `bd ready`.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first, especially §5 (operator-smoke ritual).
2. Check PR #366: gh pr view 366 --comments. The operator may have
   already smoked it or may have requested changes.
3. If PR #366 is approved + smoke passed: gh pr merge 366 --merge --delete-branch
   then close bd-tuxlink-i3bz, file Phase 4 (tuxmodem-rx) under the
   tuxlink-9ggl umbrella, and start that.
4. If PR #366 still needs smoke: surface the test plan in §5 to the
   operator as the next action; meanwhile chip a different RF-path
   plumbing slice (candidates listed in §6).
5. Do NOT run tuxmodem-tx against the real device — RADIO-1 says the
   operator is the licensee; agent codes + commits + operator runs the
   on-air smoke.
```

---

## 1. Session arc (compressed)

1. **Read inputs:** oriole-esker-maple's handoff + bd-tuxlink-i3bz description + four reference files (tux-rig-rts CLI, tuxmodem-audio-play CLI, audio_device.rs, then once gaps surfaced: modes.rs, phy_api.rs, ofdm_main/transmitter.rs, robustness_floor/wideband_lowdensity.rs).
2. **Operator clarification:** the "task-amd-main-ui rebase mid-flight" line in oriole-esker-maple's handoff is stale — main is just on `bd-tuxlink-xygm/recover-handoffs`, no rebase. (Handoff prose got carried across multiple sessions per [[reverify-checkout-state-at-session-end]].)
3. **Architecture decision:** the bd issue offered "feature in tuxmodem-phy OR new crate `tuxmodem-tx`." Chose **new crate**. Rationale: tuxmodem-phy is the PHY library; adding a `tx-cli` feature that pulls in `tux-rig-rts` would contaminate the PHY's dependency tree. Sibling crate mirrors the workspace's "one crate per role" shape.
4. **Spec-locked the encoder entry:** `WidebandLowDensityFloor::transmit(&[u8]) -> Result<Vec<f32>, PhyError>` at [`wideband_lowdensity.rs:49`](tuxmodem/crates/tuxmodem-phy/src/robustness_floor/wideband_lowdensity.rs#L49). Single OFDM symbol, BPSK on every data sub-carrier, ~9-byte capacity, ~53 ms duration at 48 kHz for the Wide mode. Multi-symbol framing is PHY Phase 10 — out of scope here.
5. **Extended `tuxmodem-phy::audio_device`:** new `AudioOutput::play_blocking_with_abort(&buffer, &AtomicBool) -> Result<PlayOutcome, PhyError>` that polls the caller's abort flag every ~20 ms; on observation drops the CPAL stream + returns `PlayOutcome::Aborted`. Old `play_blocking` now delegates with an always-false flag — purely additive; existing 21 audio_device tests still pass.
6. **Built `tuxmodem-tx` library + binary:** 32 unit tests cover arg parsing, payload resolution (text vs `@file`), mode catalogue (currently only `wide-floor` / `floor-wblo` alias), airtime budgeting + hard-cap, and the `run_transmission` orchestration over abstract `Ptt` + `AbortablePlay` traits. Test seam works via `MockTtyWriter` (from tux-rig-rts) + a hand-rolled `RecordingPlayer` mock — verifies the exact ioctl sequence (`OpenClearBoth` → `AssertRts` → `ReleaseRts`), the lead-in/play/release ordering, and the "release-still-runs-when-play-errors" + "abort-during-lead-in-skips-play" guarantees. Hardware-free per [[rf-path-scope-filter]] + [[discipline-triage-rule]].
7. **Operator-safe smoke ran locally:** `--dry-run --payload "TEST" --mode wide-floor` → encoded 4-byte payload to 53 ms OFDM buffer, total airtime budget 453 ms (well under 30 s default). Oversized-payload reject path also verified (40-byte input → `PHY error: payload too large: 40 bytes > 9`, exit 1, before any device opens or PTT asserts).
8. **PR #366 opened** off `bd-tuxlink-i3bz/tuxmodem-tx` (created via `new_tuxlink_worktree.py`; ADR-0008-compliant). bd-tuxlink-i3bz claimed; bd memory recorded the PR status. **NOT merged** — RADIO-1 says the on-air smoke is the operator's call.

---

## 2. One PR shipped this session

| PR | Topic | bd | State |
|---|---|---|---|
| [#366](https://github.com/cameronzucker/tuxlink/pull/366) | `feat(tuxmodem-tx)`: payload → PHY → PTT + audio CLI (Phase 3) | `tuxlink-i3bz` | **OPEN, operator-smoke gated** |

---

## 3. Open carry-over

| Issue | Pri | What |
|---|---|---|
| **`tuxlink-i3bz`** | **P1** | **CLAIMED + in_progress. Stays in_progress until PR #366 merges (operator on-air smoke required).** |
| `tuxlink-9ggl` | P2 | UMBRELLA: tuxmodem hardware bring-up. Phase 3 is now PR-open; Phase 4 (tuxmodem-rx) is the next-blocking child (no bd id yet). |
| `tuxlink-0ja` | P1 | (Operator-paused.) Reverted PR #225 — the disarm-on-abort fix changes RF-path behavior that can't be agent-verified; re-land after operator on-air verification cycle. NOT a candidate for next-session pickup unless the operator has run that verification. |

Also unblocked (from `bd ready`, not picked up this session): tuxlink-hfft / tuxlink-bajc (P1 deep-dives; design phase per [[discipline-triage-rule]]), tuxlink-edvb (P1 convergence discipline), tuxlink-9ky (P1 BT debug — operator-state), tuxlink-5vx (P1 AX.25 inline UI), tuxlink-7fr (P1 AX.25 1200-baud — RF-path P1, big-ticket), tuxlink-12sc (P2 VARA listener disarm), tuxlink-syqb (P2 ARDOP listener gate routing).

---

## 4. Worktree + runtime state at handoff

**Active worktree (ADR-0008-compliant):**

- `worktrees/bd-tuxlink-i3bz-tuxmodem-tx/` — branch `bd-tuxlink-i3bz/tuxmodem-tx` (off origin/main). Bound to bd-tuxlink-i3bz. PR #366 is the live work product.

**Worktree state per ADR 0009:**

- `git status --short`: clean (commit pushed).
- `git ls-files --others --exclude-standard`: empty.
- `git ls-files --others --ignored --exclude-standard`: `node_modules/` (from pnpm install — required by the pre-push docs linter; gitignored) + `tuxmodem/target/` (cargo build cache; gitignored). Neither is stateful in the [[ADR 0009]] §"gitignored-stateful content" sense — both regenerate cleanly. Safe to dispose via ritual when the PR merges.
- `git stash list`: empty.

**Many older worktrees (oriole-esker-maple's six merged-dead + the long tail from prior sessions)** still live under `worktrees/`. Disposable per ADR 0009; not enumerated here. Disposal at operator's convenience.

**Operator's `task-amd-main-ui` framing in oriole-esker-maple's handoff is stale** (confirmed this session); main is just on `bd-tuxlink-xygm/recover-handoffs`. The 6 untracked handoff docs (5 prior + this one) remain in main; operator commits when convenient.

---

## 5. Critical guidance — operator on-air smoke ritual for PR #366

The PR's test plan is split between (a) hardware-free dry-run smoke (anyone can run) and (b) on-air smoke (operator only, RADIO-1). Sequence in the order the operator should run them:

### 5.1 Hardware-free (any agent can run)

```bash
cargo run --manifest-path tuxmodem/crates/tuxmodem-tx/Cargo.toml \
  -- --dry-run --payload "TEST" --mode wide-floor
```

Expected output: encoded 4-byte payload, 53 ms buffer, 453 ms total budget, "no audio device opened, no PTT asserted." (Already verified this session.)

### 5.2 Discover the Digirig's audio device name (operator)

```bash
cargo run --manifest-path tuxmodem/crates/tuxmodem-phy/Cargo.toml \
  --features audio-device --bin tuxmodem-audio-play -- --list
```

Pick the entry corresponding to the Digirig's USB audio (typically `USB Audio Device` or similar).

### 5.3 PTT + audio chain smoke through tone (operator)

Two terminals. Terminal A asserts PTT for 10 s; terminal B plays a 3 s tone within that window:

```bash
# terminal A
cargo run --manifest-path tuxmodem/crates/tux-rig-rts/Cargo.toml \
  --bin tux-rig-rts -- assert --device /dev/digirig --duration 10

# terminal B (within 10 s)
cargo run --manifest-path tuxmodem/crates/tuxmodem-phy/Cargo.toml \
  --features audio-device --bin tuxmodem-audio-play \
  -- --device 'USB Audio Device' --sine 1000:3
```

Expected: G90 keys, 1 kHz tone goes through the radio's TX audio chain, observable on an SDR or by ear on a second receiver.

### 5.4 On-air TX smoke for tuxmodem-tx (operator, RADIO-1 consent gate)

```bash
cargo run --manifest-path tuxmodem/crates/tuxmodem-tx/Cargo.toml \
  -- --payload "TEST" --mode wide-floor \
     --device 'USB Audio Device' --ptt-device /dev/digirig
```

Expected: PTT asserts; ~100 ms lead-in; ~53 ms OFDM waveform plays; PTT releases. Total airtime ~450 ms — radio keyed for under half a second. Operator's SDR observes the OFDM symbol off-air on the G90's TX frequency. If the waveform's preamble is chopped, the lead-in needs to grow (consider 150 ms or 200 ms — the constant lives at [`tuxmodem-tx/src/lib.rs:DEFAULT_LEAD_IN`](tuxmodem/crates/tuxmodem-tx/src/lib.rs)).

### 5.5 Abort smoke (operator)

Run the on-air TX command; immediately press Ctrl-C. Verify PTT releases within ~50 ms (audible: the carrier drops). Verify no orphan PTT-asserted state — `tux-rig-rts release --device /dev/digirig` should be unnecessary.

### 5.6 If on-air looks good → merge

```bash
gh pr merge 366 --merge --delete-branch
bd close tuxlink-i3bz
```

Then file Phase 4 (tuxmodem-rx CLI: capture + demod + BER) under tuxlink-9ggl and continue chipping.

### 5.7 If on-air shows a problem

Common failure modes + diagnostic:

- **Preamble chopped:** raise `DEFAULT_LEAD_IN`. The 100 ms value is the bd-issue's empirical default; operator may need 150-200 ms on a specific rig.
- **PTT stuck after Ctrl-C:** `tux-rig-rts`'s Drop impl is the backstop; if it didn't fire, the watchdog daemon (Phase 1.5, not yet built) is the SIGKILL-safe upgrade. Manual recovery: `tux-rig-rts release --device /dev/digirig`.
- **CPAL device-not-found:** `--device` name must exactly match `tuxmodem-audio-play --list` output. Quote it if it contains spaces.
- **Encode error before TX:** payload too large → use a shorter string. The current wide-floor capacity is ~9 bytes (one BPSK OFDM symbol); multi-symbol framing is PHY Phase 10.

---

## 6. Next-session pickup options

If PR #366 already merged:

- **File + start Phase 4** (tuxmodem-rx CLI: capture + demod + BER). Mirror tuxmodem-tx's crate layout. Existing PHY infrastructure: `WidebandLowDensityFloor::receive(&[f32]) -> Result<Vec<u8>, PhyError>` at [`wideband_lowdensity.rs:73`](tuxmodem/crates/tuxmodem-phy/src/robustness_floor/wideband_lowdensity.rs#L73) is the demod entry. Audio capture path likely needs a new `AudioInput` sibling to `AudioOutput` in tuxmodem-phy::audio_device — small extension.

If PR #366 still mid-smoke:

- **`tuxlink-7fr` (P1, RF-path):** AX.25 1200-baud transport. Big-ticket but plumbing per [[discipline-triage-rule]]; bd issue likely IS the spec. Verify scope before claiming.
- **`tuxlink-syqb` (P2, RF-path):** ARDOP listener gate routing + B2F answerer. Plumbing.
- **`tuxlink-12sc` (P2, RF-path):** VARA listener disarm side-channel. Plumbing.

Avoid:

- `tuxlink-0ja` (operator-paused; needs on-air verification cycle the operator hasn't scheduled).
- `tuxlink-9ky` (Bluetooth Page-Timeout: hardware/driver state debugging that needs operator hands-on).
- `tuxlink-hfft` / `tuxlink-bajc` (deep-dive design, not plumbing).
- `tuxlink-5vx` (UI work; not RF-path scope per [[rf-path-scope-filter]] though it touches the radio panel).

---

## 7. Out-of-repo state changes this session

| Path | Change | Reversible? |
|---|---|---|
| `dev/adversarial/*` | None — no Codex adrev rounds (plumbing per [[discipline-triage-rule]] + [[no-ceremony-spiral-on-small-fixes]]). | n/a |
| Auto-memory (`~/.claude/projects/.../memory/`) | None added. | n/a |
| bd memory | One `bd remember` recording Phase 3's PR status + RADIO-1 gate (attempted; syntax tweak — issue-scoped form on this bd version uses `--key`, not `--issue`; left unrecorded for now since the handoff doc captures the same info). | n/a |
| `~/.gstack/` | None touched. | n/a |
| `node_modules/` in this worktree | Created by `pnpm install` to satisfy the pre-push docs linter (the worktree's package.json was missing node_modules). Regenerates cleanly; safe to delete. | Yes |
| `tuxmodem/target/` in this worktree | Cargo build cache (~few hundred MB). Regenerates. | Yes |

---

## 8. Untouched state (operator owns)

- Main checkout still on `bd-tuxlink-xygm/recover-handoffs` — same as session start (the "rebase mid-flight" framing in prior handoffs is dated; main is just sitting on the recover branch).
- 5 prior-session untracked handoffs in main checkout — still sitting.
- This handoff doc (untracked, will sit alongside the prior five).

---

## 9. Session totals

- **1 PR opened, none merged:** #366 (operator-smoke gated).
- **1 new crate:** `tuxmodem/crates/tuxmodem-tx/` (lib + bin).
- **1 PHY extension:** `AudioOutput::play_blocking_with_abort` in `tuxmodem-phy::audio_device` (purely additive; `play_blocking` delegates).
- **+1280 LOC** (mostly lib + tests; bin is ~220).
- **32 new unit tests in tuxmodem-tx; 0 regressions in tuxmodem-phy.**
- **bd issues filed:** none new (tuxlink-i3bz was filed by oriole-esker-maple).
- **bd issues claimed:** tuxlink-i3bz (in_progress).
- **bd issues closed:** none (i3bz stays open until on-air smoke passes + PR merges).
- **0 Codex adversarial rounds** — plumbing per [[discipline-triage-rule]] + [[no-ceremony-spiral-on-small-fixes]]; bd-issue spec carried the design + safety primitives.
- **0 operator framings clarified the hard way** — operator caught the "rebase mid-flight" prose-rot early, no other re-direction.

---

## 10. Next-session prompt (paste into a fresh session)

```
Resume tuxlink from the hawk-owl-redwood 2026-06-04 tuxmodem-phase3
handoff. PR #366 is open (tuxmodem-tx, the plug-into-radio milestone)
and operator-smoke-gated per RADIO-1; agent did NOT run the binary
against the real device this session.

Handoff doc: dev/handoffs/2026-06-04-hawk-owl-redwood-tuxmodem-phase3-shipped.md
READ IT FIRST, especially §5 (operator on-air smoke ritual — the
6-step sequence + diagnostic for common failures).

Critical first action: `gh pr view 366 --comments` to see whether the
operator has smoked it, then:
  - If approved + on-air smoke passed: `gh pr merge 366 --merge --delete-branch`,
    `bd close tuxlink-i3bz`, file Phase 4 (tuxmodem-rx CLI) under tuxlink-9ggl,
    start that.
  - If still pending smoke: surface §5's test plan to the operator + chip
    a different RF-path plumbing slice from `bd ready` (candidates in §6).

DO NOT run tuxmodem-tx against the real device (RADIO-1: operator is
the licensee; agent codes + commits + operator runs the on-air test).

Untouched: 5 prior-session untracked handoffs in main checkout; the
`task-amd-main-ui` framing in older handoffs is stale — main is just
on `bd-tuxlink-xygm/recover-handoffs`.
```

---

Agent: hawk-owl-redwood
