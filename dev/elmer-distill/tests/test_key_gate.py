"""The mechanical key-gate must PASS only the canonical fused gpt-oss layout and FAIL every
unservable variant (unfused per-expert, unfused router, hollow merge, residual bnb-quant).
GPU-free — synthetic tensor names + a hand-built safetensors header exercise the whole gate,
which is exactly how it runs on the dev Pi / CI before any pod checkpoint is trusted
(tuxlink-pt2xo)."""
import json
import os
import struct
import tempfile

from elmer_distill.key_gate import (
    classify_keys,
    gate_checkpoint,
    read_checkpoint_keys,
)

N_LAYERS = 3
N_EXPERTS = 8  # gpt-oss-120b has 128; 8 keeps the fixture small and the logic identical


def _common(n_layers=N_LAYERS):
    keys = ["model.embed_tokens.weight", "model.norm.weight", "lm_head.weight"]
    for L in range(n_layers):
        for p in ("q_proj", "k_proj", "v_proj", "o_proj"):
            keys.append(f"model.layers.{L}.self_attn.{p}.weight")
    return keys


def fused_keys(n_layers=N_LAYERS):
    """Canonical FUSED gpt-oss checkpoint — the only servable shape."""
    keys = _common(n_layers)
    for L in range(n_layers):
        keys += [
            f"model.layers.{L}.mlp.router.weight",
            f"model.layers.{L}.mlp.router.bias",
            f"model.layers.{L}.mlp.experts.gate_up_proj",
            f"model.layers.{L}.mlp.experts.gate_up_proj_bias",
            f"model.layers.{L}.mlp.experts.down_proj",
            f"model.layers.{L}.mlp.experts.down_proj_bias",
        ]
    return keys


def per_expert_keys(n_layers=N_LAYERS, n_experts=N_EXPERTS):
    """UNFUSED per-expert layout unsloth trains/merges — vLLM/GGUF reject or silently corrupt."""
    keys = _common(n_layers)
    for L in range(n_layers):
        keys.append(f"model.layers.{L}.mlp.router.linear.weight")
        for e in range(n_experts):
            keys.append(f"model.layers.{L}.mlp.experts.gate_up_projs.{e}.weight")
            keys.append(f"model.layers.{L}.mlp.experts.down_projs.{e}.weight")
    return keys


def hollow_keys(n_layers=N_LAYERS, n_experts=N_EXPERTS):
    """HOLLOW merge (unsloth #3701): per-expert modules still carry base_layer + lora_A/B."""
    keys = _common(n_layers)
    for L in range(n_layers):
        keys.append(f"model.layers.{L}.mlp.router.linear.weight")
        for e in range(n_experts):
            stem = f"model.layers.{L}.mlp.experts.gate_up_projs.{e}"
            keys += [
                f"{stem}.base_layer.weight",
                f"{stem}.lora_A.default.weight",
                f"{stem}.lora_B.default.weight",
            ]
    return keys


def test_fused_passes():
    r = classify_keys(fused_keys())
    assert r.passed, r.summary()
    assert r.per_expert_keys == 0 and r.router_unfused_keys == 0
    assert r.fused_expert_keys == 4 * N_LAYERS  # gate_up_proj(+_bias) + down_proj(+_bias) per layer
    assert r.router_fused_keys == N_LAYERS
    assert r.reasons == []


def test_per_expert_fails_with_unfused_reasons():
    r = classify_keys(per_expert_keys())
    assert not r.passed
    assert r.per_expert_keys == 2 * N_LAYERS * N_EXPERTS
    assert r.fused_expert_keys == 0
    assert r.router_unfused_keys == N_LAYERS
    assert r.router_fused_keys == 0
    joined = " ".join(r.reasons)
    assert "UNFUSED experts" in joined and "UNFUSED router" in joined


def test_hollow_merge_flagged():
    r = classify_keys(hollow_keys())
    assert not r.passed
    assert r.hollow_keys == 3 * N_LAYERS * N_EXPERTS
    assert any("HOLLOW" in x for x in r.reasons)


def test_residual_bnb_quant_flagged():
    keys = fused_keys() + [
        "model.layers.0.mlp.experts.down_proj.absmax",
        "model.layers.0.mlp.experts.down_proj.quant_map",
        "model.layers.0.mlp.experts.gate_up_proj.quant_state.bitsandbytes__nf4",
    ]
    r = classify_keys(keys)
    assert not r.passed
    assert r.residual_quant_keys == 3
    assert any("RESIDUAL bnb-4bit" in x for x in r.reasons)


def test_no_experts_at_all_flagged():
    r = classify_keys(_common() + [f"model.layers.{L}.mlp.router.weight" for L in range(N_LAYERS)])
    assert not r.passed
    assert r.fused_expert_keys == 0 and r.per_expert_keys == 0
    assert any("NO expert tensors" in x for x in r.reasons)


def test_partial_fuse_is_not_a_pass():
    """A checkpoint with SOME fused + ANY leftover per-expert key must still FAIL — the silent
    corruption we most fear is a mostly-fused checkpoint with a few experts left unfused."""
    keys = fused_keys() + ["model.layers.0.mlp.experts.gate_up_projs.7.weight"]
    r = classify_keys(keys)
    assert not r.passed
    assert r.per_expert_keys == 1 and r.fused_expert_keys > 0


# --- file I/O layer: index.json + raw safetensors header parsing ---

def _write_index(dirpath, keys):
    wm = {k: "model-00001-of-00001.safetensors" for k in keys}
    with open(os.path.join(dirpath, "model.safetensors.index.json"), "w") as f:
        json.dump({"metadata": {"total_size": 0}, "weight_map": wm}, f)


def _write_safetensors_header(path, keys):
    header = {k: {"dtype": "BF16", "shape": [1], "data_offsets": [0, 2]} for k in keys}
    header["__metadata__"] = {"format": "pt"}
    blob = json.dumps(header).encode("utf-8")
    with open(path, "wb") as f:
        f.write(struct.pack("<Q", len(blob)))
        f.write(blob)


def test_gate_checkpoint_reads_index_json_dir():
    with tempfile.TemporaryDirectory() as d:
        _write_index(d, fused_keys())
        assert gate_checkpoint(d).passed
        _write_index(d, per_expert_keys())
        assert not gate_checkpoint(d).passed


def test_read_keys_from_raw_safetensors_shard_dir():
    with tempfile.TemporaryDirectory() as d:
        _write_safetensors_header(os.path.join(d, "model-00001-of-00001.safetensors"), fused_keys())
        got = set(read_checkpoint_keys(d))
        assert "__metadata__" not in got
        assert "model.layers.0.mlp.experts.gate_up_proj" in got
        assert gate_checkpoint(d).passed


def test_gate_checkpoint_direct_safetensors_file():
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, "shard.safetensors")
        _write_safetensors_header(p, per_expert_keys())
        assert not gate_checkpoint(p).passed
