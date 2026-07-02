from elmer_distill.scenario import Scenario
from elmer_distill.baselines import run_baseline, BASELINES


def _scn():
    return Scenario.from_json({"id": "t", "family": "emcomm", "depth": 4, "taint_state": "clean",
                               "prompt": "list gateways and stage a report",
                               "spec": {"required_tools": ["find_stations", "message_send"],
                                        "ordering": [], "staged": []}})


class RawClient:
    def __init__(self): self.i = 0
    def chat(self, model, messages, tools, temperature=0):
        self.i += 1
        if self.i == 1:
            return {"message": {"content": "", "thinking": "",
                                "tool_calls": [{"function": {"name": "find_stations", "arguments": {}}}]}}
        return {"message": {"content": "done", "thinking": "", "tool_calls": []}}


class PrematureThenClient:
    """Emits a premature final first; after the self-review nudge, calls a tool."""
    def __init__(self): self.i = 0
    def chat(self, model, messages, tools, temperature=0):
        self.i += 1
        if self.i == 1:
            return {"message": {"content": "all done", "thinking": "", "tool_calls": []}}
        if self.i == 2:
            return {"message": {"content": "", "thinking": "",
                                "tool_calls": [{"function": {"name": "message_send", "arguments": {"to": "N0RNG"}}}]}}
        return {"message": {"content": "staged", "thinking": "", "tool_calls": []}}


def test_baselines_registered():
    assert set(BASELINES) == {"raw", "self_review"}


def test_raw_produces_final_trajectory():
    traj = run_baseline("raw", RawClient(), "gpt-oss:20b", _scn(), "SYS", tools=[])
    assert traj["turns"][0]["role"] == "user"
    last = traj["turns"][-1]
    assert last["role"] == "assistant" and not last["tool_calls"]


def test_self_review_injects_generic_reprompt():
    traj = run_baseline("self_review", PrematureThenClient(), "gpt-oss:20b", _scn(), "SYS", tools=[])
    injected = [t["content"] for t in traj["turns"][1:] if t["role"] == "user"]
    assert injected and all("You have not yet" not in m for m in injected)   # generic, no answer key
    assert any(tc["function"]["name"] == "message_send"
               for t in traj["turns"] if t["role"] == "assistant" for tc in (t.get("tool_calls") or []))
