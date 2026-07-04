"""Per-expert LoRA targeting is MODEL-AGNOSTIC (20b and 120b share the gpt-oss MoE
layout; only the expert COUNT differs). These guard the targeting logic WITHOUT a GPU
— the actual train run is pod-only. The 120b cold-transfer build selects the bigger
model with `--model-id unsloth/gpt-oss-120b` and nothing else changes (tuxlink-48nyh,
operator 2026-07-04)."""
import importlib.util
import os

# run_train.py is a top-level script (not under src/), load it directly.
_PATH = os.path.join(os.path.dirname(__file__), "..", "run_train.py")
_spec = importlib.util.spec_from_file_location("run_train", _PATH)
run_train = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(run_train)


def _moe_module_names(n_layers, n_experts):
    """Synthetic gpt-oss module names: attention + per-expert gate_up/down projs + router."""
    names = []
    for L in range(n_layers):
        for proj in ("q_proj", "k_proj", "v_proj", "o_proj"):
            names.append(f"model.layers.{L}.self_attn.{proj}")
        names.append(f"model.layers.{L}.mlp.router")          # router (must be excluded)
        for e in range(n_experts):
            names.append(f"model.layers.{L}.mlp.experts.gate_up_projs.{e}")
            names.append(f"model.layers.{L}.mlp.experts.down_projs.{e}")
    return names


def test_expert_suffixes_capture_every_index_20b():
    names = _moe_module_names(n_layers=2, n_experts=32)   # 20b: 32 experts
    sfx = run_train.expert_suffixes(names)
    assert f"gate_up_projs.31" in sfx and f"down_projs.31" in sfx
    assert len(sfx) == 32 * 2                              # every expert, both projs


def test_expert_suffixes_scale_to_120b_expert_count():
    """The 120b has more experts (128); the discovery must capture ALL of them, not a
    20b-sized subset — this is the whole point of dynamic per-expert targeting."""
    names = _moe_module_names(n_layers=2, n_experts=128)   # 120b: 128 experts
    sfx = run_train.expert_suffixes(names)
    assert len(sfx) == 128 * 2
    assert "gate_up_projs.127" in sfx and "down_projs.127" in sfx


def test_expert_suffixes_exclude_router_and_attention():
    sfx = run_train.expert_suffixes(_moe_module_names(1, 8))
    assert not any("router" in s for s in sfx)
    assert not any("proj" == s or "q_proj" in s for s in sfx)   # attention added separately


def test_router_params_frozen():
    assert run_train.is_router_param("model.layers.0.mlp.router.weight")
    assert run_train.is_router_param("model.layers.3.mlp.gate.weight")
    # expert + attention params must NOT be flagged as router (they train)
    assert not run_train.is_router_param("model.layers.0.mlp.experts.gate_up_projs.5")
    assert not run_train.is_router_param("model.layers.0.self_attn.q_proj")


def test_no_experts_discovered_is_a_hard_error_signal():
    """A layout with no per-expert modules yields an empty set — _attach_lora turns
    that into a RuntimeError rather than silently training attention-only."""
    dense = ["model.layers.0.self_attn.q_proj", "model.layers.0.mlp.up_proj"]
    assert run_train.expert_suffixes(dense) == []
