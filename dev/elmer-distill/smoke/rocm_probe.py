#!/usr/bin/env python3
"""ROCm / MI300X go/no-go probe — the CRUX question before betting the 120b build on AMD.

The whole train path (tuxlink-6zkb6/-5tfkm) rests on one CUDA behavior: bitsandbytes 4-bit
quantization exposes the gpt-oss MXFP4 experts as per-expert `Linear4bit` modules
(`...experts.{gate_up_projs,down_projs}.<i>`), which `_attach_lora` targets. Whether ROCm
bitsandbytes produces the SAME layout is unverified — if it doesn't, `_attach_lora` finds no
experts (the tuxlink-5tfkm failure signature) and ROCm training is a no-go.

This isolates that question cheaply (gpt-oss-20b, ~15 min) and loads TRANSFORMERS-NATIVE (no
unsloth — unsloth's AMD support is the shakiest link; if the layout is right we can decide
whether to fight unsloth on ROCm or drive peft directly as the smoke already does):

  1. load unsloth/gpt-oss-20b 4-bit via transformers + BitsAndBytesConfig
  2. ASSERT per-expert Linear4bit modules exist (reuse run_train.expert_suffixes)
  3. attach PEFT LoRA to attention + every expert (router frozen), assert trainable set
  4. 2 backprop steps on a trivial batch

PASS  -> ROCm training viable: run micro_lora_smoke.py, then run_120b.sh (ROCm stack).
FAIL@2 -> MXFP4 experts don't unpack on ROCm bnb: AMD is INFERENCE-ONLY (peft_eval / ollama);
          train on a CUDA pod. FAIL@3/4 -> peft/backward issue; capture the trace.

ROCm stack (install BEFORE running; versions to verify on first run):
  pip install --index-url https://download.pytorch.org/whl/rocm6.2 torch torchvision
  pip install "bitsandbytes>=0.44"   # needs a ROCm/HIP-enabled build (multi-backend); confirm
                                      # `python -c "import bitsandbytes"` reports the hip backend
  pip install "transformers>=4.56,<6" "peft>=0.13" "accelerate>=0.34" "datasets>=2.20"

  python3 smoke/rocm_probe.py 2>&1 | tee rocm-probe.log
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(HERE, "..")
sys.path.insert(0, ROOT)                    # run_train.py lives at the elmer-distill root

MODEL_ID = "unsloth/gpt-oss-20b"


def _p(stage, ok, msg=""):
    print(f"[{'PASS' if ok else 'FAIL'}] {stage}{': ' + msg if msg else ''}", flush=True)
    if not ok:
        sys.exit(1)


def main():
    import torch
    from transformers import AutoModelForCausalLM, BitsAndBytesConfig
    from peft import LoraConfig, get_peft_model
    from run_train import expert_suffixes, is_router_param, ATTENTION_TARGETS

    # report the accelerator so a mis-provisioned CUDA pod is obvious
    hip = getattr(torch.version, "hip", None)
    print(f"[probe] torch={torch.__version__} hip={hip} cuda_avail={torch.cuda.is_available()} "
          f"dev={torch.cuda.get_device_name(0) if torch.cuda.is_available() else 'none'}", flush=True)
    _p("0 accelerator visible", torch.cuda.is_available(), "no GPU visible to torch")

    # 1. 4-bit load, transformers-native (no unsloth)
    qcfg = BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_quant_type="nf4",
                              bnb_4bit_compute_dtype=torch.bfloat16)
    model = AutoModelForCausalLM.from_pretrained(
        MODEL_ID, quantization_config=qcfg, device_map="auto", trust_remote_code=True)
    _p("1 4-bit load (transformers-native)", True, str(next(model.parameters()).device))

    # 2. the crux — per-expert Linear4bit modules must exist
    names = [n for n, _ in model.named_modules()]
    sfx = expert_suffixes(names)
    _p("2 per-expert Linear4bit modules present", bool(sfx),
       f"{len(sfx)} expert suffixes (e.g. {sfx[:2]})" if sfx else
       "NO per-expert modules — ROCm bnb did not unpack MXFP4 (tuxlink-5tfkm class); AMD = inference-only")

    # 3. attach LoRA to attention + experts, router frozen
    model.gradient_checkpointing_enable()
    cfg = LoraConfig(r=8, lora_alpha=16, lora_dropout=0.0, bias="none", task_type="CAUSAL_LM",
                     target_modules=ATTENTION_TARGETS + sfx)
    model = get_peft_model(model, cfg)
    model.enable_input_require_grads()
    tr = [n for n, p in model.named_parameters() if p.requires_grad]
    ok = (any("q_proj" in n for n in tr)
          and any(("down_projs" in n or "gate_up_projs" in n) for n in tr)
          and not any(is_router_param(n) for n in tr))
    _p("3 LoRA on attn+experts, router frozen", ok, f"{len(tr)} trainable tensors")

    # 4. 2 backprop steps on a trivial batch
    model.train()
    ids = torch.tensor([[1, 2, 3, 4, 5, 6, 7, 8]], device=model.device)
    opt = torch.optim.AdamW([p for p in model.parameters() if p.requires_grad], lr=1e-4)
    for step in range(2):
        out = model(input_ids=ids, labels=ids)
        out.loss.backward()
        opt.step()
        opt.zero_grad()
        print(f"[probe] backprop step {step} loss={out.loss.item():.4f}", flush=True)
    _p("4 backprop through 4-bit base + expert-LoRA", True)

    print("\n=== ROCm PROBE PASSED — training path viable on this AMD device. "
          "Next: micro_lora_smoke.py, then run_120b.sh (ROCm stack). ===")


if __name__ == "__main__":
    main()
