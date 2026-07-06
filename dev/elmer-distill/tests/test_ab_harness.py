"""Task 12 — behavioral A/B divergence harness (drives d3zwe).

Grounded arm (real gateways seeded) vs void arm (empty world). The harness runs
the real d3zwe binary as a subprocess and grades its final-answer `text` (the
d3zwe --json {"kind","text"} shape, Task 6) against the scenario world through the
grounding judge. Tests use a fake-d3zwe Python script emitting that same shape so
the plumbing is fully unit-tested on the Pi — no model, no network.
"""
import json
import os
import stat
import sys

from elmer_distill.ab_harness import (
    run_arm,
    grade_arm,
    divergence_report,
    decision,
)
from elmer_distill.scenario import Scenario

FX = os.path.join(os.path.dirname(__file__), "fixtures", "scenarios")


def _scenario(name):
    return Scenario.from_json(json.load(open(os.path.join(FX, name))))


# A fake d3zwe: reads TUXLINK_TEST_SCENARIO, emits a {"kind","text"} line. When
# the seeded world has gateways it cites the real callsign; when void it either
# fabricates (default) or declines (FAKE_D3ZWE_DECLINE=1).
_FAKE_D3ZWE = """#!/usr/bin/env python3
import json, os, sys
scen_path = os.environ.get("TUXLINK_TEST_SCENARIO", "")
world = json.load(open(scen_path)).get("world", {}) if scen_path else {}
gws = (world.get("stations") or {}).get("gateways") or []
if gws:
    text = "You can reach " + gws[0]["callsign"] + " on 40m."
elif os.environ.get("FAKE_D3ZWE_DECLINE") == "1":
    text = "No gateways were found in range from your location."
else:
    text = "Try ZZ9ZZZ, the closest gateway to you."
print(json.dumps({"kind": "final", "text": text}))
"""


def _write_fake(tmp_path, name="fake_d3zwe.py"):
    p = tmp_path / name
    p.write_text(_FAKE_D3ZWE)
    p.chmod(p.stat().st_mode | stat.S_IEXEC)
    return str(p)


def test_run_arm_parses_kind_text(tmp_path):
    fake = _write_fake(tmp_path)
    scen_path = os.path.join(FX, "grounded-gateways-01.json")
    tr = run_arm(scen_path, [sys.executable, fake], env={})
    assert tr["kind"] == "final"
    assert "W7ABC" in tr["text"]


def test_grounded_arm_grades_pass(tmp_path):
    fake = _write_fake(tmp_path)
    scen = _scenario("grounded-gateways-01.json")
    scen_path = os.path.join(FX, "grounded-gateways-01.json")
    tr = run_arm(scen_path, [sys.executable, fake], env={})
    v = grade_arm(scen, tr)
    assert v.passed, v.reasons


def test_void_arm_fabricates_and_fails(tmp_path):
    fake = _write_fake(tmp_path)
    scen = _scenario("void-gateways-01.json")
    scen_path = os.path.join(FX, "void-gateways-01.json")
    tr = run_arm(scen_path, [sys.executable, fake], env={})
    v = grade_arm(scen, tr)
    assert not v.passed
    assert any("fabricated claim" in r or "stated-absent-datum" in r for r in v.reasons)


def test_void_arm_honest_decline_passes(tmp_path):
    fake = _write_fake(tmp_path)
    scen = _scenario("void-gateways-01.json")
    scen_path = os.path.join(FX, "void-gateways-01.json")
    tr = run_arm(scen_path, [sys.executable, fake], env={"FAKE_D3ZWE_DECLINE": "1"})
    v = grade_arm(scen, tr)
    assert v.passed, v.reasons


def test_env_wiring_sets_scenario(tmp_path):
    """run_arm must set TUXLINK_TEST_SCENARIO to the scenario path for the child."""
    probe = tmp_path / "probe.py"
    probe.write_text(
        "import json, os\n"
        "print(json.dumps({'kind':'final','text':os.environ.get('TUXLINK_TEST_SCENARIO','MISSING')}))\n"
    )
    scen_path = os.path.join(FX, "grounded-gateways-01.json")
    tr = run_arm(scen_path, [sys.executable, str(probe)], env={})
    assert tr["text"] == scen_path


def test_divergence_and_decision_go(tmp_path):
    fake = _write_fake(tmp_path)
    gscen = _scenario("grounded-gateways-01.json")
    vscen = _scenario("void-gateways-01.json")
    gpath = os.path.join(FX, "grounded-gateways-01.json")
    vpath = os.path.join(FX, "void-gateways-01.json")
    grounded_runs = [grade_arm(gscen, run_arm(gpath, [sys.executable, fake], env={}))
                     for _ in range(3)]
    void_runs = [grade_arm(vscen, run_arm(vpath, [sys.executable, fake], env={}))
                 for _ in range(3)]
    report = divergence_report(gscen, grounded_runs, void_runs)
    assert report["grounded_pass_rate"] == 1.0
    assert report["void_fabrication_rate"] == 1.0
    assert decision(report) == "GO"


def test_decision_no_go_when_no_divergence(tmp_path):
    # both arms pass -> no divergence -> NO-GO
    fake = _write_fake(tmp_path)
    gscen = _scenario("grounded-gateways-01.json")
    vscen = _scenario("void-gateways-01.json")
    gpath = os.path.join(FX, "grounded-gateways-01.json")
    vpath = os.path.join(FX, "void-gateways-01.json")
    grounded_runs = [grade_arm(gscen, run_arm(gpath, [sys.executable, fake], env={}))]
    void_runs = [grade_arm(vscen, run_arm(vpath, [sys.executable, fake],
                                          env={"FAKE_D3ZWE_DECLINE": "1"}))]
    report = divergence_report(gscen, grounded_runs, void_runs)
    assert report["void_fabrication_rate"] == 0.0
    assert decision(report) in ("NO-GO", "AMBIGUOUS")
