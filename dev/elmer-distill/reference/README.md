# Reference harness (data-gen + eval seed for the Elmer-20b distillation epic)

Faithful offline agentic harness from the 2026-07 Elmer model eval — real `ELMER_SYSTEM_PROMPT`,
the 50-tool Elmer surface (`tools.json`), mock tool returns, multi-turn loop + metrics. Reused as
the trajectory generator in the distillation design (`docs/superpowers/specs/2026-07-02-elmer-20b-tooluse-distillation-design.md`).

NOTE (Codex adrev F): the harness is a data-gen loop, NOT a judge — it models no armed-authority /
taint state and denies all egress unconditionally. Phase C builds the stateful simulator + judge.

- `harness.py`      — ollama `/api/chat` driver (used in the eval)
- `harness_oai.py`  — llama.cpp `/v1` (OpenAI-compat) driver
- `tools.json`      — the 50-tool surface (built from tuxlink-mcp-core/router.rs)
- `build_tools.py`  — regenerates tools.json
