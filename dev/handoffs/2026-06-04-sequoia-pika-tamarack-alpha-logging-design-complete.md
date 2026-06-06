# Handoff — sequoia-pika-tamarack — alpha-logging design phase complete; execution next

> **Date:** 2026-06-04 · **Agent:** `sequoia-pika-tamarack` · **Machine:** pandora
>
> **Arc:** Marathon brainstorm-to-plan session for the alpha-logging feature per the prior handoff's BRF directive. Six hours of design work: brainstorming → spec → spec-adrev (Codex) → spec v2 → writing-plans → plan → plan-adrev (Codex) → spec v2.1 + plan v2.1. All committed; PR-ready branch + bd issue claimed.
>
> **Status at handoff:** Design phase **complete**. Execution phase ready to start in a fresh session.

---

## 0. Critical first action — next session

```
1. cd worktrees/bd-tuxlink-qjgx-alpha-logging/
2. Read THIS handoff (you're here).
3. Read the PLAN: docs/superpowers/plans/2026-06-04-alpha-logging-implementation.md
   - Tasks 1-11 with TDD subtasks
   - "Plan v2.1 — Amendments" section before Self-review (8 amendments
     the executor merges into the corresponding original subtasks)
   - Self-review's plan-adrev disposition table maps every Codex finding
     to its location in the plan
4. Read the SPEC: docs/superpowers/specs/2026-06-04-alpha-logging-design.md
   (1500 lines; §10 acceptance criteria are the gate)
5. Invoke superpowers:subagent-driven-development to execute the plan
   task-by-task with two-stage review between tasks
   - Dispatch one subagent per Task (1 through 11)
   - Review subagent output before merging to branch
   - At completion of Task 11, run the build-phase Codex adversarial round
     per spec §10.8
6. PR creation: bd-tuxlink-qjgx/alpha-logging → main, after all 11 tasks
   land + the build-phase Codex round addresses any findings.
```

**Critical reads in priority order** — do NOT skip:

- The plan (§Plan v2.1 — Amendments is essential; ignoring it ships the bugs Codex caught)
- The spec (§10 acceptance criteria define "merged")
- This handoff §3 (worktree state)

---

## 1. What's on the branch

Branch: **`bd-tuxlink-qjgx/alpha-logging`** (worktree at `worktrees/bd-tuxlink-qjgx-alpha-logging/`)
bd issue: **`tuxlink-qjgx`** (claimed; P1; feature)

Commit chain:

| Commit | What |
|---|---|
| `c2a94c6` | Plan v1 (5831 lines) |
| `a1ad92d` | Spec v2 imported from operator's `bd-tuxlink-xygm` branch (self-contained PR) |
| `ef462a4` | Spec v2.1 amendments per plan-adrev: cms_health module placement + dict-validation mechanism |
| `ef1fbb4` | Plan v2.1 — addresses 3 CRITICAL + 8 HIGH + 3 MEDIUM plan-adrev findings (6394 lines) |
| THIS DOC | Session-end handoff |

All commits pushed to `origin/bd-tuxlink-qjgx/alpha-logging`.

---

## 2. The design phase, in compressed form

### What got built

**Brainstorm (operator + agent):** robust + compact + portable diagnostic logging for the desktop app. Single-click `.tar.zst` export. Six environment probes. Detailed-mode Off/On/Bounded UX. 14-day / 500 MB default retention. Help → Logging window. Help → Report Issue auto-export + GitHub URL pre-fill.

**Spec v1 (`5dce086` on operator's branch):** 875 lines covering architecture, schema, redaction, storage, compression, UI, env probes, acceptance, smoke plan, deferrals, risks, memory references, implementation rollout.

**Spec-adrev (Codex):** 1 CRITICAL + 16 HIGH + 22 MEDIUM findings. CRITICAL was the wire-text leak (`;PR:` plaintext) that field-name redaction couldn't catch.

**Spec v2 (`4128a25` on operator's branch):** addressed every finding inline. Grew 875 → 1497 lines. Key fixes: WireSanitizer (§5.6), fanout-architecture-correction (immutable tracing events), spans-as-array (§3.1), source-verified credential audit (§5.3 — fabricated names replaced with `ExchangeConfig`), 35 expanded acceptance criteria (§10), six probes hard alpha requirement (§9).

**Plan v1 (`c2a94c6`):** 5831 lines, 11 tasks, ~163 TDD subtasks. Sequencing rule: redaction + tests BEFORE emissions. Big-bang single-PR shape per operator direction.

**Plan-adrev (Codex):** 3 CRITICAL + 8 HIGH + 3 MEDIUM findings. Two were spec-level (cms_health placement; dict-validation mechanism); the rest were plan-level.

**Spec v2.1 (`ef462a4`):** §17 records the two amendments (cms_health moved to crate root; dict validation via known-input roundtrip).

**Plan v2.1 (`ef1fbb4`):** addressed all 14 plan-adrev findings. Inline fixes for CRITICALs + most HIGHs in their original subtasks; new "Plan v2.1 — Amendments" section (A through H) consolidating the larger architectural fixes the executor merges into corresponding subtasks. Self-review section's disposition table maps every finding.

### Why this was so much work for "just a plan"

Each adrev round caught real correctness flaws — not stylistic preferences. The spec round caught a CRITICAL credential leak path. The plan round caught CRITICAL Rust API misuse (Layer impl shape; Cargo features) that would have surfaced as compile errors during execution. Skipping either round per `no-carveout-on-cross-provider-adrev` would have produced revoked work.

The investment matches operator's `alpha-is-vettedness-not-built-ness` posture: alpha ships polished or doesn't ship.

---

## 3. Worktree state at handoff

**Worktree:** `worktrees/bd-tuxlink-qjgx-alpha-logging/`

- Branch: `bd-tuxlink-qjgx/alpha-logging` (tracking `origin/bd-tuxlink-qjgx/alpha-logging`)
- HEAD: this handoff doc's commit (when it lands)
- Tracked dirty: this handoff doc only (uncommitted at the moment of writing this paragraph; landed by the time you read it)
- Untracked: none beyond the standard `node_modules/`, `target/`, etc.
- Gitignored on disk: `node_modules/` (~600 MB; installed for pre-push hook); `dev/adversarial/` (Codex transcripts; per-machine, NOT pushed):
  - `dev/adversarial/2026-06-04-alpha-logging-spec-codex.md` — 12222 lines (spec-adrev)
  - `dev/adversarial/2026-06-04-alpha-logging-plan-codex.md` — 14578 lines (plan-adrev v1, context-exhausted before synthesis)
  - `dev/adversarial/2026-06-04-alpha-logging-plan-codex-v2.md` — 8552 lines (plan-adrev v2, real findings — REFERENCE for any post-merge questions)
- `git stash list`: empty

**Disposal:** the worktree is ACTIVE through execution. Do NOT dispose. After execution + PR merge, follow ADR 0009 ritual.

**Adversarial transcripts:** are local-only per CLAUDE.md ("dev/adversarial/ is .gitignored"). The plan-adrev-v2 transcript is the canonical reference for plan-adrev findings; the plan's Self-review disposition table cites it. If the next session needs to understand WHY a Plan v2.1 amendment exists, that transcript has the original Codex reasoning.

---

## 4. Execution checklist (what the next session does)

The next session's job is **execute Tasks 1-11 of the plan**. Following BRF discipline + subagent-driven-development:

1. **Pick a moniker** (`python3 .claude/scripts/get_agent_moniker.py`) — fresh moniker for the execution session.
2. **Read the plan + spec** (as priority-ordered in §0 above).
3. **For each Task 1-11**:
   - Dispatch a fresh subagent with the task's full subtask content as the prompt
   - The subagent reads test-driven-development skill BEFORE coding (per TDD discipline)
   - The subagent commits per-step per the plan's commit checkpoints (each subtask ends with a `git commit` step)
   - On completion, review the subagent's work: read the commits, run the tests, verify acceptance maps to spec §10
   - Apply amendments A-H per the plan's amendment section (folded into the corresponding original subtask)
4. **After Task 11**, run the **build-phase Codex adversarial round** per spec §10.8 (the spec-phase round is already done; build-phase reviews the actual implementation diff against the spec):

   ```bash
   # Prompt is in plan's Task 11.1.1 ("Create the Codex prompt")
   cat /tmp/codex-impl-adrev-prompt.txt | npx --yes @openai/codex review - 2>&1 \
     | tee dev/adversarial/2026-06-04-alpha-logging-impl-codex.md
   ```

   Address findings inline (CRITICAL/HIGH before merge; MEDIUM as bd follow-ups; LOW as bd follow-ups).
5. **Verify smoke** — `bash scripts/tuxlink-logging-smoke.sh` exits 0 with all hard-gate tests passing.
6. **PR creation:**
   ```bash
   gh pr create --base main --head bd-tuxlink-qjgx/alpha-logging \
     --title "[<moniker>] alpha-logging — robust+compact+portable diagnostic logging" \
     --body "$(cat <<'EOF'
   Implements docs/superpowers/specs/2026-06-04-alpha-logging-design.md per
   docs/superpowers/plans/2026-06-04-alpha-logging-implementation.md.

   ... full PR body matching spec §10 acceptance criteria ...
   EOF
   )"
   ```

### Estimated subagent dispatch budget

- 11 tasks × ~1 subagent each = 11 subagent rounds
- Each subagent ~30-90 minutes including review
- Plus Codex build-phase round (~10 minutes)
- Plus PR creation + any iteration on review feedback

Realistic: 8-15 hours of agent-active time spread across however many calendar sessions the operator wants.

---

## 5. Open carry-over

| Issue | State | Notes |
|---|---|---|
| `tuxlink-qjgx` | open (claimed by sequoia-pika-tamarack via worktree script) | The execution session updates status as it progresses |
| Spec v2 commits on operator's `bd-tuxlink-xygm` branch | uncommitted-but-pushed pattern from prior session | The worktree's branch has the spec cherry-picked-equivalent (commit a1ad92d imports the file). When the operator's recover-handoffs branch eventually merges, it'll duplicate the spec — acceptable since both commits have the same content. |
| Other prior-session handoff docs (in `dev/handoffs/`) | untracked on main checkout | Operator-owned; not for this session to touch. |

No other open work from this session.

---

## 6. Reference material the execution session will need

| Path | Purpose |
|---|---|
| `docs/superpowers/specs/2026-06-04-alpha-logging-design.md` | THE spec; §10 is the acceptance gate |
| `docs/superpowers/plans/2026-06-04-alpha-logging-implementation.md` | THE plan; Tasks 1-11 + Amendments A-H |
| `dev/adversarial/2026-06-04-alpha-logging-spec-codex.md` (gitignored) | Spec-adrev transcript |
| `dev/adversarial/2026-06-04-alpha-logging-plan-codex-v2.md` (gitignored) | Plan-adrev v2 transcript (the one with real findings) |
| `CLAUDE.md` | Project rules — moniker, worktree discipline, RADIO-1, destructive-git ban |
| `src-tauri/src/session_log.rs` | Existing UI ring buffer the plan extends (allocate_seq + append_with_seq) |
| `src-tauri/src/help_window.rs` | Pattern the new `logging_window.rs` mirrors |
| `src-tauri/src/lib.rs` | Where `logging::init()` wires into `.setup()` |
| `src-tauri/src/winlink/handshake.rs` | The `;PR: {response}\r` emission site needing WireSanitizer (CRITICAL spec finding) |
| `src-tauri/src/winlink/session.rs` | Where `ExchangeConfig` lives + needs manual `Debug` |

---

## 7. Out-of-repo state at handoff

| Path | Change | Reversible? |
|---|---|---|
| `dev/adversarial/` (gitignored) | 3 Codex transcripts added (spec-adrev + plan-adrev v1 + plan-adrev v2) | n/a; local-only |
| Auto-memory at `~/.claude/projects/.../memory/` | None added this session | n/a |
| bd memories | None added via `bd remember` this session | n/a |
| bd issue tracker | One issue created + claimed (`tuxlink-qjgx`) | bd close `tuxlink-qjgx` after merge |
| Worktree on disk | One new worktree `worktrees/bd-tuxlink-qjgx-alpha-logging/` | Yes (ADR 0009 ritual after merge) |
| node_modules in worktree | Installed for pre-push hook (~600 MB) | Yes (`rm -rf`) |

---

## 8. Session totals

- **6 commits** on `bd-tuxlink-qjgx/alpha-logging` (1 plan v1 + 1 spec import + 1 spec v2.1 + 1 plan v2.1 + this handoff + 1 worktree-internal `.beads/issues.jsonl` byproduct)
- **2 commits** earlier on `bd-tuxlink-xygm/recover-handoffs` (operator's branch) for spec v1 + spec v2 (those were committed before worktree existed)
- **2 Codex adrev rounds** completed (spec + plan v2; plan v1 was context-exhausted and re-run)
- **0 emission sites added** (this session built the design; execution adds them)
- **0 RADIO-1 risk** at any point in this session
- **~6 hours** of agent active time (give or take)

---

## 9. Risks the next session should manage

- **Big-bang PR shape:** the plan is one PR with ~50 subtasks. Mid-PR partial progress is OK if it doesn't violate the sequencing invariant (redaction defenses before emissions). If a subagent's work fails review, fix forward in the same branch; do NOT split into a separate PR.
- **Codex quota:** the build-phase Codex round (Task 11) needs quota at execution end. Per `feedback_codex_quota_gotcha`, if quota is hit, defer the round — do NOT substitute Claude.
- **Worktree disk:** ~600 MB of node_modules + future cargo `target/` (will be GBs). On Pi-class hardware, monitor disk before each task.
- **Merge conflict risk:** four other worktrees are active (per the multi-session contention this session hit). Coordinate windows when landing the alpha-logging PR; consider rebasing onto main mid-task if needed.

---

## 10. Next-session prompt (paste into a fresh Claude Code session)

```
Resume alpha-logging: execution phase. Brand-new session in a fresh moniker.

Worktree: cd worktrees/bd-tuxlink-qjgx-alpha-logging/
Branch:   bd-tuxlink-qjgx/alpha-logging
bd issue: tuxlink-qjgx (claimed)

READ FIRST (in order):
  1. dev/handoffs/2026-06-04-sequoia-pika-tamarack-alpha-logging-design-complete.md
  2. docs/superpowers/plans/2026-06-04-alpha-logging-implementation.md
     ESPECIALLY the "Plan v2.1 — Amendments" section before Self-review
  3. docs/superpowers/specs/2026-06-04-alpha-logging-design.md
     (§10 acceptance criteria are the gate)

Then invoke superpowers:subagent-driven-development to execute Tasks 1-11.
One subagent per task; review subagent output before next dispatch.
Task 11 is the build-phase Codex adversarial round (per spec §10.8) —
do NOT skip it.

DO NOT skip the Amendments section. Plan v2.1 addresses real Codex findings
(3 CRITICAL + 8 HIGH + 3 MEDIUM) and the executor merges each amendment
into the corresponding original subtask. The Self-review section's
disposition table maps every finding to its location.

Pre-flight: pick a fresh moniker (python3 .claude/scripts/get_agent_moniker.py).
This worktree is the work surface; commits land on bd-tuxlink-qjgx/alpha-logging.
```

---

Agent: sequoia-pika-tamarack
