import json
import os

from elmer_distill.scenario import Scenario
from elmer_distill.baseline_g0 import run_g0

FX = os.path.join(os.path.dirname(__file__), "fixtures")


def _scenario():
    return Scenario.from_json(json.load(open(os.path.join(FX, "scenarios", "emcomm-cmdpost-01.json"))))


class TwoPhaseClient:
    """First 'final' is premature (nothing staged); after a re-prompt it stages."""
    def __init__(self):
        self.calls = 0

    def chat(self, model, messages, tools, temperature=0):
        self.calls += 1
        if self.calls == 1:
            return {"message": {"content": "All done!", "thinking": "", "tool_calls": []}}
        if self.calls == 2:
            return {"message": {"content": "", "thinking": "",
                                "tool_calls": [{"function": {"name": "message_send",
                                                             "arguments": {"to": "x", "subject": "s", "body": "b"}}}]}}
        return {"message": {"content": "sent", "thinking": "", "tool_calls": []}}


def test_verifier_loop_reprompts():
    traj = run_g0(TwoPhaseClient(), "gpt-oss:20b", _scenario(), "SYS", tools=[], exemplars=[], max_reprompts=2)
    roles = [t["role"] for t in traj["turns"]]
    assert roles.count("user") >= 2          # a corrective user turn was injected
    assert any(tc["function"]["name"] == "message_send"
               for t in traj["turns"] if t["role"] == "assistant"
               for tc in (t.get("tool_calls") or []))
