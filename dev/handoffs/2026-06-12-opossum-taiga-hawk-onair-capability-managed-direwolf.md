# Session handoff — opossum-taiga-hawk — 2026-06-12

A capability-research + design session that became an architecture commitment. Started as "final alpha release-gate" work, pivoted to a deep on-air-capability research pass (operator derisking a weekend RF test), which surfaced a real product gap → office-hours design → approved design doc → `build-robust-features` → a committed implementation plan for **managed Dire Wolf**. No production code written yet (by design — execution is a fresh-session job).

## ⚠️ Read first — checkout + durability state

- **Main checkout** is on `bd-tuxlink-xygm/recover-handoffs`, ~1300 commits behind `origin/main`. Read code via `git show origin/main:<path>` or a worktree off `origin/main`. NEVER the working tree.
- **This handoff is UNCOMMITTED in the main working tree** — the main-checkout-race hook is denying main commits (two other live sessions: `bd-tuxlink-2ns7/phase4-mailbox`, `bd-tuxlink-s0r1/findstation-realapp-fixes`, both in worktrees). The real work is durable elsewhere (see below). Commit this handoff when you next own the main checkout, or it rides a future main commit.
- **All substantive work is committed + pushed on branch `bd-tuxlink-yq3l/managed-direwolf`** (worktree `worktrees/bd-tuxlink-yq3l-managed-direwolf`, off origin/main). Nothing is stranded.

## What happened (in order)

1. **Closed `tuxlink-0ye6`** (VARA umbrella) — reconciliation: shared-RadioSessionPanel premise was operator-rejected, all 5 deps shipped, no buildable scope left.
2. **On-air capability research report** → `dev/scratch/2026-06-12-onair-capability-report.md` (main checkout, gitignored scratch). Grounded in origin/main code + WLE decompile + winlink-annex + hamexandria + vendor docs (3 parallel research subagents). Key findings: tuxlink does **zero CAT/rig control** (CAT is optional — WLE defaults to manual too, so NOT a hard gate); PTT is delegated to the modem; **VARA-on-Pi5 is the one hard gate** (Wine blocked by 16k-page kernel → use ardopcf for HF); the DRA-100 keys via CM108 HID which tuxlink's managed-ardopcf can't emit (only `-p` serial/RTS); a shipped doc (`12-cat-and-rigctld.md`) falsely claims a rigctld integration that doesn't exist.
3. **office-hours design session.** Operator's deeper objection: hand-configuring Dire Wolf is an **operator-skill gate** that "separates wheat from chaff" — tuxlink must DELETE that gate, not relocate it ("software that hates its audience" = anti-goal). Reconstructed the original intent from **ADR 0015** (already decided: tuxlink manages BOTH Dire Wolf + ardopcf; only ardopcf shipped managed — Dire Wolf is an unfinished slice). Operator rejected building a native AFSK/GMSK packet modem (would be a 3rd clean-sheet modem beside TANDEM HF/FM; not confident it'd be right). **Decision: ship MANAGED Dire Wolf, done excellently.**
4. **Approved design doc** → `docs/design/2026-06-12-managed-modem-onair-accessibility-design.md` (committed `e674a23a` on yq3l). 3 slices: B = managed Dire Wolf (this branch), A = CM108 PTT for ardopcf, C = tux-rig CAT plane (5jb). Survived 1 independent adversarial review (Depends→Recommends, TXDELAY-via-KISS, stable USB-id device resolution, stuck-PTT race, ADR-quote attribution all fixed).
5. **`build-robust-features` → implementation plan** → `docs/plans/2026-06-12-managed-direwolf-plan.md` (committed `30b70a8f` on yq3l). 9-phase subagent-ready TDD plan.
6. **New memory:** `feedback_no_operator_skill_gatekeeping` (+ MEMORY.md index line).

## bd state

- **`tuxlink-yq3l`** (P1, feature, **in_progress/claimed**) — Managed Dire Wolf (Slice B). Design + plan committed; branch pushed (docs only, **no PR yet**). The build.
- `tuxlink-ptmq` (P2 bug) — reconcile the phantom `12-cat-and-rigctld.md`. Independent, can ship first/alone.
- `tuxlink-5rwl` (P2 feature) — Slice A: CM108 PTT for managed ardopcf (wire existing `tuxmodem/crates/tux-rig-cm108`). HF completeness, NOT the weekend item.
- `tuxlink-5jb` (P3, open) — Slice C: tux-rig CAT plane. Settle the wrong-freq-TX interlock design early.
- `tuxlink-80ci` (P3 bug) — CLAUDE.md wrongly namespaces the skill as `superpowers:build-robust-features`; it's bare `build-robust-features` (cost one failed invocation this session).

## In-flight worktree

- `worktrees/bd-tuxlink-yq3l-managed-direwolf` (branch `bd-tuxlink-yq3l/managed-direwolf`, off origin/main). **Tracked clean** (2 doc commits, pushed). **Untracked:** `node_modules/` (installed for the pre-push doc-link hook — gitignored). **No** gitignored-stateful content of concern. Disposal only after the eventual PR merges (ADR 0009 ritual). Do NOT dispose — it's the active build worktree.
- Other sessions' worktrees (2ns7, s0r1, + others) left untouched.

## Pending / next

1. **Execute the managed-Dire-Wolf plan** — fresh session via `/executing-plans` (or subagent-driven-development) in the yq3l worktree. Open a **draft PR up front** so CI compiles each push (no cold cargo on this Pi). Phases 4 (lifecycle/RADIO-1) + 6 (connect/abort) get the 3-round review.
2. **MANDATORY Codex cross-provider adversarial round** before the PR goes ready — **deferred: quota-blocked until ~2026-06-13 1:49 PM**. Do NOT substitute Claude (memory `no_carveout_on_cross_provider_adrev`). Run on the PR diff after reset.
3. **Operator weekend on-air smoke** = the real validation (DRA-100 → CDM-1550LS+, FM packet). Agent never transmits (RADIO-1). Also: the greenfield-install→wizard→CMS-connect smoke (PR #619, never tested on a virgin unit) is the highest-leverage home test before the trip.
4. `ptmq` doc-fix can ship anytime, independently.

## Gotchas carried forward

- Codex quota resets ~2026-06-13 1:49 PM (defer, don't substitute).
- Worktree commits: standalone `cd <worktree>` FIRST, then git op next call (hook reads payload cwd). Fresh worktrees need `pnpm install` before push (pre-push tsx doc-link hook).
- No cold cargo — draft PR + CI. Pin `--manifest-path` / `pnpm -C`.

Agent: opossum-taiga-hawk
