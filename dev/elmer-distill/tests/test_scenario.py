import json
import os

from elmer_distill.scenario import Scenario, SuccessSpec, OrderingEdge, StagedItem  # noqa: F401

FX = os.path.join(os.path.dirname(__file__), "fixtures", "scenarios", "emcomm-cmdpost-01.json")


def test_roundtrip_json():
    s = Scenario.from_json(json.load(open(FX)))
    assert s.family == "emcomm" and s.depth >= 4
    assert "message_send" in s.spec.required_tools
    assert any(e.before == "find_stations" and e.after == "message_send" for e in s.spec.ordering)
    assert s.to_json() == json.load(open(FX))


def test_staged_item_predicates():
    s = Scenario.from_json(json.load(open(FX)))
    item = next(i for i in s.spec.staged if i.tool == "message_send")
    assert "cameronzucker@gmail.com" in (item.to or [])
