import json
import os

from elmer_distill.scenario import Scenario
from elmer_distill.teacher import run_scenario, capture

FX = os.path.join(os.path.dirname(__file__), "fixtures")


def _scenario():
    return Scenario.from_json(json.load(open(os.path.join(FX, "scenarios", "emcomm-cmdpost-01.json"))))


class FakeClient:
    """Scripted: one tool call (position_status) then a final answer."""
    def __init__(self):
        self.i = 0

    def chat(self, model, messages, tools, temperature=0):
        self.i += 1
        if self.i == 1:
            return {"message": {"content": "", "thinking": "grid first",
                                "tool_calls": [{"function": {"name": "position_status", "arguments": {}}}]}}
        return {"message": {"content": "done", "thinking": "", "tool_calls": []}}


def test_run_scenario_builds_trajectory():
    traj = run_scenario(FakeClient(), "gpt-oss:120b", _scenario(), "SYS", tools=[])
    names = [tc["function"]["name"]
             for t in traj["turns"] if t["role"] == "assistant"
             for tc in (t.get("tool_calls") or [])]
    assert names == ["position_status"]
    last = traj["turns"][-1]
    assert last["role"] == "assistant" and not last["tool_calls"]
    assert traj["turns"][0]["role"] == "user"


def test_capture_returns_report_with_cells():
    rep = capture(FakeClient(), "gpt-oss:120b", [_scenario()], "SYS", tools=[])
    assert rep.total == 1
    assert ("emcomm", 6, "clean") in rep.by_cell
    assert rep.by_cell[("emcomm", 6, "clean")]["total"] == 1
