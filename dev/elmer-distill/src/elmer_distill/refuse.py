"""Stacking re-fuser: per-expert-LoRA'd gpt-oss merged checkpoint -> canonical FUSED checkpoint.

This is the durable, publishable core of tuxlink-pt2xo. unsloth trains gpt-oss MoE via
per-expert LoRA; a clean `peft` `merge_and_unload` on the 4-bit base bakes the deltas in but
leaves the experts in the per-expert layout (`experts.gate_up_projs.<i>.weight`,
`experts.down_projs.<i>.weight`, `mlp.router.linear.weight`). vLLM / SGLang / llama.cpp all
require the canonical FUSED layout. unsloth's own re-fuse (`save_pretrained_merged`) is broken
for dense per-expert LoRA (#3701). This re-fuser does the fuse directly and correctly.

Canonical fused gpt-oss expert params (transformers `GptOssExperts`, verified against source):
    experts.gate_up_proj      (num_experts, hidden,        2*intermediate)   applied as x @ W
    experts.gate_up_proj_bias (num_experts, 2*intermediate)
    experts.down_proj         (num_experts, intermediate,  hidden)           applied as h @ W
    experts.down_proj_bias    (num_experts, hidden)
    mlp.router.weight         (num_experts, hidden)   (raw Parameter; F.linear)
    mlp.router.bias           (num_experts,)

The subtlety that makes this non-trivial: the fused expert slice is stored in **(in, out)**
orientation (`x @ W`), while a per-expert `nn.Linear`/`Linear4bit` weight is **(out, in)**.
So each per-expert weight is the TRANSPOSE of its fused slice. `down_proj`'s slice can be square
(intermediate == hidden) where shape alone can't tell — so the transpose convention is derived
once from the non-square `gate_up_proj` and applied consistently. The content-oracle
(as-trained reference completions) is the final arbiter; a wrong transpose passes the key-gate
but fails the oracle.

The shape/planning logic here is torch-free and unit-tested with numpy; `refuse_checkpoint()`
(the disk driver) imports torch + safetensors lazily and runs on the pod.
"""
from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any, Callable, Iterable

# per-expert key: model.layers.<L>.mlp.experts.(gate_up_projs|down_projs).<E>.(weight|bias)
_EXPERT_KEY = re.compile(
    r"^(?P<prefix>.*\.mlp\.experts)\.(?P<kind>gate_up_projs|down_projs)\.(?P<expert>\d+)\.(?P<param>weight|bias)$"
)
# per-expert router: model.layers.<L>.mlp.router.linear.(weight|bias)  ->  ...router.(weight|bias)
_ROUTER_KEY = re.compile(r"^(?P<prefix>.*\.mlp\.router)\.linear\.(?P<param>weight|bias)$")

_FUSED_NAME = {"gate_up_projs": "gate_up_proj", "down_projs": "down_proj"}


@dataclass
class FusionPlan:
    """How to build every fused tensor from the per-expert checkpoint's keys."""

    # fused_key -> ordered list of per-expert source keys (index 0..E-1), for weights and biases
    expert_stacks: dict[str, list[str]] = field(default_factory=dict)
    # per-expert source key -> (fused_key, expert_index, kind, param)
    router_renames: dict[str, str] = field(default_factory=dict)   # src_key -> fused_key
    passthrough: list[str] = field(default_factory=list)           # copied verbatim
    num_experts: int = 0
    kinds_seen: set[str] = field(default_factory=set)


def parse_expert_key(key: str):
    """Return (prefix, kind, expert_index, param) for a per-expert key, else None."""
    m = _EXPERT_KEY.match(key)
    if not m:
        return None
    return m["prefix"], m["kind"], int(m["expert"]), m["param"]


def plan_fusion(keys: Iterable[str]) -> FusionPlan:
    """Group per-expert keys into fused stacks, map router renames, pass everything else through.

    Pure/stdlib — no tensors touched. The ordered source lists guarantee expert index i lands at
    slice i of the stacked tensor. Raises if an expert stack is ragged (missing an index).
    """
    keys = list(keys)
    plan = FusionPlan()
    # buckets: fused_key -> {expert_index: src_key}
    buckets: dict[str, dict[int, str]] = {}
    max_expert = -1
    for k in keys:
        parsed = parse_expert_key(k)
        if parsed:
            prefix, kind, e, param = parsed
            plan.kinds_seen.add(kind)
            suffix = "_bias" if param == "bias" else ""
            fused_key = f"{prefix}.{_FUSED_NAME[kind]}{suffix}"
            buckets.setdefault(fused_key, {})[e] = k
            max_expert = max(max_expert, e)
            continue
        rm = _ROUTER_KEY.match(k)
        if rm:
            plan.router_renames[k] = f"{rm['prefix']}.{rm['param']}"
            continue
        plan.passthrough.append(k)

    plan.num_experts = max_expert + 1
    for fused_key, by_idx in buckets.items():
        missing = [i for i in range(plan.num_experts) if i not in by_idx]
        if missing:
            raise ValueError(f"{fused_key}: ragged expert stack, missing experts {missing[:8]}"
                             f"{'…' if len(missing) > 8 else ''} (have {len(by_idx)}/{plan.num_experts})")
        plan.expert_stacks[fused_key] = [by_idx[i] for i in range(plan.num_experts)]
    return plan


def decide_transpose(per_expert_shape, target_slice_shape) -> bool:
    """Does a per-expert 2D weight need transposing to match the fused slice? Shape-driven.

    Returns False if it already matches, True if its transpose matches. Raises if neither —
    that means a genuine layout mismatch, not just an orientation flip.
    """
    pe = tuple(per_expert_shape)
    tgt = tuple(target_slice_shape)
    if pe == tgt:
        return False
    if pe == tgt[::-1]:
        return True
    raise ValueError(f"per-expert shape {pe} matches neither fused slice {tgt} nor its transpose")


def stack_expert_weights(per_expert: list, target_slice_shape, *, transpose: bool,
                         stack: Callable[[list], Any]) -> Any:
    """Orient each per-expert 2D weight and stack -> (num_experts, *target_slice_shape).

    `stack` is np.stack in tests / torch.stack on the pod (both accept a list and add a lead dim).
    Asserts every oriented slice matches the target exactly — a hard tripwire against a silent
    wrong-orientation fuse.
    """
    oriented = [(w.T if transpose else w) for w in per_expert]
    tgt = tuple(target_slice_shape)
    for i, w in enumerate(oriented):
        if tuple(w.shape) != tgt:
            raise ValueError(f"expert {i}: oriented shape {tuple(w.shape)} != target slice {tgt}")
    return stack(oriented)


def stack_expert_biases(per_expert: list, *, stack: Callable[[list], Any]) -> Any:
    """Stack per-expert 1D biases -> (num_experts, dim). No orientation to decide."""
    dim = tuple(per_expert[0].shape)
    for i, b in enumerate(per_expert):
        if tuple(b.shape) != dim:
            raise ValueError(f"bias expert {i}: shape {tuple(b.shape)} != {dim}")
    return stack(per_expert)


# --- pod-side disk driver (torch + safetensors, imported lazily) ---

def refuse_checkpoint(in_dir: str, out_dir: str, *, hidden_size: int | None = None,
                      intermediate_size: int | None = None, log=print) -> dict:
    """Read a per-expert merged gpt-oss checkpoint from `in_dir`, write a fused one to `out_dir`.

    Derives the transpose convention from the (non-square) gate_up_proj against the target slice
    shape (hidden, 2*intermediate); applies it consistently to down_proj. Reads sizes from
    config.json if not given. Returns a small report dict. Run `run_gate.py out_dir` after this;
    then the content-oracle diff is the go/no-go.
    """
    import glob
    import json
    import os
    import shutil

    import torch
    from safetensors.torch import load_file, save_file

    os.makedirs(out_dir, exist_ok=True)
    cfg_path = os.path.join(in_dir, "config.json")
    cfg = json.load(open(cfg_path)) if os.path.exists(cfg_path) else {}
    hidden = hidden_size or cfg.get("hidden_size")
    inter = intermediate_size or cfg.get("intermediate_size")
    if not hidden or not inter:
        raise ValueError("hidden_size/intermediate_size unknown — pass explicitly or provide config.json")

    tensors: dict[str, Any] = {}
    for shard in sorted(glob.glob(os.path.join(in_dir, "*.safetensors"))):
        tensors.update(load_file(shard))
    log(f"[refuse] loaded {len(tensors)} tensors from {in_dir}")

    plan = plan_fusion(tensors.keys())
    log(f"[refuse] plan: {plan.num_experts} experts, {len(plan.expert_stacks)} fused expert tensors, "
        f"{len(plan.router_renames)} router renames, {len(plan.passthrough)} passthrough")

    # derive transpose once from a gate_up weight stack vs its target slice (hidden, 2*inter)
    gate_up_w = next(k for k in plan.expert_stacks if k.endswith(".gate_up_proj"))
    sample = tensors[plan.expert_stacks[gate_up_w][0]]
    transpose = decide_transpose(sample.shape, (hidden, 2 * inter))
    log(f"[refuse] gate_up per-expert {tuple(sample.shape)} vs slice {(hidden, 2 * inter)} "
        f"-> transpose={transpose}")

    out: dict[str, Any] = {}
    for fused_key, src_keys in plan.expert_stacks.items():
        parts = [tensors[k] for k in src_keys]
        if fused_key.endswith("_bias"):
            out[fused_key] = stack_expert_biases(parts, stack=torch.stack).contiguous()
        else:
            slice_shape = (hidden, 2 * inter) if fused_key.endswith(".gate_up_proj") else (inter, hidden)
            out[fused_key] = stack_expert_weights(parts, slice_shape, transpose=transpose,
                                                  stack=torch.stack).contiguous()
    for src, dst in plan.router_renames.items():
        out[dst] = tensors[src].contiguous()
    for k in plan.passthrough:
        out[k] = tensors[k].contiguous()

    # single-shard write is fine for the sizes we serve; keeps the driver simple
    save_file(out, os.path.join(out_dir, "model.safetensors"), metadata={"format": "pt"})
    for fname in ("config.json", "generation_config.json", "tokenizer.json",
                  "tokenizer_config.json", "special_tokens_map.json"):
        src = os.path.join(in_dir, fname)
        if os.path.exists(src):
            shutil.copy2(src, os.path.join(out_dir, fname))
    report = {"num_experts": plan.num_experts, "transpose": transpose,
              "out_tensors": len(out), "fused_expert_tensors": len(plan.expert_stacks)}
    log(f"[refuse] wrote {len(out)} tensors -> {out_dir}  {report}")
    return report
