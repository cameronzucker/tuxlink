# Spark local-worker A/B experiment (bd tuxlink-c5ckf)

Baseline-feasibility experiment for the rationed-orchestrator concept: an
expensive frontier-model orchestrator seat driving a **local**-model worker
tier (DGX-Spark-class hardware) versus the standard all-cloud
subagent-driven-development stack. Commissioned by the operator 2026-07-15
(session ridge-magpie-cove); executed by session yew-basin-raven. Protocol of
record: bd `tuxlink-c5ckf`.

**Vehicle (real backlog, merges via arm A):** bd `tuxlink-xvd1i` — journal
`StateChanged` step/rig enrichment. See `plan.md`.

## Bundle contents (pre-registered — committed before either arm executed)

| File | Role |
|---|---|
| `plan.md` | Shared implementation plan (3 tasks, full TDD) |
| `briefs/task-{1,2,3}.md` | Per-task worker briefs: shared preamble + the plan's task section extracted byte-verbatim |
| `briefs/_preamble.md` | The shared preamble (kept for provenance) |
| `rubric.md` | Evaluation rubric, metrics M1–M6, frozen verdict thresholds, blind-eval prompt template |
| `report.md` | (post-experiment) results |

## Contamination controls

1. Plan, briefs, and rubric are frozen in the commit that introduces them —
   authored before either arm dispatched a single worker. The commit SHA is the
   pre-registration timestamp.
2. Both arms branch worktrees from the same `main` SHA (recorded in
   `report.md`).
3. Workers receive the brief text verbatim and nothing else about the
   experiment; neither arm's workers see the other arm's output.
4. The reviewer tier (Opus) and its prompt template are constant across arms;
   review of arm B does not reference arm A's diff or vice versa.
5. The blind adversarial pass sees neutral filenames only (`candidate-1.diff`,
   `candidate-2.diff`); the mapping lives in `rubric.md` §M4.
6. Orchestrator interventions are mechanical-only and logged verbatim
   (`rubric.md` §policy).

## Arm B infrastructure (validated 2026-07-15 before pre-registration)

- Endpoint: `https://inference.twin-bramble.ts.net/v1` — Tailscale-serve
  fronting vLLM on the DGX Spark (`gx10-65aa`, LAN `192.168.20.75`); tailnet
  membership is the auth boundary (no API key).
- Served model: `qwen3-coder-next` (`Qwen/Qwen3-Coder-Next-FP8`), 262 144-token
  context, `--enable-auto-tool-choice --tool-call-parser qwen3_coder`,
  `--max-num-seqs 2`.
- vLLM exposes the OpenAI **Responses API** (`/v1/responses`) natively — required:
  Codex CLI ≥ 0.140.0 removed the legacy `wire_api = "chat"` path.
- Worker invocation (config injected per-invocation; `~/.codex/config.toml`
  untouched so the ChatGPT-auth blind-eval runs stay isolated):

```bash
SPARK_API_KEY=dummy codex exec --skip-git-repo-check \
  -c model_provider=spark \
  -c 'model_providers.spark.name=Spark vLLM' \
  -c model_providers.spark.base_url=https://inference.twin-bramble.ts.net/v1 \
  -c model_providers.spark.wire_api=responses \
  -c model_providers.spark.env_key=SPARK_API_KEY \
  -m qwen3-coder-next \
  "<brief>" </dev/null
```

- End-to-end smoke passed: completion + a real shell tool call
  (`echo hello-from-spark-worker`) executed and reported. Known cosmetic warts:
  a non-fatal model-list refresh decode error, and a "model metadata not
  found" fallback warning.
- Operator's fuller panel (Qwen 3.5 122b Q4, gpt-oss-120b) requires a vLLM
  model swap on the Spark; only `qwen3-coder-next` (the PRIMARY candidate) was
  loaded at experiment time. Additional-model replications are an optional
  extension, not part of this pre-registration.

## Arm worktrees

- Arm A: `worktrees/bd-tuxlink-xvd1i-arm-a/`, branch `bd-tuxlink-xvd1i/arm-a`
  (claims bd `tuxlink-xvd1i`; MERGES via PR).
- Arm B: `worktrees/bd-tuxlink-c5ckf-arm-b-spark-replica/`, branch
  `bd-tuxlink-c5ckf/arm-b-spark-replica` (claims bd `tuxlink-c5ckf`; NEVER
  merges — diff captured for eval, then disposed per ADR 0009; a draft PR may
  exist solely to run CI and is closed unmerged).
- Shared base SHA (both arms): `e28f67db732c368d94a54b430871e911a1b701aa`.

## Operator directives recorded mid-experiment

- 2026-07-15 (after arm B attempt 1 failed on the Codex↔vLLM tool-protocol
  seam): follow-up experiments should attempt optimization with a
  **custom-built worker harness** — the same thesis as Elmer, which achieves
  good results from smaller local models precisely because its harness is
  purpose-built for them — rather than adapting Codex CLI, whose tool
  surface and prompting are tuned for GPT-5.5-class models. Goes in
  `report.md` §transferability/recommendations.

## Scale-ladder extension (operator-commissioned 2026-07-15, registered before any scale arm ran)

Additional never-merge replicas via OpenRouter (full-precision hosted serving,
"two-Sparks / office-inference-server" hardware class), tracking quality vs
model scale with the harness, briefs, base SHA, reviewer tier, and blind-eval
protocol IDENTICAL to arm B:

| Arm | Model | Isolates |
|---|---|---|
| C | `qwen/qwen3-235b-a22b-2507` | scale (operator's "235b") |
| D | `qwen/qwen3.5-397b-a17b` | scale (operator's "405b-class") |
| E | `qwen/qwen3.5-122b-a10b` | quantization — same weights as the Spark panel's Q4 candidate, full precision |

- Endpoint: `https://openrouter.ai/api/v1` (Responses API verified working
  2026-07-15 with the 235B). Key: OS keyring `service=elmer-openrouter
  account=teacher`, passed inline per invocation — never on disk or in logs.
- Worktrees `worktrees/bd-tuxlink-c5ckf-arm-{c,d,e}-*`, claimed by bd
  `tuxlink-c5ckf`, branched from the shared base `e28f67db`, run sequentially
  after arm B completes; blind eval extends to `candidate-3/4/5`.
- Arm B's `include_apply_patch_tool=true` mitigation applies to all scale arms
  (same Codex↔non-GPT tool-protocol seam).
