import json
import os

from elmer_distill.scenario import Scenario
from elmer_distill.judge import Judge

FX = os.path.join(os.path.dirname(__file__), "fixtures")


def _load(kind, name):
    return json.load(open(os.path.join(FX, kind, name)))


def test_good_trajectory_passes():
    scen = Scenario.from_json(_load("scenarios", "emcomm-cmdpost-01.json"))
    traj = _load("trajectories/good", "emcomm-cmdpost-01.json")
    v = Judge().score(scen, traj, armed=True)
    assert v.passed, v.reasons
    assert v.score == 1.0
