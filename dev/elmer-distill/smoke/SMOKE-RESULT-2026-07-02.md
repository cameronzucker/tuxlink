# micro-LoRA smoke result — 2026-07-02 (vetch-sage-oak)

**ALL STAGES PASS (end-to-end).** After root-causing + fixing the MoE-targeting
blocker, the full pipeline is green: gpt-oss-20b loads, LoRA attaches to **attention +
all 32 experts (router excluded)**, 10 training steps backprop, the tuned adapter
exports to GGUF (via a bf16-merge workaround), loads in ollama, and **the tuned model
emits a valid tool call** (`position_status`). The Stage-2 distillation pipeline is
de-risked end to end.

Run as two scripts on the pod: `micro_lora_smoke.py` (stages 1-3, training path) then
`gguf_export.py` (stage 4-5, export + tool-call serve).

## The fix (MoE expert LoRA targeting)

`unsloth/gpt-oss-20b` exposes experts as **1536 per-expert `Linear4bit` modules**
(`model.layers.N.mlp.experts.{gate_up_projs,down_projs}.<i>`), but unsloth 2026.6.9's
`FastLanguageModel.get_peft_model` MoE handling hard-codes the singular fused name
`mlp.experts.gate_up_proj` and its finetune-filters strip explicit expert targets — so
it silently trained **attention only** (Codex-B underfit).

**Fix (in `micro_lora_smoke.py::_attach_lora`):** bypass the unsloth wrapper and drive
`peft.get_peft_model` directly with `target_modules` = attention projs + the discovered
per-expert suffixes (`gate_up_projs.<i>`, `down_projs.<i>`), then `enable_input_require_grads()`.
Result: `trainable=3264, EXPERT=3072 (1536 modules × lora_A/B), attn=96, router=0`.

## Stages

- Stage 1 (load 4-bit): PASS ✅
- Stage 2 (assert attn+expert trainable, router excluded): PASS ✅ (was the failure before the fix)
- Stage 3 (10 training steps, trl SFTTrainer): PASS ✅ — `[SMOKE] TRAINING PATH PASS`
- Stage 4 (GGUF export): PASS ✅ via the bf16-merge workaround (`smoke/gguf_export.py`).
  unsloth's direct `save_pretrained_gguf` on the bnb-4bit model FAILS
  (`NotImplementedError: Quant method not yet supported: 'bitsandbytes'`), so instead:
  reload base in bf16 (MXFP4 auto-dequants) -> apply adapter -> `merge_and_unload()` ->
  save plain bf16 -> `unsloth_convert_hf_to_gguf.py --outtype bf16` -> ollama create. Works.
- Stage 5 (ollama tool-call): PASS ✅ — tuned `elmer-smoke-20b` returned tool call `position_status`.

## GGUF export note (for the real Stage-2 pipeline)

Do NOT use unsloth's `save_pretrained_gguf` directly on the 4-bit model (bnb dequant
unsupported for gpt-oss). Use the bf16-merge path in `smoke/gguf_export.py`, OR skip
GGUF entirely and serve the merged bf16 model via vLLM/transformers — gold-gen +
acceptance-eval only need inference, not ollama specifically.

## Resolved environment (pod, what actually ran the PASS)

A100-SXM4-80GB. torch 2.10.0+cu128, unsloth 2026.6.9, unsloth_zoo 2026.6.7, peft 0.19.1,
trl 0.24.0, transformers **4.55.4** (training passed; trl warns it wants >=4.56.1 but it ran —
4.57.6 also fine; the per-expert layout is identical across 4.55/4.57/5.5, so the version is
NOT the expert issue), bitsandbytes 0.49.2, accelerate 1.14.0, datasets 4.3.0, triton 3.6.0,
xformers 0.0.35. ollama 0.31.1.

Diagnostics kept in `smoke/`: `diag_experts.py` (module discovery — the one that cracked it),
`diag_moe.py` / `diag_peft.py` (targeting attempts).
