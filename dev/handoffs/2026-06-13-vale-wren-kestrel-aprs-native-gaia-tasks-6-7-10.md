# Session handoff — vale-wren-kestrel — 2026-06-13

Continued APRS Phase 2 native GAIA messaging (`tuxlink-7my9`, PR #642). Shipped the
**frame-level engine seam (Task 6, the flagged high-risk refactor) + the native driver
(Task 7) + the e2e round-trip test (Task 10)** — all CI-green on amd64. The remaining
two tasks both hit genuine boundaries this session: **Task 8 needs a persisted-config
design decision** (and its convergence channel is the blocked Codex adrev), and **Task 9
is Codex-quota-deferred** until ~13:49 today.

## ⚠️ Read first — state

- **Branch / PR:** `bd-tuxlink-2f2n/aprs-tactical-chat`, **PR #642 (draft)**. Worktree
  `worktrees/bd-tuxlink-2f2n-aprs-tactical-chat`, tracked clean, all pushed (HEAD
  `d5c2f270`). **Do NOT mark #642 ready** — gate is the operator on-air smoke, blocked by
  `tuxlink-9ky` (Pi can't BT-connect to the UV-Pro).
- **CI:** Tasks 6, 7, 10 all **green on amd64** (`verify` = clippy `-D warnings` + full Rust
  tests). Task 6's arm64 leg also confirmed green (12m32s). Tasks 7/10 arm64 was still
  running the redundant identical suite at handoff — re-verify the final conclusion of run
  `27458373617` (`gh run view 27458373617`); amd64 green is the meaningful gate.
- **The `GpsSourcePicker` vitest flake (`tuxlink-9tq3`)** did NOT bite this session, but the
  rule stands: red amd64-vitest with everything else green = the flake; re-run, don't chase.
- **No cold cargo (Mode A):** all Rust verified via GitHub CI (implement → push → CI runs
  `cargo test`). One task ≈ one CI round (~9–13 min). Keep this posture.

## What this session did (3 commits on the branch, all pushed)

1. **`d98b928c`** `feat(aprs): frame-level inbound/outbound seam for non-KISS transports` —
   **Task 6.** Factored the post-KISS body of `handle_inbound_bytes` into a transport-neutral
   `ingest_ax25` that returns auto-ACKs as **raw** AX.25. `handle_inbound_bytes` KISS-wraps
   them; new `handle_inbound_frame` returns them raw (the native reassembler hands it
   already-deframed AX.25). Split the TX drain the same way: `enqueue_send` now stores the
   **raw** AX.25 frame in the (transport-neutral) `TxQueue`, `tick_frames` drains it raw, and
   `tick` KISS-wraps `tick_frames`. **Invariant preserved: `tick == kiss_data_frame(tick_frames)`**,
   so the KISS path is byte-for-byte unchanged (a new test asserts this directly). 2 new tests.
2. **`c7566e3e`** `feat(aprs): native APRS driver bridging session <-> engine` — **Task 7.**
   New `native_driver.rs`: `AprsFrameTx` trait (production impl for `Arc<UvproSession>`),
   a sleep-free `NativeDriver` core (`ingest_inbound` / `apply_command` / `drain_due`) split
   from the `run_native` loop+cadence for unit-testing, teardown flushes pending retransmits
   to terminal (releases in-flight slots, matching the KISS `run()`). 3 tests. Registered in
   `aprs/mod.rs`. Module-level `allow(dead_code)` (run_native uncalled until Task 8).
3. **`d5c2f270`** `test(aprs): e2e round-trip across the native fragment layer` — **Task 10.**
   In `message.rs`'s test module (needs the private `header`/`CMD_EVENT_NOTIFICATION`):
   `build_ui_frame → fragment_ax25 → DATA_RXD EVENT_NOTIFICATION wire frame → decode_frame →
   Reassembler` reassembles to bytes equal to the original; the APRS codec
   (`Frame::decode`/`extract_inbound`/`parse_info`) then recovers the original callsign + text.

## Remaining — Tasks 8 + 9 (both at a real boundary)

### Task 8 — capability-gated transport selection — **NEEDS A DESIGN DECISION**
`ui_commands.rs` `aprs_listen_start` currently always brings up the KISS path
(`AprsState::start` → `connect_link_with_abort` + KISS `run()`). The blocker:
**`KissLinkConfig` has no UV-Pro-vs-generic distinction** — `Bluetooth { mac }` serves both
the KISS-over-RFCOMM path and would serve the native path. Task 8 therefore needs a **new
persisted-config capability/profile flag** to know "use native APRS for this radio."

This is a **serde-shaped + backward-compat-sensitive** decision (see the degradation note at
`config.rs:464` — a Bluetooth-aware config must not brick a non-Bluetooth build). Two project
rules say **do not finalize it unilaterally**: *no-unilateral-serde-decisions* and
*converge-cross-provider-via-Codex*. The convergence channel is the Codex adrev (Task 9),
currently quota-blocked. **Decide the config schema first** (operator shape-decision OR the
deferred Codex round), then the plan's Task 8 is straightforward: a pure `config → transport
kind` selection fn (UV-Pro → `Native`, Mobilinkd → `Kiss`) + the `AprsState` native path
(open/reuse the managed `Arc<UvproSession>` — lib.rs:228 — `take_aprs_receiver()`, spawn
`run_native`). Candidate schema shapes to weigh: a `profile` enum on the packet transport
(`KissTnc | UvproNative`); a capability field on the `Bluetooth` variant; or MAC-match against
a connected `UvproSession`.

**Task 8 is the live caller that removes ALL the TODO-tagged `allow(dead_code)`:**
`engine.rs` `handle_inbound_frame`/`tick_frames`, `native_driver.rs` module-level allow,
`session.rs` `send_aprs_frame`/`take_aprs_receiver`. Removing them is the completion signal.
It also unblocks `tuxlink-ve3j` (always-live control strip, frontend).

### Task 9 — Codex adversarial round — **CAPACITY-DEFERRED**
Codex ChatGPT-auth quota hit this session ("try again at Jun 13th 1:49 PM"). Per
*codex-quota-gotcha*: this is a **defer, not a skip — do NOT substitute Claude.** Retry after
~13:49. Prompt staged at `/tmp/codex-prompt-gaia.txt`; output → `dev/adversarial/2026-06-13-
native-gaia-aprs-codex.md` (the gitignored dir was created; the tee failed first attempt only
because the dir didn't exist). The fragment math (Tasks 1–4) is frozen and Tasks 7–8 don't
touch `tncdata.rs`/`message.rs` codec, so the adrev scope stays valid. nx95 skipped its adrev
(`tuxlink-bv0b`) — do NOT repeat that here.

## Worktree state
`worktrees/bd-tuxlink-2f2n-aprs-tactical-chat` — tracked clean, HEAD `d5c2f270`, all pushed.
Untracked/gitignored on disk: `node_modules/` (pnpm, for the pre-push lint:docs hook),
`dev/scratch/*.{png,html}` (mocks), `dev/scratch/benshi-re/` (decompile workspace + toolchain),
`dev/adversarial/` (created, empty — for the deferred Task 9). No at-risk content.
**Do NOT dispose** — active until #642 merges.

Agent: vale-wren-kestrel
