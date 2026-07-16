# Graduated difficulty ladder — bd tuxlink-7raoe milestone 1 (overnight run)

Operator-directed follow-up to the c5ckf A/B experiment
(`dev/research/2026-07-15-spark-ab-experiment/`): a discriminating instrument
for the capabilities rich briefs suppress. Registered question, arms, rungs,
scoring: `rubric.md`. Worker briefs: `briefs/rung-{1..6}.md`. Ground-truth
grading keys (orchestrator-only): `grading-keys.md`. Running ledger of every
dispatch, intervention, verification, and Spark state change: `ledger.md`.

Orchestrator session: falcon-shoal-clover (Fable), 2026-07-16 overnight.
Pre-registration freeze: the commit introducing this bundle, pushed to
`bd-tuxlink-7raoe/difficulty-ladder` BEFORE any worker dispatch (the branch
does not merge until the report ships — arm worktrees branch from a base SHA
that predates this bundle, so workers cannot see it).

## Shared base SHA (all arms)

`b82b404d` (origin/main after PR #1124). Arm worktrees:
`worktrees/bd-tuxlink-7raoe-ladder-arm-{s5,cn,q122,o397,e122}/`, branches
`bd-tuxlink-7raoe/ladder-arm-*` — never merge; disposal per ADR 0009 after
the report. All claimed by bd `tuxlink-7raoe`.

## Contamination controls (inherited from c5ckf)

1. Briefs/rubric/keys frozen and pushed before any worker run.
2. All arm worktrees branch from the same base SHA, which predates this
   bundle.
3. Workers receive brief text verbatim and nothing else about the
   experiment or other arms.
4. Reviewer tier constant (Opus 4.8, same template); reviews never
   reference another arm's diff.
5. Orchestrator interventions mechanical-only, logged verbatim in
   `ledger.md`.
6. Rungs run 1→6 within an arm; the orchestrator commits the arm worktree
   after each rung (per-rung diff = that commit), no worker git access.

## Infrastructure (validated before freeze)

- **Spark vLLM** (`gx10-65aa` / `https://inference.twin-bramble.ts.net/v1`,
  tailnet auth): serving `qwen3-coder-next` (Qwen3-Coder-Next-FP8, 262k ctx)
  in container `vllm`. Smoke 2026-07-16: codex round-trip incl. real shell
  call — pass.
- **OpenRouter** (`https://openrouter.ai/api/v1`, Responses API): smoke with
  `qwen/qwen3.5-397b-a17b` incl. real shell call — pass. Key from OS keyring
  (`service=elmer-openrouter account=teacher`), inline per invocation, never
  on disk.
- **Q122 model swap** (the arm-G recipe, report.md §Addendum): stop `vllm`
  container (do NOT remove), run a second container from the same image
  serving `nvidia/Qwen3.5-122B-A10B-NVFP4` (weights already in the HF cache)
  with a PATCHED chat template (base template + 3 patches: `developer` role
  → system; non-leading system messages rendered inline instead of raising;
  `enable_thinking` forced false). The patched template file persists at
  `/home/administrator/serving/` on the Spark. Harness smoke before rung 1
  of Q122. RESTORE: stop+remove the Q122 container, `docker start vllm`,
  verify `/v1/models` returns `qwen3-coder-next`. All state changes logged
  in `ledger.md`.

## Worker invocation

Codex arms (CN / Q122 / O397 / E122), per rung, from the arm worktree root,
30-min cap enforced by `timeout 1800`, transcript teed to
`.superpowers/sdd/rung-N-codex-transcript[.attemptM].txt`:

```bash
timeout 1800 env <PROVIDER_ENV> codex exec --skip-git-repo-check \
  --cd <ARM_WORKTREE> \
  -c model_provider=<spark|openrouter> \
  -c 'model_providers.<p>.name=...' \
  -c model_providers.<p>.base_url=<URL> \
  -c model_providers.<p>.wire_api=responses \
  -c model_providers.<p>.env_key=<KEY_ENV> \
  -m <MODEL> "<R2-WRAPPED BRIEF>" </dev/null
```

### R2 wrapper (verbatim frame around each brief, c5ckf regime R2)

```
You are implementing a task in the tuxlink repository.

## Harness usage (important)

Read files with shell commands (cat, sed -n, rg, grep). Make EDITS by running
python3 heredoc scripts that read the file, perform exact string replacement,
and write it back — then VERIFY each edit landed with grep before moving on.
Example:

    python3 - <<'PYEOF'
    p = 'path/to/file.ts'
    s = open(p).read()
    old = "exact existing text"
    new = "replacement text"
    assert old in s
    open(p, 'w').write(s.replace(old, new, 1))
    PYEOF

Do NOT use apply_patch (not available), interactive editors (ed/vi), or MCP
resource reads (no servers exist). If your edits repeatedly fail to land,
STOP and report status BLOCKED with what you tried — NEVER report work as
implemented or tests as passing unless you ran the command and saw it.

## Your job

1. Do exactly what the brief below specifies; where the brief grants design
   freedom, decide and document.
2. Verify with the exact commands the brief lists (plus any you deem
   necessary).
3. Self-review: every brief requirement met? nothing beyond scope? tests
   verify real behavior; output pristine?
4. Write your full report to .superpowers/sdd/rung-N-report.md (relative to
   the repo root), then finish.

Per the brief: do NOT run any git command and do NOT commit — the controller
commits.

End your final message with ONLY: Status (DONE | DONE_WITH_CONCERNS |
BLOCKED), a one-line test summary, concerns if any, and the report file path.

## Your brief (requirements of record):

<BRIEF>
```

Arm S5: fresh Claude Sonnet 5 subagent per rung via the Agent tool
(exact model ID `claude-sonnet-5`), same brief text and report/final-message
contract, WITHOUT the harness-usage block (Claude tooling edits natively).
Same cap and retry policy.

## Reviewer template (Opus 4.8, per worker-completed rung)

```
You are reviewing a candidate diff for the tuxlink repository (React 18 + TS
frontend, Tauri 2.x). The worker was given the brief pasted below and
produced the attached diff against a clean base. Acceptance criteria are
appended.

Review for: (1) does the diff satisfy every binding requirement of the
brief; (2) correctness of the change and its tests (do tests pin real
behavior, would they catch regressions); (3) unsolicited scope drift;
(4) hygiene (duplicated/misplaced tests, dead code, stale comments).
Verdict: Approve / Approve-with-minors / Request-changes, with findings
listed as Critical / Important / Minor, each with file:line and a one-
paragraph rationale.
```

The reviewer sees brief + diff + acceptance criteria — never the grading
keys' premise labels (rung 6) or mechanism key (rung 5); premise/mechanism
grading is orchestrator-side against `grading-keys.md`.

## Schedule (overnight plan; ledger records actuals)

1. Freeze + push this bundle.
2. Arm S5 rungs 1→6 (instrument calibration, sequential).
3. Arms CN (Spark) and O397 (OpenRouter) rungs 1→6, concurrent, each
   sequential internally.
4. Spark swap → Q122; harness smoke; Q122 rungs 1→6. E122 rungs 4→6 on
   OpenRouter concurrently.
5. Restore coder-next; verify.
6. Grade (verification is continuous; reviews may interleave), report,
   ship.
