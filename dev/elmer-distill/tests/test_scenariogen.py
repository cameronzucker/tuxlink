from elmer_distill.scenariogen import generate, split_by_task_graph, task_graph_signature
from elmer_distill.tool_surface import EGRESS_TOOLS, TIER2_WRITE_TOOLS


def test_deterministic_and_covers_cells():
    a = generate(seed=1, n_per_cell=2)
    b = generate(seed=1, n_per_cell=2)
    assert [s.id for s in a] == [s.id for s in b]              # deterministic
    fams = {s.family for s in a}
    assert {"radio_debug", "emcomm", "helpdesk", "blended"} <= fams
    assert any(s.depth >= 6 for s in a)                        # deep multi-tool present


def test_holdout_shares_no_task_graph():
    scen = generate(seed=1, n_per_cell=3)
    train, hold = split_by_task_graph(scen, holdout_frac=0.2, seed=0)
    tr = {task_graph_signature(s) for s in train}
    ho = {task_graph_signature(s) for s in hold}
    assert tr.isdisjoint(ho) and len(hold) > 0
    assert len(train) + len(hold) == len(scen)


def _by_id(scenarios):
    return {s.id: s for s in scenarios}


def test_pre_tainted_drops_egress_and_tier2():
    scen = _by_id(generate(seed=1, n_per_cell=1))
    # clean emcomm d6 requires egress (cms_connect); pre_tainted must NOT.
    clean = scen["emcomm-d6-clean-0"]
    tainted = scen["emcomm-d6-pre_tainted-0"]
    assert "cms_connect" in clean.spec.required_tools
    assert clean.spec.requires_arm is True
    for gated in EGRESS_TOOLS | TIER2_WRITE_TOOLS:
        assert gated not in tainted.spec.required_tools
    assert tainted.spec.requires_arm is False
    # staging (always allowed) is retained under taint
    assert "message_send" in tainted.spec.required_tools


def test_prompts_are_concrete_not_placeholders():
    for s in generate(seed=1, n_per_cell=1):
        assert "handle this multi-step" not in s.prompt.lower()   # old vague placeholder
        assert len(s.prompt) > 40
    # each family elicits its domain
    scen = _by_id(generate(seed=1, n_per_cell=1))
    assert "modem" in scen["radio_debug-d2-clean-0"].prompt.lower()
    assert "gateway" in scen["emcomm-d2-clean-0"].prompt.lower()
    assert "password" in scen["helpdesk-d2-clean-0"].prompt.lower()
