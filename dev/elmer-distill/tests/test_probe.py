"""Qualitative before/after probe harness (tuxlink-6zkb6).

Probe = held-out operator_authored scenarios only. Runs are persisted per label
so base-vs-trained transcripts are diffable; the card doc is hand-filled. The
empirical verdict rides along as context, never as a gate.
"""
import json
import os
import tempfile

from elmer_distill import probe
from elmer_distill.scenario import Scenario, SuccessSpec


class _StopClient:
    def chat(self, model, messages, tools, temperature=0):
        return {"message": {"content": "done", "thinking": "", "tool_calls": []}}


def _scn(sid, operator_authored):
    return Scenario(
        id=sid, family=sid.split("-")[0], depth=2, taint_state="clean",
        prompt=f"prompt for {sid}",
        spec=SuccessSpec(required_tools=[], ordering=[], staged=[]),
        operator_authored=operator_authored,
    )


def _scenarios():
    return [_scn("gen-alpha", False),
            _scn("warc-vara-plan-drive-p2p", True),
            _scn("help-tactical-identity", True)]


def test_select_is_operator_authored_only():
    ids = [s.id for s in probe.select(_scenarios())]
    assert ids == ["warc-vara-plan-drive-p2p", "help-tactical-identity"]
    assert "gen-alpha" not in ids   # generator scenario is not probed


def test_run_persists_transcripts_and_verdicts():
    out = tempfile.mkdtemp()
    rep = probe.run(_StopClient(), "m", _scenarios(), "SYS", tools=[],
                    out_dir=out, label="base-20b")
    got = {r.scenario_id for r in rep.results}
    assert got == {"warc-vara-plan-drive-p2p", "help-tactical-identity"}
    for r in rep.results:
        assert os.path.exists(r.transcript_path)
        traj = json.load(open(r.transcript_path))
        assert traj["scenario_id"] == r.scenario_id
        assert traj["turns"][0]["role"] == "user"
    # generator scenario was NOT run/persisted
    assert not os.path.exists(os.path.join(out, "base-20b", "gen-alpha.json"))


def test_render_cards_side_by_side_with_blank_fields():
    scns = _scenarios()
    out = tempfile.mkdtemp()
    before = probe.run(_StopClient(), "m", scns, "SYS", [], out, "base-20b")
    after = probe.run(_StopClient(), "m", scns, "SYS", [], out, "lora-phaseA")
    rubric = {"warc-vara-plan-drive-p2p": ["Did it adaptively re-plan?"],
              "help-tactical-identity": ["Doc-grounded, not invented menus?"]}
    doc = os.path.join(out, "cards.md")
    text = probe.render_cards(before, after, scns, rubric, doc)
    assert os.path.exists(doc)
    # each probe scenario has a card with prompt, both run labels, its leg, blanks
    assert "## warc-vara-plan-drive-p2p" in text
    assert "prompt for warc-vara-plan-drive-p2p" in text
    assert "Did it adaptively re-plan?" in text
    assert "Doc-grounded, not invented menus?" in text
    assert "base-20b" in text and "lora-phaseA" in text
    assert "- base: " in text and "- trained: " in text and "- delta: " in text
    # the generator scenario is absent from the hand-eval doc
    assert "gen-alpha" not in text


def test_load_rubric_strips_doc_keys_and_covers_probe_set():
    here = os.path.dirname(__file__)
    rubric_path = os.path.normpath(os.path.join(here, "..", "gate", "probe", "rubric.json"))
    rubric = probe.load_rubric(rubric_path)
    assert "_doc" not in rubric
    # rubric keys must be exactly the 7 operator_authored gate scenarios
    cand = os.path.normpath(os.path.join(here, "..", "gate", "candidates"))
    operator_ids = set()
    for p in [os.path.join(cand, f) for f in os.listdir(cand) if f.endswith(".json")]:
        d = json.load(open(p))
        if d.get("operator_authored"):
            operator_ids.add(d["id"])
    assert set(rubric.keys()) == operator_ids
    assert len(operator_ids) == 7
