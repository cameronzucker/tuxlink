# Serving Inkling-Small-NVFP4 on the dual DGX Spark cluster

Validated 2026-08-01 (agent chasm-wren-crag). This is the only configuration
that survived sustained real agentic load on the two GB10 boxes: a 90-minute
16/16 burn-in followed by the full 180-attempt battery ladder. Every deviation
listed below was reached by eliminating a reproduced failure, not by guesswork.
Canonical narrative and forensics: `bd show tuxlink-fa6x4`.

## The working stack

- **Image:** `eugr/spark-vllm:latest` (`af83e54adfad`), tagged `vllm-node:latest`
  on both nodes. Its Triton 3.6.0 compiles Inkling's `_fused_sconv_kernel`
  correctly on SM121; the upstream `vllm/vllm-openai:nightly` (Triton 3.7.1)
  illegal-accesses the same kernel source at init. Do not "upgrade" the image
  without re-running the burn-in.
- **Topology:** TP2 across the two nodes over the QSFP link, launched by eugr
  `run-recipe.sh`. Head runs the API server; the worker runs `--headless`
  (no local HTTP endpoint — the node-2 dashboard "not serving" tile was a
  false negative until the 2026-08-01 dashboard patch).
- **Recipe:** [`inkling-small-nvfp4.recipe.yaml`](inkling-small-nvfp4.recipe.yaml)
  — verbatim copy of the deployed file at
  `inference:~/spark-vllm-docker/recipes/inkling-small-nvfp4.yaml`.

```bash
cd ~/spark-vllm-docker && sg docker -c \
  "./run-recipe.sh inkling-small-nvfp4 -n 10.55.0.1,10.55.0.2 -d"
```

## Required fixes (all four are load-bearing)

| Fix | Where | Failure it eliminates |
|---|---|---|
| `max_num_seqs: 1` | recipe defaults | `_fused_sconv_kernel` (Triton paged conv-state) illegal-accesses on **multi-request batches**. conc-8 killed the engine in 2 s, conc-2 in minutes (probabilistic); batch-1 ran the whole ladder. Corollary: **the client must also stay at concurrency 1** — queued requests still form mixed batches at higher `max_num_seqs`. |
| `--no-enable-prefix-caching` | recipe command | Use-after-free crash pattern on the conv-state cache (KV drained to 0% → prefix hit on just-freed blocks → death 2 s later). Note vLLM V1 defaults prefix caching **on**; deleting `--enable-prefix-caching` is not enough, the explicit negative flag is required. Cost: ~3× slower turns (every turn re-prefills). |
| `mods/inkling-fix-streaming-tool-calls` | recipe mods list | Streaming tool calls leak into `content` as raw `<\|content_invoke_tool_json\|>` markup (finish=stop, zero tool deltas) whenever the model emits a tool call with no preceding thinking block. Non-streaming is unaffected, which masks the bug. Community patch (NVIDIA forum, dual-Spark thread) with anchors corrected for real parser source: [`patch_inkling_parser.py`](patch_inkling_parser.py) + [`mod-run.sh`](mod-run.sh). |
| `VLLM_USE_FLASHINFER_MOE_FP4: "0"` | recipe env | FlashInfer NvFP4 MoE backend illegal-accesses on SM121 (router GEMM). |

Also required, inherited from the eugr baseline: `mods/inkling-sm12-paged-kv`
(vendored SM12 paged-KV FA4 bundle — without it the FA4 warmup asserts
"Paged KV not supported on SM 12.0"), `mods/drop-caches` (unified-memory
headroom for the 159 GiB load), `--enforce-eager`, `LAMPORT_RS_SCONV=0`.

## Operational caveats

1. **`docker restart vllm_node` kills the serve.** Container command is
   `sleep infinity`; vLLM is exec'd in by the launcher. Relaunch via
   `run-recipe.sh`, never `docker restart`.
2. **Recipe edits do not reach running containers.** `run-recipe.sh` on live
   containers reuses the stale `/workspace/exec-script.sh` generated at
   container creation. To apply a recipe change: `docker rm -f vllm_node` on
   **both** nodes, then `run-recipe.sh` (this also re-applies all mods —
   the parser patch is idempotent).
3. **Verify after every launch** with
   [`stream_tools_probe.py`](stream_tools_probe.py) on the head — expect
   `VERDICT: TOOLS-OK-STREAMING`. A `TOOLS-LEAKED-INTO-CONTENT` verdict means
   the parser mod didn't apply.
4. Model load is ~5–8 min; `/v1/models` answering is the ready signal.
   TP2 fingerprint appears in completions (`...-tp2-...`).

## Dead ends (do not retry without new evidence)

- **Official DEP topology (TP1 per node, DP2+EP):** the recipe-recommended
  strategy for boxes without NVLink fabric, but TP1 runs the sconv kernel at
  full width where it faults even at init (warmup builds a multi-request dummy
  batch). Died on the eugr image and on `nightly` / `nightly-sm120`.
- **`vllm/vllm-openai:nightly` (Triton 3.7.1):** breaks the sconv kernel
  outright regardless of topology.
- **Cross-node TP with graphs / prefix caching / mns≥2:** the seven
  quarantined `inkling1-invalid*` runs on R2 document each failure mode.
- **SGLang / TokenSpeed:** no GB10/Spark support documented as of 2026-08-01
  (both target B200/B300/H200).

## Throughput expectations

~8 min per battery attempt (30+ provider turns) at conc 1 without prefix
caching; the full 180-attempt ladder is ~24 h. When upstream fixes the sconv
multi-request fault (no vLLM issue existed as of 2026-08-01 — consider filing
with the forensics from `bd tuxlink-fa6x4`), `max_num_seqs` and client
concurrency can be raised and prefix caching re-tested, in that order, each
behind a fresh ≥90-minute real-load burn-in.
