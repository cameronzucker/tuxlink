# 2026-06-10 maple-isthmus-willow — 4 shipped: l80q bulk-actions, version-wiring, area-weather rendering, README hero. Next: GitHub page polish.

Long session, four merged PRs, no open work-in-progress. Next session is fresh
issues (operator: "maybe more github page polish").

## What shipped (all merged to main)

1. **tuxlink-l80q** — Inbox multi-select bulk actions. Selection-aware context
   menu (right-click a selected row acts on all selected; unselected resets to
   that row) + bulk Archive + Move (bar + menu) via shared handlers + Rust
   `message_move_bulk`. TDD; Codex round caught 3 P2 incl. a **self-move
   data-loss bug** (guarded `move_between` primitive too). **PR #515**. Delete
   deferred to **tuxlink-2tg5** (still OPEN, P3).

2. **tuxlink-1k3x** — Release artifacts + Winlink handshake were stuck at
   **0.0.1** (release-please only bumped `version.txt`; never wired to
   `tauri.conf.json`/`Cargo.toml`/`package.json` — the ADR 0005 "Task 19" that
   never landed). Added release-please `extra-files` updaters (json + toml incl.
   Cargo.lock) + a `version-consistency` verify test. **PR #529**. Operator
   retargeted the catch-up to **0.42.0** mid-PR (main had cut v0.42.0). Watch the
   first post-merge release PR confirms the Cargo.lock toml-filter updater bumps.

3. **tuxlink-qyjr** — NWS area-weather catalog-reply rendering. Replies decoded
   only into a 3-field header + raw `<pre>`. Now: **SFT** (Tabular State Forecast)
   → a forecast table (locations × days: condition / lo-hi / precip); **ZFP**
   (Zone Forecast Product) → zone sections with Title-Cased period rows. TDD
   against the operator's real inbox replies. Codex round → 3 fail-closed fixes
   (title-gated parser, malformed-block fail-closed, dual-issued-time). **PR #561**.
   Follow-up: other NWS sub-products (AFD discussions, METAR…) still render
   header+raw — the framework makes each a small parser.

4. **tuxlink-7ygy** — README hero. Codex's screenshot refresh had shipped a
   jumbled, empty-mailbox hero (the harness returned `mailbox_list → []` and a
   stale `session_log_snapshot` fixture in the wrong shape that **crashed the
   render** when a radio dock mounted). Rebuilt: populated EmComm mailbox + an
   ICS-213 in the reading pane + the **active VARA HF modem dock** (config /
   Connect / live session log). Operator nitpick → fixed a **real desktop CSS
   bug**: the On-Connect Review/Download-all segments wrapped (no
   `white-space: nowrap`) into touch-proportioned chips. **PR #585**.

## Diagnoses that were NOT code bugs (don't re-chase)

- **"Installer broken" P0** → operator mistake: stale `~/.local/share/applications/*.desktop`
  from a prior dirty uninstall + an install-the-wrong-package. The dual-icon was
  already fixed on main (`fcc4926` / tuxlink-mpds). See
  [[project_main_checkout_often_stale]] — the **main checkout sits on
  `bd-tuxlink-xygm/recover-handoffs`, ~900+ commits behind origin/main**; a
  "regression" debugged off that stale tree can be a phantom. Read main via
  `git show origin/main:<path>`.

## Repo / worktree state at session end

- Main checkout on `bd-tuxlink-xygm/recover-handoffs`, in sync with origin (before
  this handoff). Working tree carried `.beads/issues.jsonl` (this session's closes)
  + a CONCURRENT session's staged handoff (`arroyo-lichen-grouse-…request-center…`)
  — committed together here; left their content untouched.
- All four of this session's worktrees disposed (l80q, 1k3x, qyjr, 7ygy). Other
  sessions live (arroyo-lichen-grouse, qjgx-alpha-logging, …).
- New memories: [[feedback_priority_is_not_emergency]], [[project_main_checkout_often_stale]].

## Open / follow-ups (none block anything)

- **tuxlink-2tg5** (P3, OPEN) — Message Delete/Trash. Net-new UX → office-hours
  brainstorm FIRST (retention/restore/empty model), then build; reuses l80q's
  bulk-command pattern.
- Area-weather: AFD / METAR / other NWS products → per-product parsers (small).
- **tuxlink-mxui** (carryover) — wire `converge_build_fixtures` into CI.
- README screenshot harness is now solid for more page polish: `dev/readme-screenshot-harness/`
  supports `?view=shell&dock=vara`, has a render-error boundary, and the README
  documents the **IPv4-bind gotcha** (`pnpm dev -- --host` mangles the flag →
  binds `[::1]`; use `pnpm exec vite --host 127.0.0.1 --port 1420 --strictPort`)
  and the **cold-vite warm-up** (snapshot.py's 20s safety vs first-load compile —
  run one throwaway snapshot to warm the module cache).

## Operator next-session starting prompt

```
Continue tuxlink — fresh issues (likely GitHub page / README polish). Last
session shipped 4 PRs (l80q bulk-actions, version-wiring, area-weather rendering,
README hero). READ the handoff first:
dev/handoffs/2026-06-10-maple-isthmus-willow-l80q-version-catalog-hero-shipped.md

For README/screenshot polish: the harness is dev/readme-screenshot-harness/
(supports ?view=shell&dock=vara). Two gotchas in its README that WILL bite:
bind vite IPv4 explicitly (`pnpm exec vite --host 127.0.0.1 --port 1420
--strictPort`, NOT `pnpm dev -- --host`), and warm vite with one throwaway
snapshot before the real capture (20s safety vs cold compile). The main checkout
is on a stale handoff branch ~900 commits behind main — read truth via
`git show origin/main:<path>`. Any net-new UI feature needs a brainstorm first.
```
