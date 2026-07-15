# Handoff — Routines plan 6 (dockable surfaces): spec + plan COMPLETE, execution next

- **Agent:** sandbar-oriole-falcon
- **Date:** 2026-07-15
- **Ended:** natural gate — operator chose "fresh session with SDD" for execution.

## READ THIS FIRST — where things stand

1. **The full build-robust-features design flow for Routines plan 6/6 (bd `tuxlink-dmwte`) is DONE.** Brainstormed with the operator (visual companion; chrome option B chosen with A as fallback; the "visual pathway" principle is the operator's own framing and is now binding in the spec §1). Spec written, then hardened by the complete 5-round adversarial cycle — round 1 Codex on **GPT-5.5** (ADR 0023 satisfied), rounds 2–5 Claude with distinct lenses — 51 findings, every P1/P2 dispositioned as an inline-tagged amendment. Operator approved the hardened spec explicitly.
2. **Spec:** `docs/superpowers/specs/2026-07-15-dockable-surfaces-design.md` (branch head `d3a24ee8`). Two audited amendments to the parent Routines spec §12 ship on the same branch (`AMD-1` missing-monitor posture, `AMD-2` ⇤/✕ intent split) — the operator approved both.
3. **Plan:** `docs/plans/2026-07-15-dockable-surfaces-plan.md` — 13 TDD tasks, 4 review-loop groups, subagent-proofed through a 3-round plan review (rounds 1–2: 5 P1 + 8 P2 fixed, incl. a main-side dock-back token contradiction that would have shipped broken ⇤ on all three surfaces; round 3: verified clean, every cited line anchor audited against real code).
4. **Branch/worktree:** `bd-tuxlink-dmwte/dockable-surfaces` in `worktrees/bd-tuxlink-dmwte-dockable-surfaces/` (claimed by the bd issue, `node_modules` installed, all pushed, working tree clean). Raw adrev transcript: `dev/adversarial/2026-07-15-dockable-surfaces-spec-codex.md` in that worktree (gitignored, local-only).
5. **Side discovery, filed as bd P2:** `stations.json` lacks `core:event:allow-emit`, so the existing Station Data snapshot handshake silently never fires in production (swallowed by `.catch`). Independent quick fix; the plan's pop-* capabilities carry the grant with a comment citing it.

## What the next session does

**Execute the plan via `superpowers:subagent-driven-development`** (operator's choice), from the existing worktree. Non-negotiables encoded in the plan's Global Constraints — enforce them as orchestrator:

- Tasks 8–10 are SEQUENTIAL (shared `AppShell.tsx` + `surfaceRegistry.tsx`); only Task 5 ∥ Task 3.
- Subagents write code + STOP; the PARENT commits (subagents invent monikers / can't commit in worktrees — standing rule). Pass the session moniker into every subagent prompt.
- No local cargo builds (contended Pi): Rust red/green happens on the PR's CI, both arches, verified BY HEAD SHA. Frontend vitest/typecheck run locally.
- Task 4's crash wiring: if `web-process-terminated` proves unreachable via `with_webview`, STOP and escalate — spec §3 forbids improvising a fallback. Same for `surface_focus` failing to raise on labwc.
- Task 13 ends with the **wire-walk hard gate** (operator supplies flows greenfield) and the **operator-run live multi-window pass** (dry-run only — RADIO-1 untouched by design, consent semantics unchanged, only where the gate renders).

## Session mechanics that bit this session (so they don't bite yours)

- `cd <worktree> && git …` in one Bash call is DENIED by the main-checkout-race hook (reads payload cwd) — run a standalone `cd`-only call first, then bare git. A `run_in_background` command resets the persistent cwd afterward. Full recipe: memory `worktree-git-mechanics`.
- Commit trailers must be INLINE in the command text (heredoc `-F -`), and fresh worktrees need `pnpm install` before the pre-push docs-link hook passes.
- Another live session holds the main checkout (`bd-tuxlink-ant8s/ardop-connect-fixes`) and the A/B experiment sessions (`bd-tuxlink-c5ckf/*`) come and go — expect the hook to be strict all session.

## State

- **bd:** `tuxlink-dmwte` in_progress (notes carry the same pointers); stations.json bug filed (P2, open); P3s `tuxlink-a54y0` / `tuxlink-y6195` untouched this session (still on the board).
- **Main checkout:** untouched all session (operator state).
- **Worktrees:** `bd-tuxlink-dmwte-dockable-surfaces` (KEEP — execution home; clean, pushed). `bd-tuxlink-dmwte-handoff-plan6` (this handoff's vehicle — disposed per ADR 0009 after the push; if you find it alive, the disposal was interrupted: inventory → rm → prune).
- **No stashes created.** Visual-companion server stopped; mockups persist under `.superpowers/brainstorm/1535596-1784139788/` (gitignored).
- **Operator gates pending downstream:** converged-build smoke still gates the release unfreeze (pre-existing, `tuxlink-t8c0`); this feature's own live pass comes at Task 13.
