#!/usr/bin/env python3
"""One-pass live dequant + re-fuse: (4-bit base + per-expert LoRA) -> canonical FUSED bf16
checkpoint (tuxlink-pt2xo). This is the path that actually works, given unsloth's fused save
hollows (#3701, confirmed empirically on this adapter).

Pipeline:
  1. load the 4-bit base + adapter, `merge_and_unload()` -> deltas baked into a clean 4-bit model.
  2. walk the live modules, dequantize each Linear4bit to bf16 (per-tensor, through the GPU), and
     fuse the per-expert experts into the canonical stacked layout using the SAME unit-tested logic
     as the file-based re-fuser (`plan_fusion`, `decide_transpose`, `stack_expert_weights`).
  3. stream sharded bf16 safetensors + an index + config/tokenizer to `--out`.

Then: `run_gate.py <out>` MUST pass, then GGUF-convert + serve + `refuse_oracle.py verify`.

  python3 run_refuse_live.py --model-id unsloth/gpt-oss-120b \
      --adapter /root/elmer-train/adapter-120b --out /workspace/merged-fused \
      --config-from /workspace/merged-a1
"""
import argparse
import json
import os
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))
from elmer_distill.refuse import (  # noqa: E402  (the unit-tested fusion IP)
    decide_transpose,
    plan_fusion,
    stack_expert_biases,
    stack_expert_weights,
)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model-id", default="unsloth/gpt-oss-120b")
    ap.add_argument("--adapter", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--config-from", default="", help="dir to copy config.json/tokenizer from")
    ap.add_argument("--max-seq-length", type=int, default=4096)
    ap.add_argument("--max-shard-bytes", type=int, default=5_000_000_000)
    a = ap.parse_args()

    import torch
    import bitsandbytes as bnb
    from safetensors.torch import save_file
    from unsloth import FastLanguageModel
    from peft import PeftModel

    print(f"[refuse-live] loading {a.model_id} (4-bit) + adapter {a.adapter}", flush=True)
    model, tok = FastLanguageModel.from_pretrained(
        model_name=a.model_id, max_seq_length=a.max_seq_length, load_in_4bit=True, dtype=None)
    model = PeftModel.from_pretrained(model, a.adapter)
    print("[refuse-live] merge_and_unload (bakes deltas into the 4-bit base) ...", flush=True)
    model = model.merge_and_unload()

    modules = dict(model.named_modules())

    def get(key):
        """bf16 CPU tensor for a logical weight/bias key, dequantizing 4-bit modules through GPU."""
        mod_name, _, param = key.rpartition(".")
        m = modules[mod_name]
        if isinstance(m, bnb.nn.Linear4bit):
            if param == "weight":
                w = bnb.functional.dequantize_4bit(m.weight.data, m.weight.quant_state)
                return w.to(torch.bfloat16).cpu().contiguous()
            return m.bias.data.to(torch.bfloat16).cpu().contiguous()
        return getattr(m, param).data.to(torch.bfloat16).cpu().contiguous()

    # collect logical leaf keys (weights + biases), skipping quant-state buffers
    keys = []
    for name, m in model.named_modules():
        if isinstance(m, bnb.nn.Linear4bit):
            keys.append(name + ".weight")
            if m.bias is not None:
                keys.append(name + ".bias")
        elif isinstance(getattr(m, "weight", None), torch.nn.Parameter) and m.weight.is_floating_point():
            keys.append(name + ".weight")
            if isinstance(getattr(m, "bias", None), torch.nn.Parameter):
                keys.append(name + ".bias")
    # straggler pass: catch bare float Parameters the module-walk misses — notably gpt-oss
    # attention `sinks` (self_attn.sinks, not a .weight), which llama.cpp/vLLM require. Generic
    # so any future bare param is preserved too. (4-bit packed weights are uint8 -> skipped.)
    have = set(keys)
    for name, p in model.named_parameters():
        if name not in have and p.is_floating_point():
            keys.append(name)
            have.add(name)

    plan = plan_fusion(keys)
    hidden = model.config.hidden_size
    inter = model.config.intermediate_size
    print(f"[refuse-live] plan: {plan.num_experts} experts, {len(plan.expert_stacks)} fused tensors, "
          f"{len(plan.router_renames)} router renames, {len(plan.passthrough)} passthrough; "
          f"hidden={hidden} inter={inter}", flush=True)

    gate_up_key = next(k for k in plan.expert_stacks if k.endswith(".gate_up_proj"))
    sample = get(plan.expert_stacks[gate_up_key][0])
    transpose = decide_transpose(sample.shape, (hidden, 2 * inter))
    print(f"[refuse-live] gate_up per-expert {tuple(sample.shape)} vs slice {(hidden, 2 * inter)} "
          f"-> transpose={transpose}", flush=True)

    os.makedirs(a.out, exist_ok=True)
    weight_map, buf, state = {}, {}, {"bytes": 0, "shard": 0, "total": 0}

    def flush():
        if not buf:
            return
        state["shard"] += 1
        fname = f"model-{state['shard']:05d}.safetensors"
        save_file(buf, os.path.join(a.out, fname), metadata={"format": "pt"})
        for k in buf:
            weight_map[k] = fname
        print(f"[refuse-live]   wrote {fname} ({state['bytes'] / 1e9:.1f} GB, {len(buf)} tensors)", flush=True)
        buf.clear()
        state["bytes"] = 0

    def emit(key, t):
        t = t.contiguous()
        buf[key] = t
        nbytes = t.numel() * t.element_size()
        state["bytes"] += nbytes
        state["total"] += nbytes
        if state["bytes"] >= a.max_shard_bytes:
            flush()

    n = len(plan.expert_stacks)
    for i, fused_key in enumerate(sorted(plan.expert_stacks)):
        parts = [get(k) for k in plan.expert_stacks[fused_key]]
        if fused_key.endswith("_bias"):
            emit(fused_key, stack_expert_biases(parts, stack=torch.stack))
        else:
            slice_shape = (hidden, 2 * inter) if fused_key.endswith(".gate_up_proj") else (inter, hidden)
            emit(fused_key, stack_expert_weights(parts, slice_shape, transpose=transpose,
                                                 stack=torch.stack))
        del parts
        if (i + 1) % 24 == 0 or i + 1 == n:
            print(f"[refuse-live]   fused {i + 1}/{n} expert tensors", flush=True)
    for src, dst in plan.router_renames.items():
        emit(dst, get(src))
    for k in plan.passthrough:
        emit(k, get(k))
    flush()

    with open(os.path.join(a.out, "model.safetensors.index.json"), "w") as f:
        json.dump({"metadata": {"total_size": state["total"]}, "weight_map": weight_map}, f, indent=2)
    cfg_src = a.config_from or a.out
    # config.json: strip any bnb quantization_config — the fused output is plain bf16, and a stale
    # 4-bit config would make GGUF-convert / model-load mis-handle it.
    src_cfg = os.path.join(cfg_src, "config.json")
    if os.path.exists(src_cfg):
        cfg = json.load(open(src_cfg))
        cfg.pop("quantization_config", None)
        json.dump(cfg, open(os.path.join(a.out, "config.json"), "w"), indent=2)
    for fname in ("generation_config.json", "tokenizer.json",
                  "tokenizer_config.json", "special_tokens_map.json", "chat_template.jinja"):
        src = os.path.join(cfg_src, fname)
        if os.path.exists(src) and not os.path.exists(os.path.join(a.out, fname)):
            shutil.copy2(src, os.path.join(a.out, fname))
    tok.save_pretrained(a.out)  # ensure a consistent tokenizer alongside the fused weights
    print(f"[refuse-live] DONE -> {a.out}  shards={state['shard']} total={state['total'] / 1e9:.1f}GB "
          f"transpose={transpose}", flush=True)


if __name__ == "__main__":
    main()
