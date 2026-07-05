"""Tool-subset helpers for the tool-count sensitivity probe (Probe 0)."""
from types import SimpleNamespace

from elmer_distill.surface import (
    load_tools,
    required_tool_names,
    subset_tools,
    tool_name,
)


def test_tool_name_handles_openai_and_flat_shapes():
    assert tool_name({"type": "function", "function": {"name": "find_stations"}}) == "find_stations"
    assert tool_name({"name": "rig_meters"}) == "rig_meters"
    assert tool_name({"function": {}}) == ""


def test_subset_tools_keeps_named_preserves_order_ignores_unknown():
    tools = [
        {"function": {"name": "a"}},
        {"function": {"name": "b"}},
        {"function": {"name": "c"}},
    ]
    kept = subset_tools(tools, {"c", "a", "nonexistent"})
    assert [tool_name(t) for t in kept] == ["a", "c"]  # order preserved, unknown dropped


def test_required_tool_names_unions_required_and_alternatives():
    scns = [
        SimpleNamespace(spec=SimpleNamespace(
            required_tools=["find_stations"], accepted_alternatives=[])),
        SimpleNamespace(spec=SimpleNamespace(
            required_tools=["position_status"],
            accepted_alternatives=[["modem_get_status", "config_get_ardop"]])),
    ]
    assert required_tool_names(scns) == {
        "find_stations", "position_status", "modem_get_status", "config_get_ardop",
    }


def test_required_subset_is_a_proper_nonempty_subset_of_the_full_surface():
    # Integration against the real gate candidates + the 55-tool surface: the pruned arm
    # must be smaller than the full surface (or the probe measures nothing) and non-empty.
    import glob
    import json
    import os
    from elmer_distill.scenario import Scenario

    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    cand = os.path.join(here, "gate", "candidates")
    scns = []
    for p in sorted(glob.glob(os.path.join(cand, "*.json"))):
        with open(p) as f:
            scns.append(Scenario.from_json(json.load(f)))

    full = load_tools()
    pruned = subset_tools(full, required_tool_names(scns))
    assert 0 < len(pruned) < len(full), (len(pruned), len(full))
