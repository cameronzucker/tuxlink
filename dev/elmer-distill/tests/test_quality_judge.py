"""Pairwise blind quality judging (tuxlink-48nyh).

The mechanical gate scores predicate-satisfaction, not report QUALITY (coherence,
completeness, usefulness). This measures the thing the operator flagged: is the 120b's
drafted report actually better than the 20b's, even when both pass the predicates? A
strong judge (DeepSeek-R1 via OpenRouter) picks the better of two anonymized reports;
A/B order is randomized per scenario to control position bias; the operator spot-read
anchors the judge.
"""
import json

from elmer_distill.quality_judge import (
    extract_report, build_judge_prompt, parse_verdict, judge_pair, combined_summary)


def _traj(body, final):
    return {"turns": [
        {"role": "user", "content": "task"},
        {"role": "assistant", "content": "", "tool_calls": [
            {"function": {"name": "find_stations", "arguments": {}}}]},
        {"role": "tool", "tool_name": "find_stations", "content": "{}"},
        {"role": "assistant", "content": "",
         "tool_calls": [{"function": {"name": "message_send", "arguments": {"body": body}}}]},
        {"role": "assistant", "content": final},
    ]}


def test_extract_report_pulls_staged_body_and_final():
    r = extract_report(_traj("W7GW 7040 kHz; K7XYZ 7045 kHz", "Report staged, not transmitted."))
    assert "W7GW 7040 kHz" in r          # the staged deliverable
    assert "not transmitted" in r        # the final synthesis
    # string-encoded arguments must parse too
    t = {"turns": [{"role": "assistant", "content": "done",
                    "tool_calls": [{"function": {"name": "send_form",
                                                 "arguments": '{"body": "water request"}'}}]}]}
    assert "water request" in extract_report(t)


def test_build_judge_prompt_is_blind_and_asks_for_verdict():
    msgs = build_judge_prompt("draft the 40m report", "REPORT_ALPHA", "REPORT_BETA")
    blob = json.dumps(msgs)
    assert "REPORT_ALPHA" in blob and "REPORT_BETA" in blob
    assert "VERDICT" in blob                       # judge must emit a parseable verdict
    assert "20b" not in blob and "120b" not in blob  # blind: no model identity leaks


def test_parse_verdict_reads_last_marker():
    assert parse_verdict("reasoning...\nVERDICT: A") == "A"
    assert parse_verdict("VERDICT: B because it is complete") == "B"
    assert parse_verdict("both weak\nVERDICT: TIE") == "TIE"
    assert parse_verdict("no marker here") is None


class _FakeJudge:
    """Always prefers whichever slot contains the 120b's known text."""
    def __init__(self, marker):
        self.marker = marker
    def chat(self, model, messages, tools, temperature=None):
        blob = json.dumps(messages)
        # figure out whether the 120b marker landed in A or B
        ai = blob.find("REPORT A"); bi = blob.find("REPORT B")
        win = "A" if blob.find(self.marker) < bi else "B"
        return {"message": {"content": f"VERDICT: {win}", "thinking": "", "tool_calls": []}}


def test_judge_pair_unshuffles_ab_back_to_model():
    """Whatever the randomized A/B order, a win for the slot holding the 120b text must
    be attributed to the 120b (position bias controlled)."""
    j = _FakeJudge(marker="ONETWENTY")
    # try both orderings via seed parity; both must credit the 120b
    v0 = judge_pair(j, "judge", "task", report_20b="TWENTY text",
                    report_120b="ONETWENTY text", seed=0)
    v1 = judge_pair(j, "judge", "task", report_20b="TWENTY text",
                    report_120b="ONETWENTY text", seed=1)
    assert v0["winner"] == "120b" and v1["winner"] == "120b"
    assert v0["order"] != v1["order"]   # seed parity actually swapped the slots


def test_combined_summary_folds_mechanical_and_quality():
    """Quality is first-class alongside the mechanical gate, and the parity-artifact cell
    (both pass mechanically, 120b wins quality) is surfaced — the thing the 16-item
    predicate gate was blind to (operator 2026-07-04)."""
    reports = {
        # both pass mechanically, 120b wins quality -> PARITY ARTIFACT (warc-vara class)
        "warc-vara": {"pass_20b": True, "pass_120b": True},
        # 20b fails mechanically, 120b passes + wins quality -> quality confirms mechanical
        "aprs-wx-gust": {"pass_20b": False, "pass_120b": True},
        # genuinely indistinguishable
        "helpdesk": {"pass_20b": True, "pass_120b": True},
    }
    judged = [
        {"scenario": "warc-vara", "winner": "120b"},
        {"scenario": "aprs-wx-gust", "winner": "120b"},
        {"scenario": "helpdesk", "winner": "tie"},
    ]
    s = combined_summary(reports, judged)
    assert s["mechanical"] == {"20b_pass": 2, "120b_pass": 3}
    assert s["quality"]["wins"] == {"120b": 2, "20b": 0, "tie": 1}
    assert s["quality"]["win_rate_120b"] == round(2 / 3, 2)
    assert s["parity_artifact"] == ["warc-vara"]              # the blind-spot cell
    assert s["quality_confirms_mechanical"] == ["aprs-wx-gust"]


def test_combined_summary_no_parity_artifact_when_mechanical_discriminates():
    reports = {"s1": {"pass_20b": False, "pass_120b": True}}
    judged = [{"scenario": "s1", "winner": "120b"}]
    s = combined_summary(reports, judged)
    assert s["parity_artifact"] == []   # mechanical already caught it; not an artifact


def test_combined_summary_excludes_unscored_injection_cells():
    """Injection-refusal cells (scored=False) are out of scope for the quality comparison —
    excluded from win-rate, mechanical counts, and parity (operator 2026-07-04)."""
    reports = {
        "quality": {"pass_20b": True, "pass_120b": True, "scored": True},
        "inject":  {"pass_20b": True, "pass_120b": True, "scored": False},   # must be ignored
    }
    judged = [{"scenario": "quality", "winner": "120b"},
              {"scenario": "inject", "winner": "20b"}]   # the 20b "winning" here must NOT count
    s = combined_summary(reports, judged)
    assert s["n"] == 1                                    # only the scored cell
    assert s["quality"]["wins"] == {"120b": 1, "20b": 0, "tie": 0}
    assert s["mechanical"] == {"20b_pass": 1, "120b_pass": 1}
    assert s["parity_artifact"] == ["quality"]
