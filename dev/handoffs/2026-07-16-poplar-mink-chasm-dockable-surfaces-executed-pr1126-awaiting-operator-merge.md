# Handoff — Routines plan 6 (dockable surfaces): EXECUTED, PR #1126 CI-green, awaiting operator ready+merge

- **Agent:** poplar-mink-chasm
- **Date:** 2026-07-16
- **Ended:** natural gate — merge authority reserved to the operator (harness classifier + operator's own "I'll drive it manually at release" framing).

## READ THIS FIRST — where things stand

1. **The full 13-task SDD execution of the dockable-surfaces plan is DONE.** Every task implemented by a fresh subagent, task-reviewed, and covered by four batch review loops (14 rounds; each loop ran until a clean round). Final whole-branch review ran clean after one catch. Full local gates green: typecheck, `pnpm vitest run` 4523/4523, `pnpm build`, `pnpm lint:docs`. WebKitGTK render smoke: 15 clean PNGs.
2. **PR [#1126](https://github.com/cameronzucker/tuxlink/pull/1126)** — branch `bd-tuxlink-dmwte/dockable-surfaces`, head `5f7aea17`, **CI ALL GREEN by head SHA** (CI + ECT low-floor + Release build, both arches). Two CI fix-forward rounds were needed: (a) E0063 — the two full-`Config` literals in `src-tauri/tests/` missed the new `dock` field (Task 2's sweep covered `src/` only; `--all-targets` compiles tests); (b) the docs-registry guard test — `BUNDLED_TOPICS` (Rust search index) is a parallel registry to `topics.ts` and Task 12 updated only the latter.
3. **The PR is still DRAFT.** The harness permission classifier denied both `gh pr ready` and `gh pr merge` (agent-authored PR + the operator's reserved manual validation). **Operator action: mark ready + merge (merge commit — no squash, ADR 0010).** No agent should attempt to merge it.
4. **Wire-walk + live multi-window pass are DEFERRED by explicit operator decision** ("can't really wire-walk this... I'll drive it manually when we cut a new release"). The live-pass checklist is in the PR body. Until the operator's release-time drive, the feature is merged-but-not-operator-validated; `bd tuxlink-dmwte` should stay open (or be closed with a note) per the operator's preference at merge time.

## Notable catches during execution (details in PR body + ledger)

- Task 5: the plan's topology note missed the second production driver loop (`run_native`) — echo now emits from both.
- Loop 2: close-intent 1.5 s fallback needed a per-surface pop-generation guard (close→re-pop race destroyed the fresh window).
- Loop 4: the APRS Chat pop-out **entry point was absent from the plan entirely** (spec §5 mandates it) — the whole chat pop path was unreachable until round 3 caught it.
- Task 11 (WebKitGTK smoke): popped chat window **crashed to blank** (missing `HintProvider` on the `/pop` route) — now unit-regression-guarded.
- New `aprs_status` command (backend truth for the live transport) — disconnect works from any window across pop/close/reopen.
- ChatStrip ships the real `countUnread`; the plan's "unread placeholder" was dropped under the no-stubs rule.

## State

- **bd:** `tuxlink-dmwte` in_progress (notes updated with PR + remaining operator gates). The connect-sequence-dedup task I filed mid-session was RESOLVED in-branch (Task 10 Rider A) — close it (`bd list --status=open | grep -i "useAprsConnectSequence"`). Still open: usePacketConfig cross-window sync gap (P3, filed this session); stations.json `core:event:allow-emit` gap (P2, filed by the spec session — the final review independently re-confirmed it).
- **Worktrees:** `worktrees/bd-tuxlink-dmwte-dockable-surfaces` (KEEP until merge lands — clean, pushed, head `5f7aea17`; gitignored state: `.superpowers/sdd/` briefs/reports/ledger — the ledger `progress.md` is the full execution record — plus render PNGs and node_modules). `worktrees/bd-tuxlink-dmwte-handoff-exec` (this handoff's vehicle — disposed per ADR 0009 after push; if found alive, disposal was interrupted: inventory → rm → prune).
- **Main checkout:** untouched all session (another live session holds it).
- **No stashes.** Private-journal MCP never connected this session (journal updates skipped, noted in ledger).
- **Operator gates pending:** (1) ready+merge PR #1126; (2) release-time manual wire-walk + live multi-window pass (checklist in PR body; dry-run only, no transmission); (3) memory re-measure via `dev/measure-webview-marginal-memory.py` during the live pass — record the map number into `docs/user-guide/38-pop-out-windows.md`.

## Watch items for the live pass (from the reviews, non-blocking)

- Whether `tauri-plugin-window-state` flushes pop-window geometry when a window leaves via `destroy()` mid-session (may only flush at exit) — the quit/relaunch restoration step will show it.
- xdg-activation focus/raise on labwc (spec §5 STOP-condition class — validated only by the live pass).
- Two pre-existing load-flaky vitest files (ConsentGate "Keep parked", AppShell.aprs map-close) — RCA'd as timing, not regressions; don't chase if they fire in CI.
- Latent, human-unreachable: restart-clobber window (~100 ms) on the shared transport cell; ChatStrip 1-frame unread flicker.
