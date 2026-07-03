# micro-LoRA smoke result — 2026-07-02 (vetch-sage-oak)

**TRAINING PATH: PASS.** After root-causing + fixing the MoE-targeting blocker, the
de-risker's core purpose is met: gpt-oss-20b loads, LoRA attaches to **attention +
all 32 experts (router excluded)**, and **10 training steps backprop cleanly**.
GGUF/ollama export (serving path) has a separate residual — see below.

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
- Stage 4 (GGUF export): **FAIL (separate, downstream)** — unsloth's gpt-oss llama.cpp
  converter raises `NotImplementedError: Quant method is not yet supported: 'bitsandbytes'`;
  it can't dequant a bnb-4bit model to GGUF.
- Stage 5 (ollama tool-call): not reached (needs stage 4).

## GGUF/ollama export residual + resolution

`save_pretrained_gguf` on the bnb-4bit model fails because unsloth's gpt-oss GGUF path
doesn't dequant `bitsandbytes`. This is a **serving/export** issue, orthogonal to whether
training works (it does). Resolution path (for the real Stage-2 pipeline, not a training gate):
1. Reload the base in **bf16** (not 4-bit), apply the trained adapter, `merge_and_unload()`,
   save a plain bf16 model, then run `unsloth_convert_hf_to_gguf.py --outtype bf16` on that
   (no bnb quant to dequant). `smoke/gguf_export.py` prototypes this. OR
2. Serve the merged model directly via vLLM / transformers (skip GGUF/ollama) — gold-gen
   and acceptance-eval only need inference, not specifically ollama.

## Resolved environment (pod, what actually ran the PASS)

A100-SXM4-80GB. torch 2.10.0+cu128, unsloth 2026.6.9, unsloth_zoo 2026.6.7, peft 0.19.1,
trl 0.24.0, transformers **4.55.4** (training passed; trl warns it wants >=4.56.1 but it ran —
4.57.6 also fine; the per-expert layout is identical across 4.55/4.57/5.5, so the version is
NOT the expert issue), bitsandbytes 0.49.2, accelerate 1.14.0, datasets 4.3.0, triton 3.6.0,
xformers 0.0.35. ollama 0.31.1.

Diagnostics kept in `smoke/`: `diag_experts.py` (module discovery — the one that cracked it),
`diag_moe.py` / `diag_peft.py` (targeting attempts).
