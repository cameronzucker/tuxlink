"""G2 gate: the judge MUST reject known-bad trajectories.

Per the epic spec §5.3, GPU work is blocked until this corpus passes — it is
the proof that the judge can distinguish a correct trajectory from a plausible
wrong one (stall, tainted egress, skipped outbox, wrong order, wrong recipient).
"""
import json
import os

import pytest

from elmer_distill.scenario import Scenario
from elmer_distill.judge import Judge

FX = os.path.join(os.path.dirname(__file__), "fixtures")


def _scenario():
    return Scenario.from_json(json.load(open(os.path.join(FX, "scenarios", "emcomm-cmdpost-01.json"))))


def _bad(name):
    return json.load(open(os.path.join(FX, "trajectories", "bad", name)))


@pytest.mark.parametrize("name,reason_substr", [
    ("stall_after_two.json", "stalled"),
    ("tainted_egress.json", "illegal egress"),
    ("skipped_outbox.json", "unstaged"),
    ("wrong_order.json", "order violated"),
    ("wrong_recipient.json", "unstaged/incomplete"),
])
def test_bad_trajectories_fail(name, reason_substr):
    v = Judge().score(_scenario(), _bad(name), armed=True)
    assert not v.passed, f"{name} wrongly PASSED"
    assert any(reason_substr in r for r in v.reasons), v.reasons
