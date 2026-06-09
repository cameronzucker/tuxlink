# 2026-06-09 opossum-lupine-magnolia — read/unread DESIGNED + BUILT + SHIPPED (PR #497 merged); Codex follow-up PR #499 open

## TL;DR

`tuxlink-etxt` (mark messages read/unread) went the full arc this session — triage pick → design (panel + operator approval) → spec → plan → 14 TDD tasks via subagent-driven-development → merge to current main → **PR #497 MERGED** (full CI green both arches) → cross-provider Codex adrev → 5 P2 fixes → **follow-up PR #499 (open, CI pending)**. Nothing blocks; two clean next-actions remain (merge #499, dispose the spent worktree).

## What shipped / state of the world

- **PR #497 — MERGED to main** (merge commit `a9415be`). The complete read/unread feature: `Ctrl/Shift+click` multi-select (no per-row checkboxes; WLE-UX-parity explicitly rejected per operator), bulk action bar, single-message context-menu item + `U` shortcut, read-state surfaced across Inbox + user folders + Archive, the auto-read fix (`message_read` pure + once-per-open client mark), Archive unread badge. **CI: verify (clippy `--all-targets` + full vitest) + build-linux ALL PASSED on amd64 + arm64.**
- **PR #499 — OPEN, CI pending** — https://github.com/cameronzucker/tuxlink/pull/499 — the 5 Codex P2 fixes (search invalidation, Enter-clears-selection, bulk id filter, archived-Sent read-state, mark-on-open guard reset) + a tsc typing fix. Branch `bd-tuxlink-kuhk/read-unread-codex-fixes`, head `817e2fd`. **Next: confirm CI green → operator smoke → merge.**

## Worktree state (ADR 0009 enumeration)

- **`worktrees/bd-tuxlink-etxt-read-unread`** — SPENT. Branch `bd-tuxlink-etxt/read-unread` is **merged-dead** (#497 merged). On-disk extras: ~31 GB `src-tauri/target/` build cache + the gitignored Codex transcript `dev/adversarial/2026-06-09-read-unread-codex.md` (findings already captured in #499). Commit `9b2a6b8` (the Codex fixes) was pushed to this dead branch via the lifecycle-override hatch, then cherry-picked onto the #499 branch — nothing unpushed/at-risk. **DISPOSE per ADR 0009** (frees ~31 GB): `rm -rf worktrees/bd-tuxlink-etxt-read-unread && git worktree prune` (from a context not rooted in it).
- **`worktrees/bd-tuxlink-kuhk-read-unread-codex-fixes`** — ACTIVE (PR #499). node_modules installed; clean tree. Dispose after #499 merges.

## Verification provenance

- CI (ubuntu, isolated) is authoritative: #497 verify+build PASSED both arches. The local Pi full-vitest produced flakes (`message-view-loaded` lazy-load race + `ModemLinkSection` USB tests) that **pass in isolation / on CI** — they are cross-session resource-contention artifacts on the shared Pi, NOT regressions (this branch never touched `MessageView.tsx`/`src/radio`). Cross-session `pkill -f vitest` from concurrent worktree sessions (6c9y Post-Office, ka3z nested-folders) repeatedly killed local vitest mid-run — **do not global-pkill on this shared host.**

## Deferred follow-ups (filed)

- `tuxlink-kuhk` — the #499 follow-up itself (close when #499 merges).
- `tuxlink-mzm4` (P3) — `store()` seeds the search-index `unread` column Inbox-only; inconsistent with `list()` post-etxt. Search-filtered-by-unread only; orthogonal to the feature.
- `tuxlink-23si` (P3) — multi-select a11y pass (`aria-selected` reflecting bulk selection + `aria-multiselectable`/grid keyboard nav).
- `tuxlink-llvk` (P4) — `ModemLinkSection` USB-serial tests flake under full-suite Pi load (pass in isolation).

## Warm-up cleanups (done at session start)

- Disposed the spent Contacts+Favorites worktree (freed ~30 GB); confirmed Dependabot #478 already merged (green CI).

## Lessons logged this session

- WLE: copy features, reject UX, but use OS conventions (Ctrl/Shift+click). Memory `feedback_wle_features_yes_ux_no_use_os_conventions`.
- Cross-provider Codex adrev earns its keep: it caught 5 real integration bugs (cross-boundary: search/selection/Archive semantics) that Claude-on-Claude per-task review missed.
- Don't trust subagent gate claims verbatim — a `cmd | tail && echo CLEAN` masks the real exit; re-verify tsc/clippy with the actual exit code (the Codex-fix subagent shipped a tsc-breaking test it reported "clean").
- Reviewing `origin/main..HEAD` on a stale long-running branch hallucinates "this PR reverts X" — review against the merge-base; re-merge main when the branch drifts (main advanced ~hourly from concurrent sessions).
