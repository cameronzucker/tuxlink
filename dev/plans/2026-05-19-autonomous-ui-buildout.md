# Autonomous UI Build-Out — Orchestration Execution Plan

**Authored:** 2026-05-19 by `willow-cypress-heron`
**For:** a fresh Claude Code session acting as **orchestrator**
**Goal:** Build all remaining v0.0.1 UI autonomously — minimal operator interaction, two smoke milestones only.

> This is a **meta-plan** (orchestration playbook), not a per-task executable plan. It sequences two underlying plans: the existing `docs/superpowers/plans/2026-05-18-wizard-cluster-plan.md` (wizard 10/11/11.5) and a main-UI-cluster plan the orchestrator **authors in Phase 0** (Tasks 8/12-16). The orchestrator dispatches subagents per `superpowers:subagent-driven-development`, reviews their output with Codex, and gates merges per the rules below.

---

## Operator decisions (locked 2026-05-19 — do NOT relitigate)

1. **Adrev gate for main UI:** ONE consolidated main-UI-cluster technical spec + ONE cross-provider Codex adrev round + executable plan. Not the full 5-round-per-task pipeline; not zero adrev. This honors `[[no-carveout-on-cross-provider-adrev]]` (the cross-provider round is the irreplaceable value) without the Wave-1 mistake (zero adrev → revoked PRs #47-52) and without 5-round ceremony.
2. **Scope:** wizard 10/11/11.5 + main UI 8/12/13/14/15/16. Task 11 builds MOCKED (`TUXLINK_TEST_SEND_MOCK=1`); live verification deferred to operator. **Excluded:** Task 6 (live-CMS, Part-97 operator-only), Tasks 17/18/19 (AppImage/README/CI, non-UI).
3. **Merge authority:** auto-merge a task's PR once orchestrator + Codex both pass it against spec AND automated gates are green (vitest + tsc + cargo) — **for low-risk render tasks only**. Tasks touching the keyring (Task 10) or Part-97-adjacent (Task 11) **stack for the milestone smoke** instead of auto-merging.
4. **Smoke milestones:** M1 = wizard cluster complete (operator smokes full first-run flow, MOCKED test-send). M2 = main UI complete (operator smokes inbox/reading/compose/log/status/tray). Two operator touchpoints total.

---

## Environment facts (verified 2026-05-19)

- **Keyring/dbus ready headless:** `gnome-keyring-daemon`, `dbus-launch`, `secret-tool` present; session bus live at `/run/user/1000/bus`. Task 10's keyring integration test runs headlessly without operator setup. (If a subagent's test environment lacks the session bus, wrap with `dbus-run-session -- <cmd>` per the wizard plan Phase 6 recipe.)
- **vitest pinned to 2.x** (Vite 5 compat; vitest 4 needs Vite 6). Already in `package.json` devDeps after PR #72.
- **webkit TV-static fixed** (PR #73): `WEBKIT_DISABLE_DMABUF_RENDERER=1` set in `run()`. Smoke windows launch clean.
- **Wizard infra shipped** (PR #72): `src/wizard/{types.ts,wizardReducer.ts,wizardContext.tsx,Wizard.tsx,Step1Welcome.tsx}` + `src-tauri/src/wizard.rs` (skeleton bodies for the 3 write commands) + `App.tsx` routing + `get_wizard_completed`.
- **PatBackend / WinlinkBackend trait shipped** (z5f, PR #67): `src-tauri/src/winlink_backend.rs` + `pat_client.rs`. The main-UI message operations consume this surface — the orchestrator's Phase-0 spec MUST read this trait to define the IPC contract against what already exists, not invent a parallel one.

---

## Task inventory + dependency graph

```
WIZARD CLUSTER (plan exists: wizard-cluster-plan.md Phases 3/4/5)
  Task 10 (tuxlink-1r5) credentials + keyring write   [HOLD for M1 — keyring]
    └─ Task 11 (tuxlink-e4x) test-send 4-substate      [HOLD for M1 — Part 97; MOCKED build]
  Task 11.5 (tuxlink-d76) offline identity             [independent of keyring; shares wizard.rs]

MAIN UI (plan authored in Phase 0)
  Task 12 (tuxlink-zsm) Inbox/Sent tabbed view  ← ROOT (message model + folder sidebar + list)
    ├─ Task 13 (tuxlink-y5c) Message reading pane      [depends on 12's message model]
    └─ Task 14 (tuxlink-dm8) Compose window            [depends on 12's message model; separate Tauri window]
  Task 8  (tuxlink-rit) System tray + window-close     [independent]
  Task 15 (tuxlink-69z) Session log pane               [independent; consumes PatBackend.stream_log]
  Task 16 (tuxlink-hvv) Status bar                      [independent; consumes config + session state]
```

**wizard.rs collision note:** Task 10 fleshes `wizard_persist_cms`; Task 11.5 fleshes `wizard_persist_offline`; Task 11 fleshes `wizard_run_test_send`. Three different functions in one file. To avoid parallel-edit conflicts, run Task 10 FIRST (lands its wizard.rs changes), then Task 11 + 11.5 may proceed (they touch different functions; rebase on 10's merge first). Do NOT dispatch 10 and 11.5 simultaneously.

---

## Phase 0 — Author the main-UI-cluster spec + plan (orchestrator, before any main-UI subagent)

**The orchestrator does this directly — it's not a subagent task.** Output gates the entire main-UI execution.

1. **Read** the canonical UX baseline: `docs/design/v0.0.1-ux-mockups.md` §5.5 (Task 12), §5.6 (Task 13), §5.7 (Task 14), and the Task 15/16/8 sections; the mock images in `docs/design/mockups/images/`; the v0.0.1 plan `docs/plans/2026-04-22-tuxlink-v0.0.1-plan.md` Tasks 8/12-16 implementation guidance; and `src-tauri/src/winlink_backend.rs` + `pat_client.rs` (the backend surface the UI consumes).

2. **Author** `docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md`. It MUST cover (this is the airtight-spec requirement that subagent-driven-development depends on):
   - **Message data model**: the TypeScript + Rust shapes for a message (id, UTC sent/received, from/to, subject, attachment indicator, compressed-size, body, routing/transport). Define against what `WinlinkBackend`/`PatBackend` actually returns — cite the trait methods (`list_messages`, `read_message`, etc.). If the trait lacks a needed method, the spec names the addition + which task owns it.
   - **IPC command contract**: the Tauri commands the frontend invokes (e.g., `list_messages(folder)`, `read_message(id)`, `send_message(draft)`, plus the `stream_log` event channel for Task 15). Async signatures, error types (mirror the `WizardError` discriminated-union pattern from `src/wizard/types.ts`).
   - **App-shell layout**: the post-wizard main shell (folder sidebar + tabbed message list + reading pane). Per design doc anti-pattern rule: NO full-view-swap on selection. Define the React component tree + which component owns selection state.
   - **Per-screen behavior + state**: Task 12 (folder sidebar: Inbox/Outbox/Sent/Drafts/Deleted, Templates disabled placeholder; column model matching Express widths, persisted overrides), Task 13 (reading pane + inline attachments + header strip with routing), Task 14 (separate Tauri window per locked decision #2 — NOT a Radix dialog), Task 15 (session-log pane consuming the stream), Task 16 (status-bar fields), Task 8 (tray + window-close-to-tray behavior).
   - **Test list per task** (5-10 cases each, not 24+): the unit tests subagents will TDD against.
   - **Cross-task ownership map**: which task creates which file; shared files (e.g., the message-model types) owned by Task 12 (the root).

3. **Codex cross-provider round** (the locked-decision requirement): run ONE round against the spec.
   ```bash
   npx --yes @openai/codex exec "Review docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md for the tuxlink v0.0.1 main-UI cluster (Tasks 8/12-16). Tauri 2 + React 18 + TS. Backend is the existing WinlinkBackend trait (src-tauri/src/winlink_backend.rs). Attack: message-model/IPC-contract mismatches with the trait, missing error states, the NO-full-view-swap layout invariant, the separate-Tauri-window decision for compose, state-ownership ambiguities that would make parallel subagent execution conflict. Distinguish VERIFIED-from-source vs INFERRED. Under 1500 words." 2>&1 | tee dev/adversarial/2026-05-19-main-ui-cluster-codex.md
   ```
   Apply findings; revise the spec. (Codex daily-quota caveat per `[[codex-quota-gotcha]]`: if it returns a usage-limit error, that's a capacity-defer — wait for reset or defer the round, do NOT substitute a Claude agent for the cross-provider round.)

4. **Author** `docs/superpowers/plans/2026-05-19-main-ui-cluster-plan.md` — task-by-task with TDD recipes (failing test → impl → green), exact file paths, behavior deltas, per-task completion checks, and the review-loop preamble. Model it on the wizard-cluster-plan's structure.

5. **Commit** the spec + plan via a worktree+PR (`bd create` a planning issue or attach to an existing one; per ADR 0008). This is a docs PR — auto-mergeable after orchestrator self-review (no Codex re-review needed for the plan doc itself; the spec already had its round).

---

## Phase 1 — Execute wizard cluster (existing plan)

Plan of record: `docs/superpowers/plans/2026-05-18-wizard-cluster-plan.md` Phases 3, 4, 5.

- **Task 10** (`tuxlink-1r5`, Phase 3): dispatch a subagent. The keyring write + `wizard_persist_cms` body + Step2Credentials.tsx + validators + integration test. Adds the `keyring` crate (per AMD-14). **HOLD for M1 smoke — do not auto-merge** (keyring-class). Orchestrator + Codex review; stack the PR.
- **Task 11.5** (`tuxlink-d76`, Phase 4): after Task 10's branch merges OR on a branch rebased on Task 10. Step2OfflineIdentity.tsx + `wizard_persist_offline` body. Lower-risk; but it completes the wizard so it rides to M1 anyway.
- **Task 11** (`tuxlink-e4x`, Phase 5): after Task 10. Step3TestSend.tsx + `wizard_run_test_send` (MOCKED via `TUXLINK_TEST_SEND_MOCK=1`) + 4-substate UI + Part-97 dedup-guard test + log streaming. **HOLD for M1 — Part-97-adjacent.**

**Wizard subagent guardrails** (from the wizard plan, non-negotiable): NO live CMS transmission from subagents; `TUXLINK_TEST_SEND_MOCK=1` set for any `pnpm tauri dev` or test; the `BEGIN_TEST_SEND` dedup-guard reducer test is required (already shipped in PR #72's reducer — Task 11 wires the command side); callsign normalization in both frontend + Rust; the `tokio::sync::Mutex` on the 3 write commands (already in `WizardMutex`).

**→ MILESTONE M1.** When 10 + 11.5 + 11 are reviewed + gates green + stacked: surface to operator. Operator smokes the full first-run wizard (`pnpm tauri dev` from the integration worktree, MOCKED test-send). On approval, merge the wizard PRs to `feat/v0.0.1`. Close `tuxlink-1r5` / `tuxlink-d76` / `tuxlink-e4x`.

---

## Phase 2 — Execute main UI (plan authored in Phase 0)

Dispatch order respecting the dep graph:

- **Wave 2A** (immediately, parallel — all independent): Task 8 (tray), Task 15 (session log), Task 16 (status bar). Three parallel subagents, three worktrees, three separate file sets. Auto-merge each after orchestrator + Codex review + green gates (all are low-risk render tasks).
- **Wave 2B** (parallel with 2A): Task 12 (inbox/sent — ROOT). One subagent. Owns the shared message-model types. Auto-merge after review + gates. **Task 13 and 14 are BLOCKED until 12 merges** (they import the message model).
- **Wave 2C** (after Task 12 merges): Task 13 (reading) + Task 14 (compose) — two parallel subagents, rebased on 12's merge. Auto-merge each after review + gates.

**Main-UI subagent guardrails:**
- Each subagent reads: CLAUDE.md, the main-UI spec (Phase 0 output), the main-UI plan, `docs/pitfalls/implementation-pitfalls.md` (SCOPE-1 — client not gateway), `docs/pitfalls/testing-pitfalls.md` §9 (native-menu/rendering — operator smoke is the only runtime gate; static tests verify model not widgets).
- Subagents CANNOT smoke (headless). They ship green automated gates + the PR; the orchestrator + Codex review against spec; visual verification is deferred to M2.
- Per-task worktree via `new_tuxlink_worktree.py --issue <id> --slug <slug>`; `EnterWorktree` before committing; `gh pr merge --merge --delete-branch` (no squash); dispose worktree after merge.

**→ MILESTONE M2.** When 8/12/13/14/15/16 are merged (auto-merged) to `feat/v0.0.1`: surface to operator. Operator smokes the full app — first-run wizard → main shell → inbox/sent → reading pane → compose window → session log → status bar → tray. On approval, the v0.0.1 UI is complete. Close `tuxlink-rit` / `tuxlink-zsm` / `tuxlink-y5c` / `tuxlink-dm8` / `tuxlink-69z` / `tuxlink-hvv`.

---

## Review model (per subagent, every task)

1. Subagent does TDD, runs `vitest` + `tsc --noEmit` + `cargo test` + `cargo build`, opens a PR with the test-plan checklist.
2. **Orchestrator review:** read the diff against the spec. Verify: the spec's behavior is implemented, the test list is covered, no scope creep, no SCOPE-1 violation (gateway functionality), the IPC contract matches the trait.
3. **Codex review:** `npx --yes @openai/codex exec "Review commit <SHA> on branch <branch> against docs/superpowers/specs/2026-05-19-main-ui-cluster-design.md §<task>. Attack: spec-contract drift, missing error states, state-ownership bugs, React anti-patterns (full-view-swap, effect races), Rust async/await + Tauri command correctness. VERIFIED vs INFERRED." 2>&1 | tee dev/adversarial/2026-05-19-<task>-codex.md` (per `[[codex-post-subagent-review]]` — parent-level Codex round on subagent commits catches self-review bias).
4. **Decision:**
   - Low-risk render task + both reviews pass + gates green → **auto-merge** (`gh pr merge --merge --delete-branch`), dispose worktree, `bd close`.
   - Keyring (Task 10) / Part-97 (Task 11) / OR a review found a substantive issue → **hold**; if issue, send findings back to a follow-up subagent dispatch (do NOT fix silently — re-dispatch with the Codex findings per the receiving-code-review discipline).
   - At a milestone → stack for operator smoke regardless of risk class.

---

## Standing guardrails (the whole run)

- **Main checkout is operator state** (`[[main-checkout-is-operator-state]]`): never `git checkout` in `/home/administrator/Code/tuxlink`. Reads use `git show <ref>:<path>`. All writes in worktrees. `EnterWorktree` to move the session cwd (a bash `cd` doesn't update it; the race hook reads session cwd).
- **Hooks are authoritative** (`[[stale-lease-means-worktree]]`): a hook denial means adapt the workflow, never patch the hook, never `--no-verify`.
- **No ceremony spiral** (`[[no-ceremony-spiral-on-small-fixes]]`): if a review finds a 5-minute fix, dispatch it directly; don't author a remediation sub-plan.
- **No atomic decisions to operator** (`[[no-atomic-decisions-to-operator]]`): converge implementation details with Codex; only M1/M2 smokes + genuine shape-changes reach the operator.
- **Browser-smoke-before-ship** (`[[browser-smoke-before-ship]]`): automated green ≠ verified UI. The milestone smokes are the real gate. Don't claim a UI task "works" — claim "automated gates green; smoke pending."
- **Part 97**: no live transmission from any autonomous context, ever. Task 11 MOCKED. Task 6 untouched.
- **Codex quota** (`[[codex-quota-gotcha]]`): a usage-limit error is capacity-defer (wait/defer), not skip, not substitute-Claude.

---

## What reaches the operator (and nothing else)

1. **M1 smoke request** — "wizard cluster ready; smoke the first-run flow."
2. **M2 smoke request** — "main UI ready; smoke the full app."
3. **A genuine blocker** — a hook denial you can't resolve via worktree, a spec ambiguity Codex can't converge, a Part-97 gate, or a Codex quota exhaustion that stalls the run. Surface with a specific question, not a status dump.

Everything else — per-task PRs, Codex reviews, auto-merges, worktree disposal, bd closes — happens without operator interaction.
