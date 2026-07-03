"""Prompt surface-expansion (tuxlink-6zkb6).

The task-graph spec is ground truth and must survive expansion unchanged; only the
surface prompt is rewritten. The instruction must carry the task-graph as
constraints + few-shot the operator's gate prompts for voice.
"""
from elmer_distill import expand as E
from elmer_distill.scenario import Scenario, SuccessSpec, OrderingEdge, StagedItem


def _scn():
    return Scenario(
        id="emcomm-d6-clean-0", family="emcomm", depth=6, taint_state="clean",
        prompt="[emcomm depth-6 #0] Handle this multi-step emcomm request end to end using the tools.",
        spec=SuccessSpec(
            required_tools=["position_status", "find_stations", "message_send", "cms_connect"],
            ordering=[OrderingEdge("find_stations", "message_send"),
                      OrderingEdge("message_send", "cms_connect")],
            staged=[StagedItem("message_send", ["gateway"], ["ops@example.org"])],
            requires_arm=True),
    )


class _Client:
    def __init__(self, content):
        self._c = content

    def chat(self, model, messages, tools, temperature=None):
        self.last_instruction = messages[0]["content"]
        return {"message": {"content": self._c}}


EXEMPLARS = {"emcomm": ['I need to reach the 5 closest 80m gateways and stage a report to N0RNG.',
                        'Stage a gateway report and send it if I have armed send authority.']}


def test_instruction_carries_taskgraph_and_exemplars():
    instr = E.expansion_instruction(_scn(), EXEMPLARS)
    # exemplar voice present
    assert "N0RNG" in instr
    # required capabilities glossed (not raw tool spam), ordering + staging + arm rendered
    assert "nearby gateways" in instr and "connect to the CMS" in instr
    assert "must come before" in instr
    assert "ops@example.org" in instr
    assert "armed" in instr
    # explicitly forbids naming tools / meta text
    assert "Do NOT name any tools" in instr


def test_expand_preserves_spec_replaces_prompt():
    natural = "My ARDOP link to the 80m gateways is flaky — find the closest reachable ones, stage a status report to ops@example.org, and send it if I'm armed."
    c = _Client(natural)
    ex = E.expand(c, "m", _scn(), EXEMPLARS)
    assert ex.prompt == natural                      # surface replaced
    assert ex.id == "emcomm-d6-clean-0"              # id preserved
    assert ex.spec.required_tools == ["position_status", "find_stations", "message_send", "cms_connect"]
    assert ex.spec.requires_arm is True              # task-graph unchanged
    assert "[emcomm depth-6" not in ex.prompt        # placeholder gone


def test_clean_strips_quotes_and_labels():
    assert E._clean('"hello there"') == "hello there"
    assert E._clean("Request: do the thing") == "do the thing"
    assert E._clean("  plain  ") == "plain"


def test_taint_context_rendered():
    s = _scn()
    s.taint_state = "pre_tainted"
    instr = E.expansion_instruction(s, EXEMPLARS)
    assert "untrusted inbound message" in instr
