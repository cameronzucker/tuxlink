# Handoff — bison-condor-grouse — Tracks A + B mid-flight (subagent-driven execution)

> **Date:** 2026-06-01 → 2026-06-02 (session crossing midnight UTC) · **Agent:** `bison-condor-grouse` · **Machine:** pandora
>
> **Arc:** This session went 100+ messages. Started as a session-resume to fix tuxlink-i63g + the operator-reported GPS regression; ended with a comprehensive Track A position-subsystem-restoration spec (v3, 547 lines, 47 adrev findings applied) + a 15-task implementation plan + a parallel Track B (radio-panel widen + ARDOP controls relocate) — and executed via the subagent-driven-development skill until the context wall forced this handoff.
>
> **Status at handoff:** Track A T1 + T2 fully reviewed and committed. Track A T3 implementer was dispatched and was still running when the handoff was called. Track B T1, T2, T3 implementer commits all landed; T3 needs spec + code-quality reviews. PRs not yet opened for either branch.

---

## 0. Critical first action — next session

```
1. Read THIS handoff first. Especially §2 (PRs to open, reviews to finish) + §3 (the in-flight subagents — their state may have changed).
2. `bd show tuxlink-c79g` + `bd show tuxlink-jmfm` + `bd show tuxlink-8rng` — confirm bd state matches §5 below.
3. Tracks A + B branches are pushed. Spec + plan are pushed. The subagent-driven-development workflow is mid-cycle. Resume per §6 below; do NOT restart the spec/plan/adrev cycles — they are complete and operator-approved.
4. The 2-stage review pattern (spec compliance → code quality) MUST continue for every subagent commit. Skipping reviews = pjih-class regression risk (R5 #7 was explicit about this).
```

Paste-ready next-session prompt at the bottom of this doc.

---

## 1. Session arc (compressed)

1. **Operator-reported bugs at session start**: GPS regression from PR #189 (pjih) + ARDOP UI clipping (cc82bf4 was insufficient) + ARDOP controls hidden in Settings → GPS+Privacy submenu.
2. **Track A (pjih regression)**: operator confirmed *"original spec was fine. We had it working for a while. Each fix was only to address regression."* and demanded *"Codex adrev on any proposed optimal fix shape. It's just adding stacked regressions at this point."*
3. **Track B (panel + ARDOP)**: operator approved *"400 px, but not JUST ARDOP. We ran into this issue before. The Radio control pane is for ALL modes."* + *"ARDOP controls hidden in the GPS + privacy sub-menu... has to be high priority fixed."*
4. **Track A discovery**: located the 2026-05-22 position-subsystem spec → it was correct as-written, pjih violated it, two implementation gaps in the original spec (chip-as-`<button>`, row-3 Set-manually affordance) were never coded.
5. **Track A v1 spec written**, operator pushed back: *"hard to tell in this format"* + *"Too many pronoun-like references to things that aren't immediately self-evident."*
6. **Track A v2 spec**: 5-round adrev (R1 Codex + R2 UX + R3 contract + R4 tests + R5 holistic) → **47 findings (6 P0, 21 P1, 20 P2)**. All P0 + all P1 applied.
7. **Track A v3 spec**: explicit-referent rewrite per operator feedback (new vocabulary section at top; every named feature + state explicit at every reference). Saved memory: [`feedback_explicit_referents_in_specs`](../../../.claude/projects/-home-administrator-Code-tuxlink/memory/feedback_explicit_referents_in_specs.md).
8. **Track A plan**: 15-task TDD plan with bite-sized steps, complete code snippets, no placeholders.
9. **Track B plan**: 3-task TDD plan (plumbing-class per `feedback_discipline_triage_rule`; bd issues serve as spec — no full BRF pipeline).
10. **Subagent-driven execution kicked off**: T1 implementer per track in parallel; spec + code-quality reviewer per task; fix subagents when reviewer found P1.
11. **Track A T1 + T2 reviewed and committed**. Track A T3 implementer dispatched. Track B T1, T2 reviewed and committed. Track B T3 implementer dispatched and completed mid-handoff.
12. **Operator called handoff**.

---

## 2. PR state

| PR | Branch | State at handoff |
|---|---|---|
| (not opened) | `bd-tuxlink-c79g/position-subsystem-restoration` | Pushed through `8266cd6`. Track A T3 implementer may have added 1-2 more commits after handoff (commits unpushed if so — `git pull` then push). PR needs to be opened ready (not draft) per `feedback_no_draft_pr_parking`. |
| (not opened) | `bd-tuxlink-jmfm/radio-panel-400px-controls-relocate` | Pushed through `4d8035b` (T3 done). T3 needs SPEC + CODE QUALITY review before PR open. PR needs to be opened ready, not draft. |

Both PRs should be opened ready (`gh pr ready` is for converting draft → ready; opening freshly bypasses draft). Operator gates merge via smoke per the standing convention.

---

## 3. The in-flight subagents at handoff

**Track A T3 implementer** (`a403e75518ce99588`): dispatched ~5 minutes before handoff was called. Task 3 = "extend the relaxation to `position_set_source('Gps')` command" + update the stale rustdoc (P2 from T2 code-quality review).

When you resume:
```bash
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-c79g-position-subsystem-restoration pull
git -C /home/administrator/Code/tuxlink/worktrees/bd-tuxlink-c79g-position-subsystem-restoration log --oneline origin/main..HEAD | head -5
```

If new commits exist past `8266cd6`, the subagent completed work; dispatch SPEC + CODE QUALITY review for those commits per §6 workflow. If no new commits, the subagent died on the harness shutdown; re-dispatch using the same prompt template (the prompt body is in the Plan §Task 3, plus the rustdoc-fix scope extension noted in §6 below).

**Track B T3 implementer** (`a8af28a411e0224a9`): DONE before handoff. Commits `9b73157` + `4d8035b` already on the branch. **Track B T3 still needs SPEC + CODE QUALITY review.** Do those before opening the Track B PR.

---

## 4. Worktree inventory

| Worktree | Branch | Status | Owns bd issue |
|---|---|---|---|
| `worktrees/bd-tuxlink-c79g-position-subsystem-restoration/` | `bd-tuxlink-c79g/position-subsystem-restoration` | **ACTIVE — Track A mid-execution** | tuxlink-c79g (in_progress) |
| `worktrees/bd-tuxlink-jmfm-radio-panel-400px-controls-relocate/` | `bd-tuxlink-jmfm/radio-panel-400px-controls-relocate` | **ACTIVE — Track B T3 needs review** | tuxlink-jmfm + tuxlink-8rng (in_progress) |
| (30+ other worktrees from prior sessions) | various | Mostly stale; dispose per ADR 0009 when reviewed by another session |

No untracked content of concern in the Track A/B worktrees beyond `node_modules/` (regenerable) and `dev/adversarial/` transcripts (gitignored per CLAUDE.md).

---

## 5. bd state

Active (claimed, in_progress):
- `tuxlink-c79g` (P0) — Track A position-subsystem restoration. ~13 tasks remaining (T3 just finished or close to it; T4–T15).
- `tuxlink-jmfm` (P1) — Track B ARDOP controls relocate. T3 done; awaiting review + PR.
- `tuxlink-8rng` (P1) — Track B panel widening. Bundled in the same PR as jmfm.

Tasks NOT closed by this session (close when the work merges):
- `tuxlink-c79g` stays open until Track A PR merges.
- `tuxlink-jmfm` + `tuxlink-8rng` stay open until Track B PR merges.

Operator-deferred bd issues filed earlier this session that are still OPEN:
- `tuxlink-gheo` (P2) — forms::http_server nested folder bug.
- `tuxlink-4g2n` (P2) — forms::http_server asset-size cap.
- `tuxlink-rk6s` (P2) — forms::http_server bounded channel.

Plus the pre-existing P1+ queue (`tuxlink-9ky` BT, `tuxlink-0ja` TOCTOU, `tuxlink-5vx` AX.25 P4, `tuxlink-7fr` AX.25 epic, `tuxlink-su2h` Outbox enable, etc.).

---

## 6. Resume protocol — subagent-driven-development workflow

The active workflow is `superpowers:subagent-driven-development`. The pattern per task:

```
1. Dispatch implementer subagent (full task text + scene-setting + TEST-1 pitfall reminder + commit trailer)
2. Implementer returns DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED
3. Dispatch spec compliance reviewer
4. If SPEC_COMPLIANT → dispatch code-quality reviewer
5. If code-quality APPROVED → mark task complete in TodoWrite, dispatch next implementer
6. If reviewer finds issues → implementer fixes, re-dispatch reviewer
```

Critical discipline (do not skip):
- **TEST-1 pitfall** burned us once in this session. The project's `tsconfig.json` has no `@types/node`. Every test must avoid `node:fs`, `node:path`, `__dirname`, `readFileSync`. Canonical Vite-native pattern: `import.meta.glob('./X', { eager: true, query: '?raw', import: 'default' })`. Sibling at `src/forms/innerhtml-ban.test.ts`.
- **Always verify BOTH gates** before committing: `pnpm exec tsc --noEmit` AND `pnpm vitest run`. The first one catches TEST-1 violations vitest misses.
- **Run `cargo build --bin tuxlink`** on top of `cargo test --lib` for Track A — `cargo test --lib` may compile fine while the bin has a missed call-site.
- **Subagent commit trailer** is `Agent: bison-condor-grouse` (NOT the subagent's dispatch-name). Pass this through every dispatch prompt.
- **Spec compliance review first**, then code quality. Never reorder.
- **P2 findings can be deferred to a future task** if that task naturally rewrites the surface; document the deferral in the commit body.

### Track A T4 dispatch context (next on the queue if T3 is done)

T4 = restore `config_set_grid` persistence of `position_source = Manual` + restore the `arbiter_set_manual_pins_manual_source` test. Plan body for T4 is at `docs/superpowers/plans/2026-06-01-position-subsystem-restoration-plan.md` Task 4. Pin the worktree path + branch in the dispatch prompt.

### Track B T3 review-and-PR finish

Before opening the Track B PR, dispatch the SPEC reviewer for `9b73157` (the T3 code commit) + then the CODE QUALITY reviewer. After APPROVED, open the PR ready (not draft):
```bash
gh pr create --base main \
  --head bd-tuxlink-jmfm/radio-panel-400px-controls-relocate \
  --title "[bison-condor-grouse] fix(radio): 400px panel width + ARDOP controls relocate (tuxlink-jmfm + tuxlink-8rng)" \
  --body "$(cat docs/superpowers/plans/2026-06-01-radio-panel-width-and-ardop-controls-relocate-plan.md | sed -n '/^gh pr create --base main/,/^EOF$/p' | sed '1d;$d' | sed -n '/cat <</,/^EOF/p' | sed '1d;$d')"
```
(The plan file's final task has the PR body template inline.)

---

## 7. Key artifacts (all pushed)

| Artifact | Path | Branch |
|---|---|---|
| Spec v3 (operator-approved) | `docs/superpowers/specs/2026-06-01-position-subsystem-restoration-design.md` | `bd-tuxlink-c79g/position-subsystem-restoration` |
| Track A 15-task plan | `docs/superpowers/plans/2026-06-01-position-subsystem-restoration-plan.md` | same |
| Track B 3-task plan | `docs/superpowers/plans/2026-06-01-radio-panel-width-and-ardop-controls-relocate-plan.md` | `bd-tuxlink-jmfm/radio-panel-400px-controls-relocate` |
| Mailbox sort PR (merged earlier this session) | PR #201 (merged) | merged to main |
| Pjih merge that triggered this restoration | PR #189 (merged 2026-06-01 16:25 UTC; commit `a6db716`) | merged |
| Adrev raw transcripts (gitignored) | `dev/adversarial/2026-06-01-position-restoration-r{1,2,3,4,5}*.md` | Track A worktree (local-only) |

---

## 8. New memory saved this session

`feedback_explicit_referents_in_specs.md` — mandatory in spec docs: name the feature + state at every reference; "the chip" / "this state" / "it" force reviewers to scroll-resolve and is a pet peeve. Triggered by Track A v2 → v3 spec rewrite.

---

## 9. Next-session prompt (paste this into a fresh session)

```
Resume Tracks A + B from the bison-condor-grouse 2026-06-01 → 2026-06-02 handoff.

Handoff doc: dev/handoffs/2026-06-01-bison-condor-grouse-tracks-a-and-b-midflight.md
READ IT FIRST — especially §0 critical first action + §3 (in-flight subagent state may have changed) + §6 (subagent-driven-development resume protocol).

Current state:
- Track A (tuxlink-c79g, position-subsystem-restoration): T1 + T2 reviewed and committed; T3 implementer was in-flight at handoff. Branch pushed through `8266cd6`. ~13 tasks remaining.
- Track B (tuxlink-jmfm + tuxlink-8rng, radio-panel-400px-controls-relocate): T1 + T2 reviewed and committed; T3 implementer DONE (`9b73157`) but NOT YET reviewed. After T3 reviews land, open the Track B PR ready (not draft).
- Both PRs are NOT yet opened. Track B opens first (only 3 tasks; ready after T3 reviews).
- TEST-1 pitfall burned us once — every test must avoid `node:fs` / `node:path` / `__dirname`; use `import.meta.glob` raw-CSS pattern. Always run `pnpm exec tsc --noEmit` alongside `pnpm vitest run`.
- The subagent-driven-development workflow is the active execution mode. Two-stage review (spec → code quality) per task. Do NOT skip reviews.

Do NOT restart the spec/plan/adrev cycles for Track A — they are operator-approved and pushed. The 47 adrev findings are applied in spec v3. The plan is the canonical task decomposition.

First check: `git -C worktrees/bd-tuxlink-c79g-position-subsystem-restoration pull` — does it surface commits past `8266cd6`? If yes, dispatch SPEC + CODE QUALITY review for those commits (Track A T3). If no, re-dispatch T3 implementer per the plan body.
```

---

Agent: bison-condor-grouse
