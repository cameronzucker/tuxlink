"""Task 8 — Scenario carries an opaque `world` + grounding predicates.

The scenario file is the single cross-half artifact: the Rust testserver reads
`{id, world}` and the Python Scenario reads the whole object. These tests pin:
  - `world` round-trips through from_json/to_json byte-for-byte (dict equality)
  - the new SuccessSpec grounding predicates parse
  - a world-less fixture still round-trips exactly (no spurious world/predicate keys)
"""
import json
import os

from elmer_distill.scenario import Scenario

FX = os.path.join(os.path.dirname(__file__), "fixtures", "scenarios")


def _load(name):
    return json.load(open(os.path.join(FX, name)))


def test_world_survives_roundtrip():
    raw = _load("grounded-gateways-01.json")
    s = Scenario.from_json(raw)
    assert s.world["stations"]["gateways"][0]["callsign"] == "W7ABC"
    assert "modem" in s.world and "position" in s.world
    assert s.world["stations"]["operator_grid"] == "CN85"
    assert s.to_json() == raw


def test_grounding_predicates_parsed():
    raw = _load("grounded-gateways-01.json")
    s = Scenario.from_json(raw)
    assert s.spec.grounded_claims == ["W7ABC", "KG7XYZ"]
    assert s.spec.must_decline_when_absent == []


def test_world_defaults_empty_when_absent():
    raw = _load("emcomm-cmdpost-01.json")
    s = Scenario.from_json(raw)
    assert s.world == {}
    assert s.spec.grounded_claims == []
    assert s.spec.must_decline_when_absent == []
    # world-less fixtures keep exact-equality round-trip (no world/predicate keys emitted)
    assert s.to_json() == raw
