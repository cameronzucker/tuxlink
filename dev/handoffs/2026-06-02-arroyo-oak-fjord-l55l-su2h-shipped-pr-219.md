# Handoff: 2026-06-02 — arroyo-oak-fjord session (P2P outbox wiring + Outbox folder, PR #219)

**Agent:** arroyo-oak-fjord
**Session shape:** Single-day fast-follow against larch-clover-delta's 2026-06-02 handoff. Two real bugs blocking actual P2P mail flow on PR #208's protocol layer; both shipped on one PR, cherry-picked off `origin/main` after PR #208 merged mid-session.

## TL;DR

- **PR [#219](https://github.com/cameronzucker/tuxlink/pull/219)** open on `bd-tuxlink-l55l/p2p-outbox-wiring`. Two commits, ~257 LOC including pinned-contract Rust tests.
  - `463892a` `feat(mailbox): enable Outbox folder in FolderSidebar (tuxlink-su2h)` — flips `enabled: false` → `enabled: true`; backend + frontend rendering already wired.
  - `bb3dcbf` `fix(winlink): wire outbox + filing into telnet_p2p_connect (tuxlink-l55l)` — reads Outbox via extracted `build_outbound_proposals` helper, files received → Inbox, moves sent MIDs Outbox→Sent, attaches the shared `SearchService` index.
- **PR #208 (the 0pnb scaffold) was merged before this work started landing.** The handoff anticipated this — fast-follow path materialized rather than the stack-on-same-branch alternative. The original `bd-tuxlink-0pnb/tcp-p2p-telnet` branch is now `merged-dead` per ADR 0017; my work is on a new follow-up branch off `origin/main`.
- **Gate before merging PR #219:** operator P2P smoke against Windows WLE — compose a draft, confirm Outbox sidebar shows it, dial, confirm `sent_count > 0`, confirm Outbox empties + Sent populates. Without this smoke the symptom-vs-bug-fix loop hasn't closed.
- **Out of scope (operator-gated):** tuxlink-native modern transport (per-peer Ed25519 / Reticulum-style identity). Don't start without explicit operator signal.

## What landed this session

### tuxlink-l55l (P1 bug) — outbox + filing wired

`src-tauri/src/ui_commands.rs::telnet_p2p_connect` no longer ships `Vec::new()`. New flow:

1. Resolves `<app_data>/native-mbox` from `app.path().app_data_dir()` (mirrors `bootstrap::install_native`).
2. Constructs `Mailbox`; if `SearchService` is registered, attaches its index Arc so P2P-received messages land in the search corpus alongside CMS-received ones.
3. Calls `crate::winlink_backend::build_outbound_proposals(&mailbox)` to build `Vec<session::OutboundMessage>` from queued `.b2f` files.
4. Logs `"Connecting to <peer> (P2P-Telnet, N queued)…"`.
5. After `Ok(exchange)`: stores each `exchange.received` message to Inbox; moves each `exchange.sent` MID from Outbox to Sent. Filing failures log to the session log but don't fail the exchange (bytes are on disk regardless; worst case is duplicate-send next dial).

`src/radio/modes/TelnetP2pRadioPanel.tsx::start` invalidates `['mailbox']` queries after a successful dial so the operator sees the empty Outbox + new Sent immediately rather than waiting for the 10s `useMailbox` refetch (mirrors `AppShell::onConnect` for `cms_connect`).

### tuxlink-su2h (P2 escalated) — Outbox folder visible

`src/mailbox/FolderSidebar.tsx`: Outbox entry flipped `enabled: false, v01: true` → `id: 'outbox', enabled: true`. Backend path (`mailbox_list` → `parse_folder` → `NativeBackend::list_messages` → `self.mailbox.list(folder)`) was already complete; `useMailbox.BACKEND_FOLDERS` already included `'outbox'`; `MessageList.correspondentLabel` already handled `folder === 'outbox'` (resolves recipient column from `msg.to`). Only the feature flag was holding it back.

### Helper extraction

`crate::winlink_backend::build_outbound_proposals(&Mailbox) -> Result<Vec<session::OutboundMessage>, BackendError>` pulled out of the three inline copies in `native_telnet_exchange` / `native_packet_exchange` / `run_ardop_b2f_exchange`. **The three existing call sites are unchanged** — no DRY pivot. The helper exists so paths bypassing `NativeBackend::connect` (P2P) have one canonical shape to call. Three new unit tests pin the contract: empty outbox returns `vec![]`, two drafts produce two proposals, drafts addressed to third parties are still offered (P2P semantics — no per-peer filter at dial-time).

### Test deltas

- `cargo test --lib`: 668 passed (was 665 baseline; +3 in `build_outbound_proposals_tests`).
- `pnpm exec vitest run`: 729 across 80 files (+1 in FolderSidebar for the new Outbox-click case). One flaky `SettingsPanel > loads current config and checks the matching radios` failure on a second run reproduced once and then green in isolation — unrelated to this work.

## Branch state

- **`bd-tuxlink-l55l/p2p-outbox-wiring`** (origin) — 2 commits ahead of `origin/main`. PR #219 open. Working tree clean.
- **`bd-tuxlink-0pnb/tcp-p2p-telnet`** — merged-dead per ADR 0017. Origin's branch was auto-deleted on PR #208 merge. Local copy lives at HEAD `9c9c3ae`, still present in the worktree's reflog if needed.

## Worktree state

Reused `worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet/` from the larch-clover-delta session. **Worktree path retained even though the in-progress bd issues are now `tuxlink-l55l` + `tuxlink-su2h`** — avoiding worktree churn (disposal ritual + re-clone + re-cargo-build) is the pragmatic call when one feature ships directly into the next. The claim is recorded via `bd remember` (key `tuxlink-l55l-tuxlink-su2h-in-progress-on-worktree`).

`git status` clean. No untracked, no stashes. The handoff doc itself (this file) is untracked in this worktree; the operator can either copy it to main and commit there, or leave it for next session.

Cargo build artifacts in `src-tauri/target/`: ~7 GB. Free for ADR-0009 disposal once l55l/su2h merge.

## Main-checkout state (NOT MINE)

When this session started:
- An interactive rebase was in progress on `task-amd-main-ui` (`git rebase -i` is banned per the destructive-git hook list — this is a different parallel session's state; left alone).
- `.beads/issues.jsonl` staged with prior bd changes (closures from larch-clover-delta + the in_progress flips from this session).
- `dev/handoffs/2026-06-01-bison-condor-grouse-tracks-a-and-b-midflight.md` untracked.
- `dev/handoffs/2026-06-02-larch-clover-delta-p2p-shipped-blocked-on-outbox-wiring.md` untracked.

Did not touch any of it. The rebase in progress is the strongest reason — main checkout is operator/parallel-session state per the standing memory.

## Other worktrees seen

Same set as larch-clover-delta enumerated (didn't disturb): `bd-tuxlink-21j8-state-machine-hooks`, `bd-tuxlink-35g0-help-menu-and-docs`, `bd-tuxlink-7fr-ax25-packet` (note: branch is `bd-tuxlink-jvp/uvpro-setup` — slug/branch mismatch), `bd-tuxlink-8d7y-dev-server-lease`, `bd-tuxlink-8zho-failure-mode-fixtures`, `bd-tuxlink-9phd-strip-pat-add-native-attachments`, `bd-tuxlink-9yx-integration-smoke`, `bd-tuxlink-c22r-themes-presets-and-designer`, `bd-tuxlink-c79g-position-subsystem-restoration`, `bd-tuxlink-hblz-vara-tcp`, `bd-tuxlink-jmfm-radio-panel-400px-controls-relocate`, `bd-tuxlink-jy6p-convergence-adrev`, `bd-tuxlink-mjc8-mailbox-sort`, `bd-tuxlink-pxmi-disposable-worktree`, `bd-tuxlink-qepd-converge-build`, `bd-tuxlink-qxqj-mailbox-bar-redesign`, `bd-tuxlink-ui3i-ci-branch-audit`, `bd-tuxlink-v1p-html-forms-execution`. Plus the local `.local/converge-build-worktree/`.

Several of these are likely in_progress from parallel sessions. `bd list --status=in_progress` showed ~50 issues, far more than one session would claim — the table is busy.

## bd state changes this session

| Issue | Action | Why |
|---|---|---|
| `tuxlink-0pnb` | Closed | PR #208 merged during this session |
| `tuxlink-l55l` | Claimed (in_progress) | Wired outbox + filing into `telnet_p2p_connect`; PR #219 |
| `tuxlink-su2h` | Claimed (in_progress) | Enabled Outbox FolderSidebar entry; PR #219 |

`bd remember` entry filed for the worktree claim mismatch.

bd state has NOT been pushed via `bd dolt push` yet (deferred to operator session-end since the main-checkout has uncommitted bd changes from prior sessions and a rebase in progress — `bd dolt push` from a worktree should be safe but I'm not touching that surface without operator signal given the active rebase).

## What's next

1. **Operator P2P smoke gate.** PR #219 should NOT merge until the operator confirms end-to-end on Windows WLE — the protocol layer was smoked under #208, but the outbox-wiring path is fresh code that hasn't yet had a real exchange flow drafts through it.
2. **Modern-transport spec** (operator-gated). Larch-clover-delta's handoff captured the chosen direction: per-peer Ed25519 pubkeys / Reticulum-style identity-from-key / no TLS-wrap of telnet P2P. Don't start without explicit operator signal.
3. **Main-checkout cleanup** (operator). The interactive rebase needs the operator's attention. The two untracked handoff docs (larch-clover-delta's + mine, if you copy mine over) want committing.

## Smoke walkthrough (operator)

```bash
# 1. Make sure no other tauri dev is bound to :1420.
ss -tlnp | grep ":1420"  # should be empty, or kill the holder

# 2. Build + run from the l55l worktree.
pnpm -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-0pnb-tcp-p2p-telnet tauri dev

# 3. In tuxlink: compose a draft, confirm it lands in the now-visible Outbox folder.
# 4. Switch to the P2P Telnet panel, dial your Windows WLE (default 127.0.0.1:8772 if local;
#    otherwise the WLE host). Confirm session log says "Connecting to ... (N queued)…".
# 5. After exchange: Outbox empties, Sent gains the row, the dial result shows sent_count > 0.
# 6. If WLE pushes anything back, Inbox gains it (and the search index too — try ⌘F).
```

If the smoke is clean: merge #219 via squash/merge per project convention. If anything is off: file a follow-on bd against l55l with the symptom + which step failed.
