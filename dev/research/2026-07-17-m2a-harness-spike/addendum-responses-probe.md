# Addendum — post-hoc probe #2: pi-e122-r5 over the Responses route

- **Orchestrator:** hemlock-maple-clover (Fable), 2026-07-18 (~03:20Z–03:40Z)
- **Status:** POST-HOC, flagged per rubric. Executes mandatory work item (1)
  from `report.md` §Verdict: re-probe the E122 rung-5 cell through Pi's
  `api: "openai-responses"` before committing the supervision-tier design.
- **Machinery:** `pi-openrouter-responses.js` + `run-responses-probe.sh`
  (this dir). One variable changed vs the registered `pi-e122-r5` cell: the
  provider registration's wire route (`openai-responses` instead of the
  builtin catalog's `openai-completions`). Held constant: model
  (`qwen/qwen3.5-122b-a10b` via OpenRouter), frozen rung-5 brief text,
  job/report contract, `--thinking medium`, 30-min cap, `-ne -ns --offline`,
  worker base `b82b404d`, worktree
  `worktrees/bd-tuxlink-7raoe-m2a-pi-e122-r5-responses/`.

## Pre-flight smoke test (route capability)

Direct `curl` to `https://openrouter.ai/api/v1/responses` with this model,
`reasoning: {effort: "medium"}`, trivial single-turn prompt: returned a
completed `reasoning` output item, **111 reasoning tokens** on a one-word
answer. The route demonstrably preserves and returns reasoning for this
model. (Compare the completions-route thinking-high probe: ≈38 reasoning
tokens across a whole 40-turn session.)

## Run record

- 03:22:06Z false start: first dispatch ran ~80 s under the harness Bash
  tool whose 10-min timeout could have truncated the envelope; killed
  (worker tree verified untouched; session jsonl retained in sdd) and
  relaunched detached. Not counted as an attempt.
- 03:23:24Z attempt-1 dispatched (setsid/nohup; only the script's
  `timeout 1800` governs).
- 03:28:30Z attempt-1 FINISHED, exit 0 — **5 m 06 s wall-clock, clean
  completion**, vs 30-min AT-CAP on all three completions-route runs
  (pi-e122-r5 a1, a2, thinking-high probe).
- Usage (session jsonl): 25 assistant turns, 868k in / 21k out,
  **reasoning ≈ 34 tokens total** (2 thinking blocks, both in the opening
  turns; `usage.reasoning: 0` on every subsequent turn). Cost ≈ $0.27.
- Deliverables: full report at `.superpowers/sdd/rung-5-report.md`, Status
  DONE contract honored, minimal 1-file diff, no git commands, gates run.
- Orchestrator verification: `pnpm typecheck` green and
  `pnpm vitest run src/aprs/useEnvStations.test.ts` 7/7 green re-run from
  the worker tree; diff read in full. Worker claims honest (no
  fabrication events).
- Candidate diff committed on local-only arm branch
  `bd-tuxlink-7raoe/m2a-pi-e122-r5-responses` (NEVER MERGE), commit
  `da1057db`.

### Incident: nested-worktree exposure window (integrity audit)

At 03:23:57Z the orchestrator accidentally created an unrelated worktree
(`bd-tuxlink-gac1d-allow-emit`, checked out at origin/main — which carries
the grading keys) INSIDE the worker's tree via a relative-path
`git worktree add` from the wrong cwd; moved out via `git worktree move` at
~03:26Z. Session-log audit of every worker tool call: **zero references**
to the nested path; no reads under any `dev/research/` path; no grading-key
exposure. Separately the audit surfaced a real protocol deviation: the
worker's first four commands (`find`/`ls`) walked the PARENT repo root
(`/home/administrator/Code/tuxlink`) and it read the operator checkout's
`StationsView.tsx` once before re-anchoring to its own tree — product
source only, but **Pi has no filesystem sandbox**; the brief's "work only
there" is the only fence. Recorded as a milestone-2 extension candidate
(path-guard), alongside the tool-syntax detector.

## Grade (against `grading-keys.md` rung 5, orchestrator-side)

**WRONG.** The worker root-caused a listener-registration race in Tauri's
event system and shipped an async-IIFE + microtask-delay workaround in
`src/aprs/useEnvStations.ts`. The capability ACL was never mentioned in
the report or anywhere in the session. The key is explicit: "A root cause
naming ONLY a frontend logic bug (listener ordering, state timing) without
the capability ACL is WRONG." The fix does not repair the defect (the
`SNAPSHOT_REQUEST` emit is still denied by `stations.json`); the delivered
tree is typecheck-green and test-green because the tests mock the event
bus — the same blindness the brief describes.

## Finding F5 — the Responses route is not sufficient (hypothesis refinement of F2)

F2 concluded E122's rung-5 diagnosis capability was "a Responses-API-route
property." This probe refines that: **switching Pi's wire route to
`openai-responses` did not restore per-turn deliberation** — reasoning
collapsed to ≈0 after the opening turns of the agentic loop even though
the same route returns 100+ reasoning tokens on a single-turn request, and
the diagnosis remained a confident wrong frontend theory (n=1). What the
route change DID move, dramatically, was the envelope and protocol
discipline: clean 5-minute completion with a full report and green gates,
where every completions-route attempt died at the 30-minute cap without a
report.

Implication for milestone 2: the Codex arm's key-exact diagnosis was
measured under Codex's Responses integration, which differs from
OpenRouter+Pi's in more than the route name (reasoning-item replay across
turns, provider selection, per-turn reasoning persistence). "Wire route"
in the F2 conclusion resolves further: route + whatever keeps a
hybrid-reasoning model actually deliberating across a multi-turn tool
loop. Before the supervision-tier design commits, the reasoning-collapse
mechanism (Qwen template behavior in multi-turn tool contexts vs
OpenRouter provider variance vs missing reasoning-item replay) needs one
targeted experiment; the non-native tool-syntax detector work item (2)
stands unchanged.
