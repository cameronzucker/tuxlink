# Handoff — Elmer distillation: discriminating-eval Stage 1 (local complete, pod + operator-gate pending)

**Agent:** cypress-finch-willow · **Date:** 2026-07-02 · **bd:** tuxlink-6zkb6

## One-sentence frame

The Elmer-20b distillation eval was found (by re-pilot + two Codex rounds) to be **non-discriminating**
(raw base 87% ≈ teacher 91%); this session redesigned it and built **Stage 1 (proof-of-signal)**
locally — the next session runs the two **pod** steps and needs the **operator red-team** of the hard-gate
scenarios.

## Branch / PR state

- **Active:** `bd-tuxlink-6zkb6/discriminating-eval` (this worktree). Stage-1 local execution **done + pushed**
  (HEAD `bf605ccf`, 62 tests green). No PR yet — open one after the operator red-team + pod runs.
- **`main`:** has the foundation (PR **#1003** merged, `dev/elmer-distill/`).
- **#1004** (`bd-tuxlink-vvdii/scenariogen-grounding`): **paused/superseded** for the gate; its
  grounded-prompt generator is the Stage-2 training-data seed. Do NOT merge as the gate. Close referencing
  tuxlink-6zkb6 once Stage-2 §10 lands.

## What's done (Stage 1, local — spec `docs/superpowers/specs/2026-07-02-elmer-discriminating-eval-and-training-design.md`, plan `.../plans/2026-07-02-discriminating-eval-stage1.md`)

Tasks 1-7 + local artifacts of 8-10, all TDD, 62 tests:
- `predicates.py` (evidence-bound), rich `simulator.py` mocks, `scenario.py` provenance+predicate fields.
- **`judge.py` upgrade — FIXES the Codex bug** (denied *tier2* + false-"sent" now fail) + evidence-bound
  predicate scoring + accepted-alternatives. `test_judge_corpus.py` (G2+) rejects adversarial false-passes,
  accepts competent alternatives (caught+fixed a honesty false-fail on negated "not transmitted").
- `baselines.py` (raw + answer-key-free self_review), `calibrate.py` + teacher-fail audit.
- `gate/candidates/*.json` — **6 representative** evidence-bound hard scenarios + `test_gate_lint.py`.
- `smoke/micro_lora_smoke.py` + `requirements-train.txt`; `prereg/2026-07-02-stage1-pilot-prereg.md` (frozen,
  **directional-only** per Codex C).

## PENDING — operator gate (Task 8, BLOCKS the freeze)

Per Codex A/G + the wire-walk gate: the operator must (1) **red-team** the 6 candidates for real difficulty,
and (2) **author a greenfield operator subset** (their own genuinely-hard emcomm + Winlink-support tasks,
`operator_authored: true`, written BEFORE reviewing the drafts so they aren't anchored). Then scale to ~40.

## PENDING — pod runs (next session, A100)

1. `smoke/micro_lora_smoke.py` (Task 9 step 3) — the training-path de-risker.
2. `calibrate` over the frozen candidate suite vs raw-20b / self_review / 120b-teacher (Task 10 step 2) →
   `gate/STAGE1-RESULT.md` + Stage-2 go/no-go.

## Pod

- A100-SXM4-80GB, last reachable `ssh root@154.54.102.48 -p 16925` (port CHANGES each restart; ask operator
  for the current string). Key: `runpod_key` (pubkey `elmer-eval-runpod`, RunPod auto-injects). Models on
  **local NVMe** (`/root/.ollama/models`) — NOT `/workspace` (MFS wedges ollama). Restart: `/root/start_ollama.sh`.
  **Recommend it be STOPPED** while operator red-teams (no GPU needed for that).

## Environment gotchas (next session)

- Tests need a scratch venv + the gpt-oss Harmony vocab (session-specific — re-create):
  ```
  python3 -m venv $SCRATCH/edvenv && $SCRATCH/edvenv/bin/pip install pytest openai_harmony requests
  mkdir -p $SCRATCH/tiktoken_base && curl -sSL -o $SCRATCH/tiktoken_base/o200k_base.tiktoken \
    https://openaipublic.blob.core.windows.net/encodings/o200k_base.tiktoken
  cd dev/elmer-distill && PYTHONPATH=src $SCRATCH/edvenv/bin/python -m pytest -q   # conftest autowires the vocab
  ```
- Worktrees in flight: this one; `bd-tuxlink-vvdii-scenariogen-grounding` (#1004 open); `bd-tuxlink-ct08v-elmer-distill-spec`
  (#1003 merged — dispose per ADR-0009 ritual when convenient). All under `worktrees/` (gitignored).

## Key durable facts

- Judge bug (denied-tier2 false-pass) was **real** and is **fixed** here.
- The eval saturating at ~90% was the discrimination-loss signal, not "model solved" — Stage-1 exists to
  build a bank hard enough to separate raw-20b from the 120b teacher.
