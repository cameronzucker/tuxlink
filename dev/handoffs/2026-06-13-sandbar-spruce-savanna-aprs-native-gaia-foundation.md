# Session handoff — sandbar-spruce-savanna — 2026-06-13

Took APRS Phase 2 from "is the premise even sound?" to a **CI-verified native protocol foundation + the data-path backend**. The headline outcome is an **architecture correction**: a vendor-app decompile proved the UV-Pro's native GAIA protocol carries control + chat + position over **one** connection (the official app never mode-switches), so the prior "KISS-xor-native mode-switch" design was an artifact of two separate RFCOMM backends. Phase 2 is now the **unified native model**: on a UV-Pro, control + chat share one link, control strip always-live.

## ⚠️ Read first — state

- **Branch / PR:** `bd-tuxlink-2f2n/aprs-tactical-chat`, **PR #642** (draft). Worktree `worktrees/bd-tuxlink-2f2n-aprs-tactical-chat`, **tracked clean, all pushed** (HEAD `04bf370b`). **Do NOT mark #642 ready** — its gate is the operator on-air smoke, still blocked by `tuxlink-9ky` (Pi can't BT-connect to the UV-Pro).
- **CI status:** Tasks 1–4 **green both legs** (golden vectors execute + pass). Task 5 **green on amd64** (clippy + Rust tests), arm64 was completing the same tests redundantly at handoff — verify the final conclusion of run on `04bf370b` first thing (`gh run list --branch bd-tuxlink-2f2n/aprs-tactical-chat`).
- **Recurring CI friction:** the `GpsSourcePicker 'rescans on demand'` vitest (`tuxlink-9tq3`) is flaky and **fail-fast-cancels the good leg** on ~half of pushes. It already has a `waitFor`; the real flake is a deeper scan-guard race. A red CI on amd64-vitest with everything else passing = the flake; **re-run** (`gh run rerun <id>`), don't chase it. If it keeps gating Tasks 6–10, fixing 9tq3 may be worth it.
- **No cold cargo on this Pi** — all Rust verified via GitHub CI (Mode A: implement → push → CI runs `cargo test`). Each task = one CI round (~7–15 min). Keep that posture.

## What this session did (7 commits on the branch, all pushed)

1. **`294480aa`** docs(aprs): resolved spec open-questions #1+#3 — the canonical record that native GAIA carries messaging. (The vendor decompile.)
2. **`0b1c03fc`** docs(plan): the 10-task implementation plan — `docs/superpowers/plans/2026-06-13-aprs-native-gaia-messaging.md`.
3. **`0da7d932`** feat: `tncdata.rs` — pure `TncDataFragment` codec + `fragment_ax25` + `Reassembler` (13 golden vectors). **Tasks 1–3.**
4. **`840bd200`** feat: `message.rs` — `CMD_HT_SEND_DATA(31)`, `encode_ht_send_data`, `Frame::SendDataReply`, `Event::DataReceived`; `decode_frame` decodes `DATA_RXD` (was ignored). **Task 4.**
5. **`c6605aca`** fix: clippy `useless_vec` (CI round 1 of Tasks 1–4).
6. **`349f6b86`** feat: `session.rs` — `Driver::send_aprs_frame` (fragments → `HT_SEND_DATA`, awaits `SendDataReply`), `apply_event` `DataReceived` → `Reassembler` → mpsc channel, `hydrate` subscribes `DataRxd`, `UvproSession` wires the channel at connect + `take_aprs_receiver`/`send_aprs_frame`. FakePeer + 3 tests. **Task 5.**
7. **`04bf370b`** fix: mpsc `channel()` name collision (a local `let channel = resolve_spp_channel(mac)` u8 shadowed the imported fn).

### Off-branch, this session
- **Vendor app decompiled** (`com.benshikj.ht.btech.ham`) → `dev/scratch/benshi-re/DECOMPILE-FINDINGS.md` (gitignored). benlink's opcode table verified an **exact match for 0–76**; the one gap (`SET_SATELLITE_INFO`=77) became **benlink PR #26** (community contribution, merged-or-open at `khusmann/benlink`). Toolchain (jadx, apkeep) + decompiled source under `dev/scratch/benshi-re/` (gitignored).
- **Unified-model visual study**: `worktrees/…/dev/scratch/aprs-unified-shot.png` (the prior mode-switch mock is superseded).
- **bd:** created `tuxlink-7my9` (this backend), rewrote `tuxlink-ve3j` to the unified model (always-live control surface), wired dep edges (`ve3j → 7my9 → nx95`). `7my9` is **in_progress** with a full task-status note (`bd show tuxlink-7my9`).

## Remaining — Tasks 6–10 (the plan has full code/anchors)

The plan is the spec; key facts: `tncdata.rs` reuses unchanged; the APRS engine's codec + `tx.rs` (ACK/timeout) reused unchanged; the TX path + `take_aprs_receiver` carry **TODO-tagged `allow(dead_code)`** that **Task 7/8 must remove** when they add a live caller.

- **Task 6** — `engine.rs`: add `handle_inbound_frame()` (non-KISS sibling of `handle_inbound_bytes`, engine.rs:118) + `tick_frames()` (raw AX.25, not KISS-wrapped). **Refactors shipped Phase-1 code** — factor the post-KISS body into a shared `ingest_ax25`; the KISS callers wrap, the native ones don't. Higher risk; review carefully.
- **Task 7** — `native_driver.rs`: drive the engine from `UvproSession::take_aprs_receiver()` → `handle_inbound_frame` → ACK frames + `tick_frames` via `send_aprs_frame`. The KISS analogue is `engine.rs:408 run()`. Removes the Task-5 dead-code allows.
- **Task 8** — `ui_commands.rs`: capability-gated transport select — UV-Pro/Benshi → native path; generic Classic-SPP TNC → KISS. Also `tuxlink-ve3j` (always-live control strip reuses the same `UvproSession`).
- **Task 9** — **Codex adversarial round** on the fragment math (nx95 skipped its adrev / `tuxlink-bv0b` — do NOT repeat). See CLAUDE.md Codex recipe; output to `dev/adversarial/` (gitignored).
- **Task 10** — e2e mock round-trip (APRS frame → fragment → decode → reassemble → codec recovers callsign+text).

## Worktree state
`worktrees/bd-tuxlink-2f2n-aprs-tactical-chat` — tracked clean, HEAD `04bf370b`, all pushed. Untracked/gitignored on disk: `node_modules/` (pnpm, for the pre-push lint:docs hook), `dev/scratch/*.{png,html}` (mocks), `dev/scratch/benshi-re/` (decompile workspace + toolchain). No at-risk content. **Do NOT dispose** — active until #642 merges.

Agent: sandbar-spruce-savanna
