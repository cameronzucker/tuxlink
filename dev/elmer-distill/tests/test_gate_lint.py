"""gate_lint: every gate candidate must load, carry provenance, use only known
predicates, and reference only real tools. Catches unresolved predicates / typos
before a candidate reaches the calibration run.
"""
import glob
import json
import os

import pytest

from elmer_distill.scenario import Scenario
from elmer_distill.tool_surface import load_tool_names

CANDIDATES = sorted(glob.glob(os.path.join(os.path.dirname(__file__), "..", "gate", "candidates", "*.json")))
KNOWN_PREDICATES = {"references_real_gateway", "schedule_has_blocks", "freq_in_band", "distance_band"}
_TOOLS = load_tool_names()


def test_there_are_candidates():
    assert CANDIDATES, "no gate candidates found"


@pytest.mark.parametrize("path", CANDIDATES, ids=lambda p: os.path.basename(p))
def test_candidate_is_valid(path):
    s = Scenario.from_json(json.load(open(path)))
    assert s.provenance is not None and s.provenance.why_hard.strip(), f"{s.id}: missing provenance/why_hard"
    for chk in s.spec.predicates:
        assert chk.predicate in KNOWN_PREDICATES, f"{s.id}: unknown predicate {chk.predicate}"
        assert chk.tool in _TOOLS, f"{s.id}: predicate tool {chk.tool} not a real tool"
    for t in s.spec.required_tools:
        assert t in _TOOLS, f"{s.id}: required tool {t} not real"
    for alt in s.spec.accepted_alternatives:
        for t in alt:
            assert t in _TOOLS, f"{s.id}: alt tool {t} not real"
    for e in s.spec.ordering:
        assert e.before in _TOOLS and e.after in _TOOLS, f"{s.id}: ordering references unknown tool"
