# 2026-06-08 osprey-mink-magpie — Contacts+Favorites SHIPPED & MERGED; next session = new feature from triage

## TL;DR

Contacts+Favorites (tuxlink-raez + tuxlink-egmp) is **done, merged, and closed**. PR **#472 is MERGED** to main; both bd issues are **closed**; the remote branch was auto-deleted by the merge. The next session starts a **new feature from the triage backlog** — nothing on CF remains except the optional cleanups below.

## What shipped

- Full feature: Contacts address book (multi-address, callsign-primary, distribution groups powering Compose To/Cc) + per-radio-mode Favorites/Recents with an empirical, time-of-day-bucketed connection record. Backend + Contacts frontend (prior sessions) + Favorites frontend B3–B7 + wrap-up C1–C3 (this work).
- Executed via subagent-driven-development (two-stage spec+quality review per task). Cross-provider Codex code adrev ran for real (RADIO-1, ToD, distance, Tauri arg-keys all confirmed SAFE; its 2 findings fixed).
- After #464 (fzm1-responsive) + #465 (catalog-builder) landed first, PR #472 hit merge conflicts — resolved by keeping BOTH features in MessageView/FolderSidebar/AppShell, plus completing the FZ-M1 compact coordination (Contacts added to the rail+flyout). Re-verified the FULL gate on the merged tree: cargo test + clippy `--all-targets -D warnings` + tsc clean + full vitest **160 files / 1818 tests**. Then operator-smoked and merged.

## Loose ends (all OPTIONAL — none block new work)

- **Worktree disposal (ADR 0009):** `worktrees/bd-tuxlink-raez-contacts-favorites` is spent (PR merged). Inventory: nothing tracked/untracked to propagate (all merged); the only on-disk extras are gitignored codex transcripts (`dev/adversarial/*-contacts-favorites-codex.md`) + `dev/scratch/*` drafts (disposable per the gitignore policy) and a **31 GB `src-tauri/target/`** build cache. The 7 `git stash` entries are **pre-existing from other branches/sessions — NOT this session's; do NOT clear them.** Dispose from a context NOT rooted in the worktree (next session from main, or now):
  ```bash
  cd /home/administrator/Code/tuxlink
  rm -rf worktrees/bd-tuxlink-raez-contacts-favorites      # frees ~31 GB
  git worktree prune
  ```
  (Not done this session: I was running from inside the worktree, and the disposal git ops are main-checkout-hook-sensitive from a worktree session.)
- **Follow-ups filed (P3, additive, not urgent):** `tuxlink-fkxb` (packet relay-chain favorites — Favorite has no relay field, prefill sets only the target callsign) and `tuxlink-41di` (de-dup haversine: #465 shipped its own `src/catalog/distance.ts` instead of the shared `src/forms/position/distance.ts`).
- **Dependabot #478** ("bump the radix-ui group", incl. `react-tabs` 1.1.0→1.1.14): it conflicted only because #472 added react-tabs to the lockfile; Dependabot auto-rebased on #472's merge and it's now MERGEABLE (CI was running). API-compatible with FavoritesTabs' usage. Operator can merge once green; if `verify` fails it'd most plausibly be the radix bump.
- **Smoke-time UX notes** (from the PR, for whoever next touches CF): ARDOP defaults to the Favorites tab (empty with no favorites → click Manual); ARDOP records both a `reached` (link-up) and a `failed` (later exchange failure) for one session by design; packet Start has no in-flight disable (pre-existing).

## NEXT SESSION — pick a new feature from the triage backlog

- `bd ready` shows **116 ready issues**. Current top P0/P1: `tuxlink-t8c0` (operator-smoke logging clear-history/archive), `tuxlink-h8gh` (v0.35.3 release has no installable assets — bug), `tuxlink-etxt` (mark read/unread), `tuxlink-ka3z` (nested folders), `tuxlink-9ky`/`tuxlink-0ja` (RF/Bluetooth bugs), `tuxlink-5vx` (AX.25 inline Radio UI).
- Also see the triage handoff `dev/handoffs/2026-06-07-savanna-moss-gorge-smoke-walk-triage.md` for the curated list.
- **Gate reminder (CLAUDE.md skill routing):** for a net-new *product* feature, run `office-hours`/`brainstorming` FIRST (don't jump to code); for a bug, run `investigate`. The operator will choose the specific issue — surface options from `bd ready`, don't auto-pick.
- Per-task branch + worktree under the chosen bd id (ADR 0004/0008); on-air RF work stays operator-gated (RADIO-1).
