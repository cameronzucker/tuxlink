# micro-LoRA smoke result — 2026-07-02 (vetch-sage-oak)

**Verdict: FAIL at stage 2 (expected/useful failure).** The training-path de-risker
did its job — it caught that the resolved environment trains **attention only**, not
the MoE experts, which would silently underfit Stage-2 (Codex-B failure mode).

## Pod / environment

- RunPod A100-SXM4-80GB. Base image torch 2.4.1+cu124; unsloth install upgraded to
  **torch 2.10.0+cu128** (CUDA still available, GPU visible: "CUDA: 8.0, Toolkit 12.8").
- ollama 0.31.1 installed (needed `zstd`) + serving on :11434.
- Resolved pins recorded in `requirements-train.txt`.

## What passed

- Stage 1: `unsloth/gpt-oss-20b` downloaded (4 files) + loaded 4-bit via Unsloth. ✅
- Attention LoRA attaches: q/k/v/o trainable (96 attn tensors, 192 total). ✅

## What failed (the finding)

- Stage 2 assertion `hit_expert`: **0 expert params trainable.** Only attention got LoRA.
- Root cause (verified across transformers **4.55.4 / 4.57.6 / 5.5.0** — all identical):
  the model exposes experts as per-expert params
  `model.layers.N.mlp.experts.gate_up_projs.<i>.weight` (plural `gate_up_projs`),
  but unsloth 2026.6.9's internal MoE-LoRA targeting hard-codes the singular fused
  name `mlp.experts.gate_up_proj` → PEFT logs *"set target_parameters but found no
  matching parameters"* and attaches nothing to the experts.
- Not a transformers-version issue; all three exposed the plural layout. trl 0.24.0
  also pins transformers>=4.56.1, so 4.55.x isn't viable anyway.

## Resolution path (next session — deliberate, NOT GPU trial-and-error)

1. Reproduce unsloth's EXACT tested gpt-oss env (their official Colab pins) where the
   `nn.Parameter -> nn.Linear` expert conversion fires; OR
2. Manually regex-target the per-expert Linears
   (`r".*mlp\.experts\.(gate_up_projs|down_projs)\.\d+$"` + attention projs), bypassing
   unsloth's singular-name mapping; assert `requires_grad` on expert params before spend.

`smoke/diag_moe.py` is the standalone diagnostic (prints expert param names + trainable
counts per targeting approach) — run it first each attempt; it loads in ~1-2 min from
the HF cache and needs no GGUF/ollama, so it's the cheap inner loop.

## Not reached

Stages 3-5 (10 training steps, GGUF export, ollama load, tool-call) were never run —
no point training a crippled adapter. GGUF/ollama path is therefore still un-smoked.
