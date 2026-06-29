# Handoff — Elmer pane: full BRF pipeline done + crate foundation built (Tasks 0-3)

**Date:** 2026-06-28
**Agent:** redwood-falcon-bluff
**One-line:** The Elmer in-app assistant pane (bd `tuxlink-13v2l`) has a complete, 3-round-reviewed implementation plan and its crate foundation (Tasks 0-3) built, committed, and pushed as draft PR #949; the next session compiles-via-CI then executes Tasks 4-12.

---

## 1. What this session did

Ran the full `build-robust-features` pipeline for Elmer (gated on the locked `tuxlink-2ouqf` taint decision), then started execution:

- **5-round cross-provider adversarial review** of the pane design (Codex gpt-5.5 + 4 Claude lenses: security/injection, correctness/architecture, field-operator UX, completeness). Strong convergence; Codex added two P0s the Claude lenses missed (clear_taint reopens a stale arm; cancel≠abort on the in-flight path). → **15 acceptance criteria AC-1..AC-15**.
- **Implementation plan** at [`docs/plans/2026-06-28-elmer-pane-plan.md`](../../docs/plans/2026-06-28-elmer-pane-plan.md) — 13 subagent-ready tasks (0-12, 8 split a/b/c) + fallback Appendix A.
- **3-round plan review** (R1: subagent-readiness + code-correctness + security-completeness → v2; R2: security-closure + code-correctness → v3). **All P0s closed and test-pinned.** Notably overturned the spine's ARCH-2 ("call McpState ports directly") — that bypasses taint, which is a *router* side-effect.
- **Executed Tasks 0-3** (the crate foundation) via subagent-driven-development; parent-verified + committed each.

## 2. The load-bearing security architecture (bake into Tasks 4-12)

From the adrev + 2ouqf (full ACs in `dev/adversarial/2026-06-28-elmer-pane-consolidated.md` — **local-only, gitignored, on this machine's disk**):

- **AC-1:** the in-process invoker dispatches through the `TuxlinkMcp` **router** (taint is set in the router `#[tool]` methods, NOT the ports — calling ports directly silently skips taint). Plan of record: in-memory rmcp duplex; fallback = name-dispatch shim (Appendix A). **First test to write: taint-parity** (`message_read` via the invoker → `guard.is_tainted()` true).
- **AC-3 (M2):** the seven gated egress tools are **withheld** from the agent's `tools()`; transmit happens ONLY via the operator-driven, **digest-bound** `elmer_connect`. The **re-digest at flush is the security boundary**, not the staging-freeze flag (a concurrent UDS client shares the outbox).
- **AC-2:** `quarantine_and_rearm` (built, Task 1) clears taint + sets fresh TTL atomically; `ElmerSession::rearm` drops ALL conversation turns + single-flight, deadlock-free **two-lock** design (op_lock + std-mutex `inner`; the run task owns its conversation by value, never re-locks `inner`).
- **AC-4:** cancel is **abort-FIRST** (issue ungated aborts before awaiting the run); wired to the app **Quit** path (NOT `CloseRequested` — that fires on tray-minimize).

## 3. Branch / PR / worktree state

- **Branch:** `bd-tuxlink-13v2l/elmer-pane` (off origin/main), **draft PR #949**. Tasks 0-3 committed + pushed (commits `2a569be6`..`f1af10f8`). bd `tuxlink-13v2l` is `in_progress`, claims this worktree.
- **Worktree:** `worktrees/bd-tuxlink-13v2l-elmer-pane/`. Working tree clean except this handoff.
- **Gitignored-but-stateful on disk (NOT in the repo — next session needs these, they're local-only on this Pi):**
  - `dev/adversarial/2026-06-28-elmer-pane-consolidated.md` — **the AC-1..AC-15 definitions** (the plan cites this path).
  - `dev/adversarial/2026-06-28-elmer-pane-codex.md` — raw Codex transcript.
  - `dev/scratch/elmer-pane-design-brief.md` — the adrev target brief.
  - `.superpowers/sdd/` — the SDD progress ledger + task briefs + implementer reports.

## 4. What's built vs remaining

**Built (Tasks 0-3, in PR #949):** `tuxlink-agent-frontend` lib crate extracted from bin-only d3zwe (provider/endpoint/mcp_client; `endpoint.rs` SEC-5 parent-verified complete); `quarantine_and_rearm`; `ToolOutcome::Cancelled` + `Conversation::{from_messages,push_user}` + exhaustive-match fix; `run_with_conversation`.

**Remaining (Tasks 4-12):** Task 4 in-process executor (router dispatch + egress withholding) — **starts with the rmcp-duplex spike (Step 0)**; Task 5 `OutboxReadPort`; Task 6 scoped approval + digest-gated flush; Task 7 `ElmerSession`; Task 8a/b/c provider+endpoint / commands / abort-on-quit; Tasks 9-12 React pane + arm affordance + approval manifest + mount.

## 5. Critical first actions for the next session (do NOT skip)

1. **Check PR #949 CI** before building anything on top. The Pi can't compile, so the foundation's compile-correctness (the implementers' flagged concerns: `thiserror` major mismatch, the `rmcp::service::RunningService`/`RoleClient` path, serde_json inference) is unverified until CI runs. **Fix any foundation compile errors first** — Task 4 depends on the new crate building.
2. **Read the plan + the local-only ACs** (`dev/adversarial/2026-06-28-elmer-pane-consolidated.md`). The plan inlines each AC's mechanism per task, but the consolidated doc is the canonical spec.
3. **Task 4 Step 0 is a spike** (does rmcp 0.8.5 serve over `tokio::io::split(tokio::io::duplex())`?). If it fails, use Appendix A — do NOT silently switch; report which path.
4. Continue subagent-driven-development from the ledger (`.superpowers/sdd/progress.md` marks Tasks 0-3 complete — resume at Task 4). Same constraints: Pi can't compile (CI gate), subagents STOP uncommitted, parent commits.
