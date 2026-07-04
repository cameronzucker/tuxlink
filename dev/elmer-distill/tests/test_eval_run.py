"""Full-bank eval runner — plumbing + persistence (tuxlink-6zkb6).

Exercises evaluate() with a fake client (network-free). Model QUALITY is not
under test here (a trivial client fails the real specs); this locks that every
scenario is run, judged, and persisted, and that the agent-gate / operator-probe
split is counted correctly.
"""
import glob
import json
import os
import tempfile

from elmer_distill.eval_run import evaluate
from elmer_distill.scenario import Scenario, SuccessSpec

HERE = os.path.dirname(__file__)
CANDIDATES = os.path.normpath(os.path.join(HERE, "..", "gate", "candidates"))


class _StopClient:
    def chat(self, model, messages, tools, temperature=0):
        return {"message": {"content": "done", "thinking": "", "tool_calls": []}}


def _scn(sid, op):
    return Scenario(id=sid, family=sid.split("-")[0], depth=2, taint_state="clean",
                    prompt=f"p {sid}",
                    spec=SuccessSpec(required_tools=[], ordering=[], staged=[]),
                    operator_authored=op)


def _load_bank():
    scns = []
    for p in sorted(glob.glob(os.path.join(CANDIDATES, "*.json"))):
        with open(p) as f:
            scns.append(Scenario.from_json(json.load(f)))
    return scns


def test_evaluate_persists_and_counts():
    out = tempfile.mkdtemp()
    scns = [_scn("gen-a", False), _scn("warc-vara-plan-drive-p2p", True)]
    summ = evaluate(_StopClient(), "m", scns, "SYS", tools=[], out_dir=out, label="base-20b")
    assert summ.n == 2
    # empty specs -> both pass; split counted by authorship
    assert summ.gate_agent_total == 1 and summ.probe_operator_total == 1
    assert summ.passed == 2
    # transcripts + results.json on disk
    assert os.path.exists(os.path.join(out, "base-20b", "gen-a.json"))
    res = json.load(open(os.path.join(out, "base-20b", "results.json")))
    assert res["n"] == 2 and len(res["results"]) == 2


class _FalseSentClient:
    """Ends every scenario claiming a send with no egress — the honesty violation."""
    def chat(self, model, messages, tools, temperature=0):
        return {"message": {"content": "Message sent to N0RNG.", "thinking": "", "tool_calls": []}}


def test_scored_segregation_and_honesty_measurement():
    """Injection-refusal cells (scored=False) are MEASURED but excluded from the headline;
    false 'sent' claims are counted as the honesty measurement, split by cell type
    (operator 2026-07-04: punt injection-refusal, measure honesty, don't train it)."""
    out = tempfile.mkdtemp()
    arm = SuccessSpec(required_tools=[], ordering=[], staged=[], requires_arm=True)
    scored = Scenario("gen-clean", "gen", 4, "clean", "p", arm)                    # scored (default)
    unscored = Scenario("gen-inject", "gen", 4, "clean", "p", arm, scored=False)   # injection cell
    summ = evaluate(_FalseSentClient(), "m", [scored, unscored], "SYS", tools=[],
                    out_dir=out, label="t")
    # headline excludes the injection cell
    assert summ.scored_total == 1 and summ.unscored_total == 1
    assert summ.scored_passed == 0                          # false-sent fails the scored cell
    # honesty measured on BOTH, split by whether an injection could have instructed the lie
    assert summ.false_sent_claims == 2 and summ.unscored_false_sent == 1
    res = json.load(open(os.path.join(out, "t", "results.json")))
    assert res["scored_total"] == 1 and res["false_sent_claims"] == 2
    assert {r["id"]: r["scored"] for r in res["results"]} == {"gen-clean": True, "gen-inject": False}


def test_evaluate_over_real_bank_splits_7_probe():
    out = tempfile.mkdtemp()
    scns = _load_bank()
    summ = evaluate(_StopClient(), "gpt-oss:20b", scns, "SYS", tools=[],
                    out_dir=out, label="smoke")
    assert summ.n == 16
    assert summ.probe_operator_total == 7    # the operator held-out probe set
    assert summ.gate_agent_total == 9
    # every scenario got a persisted transcript
    for r in summ.results:
        assert os.path.exists(r["transcript_path"])
