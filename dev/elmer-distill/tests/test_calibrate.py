from elmer_distill.scenario import Scenario
from elmer_distill.calibrate import calibrate


def _scn(sid, prompt, required):
    return Scenario.from_json({"id": sid, "family": "emcomm", "depth": 4, "taint_state": "clean",
                               "prompt": prompt,
                               "spec": {"required_tools": required, "ordering": [], "staged": []}})


def _has_tool_result(messages):
    return any(m.get("role") == "tool" for m in messages)


class TeacherFake:
    """Competent: does all tools in one turn, then finals. Passes both scenarios."""
    def chat(self, model, messages, tools, temperature=0):
        if not _has_tool_result(messages):
            return {"message": {"content": "", "thinking": "", "tool_calls": [
                {"function": {"name": t, "arguments": {}}} for t in
                ("position_status", "find_stations", "message_send")]}}
        return {"message": {"content": "done", "thinking": "", "tool_calls": []}}


class BaseFake:
    """Weak: only handles EASY (one tool); stalls immediately on HARD."""
    def chat(self, model, messages, tools, temperature=0):
        user = next((m["content"] for m in messages if m["role"] == "user"), "")
        if "EASY" in user and not _has_tool_result(messages):
            return {"message": {"content": "", "thinking": "",
                                "tool_calls": [{"function": {"name": "position_status", "arguments": {}}}]}}
        return {"message": {"content": "done", "thinking": "", "tool_calls": []}}


def test_calibration_buckets_scenarios():
    hard = _scn("hard-1", "HARD build a report", ["find_stations", "message_send"])
    easy = _scn("easy-1", "EASY where am I", ["position_status"])
    clients = {"raw": BaseFake(), "self_review": BaseFake(), "teacher": TeacherFake()}
    rep = calibrate(clients, [hard, easy], "SYS", tools=[])
    assert "hard-1" in rep.discriminating       # teacher passes, base fails
    assert "easy-1" in rep.too_easy             # base already passes
    assert rep.total == 2


def test_teacher_fail_audit_lists_unlabeled():
    # a scenario the TeacherFake cannot pass (requires a tool it never calls)
    impossible = _scn("imp-1", "HARD need packet", ["packet_connect"])
    clients = {"raw": BaseFake(), "self_review": BaseFake(), "teacher": TeacherFake()}
    rep = calibrate(clients, [impossible], "SYS", tools=[])
    assert "imp-1" in rep.too_hard
    audit = rep.teacher_fail_audit()
    assert audit and audit[0]["id"] == "imp-1" and audit[0]["label"] is None
