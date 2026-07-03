"""Contamination guard — the frozen gate is the TEST set, never training gold.

The before/after qualitative probe (tuxlink-6zkb6) is only valid if the operator
scenarios are "problems the model hasn't seen." That holds ONLY if those prompts
are held out of gold-gen. Two enforced layers:

  1. `teacher.capture` diverts `operator_authored` passing trajectories into
     `held_out` (measured for the probe) instead of `gold` (training data).
  2. `dataset.assemble` refuses — hard — to write a dataset whose gold contains
     any held-out scenario id, keyed on the WHOLE frozen gate (operator AND
     agent authored), so the 6 agent-penned candidates can't leak either.

Before this guard, `operator_authored` existed in the schema but nothing read
it: a decorative flag that would have silently poisoned the eval on first
gold-gen.
"""
import os
import tempfile

import pytest

from elmer_distill import teacher
from elmer_distill.scenario import Scenario, SuccessSpec

harmony = pytest.importorskip("elmer_distill.harmony")
from elmer_distill.dataset import (  # noqa: E402
    assemble, ContaminationError, holdout_ids_from_dir,
)


def _enc_available():
    try:
        harmony._enc()
        return True
    except Exception:
        return False


needs_vocab = pytest.mark.skipif(
    not _enc_available(),
    reason="gpt-oss Harmony vocab unavailable (set TIKTOKEN_ENCODINGS_BASE)")


class _StopClient:
    """One final assistant turn, no tool calls — any scenario trivially completes;
    with an empty spec the Judge passes it, so capture would gold it if unguarded."""
    def chat(self, model, messages, tools, temperature=0):
        return {"message": {"content": "done", "thinking": "", "tool_calls": []}}


def _scn(sid, operator_authored):
    return Scenario(
        id=sid, family=sid.split("-")[0], depth=2, taint_state="clean",
        prompt="do the thing",
        spec=SuccessSpec(required_tools=[], ordering=[], staged=[]),
        operator_authored=operator_authored,
    )


def _traj(sid):
    return {"scenario_id": sid, "turns": [
        {"role": "user", "content": "hi"},
        {"role": "assistant", "thinking": "", "content": "hello", "tool_calls": []},
    ]}


def test_capture_diverts_operator_authored_out_of_gold():
    scns = [_scn("gen-alpha", False), _scn("warc-vara-plan-drive-p2p", True)]
    rep = teacher.capture(_StopClient(), "m", scns, "SYS", tools=[])
    gold_ids = {t["scenario_id"] for t in rep.gold}
    held_ids = {t["scenario_id"] for t in rep.held_out}
    # both PASS the empty spec — the divert is by role, not by failure
    assert rep.passed == 2
    assert "gen-alpha" in gold_ids
    assert "warc-vara-plan-drive-p2p" not in gold_ids   # test set: never trained
    assert "warc-vara-plan-drive-p2p" in held_ids        # measured for the probe


def test_assemble_refuses_held_out_leak():
    gold = [_traj("gen-alpha"), _traj("warc-vara-plan-drive-p2p")]
    out = os.path.join(tempfile.mkdtemp(), "train.jsonl")
    with pytest.raises(ContaminationError) as ei:
        assemble(gold, "SYS", out, holdout_ids={"warc-vara-plan-drive-p2p"})
    assert "warc-vara-plan-drive-p2p" in str(ei.value)
    assert not os.path.exists(out)   # a contaminated dataset is never written


@needs_vocab
def test_assemble_clean_gold_still_writes():
    out = os.path.join(tempfile.mkdtemp(), "train.jsonl")
    st = assemble([_traj("gen-alpha")], "SYS", out,
                  holdout_ids={"warc-vara-plan-drive-p2p"})
    assert st.n == 1 and os.path.exists(out)


def test_assemble_backcompat_no_holdout():
    # default (no holdout) must not raise — existing callers unaffected
    out = os.path.join(tempfile.mkdtemp(), "train.jsonl")
    if _enc_available():
        st = assemble([_traj("gen-alpha")], "SYS", out)
        assert st.n == 1


def test_holdout_ids_from_dir_covers_full_gate():
    here = os.path.dirname(__file__)
    cand = os.path.normpath(os.path.join(here, "..", "gate", "candidates"))
    ids = holdout_ids_from_dir(cand)
    assert "warc-vara-plan-drive-p2p" in ids   # operator-authored
    assert "cmdpost-rotation-80m" in ids         # agent-authored — also held out
    assert len(ids) == 16
