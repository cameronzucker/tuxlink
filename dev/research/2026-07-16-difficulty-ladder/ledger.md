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
2026-07-16T08:14:21Z arm-s5 rung-3 worker DONE (~10.6min) but orchestrator 3x-rerun caught intermittent regression (stale Vara pinning test :1092, racy); fix round 1 dispatched to same worker. Grading-keys amendment appended.
2026-07-16T08:17:19Z SPARK STATE CHANGE: staged patched chat template at /home/administrator/serving/qwen35-122b-nvfp4.chat-template.jinja (3 shims: developer-role->system x3 sites, non-leading system rendered inline, generation prompt forced no-think). No container change yet.
- 2026-07-16 ~10:01Z FIRST SEPARATION DATUM: rung 5 (symptom-only diagnosis).
  S5 = complete-clean (mechanism KEY-EXACT). O397 = FAILED both attempts
  (confident window-scoped-emit theory; internally inconsistent — relied on
  emit() broadcasting for the reply while claiming it can't for the request;
  never considered the Tauri capability ACL; both fixes would be denied by
  the same missing-permission class). Integrity honest both attempts.
  CN rung-3 attempt-1 hit the 30-min cap (7 sites wired, zero tests); retry
  running.
