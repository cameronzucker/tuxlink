"""Mechanical key-gate for fine-tuned gpt-oss MoE checkpoints (tuxlink-pt2xo).

A fine-tuned gpt-oss checkpoint is servable by vLLM / SGLang / llama.cpp ONLY when
its experts are stored in the canonical FUSED layout. unsloth trains via per-expert
LoRA, so a naive merge/save can silently emit an unservable checkpoint that no server
rejects loudly (vLLM in particular loads a wrong layout and *silently corrupts*). This
gate is the cheap, GPU-free tripwire the handoff mandates running on ANY checkpoint
BEFORE feeding it to a server.

Canonical FUSED gpt-oss keys (PASS shape):
    model.layers.{L}.mlp.experts.gate_up_proj        (+ _bias)   # stacked (num_experts, ...)
    model.layers.{L}.mlp.experts.down_proj           (+ _bias)   # stacked
    model.layers.{L}.mlp.router.weight               (+ .bias)

UNFUSED per-expert (unsloth training) keys (FAIL shape):
    model.layers.{L}.mlp.experts.gate_up_projs.{i}.weight        # plural + index
    model.layers.{L}.mlp.experts.down_projs.{i}.weight
    model.layers.{L}.mlp.router.linear.weight

The gate also catches two adjacent failure modes seen in the wild:
  * HOLLOW merge (unsloth #3701): LoRA never baked in — `base_layer` / `lora_A|B` keys remain.
  * Residual bnb-4bit quant: `.absmax` / `.quant_map` / `quant_state` tensors — not plain bf16,
    so llama.cpp's `convert_hf_to_gguf` will choke (or MXFP4 re-export will misfire).

Stdlib-only by design: it parses `model.safetensors.index.json` OR a raw `.safetensors`
header (no torch, no safetensors package), so it runs on the dev Pi, in CI, or on the pod.
"""
from __future__ import annotations

import glob
import json
import os
import re
import struct
from dataclasses import dataclass, field
from typing import Iterable

# --- classification patterns (anchored to the gpt-oss module tree) ---
_PER_EXPERT = re.compile(r"experts\.(?:gate_up_projs|down_projs)\.\d+")
# fused expert weight, but NOT the plural `...projs.<i>` form (negative lookahead on the `s`)
_FUSED_EXPERT = re.compile(r"experts\.(?:gate_up_proj|down_proj)(?!s)")
_ROUTER_UNFUSED = re.compile(r"router\.linear\.weight")
_ROUTER_FUSED = re.compile(r"mlp\.router\.weight$")
_HOLLOW = re.compile(r"\.base_layer\.|\.lora_[AB]\.|lora_embedding_|\.lora_magnitude_")
_RESIDUAL_QUANT = re.compile(r"\.absmax$|\.quant_map$|\.quant_state|\.nested_absmax$|SCB$|\.weight_format$")


@dataclass
class GateResult:
    """Verdict of the mechanical key-gate over one checkpoint's tensor names."""

    passed: bool
    reasons: list[str] = field(default_factory=list)       # why it FAILED (empty on pass)
    fused_expert_keys: int = 0
    per_expert_keys: int = 0
    router_fused_keys: int = 0
    router_unfused_keys: int = 0
    hollow_keys: int = 0
    residual_quant_keys: int = 0
    total_keys: int = 0
    samples: dict[str, list[str]] = field(default_factory=dict)  # offending key exemplars

    def summary(self) -> str:
        head = "PASS — canonical fused layout" if self.passed else "FAIL — NOT servable as-is"
        lines = [
            f"[key-gate] {head}",
            f"  total tensors        : {self.total_keys}",
            f"  fused experts        : {self.fused_expert_keys}   (need >0)",
            f"  per-expert (unfused) : {self.per_expert_keys}   (need 0)",
            f"  router fused         : {self.router_fused_keys}   (need >0)",
            f"  router unfused       : {self.router_unfused_keys}   (need 0)",
            f"  hollow (lora/base)   : {self.hollow_keys}   (need 0)",
            f"  residual bnb-quant   : {self.residual_quant_keys}   (need 0)",
        ]
        for reason in self.reasons:
            lines.append(f"  ✗ {reason}")
        for label, keys in self.samples.items():
            lines.append(f"    e.g. {label}: {keys[0]}")
        return "\n".join(lines)


def classify_keys(keys: Iterable[str]) -> GateResult:
    """Pure classifier — the empirical heart of the gate. Takes tensor names, returns a verdict.

    PASS iff experts are fused (some fused-expert keys, zero per-expert keys), the router is
    fused (some `router.weight`, zero `router.linear.weight`), the merge is baked (no hollow
    LoRA/base_layer keys), and there is no residual bnb-4bit quant state.
    """
    keys = list(keys)
    res = GateResult(passed=False, total_keys=len(keys))
    samples: dict[str, list[str]] = {
        "per_expert": [], "fused_expert": [], "router_unfused": [],
        "router_fused": [], "hollow": [], "residual_quant": [],
    }
    for k in keys:
        if _PER_EXPERT.search(k):
            res.per_expert_keys += 1
            samples["per_expert"].append(k)
        elif _FUSED_EXPERT.search(k):
            res.fused_expert_keys += 1
            samples["fused_expert"].append(k)
        if _ROUTER_UNFUSED.search(k):
            res.router_unfused_keys += 1
            samples["router_unfused"].append(k)
        elif _ROUTER_FUSED.search(k):
            res.router_fused_keys += 1
            samples["router_fused"].append(k)
        if _HOLLOW.search(k):
            res.hollow_keys += 1
            samples["hollow"].append(k)
        if _RESIDUAL_QUANT.search(k):
            res.residual_quant_keys += 1
            samples["residual_quant"].append(k)

    reasons: list[str] = []
    if res.hollow_keys:
        reasons.append(f"HOLLOW merge — {res.hollow_keys} LoRA/base_layer keys remain "
                       "(deltas not baked in; unsloth #3701)")
    if res.per_expert_keys:
        reasons.append(f"UNFUSED experts — {res.per_expert_keys} per-expert `...projs.<i>` keys "
                       "(need stacked `experts.gate_up_proj`/`down_proj`)")
    if res.fused_expert_keys == 0 and res.per_expert_keys == 0:
        reasons.append("NO expert tensors found at all — is this a gpt-oss MoE checkpoint?")
    elif res.fused_expert_keys == 0:
        reasons.append("no fused expert tensors present")
    if res.router_unfused_keys:
        reasons.append(f"UNFUSED router — {res.router_unfused_keys} `router.linear.weight` keys "
                       "(need `router.weight`)")
    if res.router_fused_keys == 0 and res.router_unfused_keys == 0:
        reasons.append("NO router tensors found (expected `mlp.router.weight` per layer)")
    if res.residual_quant_keys:
        reasons.append(f"RESIDUAL bnb-4bit quant — {res.residual_quant_keys} quant-state tensors "
                       "(not plain bf16; GGUF/MXFP4 export will choke)")

    res.passed = not reasons
    res.reasons = reasons
    # keep only the exemplars that matter (offending categories + one confirming fused sample)
    res.samples = {kind: v[:1] for kind, v in samples.items() if v}
    return res


# --- checkpoint I/O layer (stdlib-only; no safetensors/torch dependency) ---

def _safetensors_header_keys(path: str) -> list[str]:
    """Read tensor names from a .safetensors file header without loading any weights.

    Layout: 8-byte little-endian u64 header length, then that many bytes of JSON whose
    keys are tensor names (plus an optional `__metadata__`).
    """
    with open(path, "rb") as f:
        n = struct.unpack("<Q", f.read(8))[0]
        header = json.loads(f.read(n))
    return [k for k in header.keys() if k != "__metadata__"]


def read_checkpoint_keys(path: str) -> list[str]:
    """Collect all tensor names for a checkpoint given a directory, index.json, or .safetensors.

    Prefers `model.safetensors.index.json` (sharded) → its `weight_map` keys. Falls back to
    reading every `*.safetensors` shard header in a directory. Also accepts a direct file path.
    """
    if os.path.isdir(path):
        index = os.path.join(path, "model.safetensors.index.json")
        if os.path.exists(index):
            return read_checkpoint_keys(index)
        shards = sorted(glob.glob(os.path.join(path, "*.safetensors")))
        if not shards:
            raise FileNotFoundError(
                f"{path}: no model.safetensors.index.json and no *.safetensors shards")
        keys: list[str] = []
        for shard in shards:
            keys.extend(_safetensors_header_keys(shard))
        return keys
    if path.endswith(".index.json") or os.path.basename(path) == "model.safetensors.index.json":
        with open(path) as f:
            data = json.load(f)
        wm = data.get("weight_map")
        if wm is None:
            raise ValueError(f"{path}: no `weight_map` — not a safetensors index")
        return list(wm.keys())
    if path.endswith(".safetensors"):
        return _safetensors_header_keys(path)
    raise ValueError(f"{path}: expected a directory, *.index.json, or *.safetensors")


def gate_checkpoint(path: str) -> GateResult:
    """Read a checkpoint's tensor names from disk and run the mechanical key-gate on them."""
    return classify_keys(read_checkpoint_keys(path))
