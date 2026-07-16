# Ladder run ledger — dispatches, interventions, verifications, Spark state

Append-only during the run. Times UTC.

## Pre-freeze

- 2026-07-16 ~07:20 Spark smoke: codex→vLLM coder-next round-trip incl. real
  shell call (`ladder-smoke-spark-ok`) — pass, 45.4k tokens.
- 2026-07-16 ~07:20 OpenRouter smoke: codex→`qwen/qwen3.5-397b-a17b`
  round-trip incl. real shell call (`ladder-smoke-or397-ok`) — pass, 44.3k
  tokens. Key from keyring, inline env only.
- Spark state at session start: container `vllm` Up, serving
  `qwen3-coder-next` (verified via /v1/models) — matches the arm-G restore
  claim in the handoff.
