# Handoff — bog-bluff-mesa session end (2026-06-03 evening)

> **Date:** 2026-06-03 · **Agent:** `bog-bluff-mesa` · **Machine:** pandora
>
> **Arc (second half of session):** PR #312 (help window) merged early
> evening; rapid operator-smoke loop surfaced one crash + seven polish
> items + scroll perf. Shipped four PRs (#331 crash fix, #333 polish r1,
> #334 polish r2). Operator now wants to pivot to the **docs content
> itself** — `tuxlink-s8qu` (docs expansion + Hamexandria attribution).

---

## 0. Critical first action — next session

```
1. Read THIS handoff. §1 has the chronological status; §3 has bd state;
   §6 surfaces the Hamexandria ethical-attribution discipline that
   gates the next arc.
2. The operator's IMMEDIATE NEXT FOCUS is the docs-content arc, not
   more help-window polish. Don't propose more help fixes unless the
   operator surfaces them. The work is to expand docs/user-guide/*.md
   with proper attribution.
3. Main checkout state (re-probed at end-of-session per memory
   feedback_reverify_checkout_state_at_session_end):
   - HEAD: bd-tuxlink-xygm/recover-handoffs (operator's daily driver
     since the rebase-abort recovery earlier this session).
   - NOT mid-rebase.
   - Local `main` is 509 commits behind origin/main (135 PRs from
     earlier in the session PLUS 4 PRs we landed since). Operator
     hygiene; not blocking.
4. PR #334 (tuxlink-q5td) is still OPEN — operator-side merge gate.
   Once merged, the q5td worktree can be disposed per ADR 0009.
```

Paste-ready next-session prompt at the bottom.

---

## 1. Session arc (second half — help-window crash → polish → pivot)

Earlier today (first half of session, already documented in
`dev/handoffs/2026-06-03-bog-bluff-mesa-help-window-redesign-shipped.md`):
brainstorm → spec → plan → execution of PR #312 (help-window redesign).
It merged 2026-06-03 10:17 UTC.

Second half:

1. **Operator smoked PR #312 against `pnpm dev:converged`** —
   diagnosed that `dev:converged` was missing from their `~/Code/tuxlink`
   because their checkout was on `task-amd-main-ui` which never carried
   the script. The script lived on origin/main.
2. **`feedback_operator_tooling_lives_on_main`** memory entry saved
   to capture the principle: operator-facing scripts must live on main
   and be reachable from the project root.
3. **Cause of `dev:converged` disappearance**: the previously-in-progress
   `task-amd-main-ui` rebase was aborted at 2026-06-03 02:31:40 — the
   operator was happily using `dev:converged` from the mid-rebase
   working tree (HEAD at f24c06a, atop dea086f, post-PR-#203 → script
   present); abort snapped HEAD back to 3ba63bd (June 1 pre-rebase tip,
   pre-PR-#203 → no script).
4. **17 handoff/plan/bug-hunt docs orphaned by the abort** (none on
   origin/main). Recovered via the `bd-tuxlink-xygm/recover-handoffs`
   branch (origin/main + the orphaned doc-file content + the
   f24c06a-only plover-willow-basalt handoff from reflog). Operator
   adopted this branch as their main-checkout daily driver.
5. **PR #312 actual smoke**: "the new docs window opens but displays
   nothing." Diagnosed via the help window's DevTools console:
   `Error: No QueryClient set, use QueryClientProvider to set one`.
   Root cause: App.tsx wrapped `<QueryClientProvider>` only around
   `<AppShell />`; the `isHelpWindow` branch mounted `<HelpView>`
   bare. Tests passed because every unit test wrapped its own
   `QueryClientProvider` in the test render —
   **dependency-injection-by-test that left production unsupported**.
6. **PR #331 (tuxlink-n4hz)** shipped + merged: lifted
   `QueryClientProvider` above all routing branches in App.tsx +
   regression test using a sentinel mock that calls `useQueryClient()`
   (imported at top-level ESM so the mock factory references the
   same `@tanstack/react-query` module instance App.tsx imports — a
   `require()` inside the factory would resolve via CJS and produce
   a duplicate context).
7. **`feedback_test_production_mount_path_not_just_units`** memory
   entry saved to capture the lesson.
8. **Operator post-merge smoke surfaced seven polish items + scroll
   perf.** Bundled six into PR #333 (tuxlink-ew3k):
   - Scroll reset on topic change (useEffect on `topic.slug`).
   - Text-size dropdown looked dead — `--help-font-size: 18px` default
     on `.tux-help-root` was masking the `<html>`-level value set by
     useFontSize (closer-ancestor CSS custom property wins).
   - Custom titlebar — new `HelpTitleBar` + `.decorations(false)` in
     `help_window.rs` + reused `ResizeHandles`.
   - Width-preset toggle — new `useReadingWidth` hook + "Width:
     Narrow / Wide" button (720 / 980 px presets, persisted).
   - Markdown list-continuation fix in `shell/markdownRender.ts` — an
     indented continuation line after a list item was leaking out as
     an orphan paragraph; visible at the bottom of `07-settings.md`.
   - `parseMarkdown` memoization by `topic.body` — was re-parsing on
     every render.
   PR #333 merged 2026-06-03 21:00 UTC.
9. **Operator re-smoke after #333**: scroll perf STILL sluggish (LAN-
   fast VNC ruled out as cause); last paragraph nearly flush with
   window bottom. **PR #334 (tuxlink-q5td)** open: pure-CSS GPU
   compositing on `.tux-help-reading` (`transform: translateZ(0)` +
   `will-change: transform`) + `contain: content` on
   `.tux-help-reading-content` + bottom-pad 80 → 160 px.
10. **Pre-push hook caught an anti-pattern**: I had committed the
    round-2 CSS on the merged-dead `bd-tuxlink-ew3k/help-polish`
    branch; the branch-lifecycle hook (ADR 0017) denied the push. Re-
    issued cleanly on `bd-tuxlink-q5td/help-polish-r2` off post-#333
    origin/main. **Good guardrail; ADR 0017 works.**
11. **Operator hit context wall + pivoted**: "Hand this off so we may
    continue with the docs content itself." This handoff doc.

---

## 2. PR state

| PR | Title | Branch | Merged | bd issue |
|---|---|---|---|---|
| [#312](https://github.com/cameronzucker/tuxlink/pull/312) | Help window redesign | bd-tuxlink-0gsy/help-window-redesign | 10:17 UTC | tuxlink-0gsy |
| [#331](https://github.com/cameronzucker/tuxlink/pull/331) | QueryClient fix | bd-tuxlink-n4hz/helpview-queryclient | 20:28 UTC | tuxlink-n4hz |
| [#333](https://github.com/cameronzucker/tuxlink/pull/333) | Polish round 1 | bd-tuxlink-ew3k/help-polish | 21:00 UTC | tuxlink-ew3k |
| [#334](https://github.com/cameronzucker/tuxlink/pull/334) | Polish round 2 (CSS perf) | bd-tuxlink-q5td/help-polish-r2 | **OPEN** | tuxlink-q5td |

`gh pr list --state open` at next session start for the live state.

---

## 3. bd state

Closed this session (or will close on PR-merge):
- `tuxlink-0gsy` — help window redesign (closed on PR #312 merge).
- `tuxlink-n4hz` — QueryClient crash fix (closed on PR #331 merge).
- `tuxlink-ew3k` — polish round 1 (closed on PR #333 merge).
- `tuxlink-q5td` — polish round 2 (closes on PR #334 merge, pending).
- `tuxlink-xygm` — recovery branch (operator adopted as daily driver;
  the issue itself can be closed as the recovery work is done).

Standing follow-ups (priority order):

| bd id | Priority | Title | Status |
|---|---|---|---|
| **tuxlink-s8qu** | **P3 (operator's NEXT FOCUS — promote to P1?)** | **Docs expansion + polish + Hamexandria-sourced content (ethical attribution)** | Open |
| tuxlink-h7q7 | P2 | Main-client device-target support: Panasonic FZ-M1 | Open |

The operator's immediate next focus is **tuxlink-s8qu**. See §6 for the
ethical-attribution discipline that gates it.

---

## 4. Worktree inventory at handoff

| Worktree | Branch | PR | Disposal status |
|---|---|---|---|
| `~/Code/tuxlink` (main checkout) | `bd-tuxlink-xygm/recover-handoffs` | n/a (recovery, operator's daily driver) | KEEP — operator state |
| `worktrees/bd-tuxlink-0gsy-help-window-redesign/` | bd-tuxlink-0gsy/help-window-redesign | #312 MERGED | Ready for ADR 0009 disposal |
| `worktrees/bd-tuxlink-n4hz-helpview-queryclient/` | bd-tuxlink-n4hz/helpview-queryclient | #331 MERGED | Ready for ADR 0009 disposal |
| `worktrees/bd-tuxlink-ew3k-help-polish/` | bd-tuxlink-ew3k/help-polish | #333 MERGED | Ready for ADR 0009 disposal |
| `worktrees/bd-tuxlink-q5td-help-polish-r2/` | bd-tuxlink-q5td/help-polish-r2 | #334 OPEN | KEEP until PR merges |

Disposal commands for the three merged worktrees (per ADR 0009):

```bash
for W in bd-tuxlink-0gsy-help-window-redesign bd-tuxlink-n4hz-helpview-queryclient bd-tuxlink-ew3k-help-polish; do
  echo "=== $W ==="
  cd "/home/administrator/Code/tuxlink/worktrees/$W"
  git status --short
  git ls-files --others --exclude-standard
  git stash list
  cd /home/administrator/Code/tuxlink
  rm -rf "/home/administrator/Code/tuxlink/worktrees/$W"
done
git -C /home/administrator/Code/tuxlink worktree prune
```

---

## 5. Discipline notes / decisions captured as memory

Four new memory entries saved this session:

1. **`feedback_userbase_old_internet_navigation`** — sidebar-ToC +
   reading pane signals competence for the tuxlink audience; default
   for text-heavy reference UI.
2. **`feedback_reverify_checkout_state_at_session_end`** — re-probe
   rebase markers, branch HEAD, local-vs-origin DIRECTLY when writing
   the handoff; don't carry the session-start observation forward
   (caught the "main is mid-rebase" claim propagating through three
   consecutive handoffs).
3. **`feedback_operator_tooling_lives_on_main`** — operator-facing
   scripts (`converge-build`, dev wrappers, install scripts) must
   live on main and be reachable from project root; never confined
   to worktrees or feature branches.
4. **`feedback_test_production_mount_path_not_just_units`** — when
   a unit test wraps a context provider (QueryClient, Router,
   Theme, Auth) as scaffolding, ALSO write an App-level test that
   mounts the production path; otherwise the unit test silently
   injects what production fails to provide.

---

## 6. The next arc — `tuxlink-s8qu` (docs expansion + Hamexandria + ethics)

**Operator's exact framing when filing the issue (2026-06-03):**

> Out content is pretty anemic. For example, we don't mention anything
> about what ARDOP is, how it works on the local machine, etc. We
> should make use of images — either cropped or full screenshots —
> where possible in the actual content. They're often worth 1,000
> words.

> [On Hamexandria as a source:] Hamexandria content may be copyrighted
> and we'll have to be careful and ethical about this. Creators we're
> referencing are my people and they work hard for little compensation.
> We don't want to steal their work.

**Six attribution disciplines codified in tuxlink-s8qu** (lift these
verbatim into any draft):

1. NEVER copy YouTube transcripts verbatim into `docs/user-guide/*.md`.
2. USE Hamexandria as a research aid to inform tuxlink's own
   (originally-written) explanations.
3. Where a creator's framing is genuinely the right framing to cite,
   attribute prominently (creator name, video title, link to source
   video) and quote only what fair use plainly permits.
4. Prefer to link OUT to the creator's video rather than embed their
   words.
5. Get explicit permission from creators for any substantial extract.
6. Maintain a CREDITS / acknowledgments section listing creators whose
   explanations shaped the docs.

**Hamexandria access** (memory `reference_hamexandria`):
`uv run ham-search` in `dev/scratch/ham-knowledge-store/` (gitignored,
no commits ever pushed); 267 MB SQLite over YT transcripts; pair with
`youtube-fetcher-to-markdown` skill for full-text follow-up.

**Likely scoping**:
- Phase 1: polish + accuracy pass on existing 10 topics
  (`docs/user-guide/01-getting-started.md` through
  `10-troubleshooting.md`).
- Phase 2: topics for shipped-but-undocumented features (themes,
  Theme Designer, VARA TCP, ARDOP UI, HTML Forms, search, user
  folders, MessageView attachments, Saildocs GRIB, WLE catalog,
  message sort).
- Phase 3: Hamexandria-informed conceptual / how-it-works topics
  with the attribution discipline.

The skill plausibly worth running before the writing phase:
**`document-release`** (gstack family). It reads project docs +
diffs and proposes README / CHANGELOG / CLAUDE.md updates — useful
to baseline the current state before expanding. Operator surfaced
it earlier as something to consider; haven't run it yet.

---

## 7. Useful pointers for next session

**Help window source code (post-q5td polish merge):**
- [src/help/HelpView.tsx](../../src/help/HelpView.tsx) — root with
  HelpTitleBar + width toggle + font dropdown + sidebar + reading pane.
- [src/help/ReadingPane.css](../../src/help/ReadingPane.css) — GPU
  composite + `contain: content` + 160 px bottom pad live here.
- [src/help/HelpTitleBar.tsx](../../src/help/HelpTitleBar.tsx) — custom
  chrome titlebar.
- [src/help/useReadingWidth.ts](../../src/help/useReadingWidth.ts) —
  Narrow / Wide preset.
- [src/help/useFontSize.ts](../../src/help/useFontSize.ts) — Normal /
  Large / X-Large / Huge.
- [src/help/useHelpSearch.ts](../../src/help/useHelpSearch.ts) — FTS5
  query via the existing `src-tauri/src/search/`.

**Docs source** (the s8qu work target):
- [docs/user-guide/](../../docs/user-guide/) — the 10 markdown topics
  bundled at build time via `import.meta.glob` + Rust-side
  `include_str!` (`src-tauri/src/search/docs_bundle.rs`).

**Memory + spec** (the s8qu input):
- Memory: `reference_hamexandria`, `feedback_writing_voice_no_first_person`,
  `feedback_explicit_referents_in_specs`.
- bd issue: `bd show tuxlink-s8qu` — full attribution discipline list.

---

## 8. Next-session paste-ready prompt

```
Resume from bog-bluff-mesa's 2026-06-03 evening session-end handoff.

Handoff doc: dev/handoffs/2026-06-03-bog-bluff-mesa-help-window-complete-pivoting-to-docs-content.md
READ IT FIRST. §6 gates the next arc with the Hamexandria
ethical-attribution discipline.

The HELP WINDOW work is COMPLETE through PR #334 (open at handoff
time). Do not propose more help fixes unless the operator surfaces
them. The next focus is tuxlink-s8qu — docs expansion + polish +
Hamexandria-informed conceptual content with the six explicit
attribution disciplines in the bd-issue description.

Before the writing phase, consider running the `document-release`
skill (gstack family) to baseline the current docs/user-guide/ + diff
against shipped features (themes, VARA TCP, ARDOP UI, HTML Forms,
search, user folders, attachments, Saildocs GRIB, WLE catalog,
sort). The operator surfaced this skill earlier in the session as
something to consider.

Re-verify main-checkout state at session end per memory
feedback_reverify_checkout_state_at_session_end (probe rebase
markers + local-vs-origin staleness DIRECTLY). Operator's main
checkout is currently on bd-tuxlink-xygm/recover-handoffs at HEAD
56cd8f1; local main is 509 commits behind origin/main (operator
hygiene, not blocking).

Worktrees pending disposal (per ADR 0009) — commands in §4 of the
handoff:
- bd-tuxlink-0gsy-help-window-redesign  (PR #312 MERGED)
- bd-tuxlink-n4hz-helpview-queryclient  (PR #331 MERGED)
- bd-tuxlink-ew3k-help-polish           (PR #333 MERGED)
- bd-tuxlink-q5td-help-polish-r2        (PR #334 OPEN — keep alive)
```

---

Agent: bog-bluff-mesa
