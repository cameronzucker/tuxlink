"""The stacking re-fuser must fuse per-expert gpt-oss weights into the canonical layout with the
CORRECT orientation, and its output must pass the mechanical key-gate. The transpose convention
is the danger (a wrong transpose passes the gate but corrupts generations), so it is tested for
both orientations and for the square down_proj where shape alone is ambiguous. numpy stands in
for torch — the reshape math is identical (tuxlink-pt2xo)."""
import numpy as np
import pytest

from elmer_distill.key_gate import classify_keys
from elmer_distill.refuse import (
    decide_transpose,
    parse_expert_key,
    plan_fusion,
    stack_expert_biases,
    stack_expert_weights,
)

HIDDEN = 6
INTER = 4          # non-square gate_up + non-square down (distinct hidden/inter for clarity)
N_LAYERS = 2
N_EXPERTS = 3


def _per_expert_merged_keys(n_layers=N_LAYERS, n_experts=N_EXPERTS):
    """Keys of a clean per-expert merged checkpoint (peft merge_and_unload on the 4bit base)."""
    keys = ["model.embed_tokens.weight", "model.norm.weight", "lm_head.weight"]
    for L in range(n_layers):
        for p in ("q_proj", "k_proj", "v_proj", "o_proj"):
            keys.append(f"model.layers.{L}.self_attn.{p}.weight")
        keys.append(f"model.layers.{L}.mlp.router.linear.weight")
        keys.append(f"model.layers.{L}.mlp.router.linear.bias")
        for e in range(n_experts):
            stem = f"model.layers.{L}.mlp.experts"
            keys += [
                f"{stem}.gate_up_projs.{e}.weight",
                f"{stem}.gate_up_projs.{e}.bias",
                f"{stem}.down_projs.{e}.weight",
                f"{stem}.down_projs.{e}.bias",
            ]
    return keys


def test_parse_expert_key():
    k = "model.layers.5.mlp.experts.gate_up_projs.31.weight"
    assert parse_expert_key(k) == ("model.layers.5.mlp.experts", "gate_up_projs", 31, "weight")
    assert parse_expert_key("model.layers.5.mlp.router.linear.weight") is None
    assert parse_expert_key("model.norm.weight") is None


def test_plan_fusion_groups_and_renames():
    plan = plan_fusion(_per_expert_merged_keys())
    assert plan.num_experts == N_EXPERTS
    # one fused gate_up_proj + one down_proj (+ their _bias) per layer = 4 stacks/layer
    assert len(plan.expert_stacks) == 4 * N_LAYERS
    gk = "model.layers.0.mlp.experts.gate_up_proj"
    assert gk in plan.expert_stacks
    assert plan.expert_stacks[gk] == [
        f"model.layers.0.mlp.experts.gate_up_projs.{e}.weight" for e in range(N_EXPERTS)
    ]
    # router renamed linear.weight/bias -> weight/bias, both layers
    assert plan.router_renames["model.layers.0.mlp.router.linear.weight"] == \
        "model.layers.0.mlp.router.weight"
    assert plan.router_renames["model.layers.1.mlp.router.linear.bias"] == \
        "model.layers.1.mlp.router.bias"
    # attention + embeddings + norms pass through
    assert "model.layers.0.self_attn.q_proj.weight" in plan.passthrough
    assert "model.embed_tokens.weight" in plan.passthrough


def test_plan_fusion_rejects_ragged_stack():
    keys = _per_expert_merged_keys()
    keys.remove("model.layers.0.mlp.experts.gate_up_projs.1.weight")  # drop one expert
    with pytest.raises(ValueError, match="ragged"):
        plan_fusion(keys)


def test_decide_transpose():
    # per-expert Linear weight is (out, in) = (2*inter, hidden); fused slice is (hidden, 2*inter)
    assert decide_transpose((2 * INTER, HIDDEN), (HIDDEN, 2 * INTER)) is True
    assert decide_transpose((HIDDEN, 2 * INTER), (HIDDEN, 2 * INTER)) is False
    with pytest.raises(ValueError, match="matches neither"):
        decide_transpose((HIDDEN, HIDDEN + 1), (HIDDEN, 2 * INTER))


def test_stack_expert_weights_transposes_to_target():
    # per-expert gate_up weights in (out=2*inter, in=hidden) orientation
    per_expert = [np.arange(2 * INTER * HIDDEN).reshape(2 * INTER, HIDDEN) + e for e in range(N_EXPERTS)]
    fused = stack_expert_weights(per_expert, (HIDDEN, 2 * INTER), transpose=True, stack=np.stack)
    assert fused.shape == (N_EXPERTS, HIDDEN, 2 * INTER)
    # slice 0 must equal per_expert[0] transposed (value-level orientation check)
    assert np.array_equal(fused[0], per_expert[0].T)


def test_square_down_proj_uses_derived_transpose():
    """When intermediate == hidden the down_proj slice is square; shape can't disambiguate, so the
    convention derived from gate_up must be applied and must still transpose the values."""
    sq = HIDDEN  # force square
    per_expert = [np.arange(sq * sq).reshape(sq, sq) + e for e in range(N_EXPERTS)]
    # derived from gate_up elsewhere: transpose=True
    fused = stack_expert_weights(per_expert, (sq, sq), transpose=True, stack=np.stack)
    assert fused.shape == (N_EXPERTS, sq, sq)
    assert np.array_equal(fused[1], per_expert[1].T)  # values ARE transposed, not just copied


def test_stack_biases():
    per_expert = [np.arange(2 * INTER) + e for e in range(N_EXPERTS)]
    fused = stack_expert_biases(per_expert, stack=np.stack)
    assert fused.shape == (N_EXPERTS, 2 * INTER)
    assert np.array_equal(fused[2], per_expert[2])


def test_end_to_end_fused_keys_pass_the_gate():
    """The whole point: applying the plan produces the canonical key set the key-gate accepts."""
    plan = plan_fusion(_per_expert_merged_keys())
    fused_keys = (
        list(plan.expert_stacks.keys())
        + list(plan.router_renames.values())
        + plan.passthrough
    )
    result = classify_keys(fused_keys)
    assert result.passed, result.summary()
    assert result.per_expert_keys == 0 and result.router_unfused_keys == 0
