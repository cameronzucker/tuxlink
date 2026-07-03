# Reference harness (data-gen + eval seed for the Elmer-20b distillation epic)

Faithful offline agentic harness from the 2026-07 Elmer model eval — real `ELMER_SYSTEM_PROMPT`,
the 50-tool Elmer surface (`tools.json`), mock tool returns, multi-turn loop + metrics. Reused as
the trajectory generator in the distillation design (`docs/superpowers/specs/2026-07-02-elmer-20b-tooluse-distillation-design.md`).

NOTE (Codex adrev F): the harness is a data-gen loop, NOT a judge — it models no armed-authority /
taint state and denies all egress unconditionally. Phase C builds the stateful simulator + judge.

- `harness.py`      — ollama `/api/chat` driver (used in the eval)
- `harness_oai.py`  — llama.cpp `/v1` (OpenAI-compat) driver
- `tools.json`      — the 53-tool surface: 50 mirror `tuxlink-mcp-core/router.rs`, plus 3
  **eval-proposed** APRS read tools (`aprs_list_stations` / `aprs_station_track` /
  `aprs_read_messages`, tuxlink-6zkb6) that intentionally LEAD router.rs so the gate can
  eval agent-driven APRS (tactical-map) tasks. The reference harness stubs unknown tools
  gracefully; the eval's `StatefulSimulator` mocks them richly (with taint on the message read).
- `build_tools.py`  — regenerates tools.json from router.rs. **WARNING:** a plain regen DROPS
  the 3 eval-proposed APRS tools until router.rs actually exposes them — re-add them (or teach
  build_tools.py to append the eval-only set) after any regen.
