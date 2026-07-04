# Handoff — elmer-120b TRAINED on a cheap Blackwell card; serving hits an unsloth-unfused ↔ vLLM-fused wall

**Agent:** moss-oak-lichen · **Date:** 2026-07-04 · **bd:** tuxlink-48nyh (perfect the 120b)
**Branch:** `bd-tuxlink-6zkb6/discriminating-eval` · **Worktree:** `worktrees/bd-tuxlink-6zkb6-discriminating-eval/`
All code/docs committed + pushed. This was a long live-pod session; read this before touching serving.

## The headline
**gpt-oss-120b QLoRA trains cleanly on a single ~half-price RTX PRO 6000 Blackwell (96 GB, CUDA).**
That was the open question and the answer is **yes** — fits ~72 GB during training, healthy loss
(1.48→0.25 over 3 epochs, 79 gold examples), no ROCm gamble. The remaining problem is **serving**
the fine-tuned model, which is a genuine checkpoint-format engineering task (below).

## What shipped this session (committed)
- **Instrument repair + punt-restraint** (earlier commits): `schedule_has_blocks` evidence-bound;
  scaffold no longer teaches hollow output; quality-eval first-class (`parity_artifact`); injection-
  refusal PUNTED to the tool-layer guard + operator (scored=False on those gate cells); honesty
  MEASURED not trained; gold-gen = pure grounded-quality cold-transfer (normal bank, drop-taint, no
  20b restraint borrow). See [[project_elmer_teacher_ceiling_pivot]] (updated: **120b is THE target;
  local-20b endgame AXED; served/networked 120b accepted**).
- **run_train `--model-id` + paged 8-bit optim** (fits 96 GB); **run_120b.sh** end-to-end pipeline
  (PEP668 + openai_harmony install + ollama-VRAM-release-before-train guards, all added live).
- **`peft_eval.py` + `hf_client.py`** in-process eval (works, but abandoned — see below).
- **`docs/spark-transition.md`** — verified DGX Spark readiness (largely turnkey; NVIDIA playbooks;
  bnb sm_121 wheels; gpt-oss needs 3 transformers patches; `micro_lora_smoke` stays the go/no-go).
- bd feature + [[feedback_remote_run_visibility]]: build a remote/mobile run dashboard (no VNC-to-Pi).

## Artifacts + state (CRITICAL)
- **Trained adapter PRESERVED locally**: `/home/administrator/elmer-artifacts/adapter-120b-2026-07-04/`
  (4.1 GB, byte-identical to the pod). base=`unsloth/gpt-oss-120b-unsloth-bnb-4bit`, r=16, per-expert
  target modules. THIS is the essential artifact; everything else is re-derivable from it.
- **On the pod** (`103.196.86.190:19372`, RTX PRO 6000, BILLING): the adapter (`/root/elmer-train/
  adapter-120b`), a HOLLOW merge (`/root/elmer-merged-mxfp4`, do not use), and a CLEAN merge
  (`/root/elmer-merged-clean`, 58 GB, deltas baked in but unfused per-expert). vLLM installed in
  `/root/vllm-venv` (v0.24.0) + bnb. **Operator: decide pod fate** — adapter is safe, so spinning
  down is fine; the serving work below does not need this pod until testing.
- **The 20b re-baseline (repaired gate) numbers**: 20b cold 2.6/16 scaffold 13.6; 120b cold 4.2
  scaffold 12.6 (over all 16; the 2 injection cells are now scored=False, which lifts the 120b).

## The serving wall — EXACTLY what we observed (ground truth, not memory)
Goal: OpenAI-compatible endpoint for the fine-tuned 120b (fast eval + Tuxlink). Chain of observed
failures (vLLM 0.24.0, FlashInfer disabled for a separate sm_121 sampler bug, bnb in the venv):
1. `vllm serve <base> --enable-lora <adapter>` → **vLLM wants LoRA on the FUSED `experts` module**,
   ours is per-expert (`experts.gate_up_projs.<i>`). Rejected.
2. unsloth `save_pretrained_merged(mxfp4)` on our raw-PeftModel = **HOLLOW** (84240/84423 keys still
   `base_layer`/`lora`; nothing merged).
3. peft `merge_and_unload()` → **CLEAN** (0 base_layer keys, deltas baked in) but the checkpoint is
   **unfused per-expert bnb-4bit** (`experts.down_projs.0…`, `router.linear.weight`).
4. `vllm serve <clean-merged>` → **`weights not initialized`**: vLLM's gpt-oss loader expects the
   **FUSED** format (`experts.routed_experts.w1/w2_weight`, `router.weight`).

**Root tension:** per-expert LoRA was necessary to train the MoE experts through unsloth, but that is
a TRAINING-ONLY representation; vLLM's gpt-oss SERVING loader needs the canonical fused checkpoint.
Also: in-process transformers eval (`peft_eval.py`) WORKS but is impractically slow (re-prefill per
turn, ~91 GB, ~10-15 min/scenario) — glue was fully debugged (openai_harmony install; tools must be
wrapped + every fn/param needs a `description`; generate must stop on `[<|return|>,<|call|>]` = ids
[200002,200012] or it runs to max_new_tokens and OOMs the KV cache).

## ADREV VERDICT (independent fact-grounded review + web research, 2026-07-04)
Diagnosis **confirmed** by primary sources (HF indexes, vLLM `gpt_oss.py`); no material hallucinations.
Key corrections/additions:
- The hollow `save_pretrained_merged` (#2 above) is a **known unsloth bug — issue #3701** (`# of LoRAs
  = 9360 does not match # of saved modules = 144`), triggered by dense **per-expert** LoRA. Not our
  misuse; unsloth's saver has hardcoded module-count assumptions. No maintainer fix posted.
- Cosmetic-only labels (not diagnostic errors): vLLM's internal buffer is `w13_weight/w2_weight` (not
  `routed_experts.w1/w2`); serialized router key is `router.weight` (not `router.linear.weight`).
- **Do NOT try to reshape the existing peft bnb-4bit `merge_and_unload` checkpoint — it is servable by
  nothing as-is. Re-export from adapter+base instead.**

**Recommended path — PRIMARY: llama.cpp / llama-server via GGUF** (the ONLY end-to-end route unsloth
documents for a *fine-tuned* gpt-oss; native OpenAI `/v1` endpoint; fits one 96GB card; ports to GB10
aarch64 as-is; ~58 tok/s single-stream ≈ vLLM/SGLang):
1. Re-export merged from adapter+base: `model.save_pretrained_merged("merged", tok,
   save_method="merged_16bit")` (**bf16** — MXFP4 export hits "No MXFP4 tensors found" on a dequantized
   merge; llama.cpp #15146 / unsloth #3817).
2. If #3701 hollow-merge bites again: load the **bf16** base (NOT bnb-4bit) + `PeftModel.from_pretrained`
   + `merge_and_unload()` (experts dequantize to clean bf16) → `save_pretrained`.
3. **MECHANICAL GATE before trusting any checkpoint** — `json.load("merged/model.safetensors.index.json")`
   keys: PASS iff `experts.gate_up_proj`/`down_proj` (stacked) + `mlp.router.weight`; FAIL if
   `gate_up_projs.0…` or `router.linear.weight` (fuse didn't happen → vLLM/GGUF will reject).
4. `convert_hf_to_gguf.py merged/ --outfile elmer.gguf` → `llama-server -m elmer.gguf --jinja` → point
   the eval (`api_client`) + Tuxlink Elmer at `http://…/v1`.

**SECONDARY — vLLM 0.24.0 live 2D LoRA, no merge** (if throughput matters): serve stock **fused**
`unsloth/gpt-oss-120b-BF16` + adapter with `--enable-lora --enable-mixed-moe-lora-format` and
`is_3d_lora_weight:false` (PR #42242, present in 0.24.0) — BUT rename adapter keys
`experts.gate_up_projs.{i}` → vLLM's 2D pattern `experts.{idx}.gate_proj`. vLLM does NOT error on a
wrong layout — it silently corrupts, so validate against approach A.

**Approach A (slow transformers server) = keep as the CORRECTNESS ORACLE** — generate a few reference
completions from the as-trained model to validate any converted/GGUF checkpoint against (catches the
silent-corruption failure mode). Not the endpoint.

SGLang: also needs the fused layout (no easier). TGI: no gpt-oss support, skip. No off-the-shelf fuser exists.
Full sources in the adrev (unsloth #3701/#3405/#3817, vLLM PR #42242, LLaMA-Factory #8969, llama.cpp #15146).

## NEXT SESSION — start here
1. Read this doc (esp. the ADREV VERDICT) before any serving work. The observed errors + the adrev are
   ground truth — do NOT re-derive from scratch or from memory.
2. Serving = its own focused effort. **Start with the PRIMARY llama.cpp/GGUF path**; the mechanical
   key-gate (step 3) is the highest-leverage de-risk — run it before feeding any checkpoint to a server.
3. Trained adapter: `/home/administrator/elmer-artifacts/adapter-120b-2026-07-04/` (4.1 GB, the one
   irreplaceable artifact). base=`unsloth/gpt-oss-120b-unsloth-bnb-4bit`, r=16, per-expert targets.
4. Pod (`103.196.86.190:19372`) may be spun down by now — re-provision + `pod_bootstrap.sh` + re-merge
   from the adapter as needed. Serving work needs a GPU box only at test time.
5. Eval harness (`peft_eval.py` glue, or point `api_client` at the llama-server `/v1`) already exists;
   once served, the fast acceptance eval (scored cold-transfer + honesty) is quick.
