# Handoff: 2026-06-02 — three plumbing PRs (sort UI, menu disabled-badging, react-query refactor) — bluff-birch-cove

**Agent:** bluff-birch-cove (this session)
**Predecessor:** alder-gully-basalt (VARA Phase 2 end-to-end + 7 fast-follows, see [2026-06-02-alder-gully-basalt-vara-phase-2-end-to-end-shipped.md](2026-06-02-alder-gully-basalt-vara-phase-2-end-to-end-shipped.md))
**Session shape:** Short, decisive. Operator opened with three priorities: read alder-gully-basalt handoff, then chip `tuxlink-2x0l` (MessageList sort UI). Done. Then per the decisive-autonomous-execution discipline, chipped two more plumbing-grade bd issues that didn't need operator decisions or RF-path gates.

## TL;DR

| PR | bd issue | Topic | State |
|---|---|---|---|
| [#244](https://github.com/cameronzucker/tuxlink/pull/244) | tuxlink-2x0l | MessageList sort UI — Phase 2 of mailbox-sort (operator-selectable sort with persistence) | OPEN |
| [#245](https://github.com/cameronzucker/tuxlink/pull/245) | tuxlink-dpf | Menu items — mark unwired Message/Session items disabled+badged | OPEN |
| [#247](https://github.com/cameronzucker/tuxlink/pull/247) | tuxlink-i9vn | useStatusData → react-query so T14 invalidate actually refetches | OPEN |

All three branched off `origin/main`. No cross-PR dependencies. All vitest + tsc green at push time. All three explicitly note "operator browser-smoke recommended before merge" because the converge-build dev server on :1420 was held by yesterday's session and the auto-mode classifier correctly refused to kill it.

## Sequence and detail

### PR #244 — MessageList sort UI ([tuxlink-2x0l](https://github.com/cameronzucker/tuxlink) — P2)

Fast-follow to `tuxlink-mjc8` / PR #201 (backend deterministic ordering). The backend already returns date-desc; this adds an operator-facing sort affordance.

- **5 files**: new `src/mailbox/messageSort.ts` (pure helper — `SortMode` enum, `compareMessages`, `sortMessages`, `loadSortMode`/`saveSortMode`), new `src/mailbox/MessageListSortControl.tsx` (native `<select>` + label), `src/mailbox/MessageList.tsx` (accepts `sortMode` + `onSortModeChange` props; sorts client-side before Virtuoso), `src/shell/AppShell.tsx` (owns state, lazy-inits from localStorage), `src/shell/AppShell.css` (additive `.rows-pane-header` + `.message-list-sort` selectors, no overrides).
- **Folder-aware sender key**: sent/outbox sort by recipient (the first `to` entry — what the row actually shows), everywhere else by sender. Matches `correspondentLabel` in MessageList.tsx.
- **35 new tests** (27 helper + 4 control + 4 integration). 900 vitest pass overall.
- **localStorage key**: `tuxlink.messageList.sortMode`. Lazy-init on mount → no flash of default.
- **Design choice**: dropdown header vs clickable column headers — rows are 3-line grids, not tabular, so column-header semantics don't fit (per bd issue body).

### PR #245 — Dead-stub menu items ([tuxlink-dpf](https://github.com/cameronzucker/tuxlink) — P2, bug from 2026-05-22)

`MenuBar` rendered every item as an enabled button even when `dispatchMenuAction` had no handler. The dispatcher's safe-no-op default caught the fallthroughs, but the buttons "looked broken on click."

- **2 files**: `src/shell/chrome/menuModel.ts` (mark 5 items `disabled: true`), `src/shell/chrome/MenuBar.test.tsx` (5 new `it.each` cases).
- **Items disabled**: `menu:message:print`, `menu:session:disconnect`, `menu:session:log`, `menu:session:verify_cms`, `menu:session:show_transport`.
- **`dispatchMenuAction` unchanged**: the menu:* vocabulary stays in `MENU_ACTION_IDS` (parity test still passes), only the rendering changes. About / Documentation / Report Issue from the original bug report were already wired by tuxlink-35g0; Preferences was already removed; Tools/Templates / Tools/Rig Control / Tools/Connection were already disabled.
- **Ctrl+P accelerator binding left in place**: removing it would risk WebKitGTK falling back to its native print dialog (worse UX than the current invisible no-op). Disabled rendering skips the accel hint anyway, so the user doesn't see "Ctrl+P" promised.

### PR #247 — useStatusData → react-query ([tuxlink-i9vn](https://github.com/cameronzucker/tuxlink) — P2)

Track A T14 (`tuxlink-c79g`) added `queryClient.invalidateQueries({ queryKey: ['config_read'] })` in DashboardRibbon after `onCommit` + `onUseGps` — but `useStatusData` polled `config_read` via raw `useState` + `useEffect` + `setInterval(load, 5000)`, not via `useQuery`. The invalidate had no real refetch target; the source chip + grid display still lagged up to 5s after a user action.

- **2 files**: `src/shell/useStatus.ts` (three useQuery polls; `STATUS_QUERY_KEYS` exported), `src/shell/status.test.ts` (9 renderHook sites wrapped via new `renderUseStatusData` helper; `setTimeout(0)` + `act` → `waitFor`).
- **3 queries**: config_read 5s, backend_status 2s, position_status 2s. All `enabled: !DEV_FIXTURE`; `retry: false` (matches App.tsx default).
- **`listen('backend_status:change')` event path preserved**: it now `setQueryData(STATUS_QUERY_KEYS.backend, payload)` so listener + 2s refetch write to the same source of truth. The 2s poll stays as a snapshot backstop in case events drop.
- **No behavior change** for the consumers (DashboardRibbon, StatusBar, GridEdit) — same `StatusBarData` contract. The win is that T14's invalidate now actually refetches.
- 865 vitest pass, tsc clean.

## What didn't land — open issues + filed follow-ups

No new bd issues filed beyond the handoff-tracking one (`tuxlink-o1j5` — for this worktree's bookkeeping). Three priority slices remain in `bd ready`:

- **RF-path P1s** (`tuxlink-9ky` BT Page-Timeout, `tuxlink-0ja` disarm TOCTOU, `tuxlink-5vx` AX.25 P4 inline Radio UI, `tuxlink-7fr` AX.25 1200-baud transport) — all need operator on-air verification. Per RADIO-1 and the `feedback_rf_path_scope_filter` memory, agent work needs operator green-light + smoke plan before claiming.
- **`tuxlink-fzl7`** (VARA Phase 3) — explicitly deferred by operator per the alder-gully-basalt handoff ("VARA is closed for now").
- **`tuxlink-edvb`** (convergence discipline) — operator-driven infra cluster.
- **`tuxlink-ylra`** (Position CSS polish) — explicit operator-smoke gate ("walk through all 5 states") that I couldn't run this session.
- **`tuxlink-sox`** (Packet panel transport segment reset) — RF-path packet panel; diagnosis says "verify, do not assume."
- **`tuxlink-7gb`** (docs/development.md refresh) — operator decision needed on scope (file doesn't exist; per peregrine-maple-thistle note).
- **`tuxlink-ca5x`** (Archive folder) — needs backend storage + move-op design first; not plumbing.
- **`tuxlink-f62f`** (Custom user folders) — same shape as ca5x; bigger surface.

## bd state at handoff

- **Claimed this session, PR open:** tuxlink-2x0l, tuxlink-dpf, tuxlink-i9vn
- **Filed for handoff bookkeeping:** tuxlink-o1j5 (this branch's worktree claim)
- **Untouched by this session:** all P1 RF-path issues, VARA P3, the convergence-discipline cluster, etc.

## In-flight worktrees at handoff

| Worktree | bd issue | State |
|---|---|---|
| `worktrees/bd-tuxlink-2x0l-message-list-sort-ui` | tuxlink-2x0l (PR #244 open) | active, push complete |
| `worktrees/bd-tuxlink-dpf-dead-stub-menu-items` | tuxlink-dpf (PR #245 open) | active, push complete |
| `worktrees/bd-tuxlink-i9vn-usestatus-react-query` | tuxlink-i9vn (PR #247 open) | active, push complete |
| `worktrees/bd-tuxlink-o1j5-session-handoff` | tuxlink-o1j5 (this handoff) | active, push imminent |

All four are clean (committed + pushed). No untracked / gitignored-stateful content beyond per-worktree `node_modules` + `dist` (gitignored). After all three PRs land, dispose them via the ADR 0009 ritual.

Inherited from alder-gully-basalt's session (already enumerated in the predecessor handoff as disposable):
- `worktrees/bd-tuxlink-dfmf-vara-phase-2-ui` (closed)
- `worktrees/bd-tuxlink-2bp0-strip-vscode` (closed)
- `worktrees/bd-tuxlink-3inw-vara-banner-shorten` (closed)
- `worktrees/bd-tuxlink-6dzo-remove-loading-state` (closed)
- `worktrees/bd-tuxlink-poh6-vara-start-handler-guard` (closed)
- `worktrees/bd-tuxlink-rsus-vara-mycall-and-logs` (closed)
- `worktrees/bd-tuxlink-6qgn-session-handoff` (alder-gully-basalt's handoff branch, push complete)

Many other older worktrees still exist in `git worktree list`. Not mine to dispose.

## Main checkout state at handoff (operator state, NOT mine to fix)

`/home/administrator/Code/tuxlink` is at detached HEAD `5c09717` with an **in-progress interactive rebase** of `task-amd-main-ui` onto `dea086f` (10 commands done, 7 remaining, all conflicts marked fixed — waiting on `git rebase --continue`). Per memory `feedback_main_checkout_is_operator_state` I did not touch it.

Per memory `feedback_no_pr_for_handoffs` — handoffs should commit directly on the operator's current branch, not a feature branch. The main checkout's rebase state made that impossible this session, so this handoff lives on its own branch (mirroring alder-gully-basalt's same workaround). The operator can:
1. Finish the rebase on `task-amd-main-ui` (or abort + leave it) at their convenience.
2. Cherry-pick / merge this handoff doc into the main branch when ready.
3. Or simply read it via `git show origin/bd-tuxlink-o1j5/session-handoff:dev/handoffs/2026-06-02-bluff-birch-cove-three-plumbing-prs.md` without needing to merge.

Untracked files in the main checkout (from prior sessions, NOT mine):
- `dev/handoffs/2026-06-01-bison-condor-grouse-tracks-a-and-b-midflight.md`
- `dev/handoffs/2026-06-02-larch-clover-delta-p2p-shipped-blocked-on-outbox-wiring.md`
- modified `.beads/issues.jsonl` (operator state)

## Anti-patterns I successfully avoided (carried forward from alder-gully-basalt)

The alder-gully-basalt handoff enumerated four anti-patterns. Reviewing mine against them:

1. **"Don't claim mirror X exactly while adding fields the source doesn't have."** I didn't claim to mirror any existing hook precisely; I built new modules (`messageSort`, `MessageListSortControl`) and converted an existing hook (`useStatusData`) to a different paradigm (react-query). For the conversion I preserved the exact public contract (`StatusBarData` shape, pre-load semantics, error semantics) and called out behavior-change vs preservation explicitly in the PR body.

2. **"When removing a gating predicate, grep for ALL sites that gate on it."** Most relevant to the dpf PR — for each disabled menu item I cross-checked: (a) the dispatchMenuAction switch (still has no handler — safe no-op), (b) MENU_ACTION_IDS parity (vocabulary unchanged), (c) ACCELERATORS bindings (Ctrl+P binding left intentional, documented in commit body). The convention-following nature of the change kept this cheap.

3. **"The bash cwd reverts silently (memory `feedback_pin_paths_in_worktree_sessions`)."** Hit during my session. After dpf-worktree's `pnpm vitest run`, a follow-up `grep` ran from the wrong cwd and gave stale output. Recovered by using absolute paths in the next grep (`grep -n ... /home/administrator/...`). Lesson reinforced: pin absolute paths or use `pnpm -C`, `git -C` flags.

4. **"Don't ship banners as documentation."** Not relevant — none of my three changes added banners. The sort UI added a 1-line header above the list (a control, not an info banner). The menu fix changed existing items' rendering style. The status hook refactor was zero UI surface.

## What the operator should do on wake

Three PRs are ready for review/merge. None depend on each other; merge in any order. Each PR body includes a suggested operator-smoke flow:

- **#244 (sort UI):** Open Inbox → see "Sort: Newest first ▾" above the list. Change to Subject A→Z, reload, switch folders. Should persist across folder changes and across reload.
- **#245 (dead stubs):** Open Message + Session menus, confirm Print + Disconnect + Session Log + Verify CMS + Show transport now show "soon" badges and are unclickable.
- **#247 (react-query):** Edit grid in Settings panel, confirm the dashboard ribbon source chip + grid update **without** the prior up-to-5s lag. This is the only PR with a behavior change to observe.

If the operator has bandwidth for more plumbing-grade work after review, the next candidates in `bd ready` are listed above under "What didn't land." Stay clear of RF-path P1s without a green-light + smoke plan (per `feedback_rf_path_scope_filter`).

---

Agent: bluff-birch-cove
