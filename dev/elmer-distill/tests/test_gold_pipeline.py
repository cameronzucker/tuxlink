"""Mixed-source gold + prompt expansion for the 120b build (tuxlink-48nyh).

The 120b can't teach restraint (fails taint 1/5); capture_mixed borrows those cells from a
restraint teacher (the 20b, 5/5) while the 120b teaches the quality cells. These guard the
ROUTING + provenance with fakes — no pod.
"""
from elmer_distill.scenario import Scenario, SuccessSpec
from elmer_distill import gold_pipeline as G


def _scn(sid, family, depth, taint):
    spec = SuccessSpec(required_tools=["docs_search"], ordering=[], staged=[],
                       requires_arm=False, predicates=[])
    return Scenario(sid, family, depth, taint, "handle it", spec)


def _bank():
    return [
        _scn("emcomm-d6-pre_tainted-0", "emcomm", 6, "pre_tainted"),   # restraint
        _scn("blended-d4-pre_tainted-0", "blended", 4, "pre_tainted"),  # restraint
        _scn("emcomm-d6-clean-0", "emcomm", 6, "clean"),                # quality
        _scn("aprs-d2-clean-0", "aprs", 2, "clean"),                    # quality (depth<4)
        _scn("helpdesk-d6-pre_tainted-0", "helpdesk", 6, "pre_tainted"),  # quality (family not predicate)
    ]


def test_split_restraint_partitions_taint_discipline_cells():
    restraint, other = G.split_restraint(_bank())
    assert {s.id for s in restraint} == {"emcomm-d6-pre_tainted-0", "blended-d4-pre_tainted-0"}
    assert len(other) == 3
    assert G.is_restraint_cell("emcomm", 6, "pre_tainted")
    assert not G.is_restraint_cell("emcomm", 6, "clean")       # clean is not restraint
    assert not G.is_restraint_cell("helpdesk", 6, "pre_tainted")  # non-predicate family
    assert not G.is_restraint_cell("blended", 2, "pre_tainted")   # depth<4


class _AlwaysPass:
    """A runner-free client that always yields a passing docs_search trajectory."""
    def chat(self, model, messages, tools, temperature=0):
        return {"message": {"content": "",
                            "tool_calls": [{"function": {"name": "docs_search", "arguments": {}}}]}}


def _passing_runner(client, model, scenario, system, tools, max_turns):
    return {"scenario_id": scenario.id, "turns": [
        {"role": "user", "content": "x"},
        {"role": "assistant", "content": None,
         "tool_calls": [{"function": {"name": "docs_search", "arguments": {}}}]},
        {"role": "assistant", "content": "done"}]}


def test_capture_mixed_routes_and_tags_by_source():
    qf = lambda a: _AlwaysPass()
    rf = lambda a: _AlwaysPass()
    merged, prov = G.capture_mixed(qf, "gpt-oss:120b", rf, "gpt-oss:20b",
                                   _bank(), "SYS", tools=[], n_attempts=1, max_turns=5,
                                   runner=_passing_runner)
    # every scenario captured exactly once, across both sources
    assert merged.total == 5 and len(merged.gold) == 5
    assert prov == {"quality_model": "gpt-oss:120b", "quality_cells": 3, "quality_gold": 3,
                    "restraint_model": "gpt-oss:20b", "restraint_cells": 2, "restraint_gold": 2}
    # restraint trajectories are tagged with the 20b; quality with the 120b
    by_model = {t["scenario_id"]: t["_teacher_model"] for t in merged.gold}
    assert by_model["emcomm-d6-pre_tainted-0"] == "gpt-oss:20b"
    assert by_model["blended-d4-pre_tainted-0"] == "gpt-oss:20b"
    assert by_model["emcomm-d6-clean-0"] == "gpt-oss:120b"


def test_capture_mixed_preserves_one_gold_per_scenario_bound():
    """Because each scenario lives in exactly one partition, the merged gold count can never
    exceed the bank size even with generous n_attempts."""
    qf = rf = lambda a: _AlwaysPass()
    merged, _ = G.capture_mixed(qf, "q", rf, "r", _bank(), "SYS", tools=[],
                                n_attempts=5, max_turns=5, runner=_passing_runner)
    assert len(merged.gold) == len(_bank())


class _ExpandClient:
    """Fake prompt-author: echoes a natural request so expand_bank rewrites the prompt."""
    def chat(self, model, messages, tools, temperature=None):
        return {"message": {"content": "Please handle the 40m gateway sweep and stage a report."}}


def test_expand_bank_rewrites_prompts_but_keeps_spec():
    bank = [_scn("emcomm-d6-clean-0", "emcomm", 6, "clean")]
    exemplars = {"emcomm": ['"reach the 30m gateways and send all-hands"']}
    out = G.expand_bank(_ExpandClient(), "author", bank, exemplars)
    assert out[0].prompt == "Please handle the 40m gateway sweep and stage a report."
    assert out[0].id == bank[0].id                     # id unchanged (contamination key)
    assert out[0].spec is bank[0].spec                 # task-graph ground truth untouched
