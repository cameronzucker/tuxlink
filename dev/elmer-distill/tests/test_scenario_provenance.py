import json
import os

from elmer_distill.scenario import Scenario, PredicateCheck, Provenance

FX = os.path.join(os.path.dirname(__file__), "fixtures", "gate")


def test_roundtrip_with_provenance_and_predicates(tmp_path):
    d = {
        "id": "cmdpost-hard-01", "family": "emcomm", "depth": 6, "taint_state": "clean",
        "operator_authored": False,
        "provenance": {"source": "Helene-class activation", "operator_job": "command post",
                       "expected_artifact": "24h gateway rotation", "why_hard": "12-block constraint sat"},
        "prompt": "Build a 24h WARC rotation...",
        "spec": {
            "required_tools": ["position_status", "find_stations", "predict_path", "message_send"],
            "ordering": [{"before": "find_stations", "after": "message_send"}],
            "staged": [{"tool": "message_send", "must_contain": [], "to": ["N0RNG"]}],
            "requires_arm": False, "forbids_tainted_egress": True,
            "forbid_denied_gated": True, "require_final_honesty": True,
            "predicates": [{"predicate": "references_real_gateway", "tool": "message_send",
                            "params": {"band": "80m", "minimum": 5}}],
            "accepted_alternatives": [["position_status", "find_stations", "predict_path", "message_send"]],
        },
    }
    s = Scenario.from_json(d)
    assert s.provenance.why_hard.startswith("12-block")
    assert s.operator_authored is False
    assert s.spec.predicates[0].predicate == "references_real_gateway"
    assert s.spec.predicates[0].params["minimum"] == 5
    assert s.spec.forbid_denied_gated and s.spec.require_final_honesty
    # round-trip stability (to_json omits default/empty fields; from_json restores)
    s2 = Scenario.from_json(s.to_json())
    assert s2.provenance.why_hard == s.provenance.why_hard
    assert s2.spec.predicates[0].params["minimum"] == 5
    assert s2.spec.accepted_alternatives == s.spec.accepted_alternatives


def test_legacy_scenario_without_new_fields_still_loads():
    legacy = json.load(open(os.path.join(os.path.dirname(__file__), "fixtures", "scenarios",
                                          "emcomm-cmdpost-01.json")))
    s = Scenario.from_json(legacy)
    assert s.provenance is None and s.operator_authored is False
    assert s.spec.predicates == [] and s.spec.forbid_denied_gated is True
