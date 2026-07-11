# Handoff — 2026-07-10 — `tanager-sequoia-opossum` — P2P plan complete, 3 plan-review rounds folded, ready for execution

Picks up from `harrier-glade-osprey`'s 2026-07-10 handoff (design spec + 5-round
adversarial review complete). **BRF Step 3 (writing-plans) and Step 4 (plan
review) are now done.** The next session executes the plan (BRF Step 5).

## What this session did

Resumed `build-robust-features` at Step 3 and produced the implementation plan,
then ran the mandatory plan-review rounds and folded every finding.

- **In-tree grounding:** five parallel read-only Explore agents pulled exact
  signatures for the VARA protocol layer, persistence patterns, session/backend
  hook sites, the MCP surface, and the finder/map frontend. Every plan task
  cites real `file:line` and mirrors a named in-tree pattern.
- **Plan written:** `docs/plans/2026-07-10-p2p-peer-model-plan.md` — 28
  test-first tasks + one review-added Task 23a, sequenced by the ADR-0018
  integration matrix (all rows land together; capability bits hide, not stub,
  unshipped rows). Each task has exact code, exact commands, and expected output.
- **Three plan-review rounds (BRF Step 4), all folded:**
  - **R1 (Codex, 10 findings)** — code-grounding lens. Verified plan claims
    against source; caught cross-surface compile ripples (shared `DialCandidate`),
    wrong error mapper, unreachable quarantine path, provenance contradiction.
  - **R2 (Claude, 13 findings)** — data-model/integration/frontend lens. Caught
    the **systemic hole**: the frontend Connect seam (`connectDispatch.ts`) never
    sends P2P intent/via/freq, so every peer dial would have shipped a stub with
    an empty store. Fixed by the new **Task 23a**.
  - **R3 (Claude re-attack)** — Codex was quota-limited (reset ~05:24), so per
    project guidance (`feedback_self_adrev_no_codex_gating`) this ran as a Claude
    self-adrev rather than deferring. It verified the fold against source and
    caught **two P1 defects the fold itself introduced** (capability-bit
    narrowing that broke 5 downstream steps → E0560; `validate_presented_callsign`
    named but wired nowhere → `W6ABC/P` dropped at the store write) plus four
    amendments that named a fix without a viable mechanism (gate-outside-lock
    misdiagnosis, conflict-evict hole, reject→limiter plumbing, connectFor vs
    FavoriteDial contradiction). All corrected.

The fold lives as a **"Review-fold binding amendments"** section near the top of
the plan (one new task + task-keyed amendments). The header instructs the
executor to apply Task N's amendment as part of Task N.

## Deliverable

- **Plan (committed + pushed):** `docs/plans/2026-07-10-p2p-peer-model-plan.md`,
  commit `051179b6` on `bd-tuxlink-c39af/vara-p2p-session`, **up to date with
  origin**. Three commits this session: `5b92f566` (draft), `8694fb76` (R1+R2
  fold), `051179b6` (R3 fold).
- **Fold ledger (gitignored, LOCAL ONLY):**
  `dev/adversarial/2026-07-10-p2p-plan-fold-ledger.md` — every finding →
  disposition across all three rounds, plus the author's 6 pre-found items.
  Raw transcripts also local: `…-plan-r1-codex.md` (20.5k lines), `…-plan-r3-codex.md`
  (quota-stub). Not on origin.

## The ONE open item — do NOT let the execution agent skip it

**The plan's "Definition of done — operator wire-walk flows" section is
`STATUS: PENDING` by design.** The wire-walk Iron Law forbids the agent from
drafting these flows — the operator supplies them greenfield at build start, and
they become the definition-of-done that Task 28 traces to code. The operator
(2026-07-10) chose to hold the flows for the execution agent rather than supply
them at plan time.

**The execution agent MUST, as its first action, ask the operator cold for the
key user flows** (short task statements, with cold-start states: fresh install,
empty roster, post-upgrade), record them verbatim in that section, and only then
begin Task 1. Task 28 (the wire-walk gate) cannot pass until they are recorded.
Do not self-generate them.

## Branch / worktree state

- Worktree `worktrees/bd-tuxlink-c39af-vara-p2p-session/`, branch
  `bd-tuxlink-c39af/vara-p2p-session`, clean, up to date with origin.
- `pnpm install` already run here; `pnpm lint:docs` green.
- Untracked/gitignored in this worktree: `dev/adversarial/*.md` (the fold ledger
  + raw review transcripts) — local dev scratch, intentionally not pushed.
- bd `tuxlink-c39af` remains `in_progress` (design + plan done, not built); note
  updated to reflect plan-complete. Coordination edge to `tuxlink-sg5zw.2`
  (telnet_p2p agent-tool rebuild consumes the peer store) still stands.
- A concurrent operator session was active on the main checkout
  (`bd-tuxlink-ant8s/ardop-connect-fixes`) during this session; the
  main-checkout race hook briefly blocked the final commit until it cleared.
  All work landed from the worktree.

## Next-session order (BRF Step 5 — execution)

1. Read the plan (source of truth) — especially the "Review-fold binding
   amendments" section; apply each Task N amendment as part of Task N.
2. **Ask the operator for the greenfield wire-walk flows and record them
   verbatim** in the Definition-of-done section (Iron Law — do not draft).
3. Execute via **`subagent-driven-development`** (recommended): fresh subagent
   per task, two-stage review between tasks. The plan is 28 mostly-sequential
   tasks with parallel leaves within a phase. Phases 0-4 are backend-Rust-heavy
   (open the PR, let CI compile — the Pi does not finish cold cargo builds);
   Phase 5 is frontend (local `pnpm vitest run <file>`). Task 25 has a
   high-fidelity-mock design gate before its code. Task 28 is the wire-walk hard
   gate needing step 2's flows.
4. The integration matrix rows must land together (ADR 0018) — a partial build
   is a stub. Capability bits hide unshipped UI rows; the 5 agent/store/protocol
   bits are informational (land atomically).

## Process notes

- Codex quota-limits mid-review: run the round as a Claude self-adrev, don't
  defer (`feedback_self_adrev_no_codex_gating`). R3 did this.
- The three review rounds converged on the frontend→backend Connect seam as the
  single highest-risk gap — the backend record sites are only reachable if a
  peer dial forces `SessionIntent::P2p`, which no original task wired. Task 23a
  is load-bearing; the wire-walk gate would (correctly) fail without it.

Agent: tanager-sequoia-opossum
