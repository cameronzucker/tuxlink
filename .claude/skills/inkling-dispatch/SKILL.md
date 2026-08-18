---
name: inkling-dispatch
description: Route a bounded, spec-complete, leaf-scope code task to the free local Inkling subagent (pi harness on R2) instead of spending frontier tokens. Use at task pickup when the eligibility checklist passes; the parent writes the spec, verifies on R2 gates, and commits with attribution.
---

# inkling-dispatch — the free local subagent lane

Evidence base: graded evals (3/3) in
`dev/evals/2026-08-09-inkling-pi-subagent-eval.md` and the real-P1 A/B in
`dev/evals/2026-08-10-nyyr2-ab-inkling-vs-sonnet.md`. Summary: Inkling
(`inkling-small-nvfp4`; API model id `thinkingmachines/Inkling-Small-NVFP4`; 256K, Spark vLLM) completes bounded real tasks in
25–90 min at zero token cost with ~zero touches — when and only when the
parent pays the fixed cost of a spec-complete task file. Serving pace makes
this a background lane, never a pairing partner.

## Step 0 — Serving pre-flight (dispatch-and-hope is banned)

Local compute is not guaranteed up (Spark reboots, profile switches, engine
wedges). BOTH probes must pass before any dispatch; on failure, route frontier
or defer — never launch a 90-minute round at a dead endpoint.

```bash
# 1. Catalog: the endpoint answers AND serves the expected model.
curl -sS -m 8 https://inference.twin-bramble.ts.net/v1/models \
  | jq -e '.data[].id | select(. == "thinkingmachines/Inkling-Small-NVFP4")' >/dev/null \
  || { echo "SERVING DOWN or wrong profile — do not dispatch"; }

# 2. Generation: a real 1-token completion. A dead engine can still return
#    instant empty "completions" (tuxlink-ulzuv class) — require non-empty
#    reasoning or content, and expect this to take seconds, not instantly.
curl -sS -m 90 https://inference.twin-bramble.ts.net/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"thinkingmachines/Inkling-Small-NVFP4","messages":[{"role":"user","content":"Say OK"}],"max_tokens":16}' \
  | jq -e '.choices[0].message | (.reasoning // .content // "") | length > 0' >/dev/null \
  || { echo "ENGINE NOT GENERATING — do not dispatch"; }
```

Also confirm the harness sees the provider: `pi auth check --provider spark
--json` must report `ready` (see gotchas below). If the Spark is serving a
different control-plane profile (bench runs, reprovisioning), do NOT switch
profiles to enable dispatch — serving ownership is the operator's; fall back
to frontier for this task.

## Step 1 — Eligibility checklist (ALL must hold, else use a frontier agent)

- [ ] Scope is leaf/adapter-level: one crate or one frontend package;
      NO cross-crate contract changes, NO new public API on a shared trait,
      NO security-boundary or architecture decisions.
- [ ] You can write the spec COMPLETE before dispatch: acceptance criteria,
      real fixtures (captured wire data, file samples), named files/seams,
      and every constraint (MSRV 1.75, style, no-network, no-git).
- [ ] Verification is mechanical: compiler + tests + clippy (or vitest)
      decide success. Anything needing visual judgment, on-air validation,
      or operator taste is ineligible.
- [ ] Latency-tolerant: nothing blocks on this for the next ~2 hours.
- [ ] The diff will be small enough to review line-by-line (≲500 lines).

Route to a frontier subagent instead when the task needs design-space
exploration, codebase archaeology across crates, or resolves ambiguity the
spec can't close. (A/B evidence: the frontier arm found cross-crate
precedents and shipped the contract-level design; the local arm shipped the
verified minimal fix.)

## Step 2 — Spec file (the load-bearing fixed cost)

Write `TASK-SPEC.md` from this skeleton — the A/B proved both model families
execute it faithfully:

1. The bd issue verbatim (never paraphrase away constraints).
2. "Verified starting points" — file paths, line-anchored seams you have
   personally read, prior-art pointers. Say "trust but re-verify".
3. Real fixtures (captured wire shapes, byte-faithful file samples).
4. Numbered acceptance criteria, each mechanically checkable, including the
   exact cargo/vitest/clippy commands and MSRV note.
5. Scope fence: which crates/dirs; "state why in SUMMARY.md if you must
   leave them". No UI changes unless the task IS UI.
6. Deliverable: working tree + `SUMMARY.md` (what changed, chosen semantics,
   judgment calls for the reviewer).
7. Environment truths: "cargo IS available — iterate until green" (R2) or
   "cargo is NOT available — one careful pass, compiled elsewhere" (Pi);
   "git is NOT available; do not attempt it"; "no network — fixtures only".

## Step 3 — Workspace + dispatch (R2 is the standard host)

```bash
# Isolated repo copy — NO .git (removes the whole banned-git-op class):
ssh r2-poe 'mkdir -p ~/inkling-task-<slug>'
rsync -a --delete --exclude .git --exclude node_modules --exclude target \
  --exclude worktrees --exclude dev/scratch --exclude .local --exclude dist \
  <worktree>/ r2-poe:~/inkling-task-<slug>/repo/
scp TASK-SPEC.md r2-poe:~/inkling-task-<slug>/repo/

# Prewarm the scoped build (PATH is load-bearing: rustup, not apt cargo 1.75):
ssh r2-poe 'export PATH=$HOME/.cargo/bin:$PATH; cd ~/inkling-task-<slug>/repo/src-tauri \
  && cargo test -p <crate> --no-run'

# Launch one round (background task; 5400s cap; sessions kept for forensics):
ssh r2-poe 'export PATH=$HOME/.cargo/bin:$HOME/.local/node22/bin:$PATH; \
  cd ~/inkling-task-<slug>/repo && timeout 5400 npx --yes @earendil-works/pi-coding-agent \
  --provider spark --model "spark/inkling-small-nvfp4" \
  --session-dir ~/inkling-task-<slug>/sessions --name <slug>-round1 --thinking medium \
  -p @TASK-SPEC.md "Work this task to completion. cargo is available - iterate \
  until every acceptance criterion is green. git is not available. End with the \
  final cargo test and clippy result lines and confirm SUMMARY.md is written."'
```

Harness gotchas (each cost real debugging once — do not rediscover):
- `@TASK-SPEC.md` must be its OWN argv token; inside a quoted message it is
  read as a filename and the run dies in seconds.
- Provider config is `~/.pi/agent/models.json` with a TOP-LEVEL `"providers"`
  key; keyless vLLM needs a dummy `apiKey`; set `reasoning: true` and
  `compat: {supportsDeveloperRole: false, supportsReasoningEffort: false}`.
  An invalid provider makes `-p` mode hang silently forever —
  `pi auth check --provider spark --json` must say `ready` first.
- pi needs node ≥22.19: prefix `~/.local/node22/bin` (installed userland on
  both Pi and R2).
- pi walks parent dirs for context files; a repo copy carries CLAUDE.md /
  AGENTS.md and the model WILL absorb conventions (and invent commit-trailer
  monikers for commits it cannot make). Expected, harmless, verify anyway.

## Step 4 — Verify (never trust the self-report)

Run the acceptance commands YOURSELF on the workspace (same shell, your own
invocation — `| tail` masks exit codes; gate on `PIPESTATUS[0]` or result
lines). Then read the full diff (`rsync -rcn` against a pristine copy lists
changed files; new files need the reverse direction too). Classify any
verification claims in SUMMARY.md: ran-the-gate vs verified-by-simulation vs
asserted — the local model has once worded simulation as a test run.

## Step 5 — Land

The parent commits (subagents never commit): apply the diff in a proper
bd-claimed worktree branch, commit with attribution in the body
("authored end-to-end by the local Inkling subagent (pi harness), N rounds,
parent-verified <gates>") plus YOUR moniker trailer. Then normal code rules —
CI is the merge gate, parent review is mandatory, fix-forward. One round of
feedback (relaunch with `--continue` or a fresh round citing the failing
gate) is normal; a task needing a third round was misrouted — pull it back
and do it yourself or send it frontier.

Log the outcome (rounds, wall time, verdict) in the bd issue's notes so the
lane's track record stays measurable.
