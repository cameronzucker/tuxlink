# 2026-06-10 marsh-lupine-isthmus — 0.44.0 bug triage (#1 fixed, #2/#3 filed) + smoke-walk tracker reconcile

## What happened this session

1. **6c9y post-merge cleanup** — disposed the dead `worktrees/bd-tuxlink-6c9y-telnet-post-office`
   worktree (ADR 0009 ritual; ~30G reclaimed; design scratch archived to
   `.claude/worktree-archives/`). Recovered 3 orphaned handoff docs (committed + pushed on
   `bd-tuxlink-xygm/recover-handoffs`, commit `6e42303`).

2. **6/6 smoke-walk tracker reconciliation** — the tracker was badly under-reporting: bd
   `open`/`in_progress` ≠ unimplemented (sessions merge PRs without closing issues). Verified all 40
   smoke-walk findings against **landed code on origin/main** (`git log origin/main --grep`), then
   closed **12 merged-but-stale** issues: `bsiy 0gsy 2hyf 2x0l mjc8 asa7 4or5 9ylw h7q7 ligz qxqj yt2g`.
   ~37 of 40 findings are now implemented; the rest are net-new feature scope (maps/GPS/packaging).
   Lesson saved to auto-memory: `feedback_bd_status_underreports_completion`.

3. **Three new 0.44.0 bugs reported by operator** → routed through `investigate`, root-caused each
   (3 parallel Explore agents against origin/main, since the local checkout is ~1006 commits behind):

   - **#1 Mermaid flowcharts oversized + clipped text** → `tuxlink-3xnf` (P2 bug). **FIXED, PR #558.**
   - **#2 Favorite station list has no edit/delete/rename UI** → `tuxlink-oi1g` (P2). **FILED.**
   - **#3 VARA HF/FM pane has no station-target input or favorites (can't dial VARA from UI)** →
     `tuxlink-xglf` (P1). **FILED.**

## #1 Mermaid — SHIPPED to PR #558 (awaiting CI + operator grim-smoke)

- **Root cause (corrected from the first pass):** NOT missing `useMaxWidth`, NOT a version bump, NOT
  the recent theming commits. Mermaid v11 emits `<svg width="100%" style="max-width:Npx" viewBox="0 0 W H">`.
  Chromium honors that → natural size; **WebKitGTK** (production webview) stretches `width="100%"` to
  the full pane + scales height off the aspect ratio (enormous) and clips foreignObject node labels.
  The existing `mermaid-probe` renders in Chromium, which is why the bug was invisible to it
  (Chromium is not a WebKitGTK proxy).
- **Fix:** `normalizeMermaidSvgSize()` in `src/help/useMermaidRender.ts` pins explicit intrinsic
  `width`/`height` from the viewBox + drops inline `max-width`; CSS `max-width:100%; height:auto` still
  downscales on narrow panes. JS-only, minimal diff, no-op in Chromium. Regression test added.
- **Gates:** typecheck clean, `vitest src/help/` 134/134. **CI (full vitest both arches) is the merge gate.**
- **⚠️ STILL OWED:** operator **WebKitGTK grim-smoke** — open a docs topic with a flowchart, confirm
  sane size + full node text. jsdom/Chromium cannot reproduce the bug, so this is the only real visual
  confirmation. (Per project norm: ship on automated gates, smoke post-merge / fix-forward.)

## #2 / #3 — FILED for the follow-up session (root causes in the bd issue bodies)

- **`tuxlink-oi1g` (#2, P2):** RF favorites (ARDOP/Packet/Telnet) are read + star-to-promote ONLY —
  no delete/rename/reorder/edit UI; backend `favorite_delete`/`favorite_upsert` exist but are unwired.
  Only explicit create path is Catalog Builder (Tools menu → buried). Network PO favorites are
  add/remove only (no edit-in-place; `network_po_favorites_set` exists, frontend never calls it).
- **`tuxlink-xglf` (#3, P1, highest-impact):** `src/radio/modes/VaraRadioPanel.tsx` renders ONLY
  transport Start/Stop — no target-callsign input, no FavoritesTabs, no Send/Receive. ARDOP/Packet/Telnet
  got the Phase-3 connect+favorites integration; VARA HF/FM never did. Backend
  `modem_vara_b2f_exchange(target,...)` is ready and waiting. Net: you cannot dial Winlink over VARA
  from the app. RADIO-1 bar applies to the dial path (bounded airtime + working abort; agent writes/tests,
  operator runs on-air per ADR 0018).

**Both #2 and #3 are UI work → brainstorm/design pass FIRST per project norms** (don't jump to code).
#3 is the bigger, P1 item.

## State at handoff (re-verified)

- **Operator main checkout:** `bd-tuxlink-xygm/recover-handoffs`, ~1006 behind / 31 ahead of origin/main
  (routinely stale — read code via `git show origin/main:<path>`; this handoff commits here per the
  no-PR-for-handoffs convention).
- **In-flight worktree:** `worktrees/bd-tuxlink-3xnf-mermaid-sizing` (branch `bd-tuxlink-3xnf/mermaid-sizing`,
  PR #558 open). Tracked-clean after commit `6f95658`; untracked: none; gitignored-on-disk: `node_modules/`
  (fresh pnpm install) + no `target/` (frontend-only). **Do NOT dispose** until PR #558 merges (ADR 0009).
- **bd:** `3xnf` in_progress (PR #558). `oi1g`/`xglf` open, unclaimed. The 12 reconciled closures persisted
  in local Dolt.

## Next session — START HERE

Claim `tuxlink-xglf` (#3 VARA HF dial, P1) first — biggest gap. Brainstorm the pane (mirror ARDOP/Packet
+ RADIO-1 abort) BEFORE coding. Then `tuxlink-oi1g` (#2 favorites edit). Both off a fresh worktree from
origin/main. Mermaid PR #558 just needs CI + the operator's grim-smoke.
