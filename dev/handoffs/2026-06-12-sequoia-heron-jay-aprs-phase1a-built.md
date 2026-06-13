# Session handoff — sequoia-heron-jay — 2026-06-12

Built **APRS tactical chat Phase 1a (tuxlink-2f2n)** end-to-end via build-robust-features → writing-plans → 14-task subagent-driven-development. The accessibility/power thesis: run VHF tactical APRS chat on the plugged-in Pi (not a battery phone+HT) inside tuxlink's Winlink workspace. **All 14 tasks built, committed, pushed to draft PR #642.** Backend (Tasks 1–11) is **CI-green**; the frontend + a final-review leak fix are verified locally but their CI confirmation is pending a GitHub Actions queue stall (see ⚠️ below).

## ⚠️ Read first — state

- **Branch `bd-tuxlink-2f2n/aprs-tactical-chat`** (worktree `worktrees/bd-tuxlink-2f2n-aprs-tactical-chat`, off origin/main). Working tree clean; all commits pushed. **Draft PR #642.**
- **Plan:** [`docs/plans/2026-06-12-aprs-tactical-chat-phase1a-plan.md`](../../docs/plans/2026-06-12-aprs-tactical-chat-phase1a-plan.md) (2-round adversarial plan review hardened it; 5 blockers fixed pre-build). **Spec:** [`docs/design/2026-06-12-aprs-tactical-chat-design.md`](../../docs/design/2026-06-12-aprs-tactical-chat-design.md) (see its "Phase 1a — BUILT" addendum).
- **No cold cargo on this Pi** — all Rust validated via GitHub CI on the draft PR. Keep that posture.
- **Codex was substituted by Claude self-adrev** throughout (operator standing decision; Codex quota-down). The 2-round plan review + the final code review were self-adrev.

## ⚠️ CI confirmation gap (the one open item before "done")

- **Backend (Tasks 1–11): CI-GREEN** at commit `ad0eccbd` — `clippy --all-targets --locked -D warnings` clean + Rust tests pass, BOTH arches (arm64 + amd64). Verified.
- **Frontend (Tasks 12–14, commit `5039eac0`): verified LOCALLY** — 122 vitest tests pass (8 new aprs + 114 shared-component, zero regressions) + `tsc --noEmit` clean. These are the same gates CI runs; no cold-compile concern.
- **Final-review leak fix (commit `b1a9515a`, `engine.rs` only): NOT yet CI-confirmed.** Small, well-traced change (drains pending → terminal on driver teardown + emits a terminal on encode-failure + a regression test). The risk surface is a clippy lint only.
- **Why pending:** GitHub Actions stopped creating runs for this branch after `ad0eccbd` (13:07). Pushes of `5039eac0`, `b1a9515a`, `2bf40742` did NOT trigger `pull_request: synchronize` runs — a queue/trigger stall, not a code failure (the workflow only runs on push-to-main / pull_request; a background poller `bpy583ydj` was watching for the run to appear). **Next session: confirm CI green on HEAD (`gh pr checks 642`); if still no run, re-trigger** (a fresh trivial push, or `gh pr ready 642 --undo` toggles, or wait for the queue to clear). Do NOT mark PR ready until that clippy gate is green.

## What was built (16 commits: `fdc39d33`…`2bf40742`)

A full APRS-over-KISS messaging stack. New `src-tauri/src/winlink/aprs/` module (7 files) + frontend `src/aprs/` (5 files):

- **T1** `Control::Ui` AX.25 variant (`frame.rs`) — the enum had none; UI frames carry PID 0xF0 + info like I-frames.
- **T2–T4** `message.rs` — APRS message-format codec (encode/parse/ack/rej), **pinned to direwolf `encode_aprs.c`/`decode_aprs.c` + aprslib** (9-char space-padded addressee, ≤67 text, `{msgid`, ack addressed to the original SENDER, lowercase literal, reply-ack tolerated).
- **T5** `identity.rs` + `[aprs]` config — APRS station identity (source CALL-SSID, **tocall=`APZTUX`**, WIDE1-1,WIDE2-1 path) separate from Winlink. *(Patched all 14 `Config{}` literal sites incl. 2 in `tests/` that the plan's grep missed — a CI fix.)*
- **T6** `dedupe.rs` — time-windowed suppressor, injected clock.
- **T7** `framebuild.rs` — APRS UI-frame builder + inbound extractor (reuses `Path::encode`).
- **T8** `tx.rs` — bounded-retransmit TX queue (RADIO-1): +30/60/120s retries, timeout 30s after the last *actual* send (jitter-robust), one-send-per-tick, hard cap 4 sends/msg, concurrent cap 8, single abort.
- **T9** `engine.rs` — `AprsEngine`: promiscuous RX (bypasses the dest-filtering `recv_frame`/SABM-waiting `answer`), **two-window dedupe** (long display window 300s + short auto-ACK throttle 5s = lost-ACK recovery without an ACK storm), REJ terminates retransmit, 4-state delivery.
- **T10** `engine.rs` + `lib.rs` — `AprsState` lifecycle on a **`tokio::task::spawn_blocking` sync driver** (NOT a blocking read on the async executor); synchronous msgid mint + capacity gate; `TauriEventSink` (camelCase events).
- **T11** `ui_commands.rs` + `lib.rs` — six commands (`aprs_config_get/set`, `aprs_listen_start/stop`, `aprs_send`→msgid, `aprs_abort`); real `backend.active_identity().mycall()` resolution; `UiError::Internal` (no `From<String>` exists).
- **T12–T14** `src/aprs/` — `useAprsChat` event hook (optimistic out-bubble only on send success), inline `AprsChatPanel` (4-state chips, listening indicator, no pop-up window), `AprsSettings`, mounted via an `'aprs'` pseudo-folder in `AppShell` (mirrors the contacts nav; avoids touching `menuModel.ts`'s contract test) + Start/Stop toggle.

**Final adversarial review (self-adrev) caught + FIXED one BLOCKER:** `AprsState::send` reserves an in-flight capacity slot, released only via the engine's terminal-state emissions; `stop()`-with-pending and encode-failure leaked slots → eventually wedged `send`. Fix `b1a9515a` drains pending → `TimedOut` on driver teardown + emits a terminal on encode-failure (+ regression test). All other seams reviewed clean (promiscuous RX, dedupe split, ack direction, bounded airtime, spawn_blocking, serde wire forms, no panicking unwrap on RF input).

## Remaining before PR #642 → ready (in order)

1. **Confirm CI green on HEAD** (the leak-fix clippy gate — see the ⚠️ CI gap above; re-trigger if the queue is still stalled).
2. **Operator on-air smoke** (RADIO-1, operator-only — the agent never transmits): UV-Pro Bluetooth KISS on a real APRS frequency. Send to a known station / a second radio; confirm (a) inbound messages land in per-callsign threads, (b) the ACK round-trip flips the outgoing bubble to "Acked", (c) **Stop/abort de-keys cleanly** (the documented watch item). The APRS Chat surface is in the Address section of the sidebar (next to Contacts) — operator may want it relocated; one-line move in `FolderSidebar.tsx`.
3. Mark PR #642 **ready** + merge once 1–2 pass (per no-draft-PR-parking).

## Carried-forward notes (non-blocking, in the spec addendum)
- **UI-frame C-bit:** reuses `Path::encode`'s connected-mode default (dest C=1/src C=0); on-air-correct + ignored by APRS decoders. P2 should add an explicit APRS-intent comment/test so a refactor can't silently flip it.
- **Dedupe msgid reuse:** a peer whose msgid wraps within 300s could have a new message swallowed. 1b robustness note.

## Phase 1b backlog (filed)
`tuxlink-a20f` (multi-transport: managed Dire Wolf + any TNC), `tuxlink-wiww` (channel-monitor + per-SSID/category filtering), `tuxlink-2p8z` (listening-state/BT-host-handoff polish), `tuxlink-f9is` (REJ UX + reply-ack emit). Phase 2 (native Benshi control) + Phase 3 (position/beacon) + Winlink-over-APRS remain separate per the spec.

## Worktree
`worktrees/bd-tuxlink-2f2n-aprs-tactical-chat` — tracked clean, all pushed. Untracked: `node_modules/` (gitignored; installed for the pre-push doc-link hook). No gitignored-stateful content of concern. **Do NOT dispose** — active until PR #642 merges (then ADR-0009 ritual).

Also still open from the prior session: **managed Dire Wolf PR #628** (draft) — its operator on-air smoke (DRA-100 → CDM-1550LS+) + the `tuxlink-sr86` branch-currency item remain.

Agent: sequoia-heron-jay
