# Handoff: 2026-06-03 — UI perf sweep + help-window bug + worktree cleanup — bluff-birch-cove

**Agent:** bluff-birch-cove (continuation session — context reset earlier; full prior arc spanned WLE-CMS-request sprint + sort UI polish + search fixes)
**Supersedes:** [2026-06-03-bluff-birch-cove-wle-sprint-shipping-done.md](2026-06-03-bluff-birch-cove-wle-sprint-shipping-done.md)
**Session shape:** Long. Picked up post-compaction with the WLE-sprint shipping-done + GRIB merge (PR #288); then operator asked for cold-start improvements; that turned into a 6-PR UI perf sweep; then a Codex-caught bug fix; then a layout regression revert; then a P0 idle-CPU bug (theme broadcast loop in the new help window); then help reading-column width preferences; then worktree cleanup.

## What shipped this session

### 10 PRs merged (chronological)

| PR | Slug | Headline |
|---|---|---|
| #293 | tuxlink-0fyj | MessageView attachment Save As — closed the inbound half of the GRIB feature so binary attachments can actually be extracted |
| #297 | tuxlink-k0q3 | Cold-start: pre-paint skeleton + theme bg in `index.html` + 11 lazy panels |
| #301 | tuxlink-01vd | Cold-start follow-up: packaged-CSP-blocked inline script fixed (restored `applyColorScheme` in `main.tsx`) + custom-theme `--bg` honored at boot |
| #305 | tuxlink-sndh | Kill 4Hz modem-status render storm via `useModemIsActive()` selector + `React.memo` on `MessageRow` + scope clock tick to `ClockCell` + memo `AppShell.counts` |
| #314 | tuxlink-twym | Lazy-load 5 radio panels + 2 search overlays. Main bundle 518→472 kB (-46 kB raw / -12 kB gz) |
| #316 | tuxlink-djnl | `useStatusData` `useMemo`'d return + `React.memo` on `DashboardRibbon` + `StatusBar` |
| #323 | tuxlink-u8z7 | Lazy-split `MessageView` (off the forms registry) + `React.memo` on `FolderSidebar`. Bundle 472→452 kB |
| #327 | tuxlink-268k | Codex adrev follow-ups: row-date staleness fix + `FolderSidebar` callback-stability fix + `MessageViewLoading` fallback (was visually wrong copy) |
| #329 | tuxlink-40u8 | UI: revert tuxlink-8rng's mailbox-list shrink — radio panel column now takes its 400 px from the 1fr reader only |
| #338 | **tuxlink-och6** | **P0 — break infinite theme-broadcast loop pegging WebKit + Rust at idle when help window is open** |
| #342 | tuxlink-d7a7 | Help reading-column: default to Wide + bump Wide 980→1280 px |

### Headline outcomes

- **Cold-start** (pre-paint skeleton + 11 lazy chunks + main-bundle code-split): on Pi5 the window paints Tuxlink-shaped chrome ~30-50 ms after launch instead of 1-2 s of white-flash. Operator-confirmed.
- **Bundle delta** (vite production build, cumulative): main JS **518.56 → 452.61 kB** (-66 kB / -16 kB gz, 12.7% reduction), main CSS 62.12 → 47.35 kB (-15 kB / 24%), plus 14 lazy chunks totaling ~165 kB JS + ~33 kB CSS moved off the cold-start critical path.
- **Render storms killed**: ARDOP 4 Hz broadcaster, 2 s status polls, 1 s clock tick — all dedupe / scope / memo'd so AppShell doesn't ripple them through the shell.
- **P0 idle-CPU bug** (PR #338): help window's `useHelpTheme` listener called `applyColorScheme()` which broadcasts `color_scheme_changed`, which Tauri delivers to all windows including sender, which re-fires the same listener — infinite loop. Pegged both WebKitWebProcess + Rust backend at 40-70% CPU idle whenever the help window was open. Fix: `applyColorScheme(scheme, { broadcast?: boolean })` option, default `true`; help-window callsites pass `false`.
- **Help reading width**: default flipped to Wide, Wide bumped 980→1280 px for better 1080p use.
- **Worktree disk cleanup**: 57 worktrees disposed, 122 GB reclaimed (546→424 GB used; 51% disk usage from 65%).

## bd state at handoff

**Closed this session** (all from the PRs above): tuxlink-0fyj, tuxlink-k0q3, tuxlink-01vd, tuxlink-sndh, tuxlink-twym, tuxlink-djnl, tuxlink-u8z7, tuxlink-268k, tuxlink-40u8, tuxlink-och6, tuxlink-d7a7.

**Plus carry-forward from earlier in the same agent session (pre-compaction)**: the WLE CMS-request parity sprint sub-issues (tuxlink-2u4n + tuxlink-ddiq + tuxlink-vrpk + tuxlink-v8ee + tuxlink-tkdc + tuxlink-73ox) all closed via PRs #282, #283, #286, #288 — see the prior handoff for details.

**Filed but not claimed** (queued from Codex adrev "what to try next" round):
- Hoist `useSearch` per-keystroke storm into a `SearchZone` subtree (bigger refactor — would eliminate the per-keystroke shell re-render that the memo'd children currently bail on but AppShell itself still pays).
- Coalesce mailbox polling into a `mailbox_summary` backend command (5 folder × 10s polls → 1 summary command).
- Lazy-init backend `SearchService` (defers SQLite open past first paint on Pi5).
- Inter font swap to system UI on Pi (design call).

None are filed as bd issues yet — operator can decide which to prioritize.

## In-flight worktrees at handoff (post-cleanup)

15 worktrees remain on disk (~31 GB total):

| Worktree | Branch | Why kept |
|---|---|---|
| `bd-tuxlink-2x0l-message-list-sort-ui` | bd-tuxlink-2x0l/message-list-sort-ui | Dirty merged (PR #244 shipped); 187 MB. Sort UI edits — abandoned scratch or unmerged follow-up? Operator triage pending. |
| `bd-tuxlink-7vea-listener-ui-ardop-wiring` | bd-tuxlink-7vea/listener-ui-ardop-wiring | Dirty merged (PR #340 shipped); 5.9 GB. Edits to `telnet_listen.rs` + `ui_commands.rs`. Same question. |
| `bd-tuxlink-hblz-vara-tcp` | bd-tuxlink-hblz/vara-tcp | CLOSED #192 without merge; 5.4 GB. Operator's call. |
| `bd-tuxlink-61yg-telnet-mailbox-ardop-e2e` | bd-tuxlink-61yg/telnet-mailbox-ardop-e2e | no-PR; active work. |
| `bd-tuxlink-6qgn-session-handoff` | bd-tuxlink-6qgn/session-handoff | no-PR; prior session handoff. |
| `bd-tuxlink-73ox-session-end-handoff` | bd-tuxlink-73ox/session-end-handoff | no-PR; this agent's earlier handoff branch. |
| `bd-tuxlink-7fr-ax25-packet` | **bd-tuxlink-jvp/uvpro-setup** | no-PR; note the branch name doesn't match the worktree dir. |
| `bd-tuxlink-9yx-integration-smoke` | bd-tuxlink-9yx/integration-smoke | no-PR; active work. |
| `bd-tuxlink-jy6p-convergence-adrev` | bd-tuxlink-jy6p/convergence-adrev | no-PR; convergence adrev work. |
| `bd-tuxlink-o1j5-session-handoff` | bd-tuxlink-o1j5/session-handoff | no-PR; prior session handoff. |
| `bd-tuxlink-qwpp-session-end-handoff` | bd-tuxlink-qwpp/session-end-handoff | no-PR; prior session handoff. |
| `bd-tuxlink-unb0-session-end-handoff` | bd-tuxlink-unb0/session-end-handoff | no-PR; prior session handoff. |
| `bd-tuxlink-ymiv-docs-knowledge-base-spec` | bd-tuxlink-ymiv/docs-knowledge-base-spec | no-PR; docs/KB design work. |
| `bog-bluff-mesa-handoff` | agent-bog-bluff-mesa/session-end-handoff | no-PR; prior agent's handoff. |
| `willow-yew-esker-handoff` | agent-willow-yew-esker/session-end-handoff | no-PR; prior agent's handoff. |

**Preserved** during disposal: `dev/handoffs/2026-06-02-arroyo-oak-fjord-l55l-su2h-shipped-pr-219.md` (was untracked in the 12 GB `0pnb` worktree; copied into the main checkout's `dev/handoffs/` before disposal; ready to `git add` whenever).

## Main checkout state

Branch: `bd-tuxlink-xygm/recover-handoffs` (operator's prior recovery work, last commit `56cd8f1` from before this session).

Working tree:
- `.beads/issues.jsonl` modified (bd state changes from this session — gitignored auto-export tracked-by-mistake; per `feedback_never_hold_a_push` don't commit this).
- 3 untracked handoff/docs files including this one + the preserved `arroyo-oak-fjord` doc + the listener-UI mocks `docs/design/mockups/2026-06-03-listener-ui-mocks.html` (origin uncertain — might have been operator's local work).

## What the next session should know (read this before claiming new work)

**Operator-stated priority going forward:** "further UX work."

**Recent failure modes worth carrying:**

1. **Speculation vs investigation** — early in this session I speculated on the idle-CPU bug (blaming the debug build + the 4Hz Rust broadcaster) instead of grounding the RCA in actual code. Operator correctly pushed back ("doesn't make sense contextually"). Once I actually traced the broadcast → listen → broadcast cycle, the real bug (PR #338) was obvious. **Lesson:** when an RCA contradicts the operator's context ("nothing fundamentally changed"), they're probably right — investigate, don't theorize.
2. **Stale local checkout** — when I told the operator "there's no width control in the help window," I had been working from an older perf-sweep worktree and missed `useReadingWidth.ts` + `HelpTitleBar.tsx` that landed in PR #333. Operator screenshotted the control sitting in the header. **Lesson:** `git fetch origin && git ls-tree -r origin/main <subdir>` before claiming the code doesn't exist.
3. **Destructive-git hook false positives via comment text** — `git worktree prune` (allowed) got denied because my bash block had the literal string "git worktree remove" in a code comment. The hook regex-matches the whole command text, including comments. Same class as the `merge-base` false positive in `feedback_worktree_git_hook_cwd_and_mergebase`. **Lesson:** keep banned-phrase strings out of comments inside bash blocks.

**Codex remains the canonical "second look"** — caught the 2 PR #297 production bugs (CSP-blocked inline script + custom-theme `--bg` flash) plus the 3 follow-ups in PR #327 (row-date staleness, FolderSidebar memo defeat, lazy-MessageView fallback wrong-copy). Run it on substantial PRs.

**The two dirty worktrees still on disk** (2x0l + 7vea) need ~30 seconds of operator triage each — peek the diff, decide if it's worth keeping or discardable. They're consuming ~6 GB combined.

## Agent: bluff-birch-cove
