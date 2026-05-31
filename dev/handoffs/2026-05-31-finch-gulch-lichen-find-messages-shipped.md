# Handoff: 2026-05-31 — find-messages v0.1 shipped + iterated through operator smoke

**Agent:** finch-gulch-lichen
**Branch:** `bd-tuxlink-1hu/find-messages` — **PR #163 MERGED** to `main` (then continued landing follow-up fixes on the same branch).
**Working-tree state:** clean except the just-added `src-tauri/examples/seed_test_messages.rs` (committed below).

## TL;DR

- v0.1 capability 1.15 **find-messages shipped** via PR #163 (release 0.7.0). Backend FTS5 + frontend Gmail-style inline operator parser + SearchBar + SearchDropdown + SavedSearchesPanel. Operator declared it shipped after multiple rounds of smoke feedback that this branch iterated on as fixup commits stacked after the original merge.
- Tip of branch as of this handoff: **after `eb2a516`** + the seed-binary commit below. Branch is well-ahead of `origin/main`; another PR cycle is appropriate when the operator wants the smoke-cluster fixes merged.
- **Mailbox-seeding example added** (`src-tauri/examples/seed_test_messages.rs`) so the operator can populate the inbox without depending on the CMS — Tuxlink is still an unregistered Winlink client so the prod CMS rejects and the dev-CMS responses give 2 messages total which is too sparse to exercise search.

## What landed this session (chronological)

### Phase 1 — design + plan + implementation (Tasks 1–21)

1. Read the prior fen-alder-bog handoff. Q1–Q3 of the find-messages brainstorm were settled; Q4 was open.
2. Worktree `bd-tuxlink-1hu-find-messages` created (off main). bd issue `tuxlink-1hu` filed + claimed.
3. Re-posed Q4 → operator wanted to reframe ("am I bound by this?") → I converted to "what's the extraction surface?" (load-bearing) → **conservative-max**.
4. Settled Q5 (saved-search model = pure query spec, live re-run), Q6 (UI placement → after several mockup iterations Option E: ribbon search + combined saved/recent dropdown), Q7 (sync = in-process hooks + explicit rebuild).
5. Wrote design spec [`docs/design/2026-05-30-find-messages-design.md`](../../docs/design/2026-05-30-find-messages-design.md) — operator approved as-written.
6. Wrote 21-task plan [`docs/superpowers/plans/2026-05-30-find-messages.md`](../../docs/superpowers/plans/2026-05-30-find-messages.md).
7. Executed via superpowers:subagent-driven-development. All 21 tasks landed: Rust FTS5 backend + extractor + mailbox hooks + Tauri command surface + React types/hooks/components + AppShell wiring + integration tests + smoke checklist + Codex adrev.
8. Codex adrev surfaced **5 findings**: 3 fixed inline (commit `fff6001`), 2 filed as bd issues (`tuxlink-c7qz` — search results not rendered in MessageList; `tuxlink-gme3` — ghost-chip onClick deferred to v0.2).
9. PR #163 opened and **merged** by operator.

### Phase 2 — post-merge smoke iteration (the bulk of this session)

After merge, operator smoked the UI and reported a series of issues. Each fixed as a stacked commit on the still-live branch. In order:

| Fix | Commit | One-line |
|---|---|---|
| Merge conflicts with main (Cargo.toml / lib.rs / AppShell.tsx) | `c1fdc6c` | union-resolved; CI green |
| Tuxlink-c7qz — search results not wired to MessageList | (operator-asked, separate commit) | Maps DTO → MessageMeta, computes match highlights client-side, enables cross-folder badge |
| SearchBar emoji + ⌘F + size + click-outside-close | `b6ca42d` | First-pass smoke bundle |
| **SearchBar.css selectors were a NO-OP** (`.layout-b .dashboard .search-bar` didn't match the DOM) | `998f383` | Real root cause of the bar looking unstyled — every rule in SearchBar.css was scoped wrong from Task 14 ship onward |
| Font sizes too small | `126a1ce` | Bumped to project body 13px |
| Caret height inconsistent empty vs typed | `df7101a` | Pinned `line-height: 20px; height: 20px` on input |
| Pill radius too round | `ec84efc` | Reverted to spec's 4px |
| Lenient `key: value` parsing + result click "Message not found" | `710042c` | parseQuery collapses `from: foo` → `from:foo`; AppShell uses the result row's own folder for message_read |
| Chip strip removed; Gmail-style inline operators in search bar | `d381298` | parseQuery + deparseQuery added; useSearch refactored to track rawText |
| Save-current row in dropdown + remove panel form | `36e83e3` | Replaced window.prompt with inline rename; SavedSearchesPanel gutted |
| Dropdown didn't reopen on input click; saved-search mode was "fatal" | `4424d68` | Removed saved-mode branch; saved became a non-modal badge before the always-rendered input |
| Dropdown highlighted two rows after running a saved | `18bb3d0` | Initialize focusIdx from activeSavedId |
| Save-this-search row dropped (operator decided it's clutter) | `eb2a516` | Save flow is now ONLY star-from-recent |
| Recent searches logged every keystroke | `74e80f6` | Earlier in the smoke cycle: split run() from record_recent + inline-rename on ☆ |

### Phase 3 — bookkeeping / disk hygiene

- Disposed 13 fully-clean worktrees per ADR-0009 ritual. Recovered ~3G. Operator deferred the bigger STATEFUL-bucket disposal.
- Confirmed Hamexandria is in main checkout only (622M, its own .git, NOT in any worktree); disposal plan never touched it.
- **Issue:** disposed `bd-tuxlink-1hu-post-1hu-smoke` while operator's `tauri dev` was running from it → ugly "Could not find Cargo.toml" cascade. Recovery: switched dev to `bd-tuxlink-1hu-find-messages` worktree which had the warm cargo cache. **Lesson for next agent: confirm no live dev server is bound to a worktree before disposal.**

## Current state of code

- All find-messages code is on `bd-tuxlink-1hu/find-messages` ahead of `origin/main`.
- `origin/main` has the original PR #163 merge (release 0.7.0). The smoke-cluster fixes after the merge are NOT on `origin/main` yet — they need a follow-up PR.
- **Test gates:** Rust lib 501/501; vitest 489+ (varies slightly per HMR shuffle); tsc clean; integration tests 4/4.
- **Branch tip:** `eb2a516` + the seed-binary commit added below.

## What's NOT done / open follow-ups

| Issue | Status |
|---|---|
| **tuxlink-g4dj** | Empty subject in search-result rows. messages_meta lacks subject column. Real shipping gap but bounded fix (schema bump → extractor → upsert → query → DTO). NOT done. |
| **tuxlink-xoom** | TOCTOU on `Index::open` path-exists. v0.5+ hardening, not reachable under single-writer ops. |
| **tuxlink-gme3** | Ghost-chip onClick — superseded by the chip-strip removal in `d381298`, technically resolves itself. Issue still OPEN — can be closed by next agent. |
| **tuxlink-c7qz** | CLOSED inline this session via the AppShell wiring fix. |
| **Follow-up PR** | Branch has ~14 commits ahead of `main` (all smoke-cluster fixes). Operator hasn't asked for a PR yet — they're still iterating on the running build. The branch IS still owned by the find-messages bd issue, so it can stay parked until they decide to land. |

## How to continue testing find-messages from scratch

The CMS path is **blocked** at the auth layer — Tuxlink isn't a registered Winlink client. The 2 messages in operator's inbox are CMS "unknown client" rejection responses. Until registration happens (out of scope for find-messages), use the seed binary committed at the end of this session:

```bash
# Optionally clear the mailbox first
cargo run --manifest-path src-tauri/Cargo.toml --example seed_test_messages -- --clear
# Or just seed alongside existing messages
cargo run --manifest-path src-tauri/Cargo.toml --example seed_test_messages
```

Writes 35 synthetic EmComm-shaped messages (8 senders, 4 form types, 3 folders, spread across 30 days) into `~/.local/share/com.tuxlink.app/native-mbox/`. Then **Settings → Saved Searches → Maintenance → Rebuild search index** to populate FTS5.

After that, exercise these query patterns:

| Query | Tests |
|---|---|
| `damage` | free-text FTS5 across subject + body |
| `from:KX5DD damage` | combined free-text + sender filter |
| `from: KX5DD` (space) | lenient parser tolerates space |
| `form:ICS-213` | form-type filter via the form-payload sniff |
| `is:unread` | read-state filter |
| `has:attach` | has-attachments filter (note: extractor counts `File:` headers, fixture probably has none — may always show 0; not a smoke blocker) |
| `date:7d` | last-7-day filter |
| `to:N7CPZ` (case-insensitive LIKE) | recipient filter |
| star a Recent → name it → run from Saved | full save round-trip |

## Critical implementation notes for the next agent

1. **CSS selector trap.** Component CSS that scopes under wrong parent classes is silently a no-op. Vitest passes because tests don't compute styles. Tsc passes because TS doesn't see CSS. This session's most invisible bug was `.layout-b .dashboard .search-bar` not matching the actual DOM for ~20 commits. If a component "looks unstyled," grep the CSS selector against the rendered DOM.
2. **`SearchBar.tsx` always renders the input now.** Saved-search "mode" was removed in `4424d68`. The activeSaved label sits as a compact badge BEFORE the input. Typing detaches the badge automatically. Don't restore the modal split.
3. **`useSearch` derives spec from rawText.** spec.filters comes from `parseQuery(rawText)`. The `setRawText` setter has an auto-detach guard for activeSaved. `setActiveSavedSearch` uses `setRawTextInner` to bypass the guard.
4. **The plan's plan.md still has the original 21-task spec** including chip strip. The chip strip is now gone but the plan document was never amended. Future plan readers may want to know the v0.1 ship deviated from the chip-strip UX in the plan.

## Repo state at session end

```
Branch: bd-tuxlink-1hu/find-messages
Tip:    eb2a516 (refactor(search): remove "Save this search" row...)
        + seed-binary commit (this session, below)
        — N commits ahead of origin/main, all smoke-fix iterations
Worktree: clean (except the seed binary added this session)
```

Agent: finch-gulch-lichen
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
