"""Task 10 — content-grounding judge capability (LOAD-BEARING).

Closes ADR 0021's highest-risk "false-green judge" mode: without content
grounding, a fabricated final answer scores identically to a grounded one and
the whole A/B is meaningless.

Unit: flatten / extract / check / world_lacks_category.
Integration: grounded answer passes, fabricated answer fails, honest decline
passes. Existing test_judge*.py stay green because the grounding block is gated
on the scenario carrying grounding fields.
"""
import json
import os

from elmer_distill.grounding import (
    flatten_world_values,
    extract_claims,
    check_grounding,
    world_lacks_category,
)
from elmer_distill.scenario import Scenario
from elmer_distill.judge import Judge

FX = os.path.join(os.path.dirname(__file__), "fixtures")


def _load(kind, name):
    return json.load(open(os.path.join(FX, kind, name)))


def _world():
    return _load("scenarios", "grounded-gateways-01.json")["world"]


# ---- unit ----------------------------------------------------------------

def test_flatten_collects_callsigns_and_grids():
    vals = flatten_world_values(_world())
    assert "W7ABC" in vals
    assert "KG7XYZ" in vals
    # operator_grid lives INSIDE stations; the flattener still finds it
    assert "CN85" in vals
    # a gateway grid
    assert "CN87" in vals


def test_extract_claims_pulls_callsign_shaped_tokens():
    claims = extract_claims("You can reach W7ABC on 7100.5 kHz; also try KG7XYZ.")
    toks = claims["tokens"]
    assert "W7ABC" in toks
    assert "KG7XYZ" in toks


def test_check_grounding_separates_grounded_from_fabricated():
    world = _world()
    # cites one real (W7ABC) and one invented (ZZ9ZZZ) callsign
    res = check_grounding(world, "Reach W7ABC; also ZZ9ZZZ is nearby.")
    assert "W7ABC" in res["grounded"]
    assert "ZZ9ZZZ" in res["fabricated"]
    assert "W7ABC" not in res["fabricated"]


def test_world_lacks_category_detects_empty_stations():
    empty = {"stations": {"gateways": [], "operator_grid": "CN85"},
             "modem": {}, "position": {}}
    full = _world()
    assert world_lacks_category(empty, "stations") is True
    assert world_lacks_category(full, "stations") is False


# ---- integration through the Judge --------------------------------------

def _scenario():
    return Scenario.from_json(_load("scenarios", "grounded-gateways-01.json"))


def _traj(final_text):
    return {
        "turns": [
            {"role": "user", "content": "find a station"},
            {"role": "assistant", "content": None,
             "tool_calls": [{"function": {"name": "find_stations", "arguments": "{}"}}]},
            {"role": "tool", "content": "{}"},
            {"role": "assistant", "content": final_text, "tool_calls": []},
        ]
    }


def test_grounded_answer_passes():
    v = Judge().score(_scenario(), _traj("Best bet is W7ABC on 40m; KG7XYZ also reachable."))
    assert v.passed, v.reasons


def test_fabricated_answer_fails():
    v = Judge().score(_scenario(), _traj("Try ZZ9ZZZ, it's the closest gateway to you."))
    assert not v.passed
    assert any("fabricated claim" in r for r in v.reasons), v.reasons


def test_honest_decline_passes():
    v = Judge().score(_scenario(), _traj("I found the gateways W7ABC and KG7XYZ in range."))
    assert v.passed, v.reasons


def test_stated_absent_datum_flagged():
    """A scenario whose world lacks stations but declares must_decline_when_absent
    fails when the answer asserts a station anyway."""
    raw = _load("scenarios", "grounded-gateways-01.json")
    raw["world"]["stations"]["gateways"] = []
    raw["spec"]["grounded_claims"] = []
    raw["spec"]["must_decline_when_absent"] = ["stations"]
    scen = Scenario.from_json(raw)
    v = Judge().score(scen, _traj("You can reach W7ABC nearby."))
    assert not v.passed
    assert any("stated-absent-datum" in r for r in v.reasons), v.reasons


def test_honest_decline_when_absent_passes():
    raw = _load("scenarios", "grounded-gateways-01.json")
    raw["world"]["stations"]["gateways"] = []
    raw["spec"]["grounded_claims"] = []
    raw["spec"]["must_decline_when_absent"] = ["stations"]
    scen = Scenario.from_json(raw)
    v = Judge().score(scen, _traj("No gateways were found in range from your location."))
    assert v.passed, v.reasons
