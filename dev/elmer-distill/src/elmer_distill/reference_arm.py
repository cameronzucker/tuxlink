"""Optional reference arm — active simulator {ok:true} (cnz5o Task 13).

A THIRD data point alongside the confound-free grounded-vs-void A/B (Task 12):
the agent run against the ACTIVE StatefulSimulator, whose read tools return
content-free `{ok:true}` stubs instead of world-projected DTOs. With no seeded
data to cite, a fabricating agent invents callsigns/grids; grading the final
answer against the (void) scenario world surfaces that fabrication.

This is NOT part of the primary A/B: the simulator differs from the real
testserver in loop and transport (in-process stub vs router-over-socket), so a
difference in fabrication rate confounds "grounded data present" with "different
harness". The report carries an explicit caveat to that effect. It is a
supporting signal, not the measurement.
"""
from .ab_harness import grade_arm

_CAVEAT = (
    "Reference arm runs the agent against the active simulator ({ok:true} stubs), "
    "which differs from the real testserver in loop and transport (in-process stub "
    "vs the real MCP router over a socket). Any sim-vs-void fabrication-rate "
    "difference therefore confounds data-grounding with harness identity; treat this "
    "as a supporting third data point, not part of the confound-free primary A/B."
)


def grade_reference(scenario, transcript):
    """Grade a simulator-arm transcript's final-answer `text` against the scenario
    world through the grounding judge. Same grading path as the A/B arms; the
    distinction is which harness produced the transcript (recorded in the report's
    caveat)."""
    return grade_arm(scenario, transcript)


def _fabrication_rate(verdicts):
    if not verdicts:
        return 0.0
    def _fabricated(v):
        return any("fabricated claim" in r or "stated-absent-datum" in r
                   for r in v.reasons)
    return sum(1 for v in verdicts if _fabricated(v)) / len(verdicts)


def reference_report(scenario, sim_runs, void_fabrication_rate):
    """Summarize the reference arm: the sim fabrication rate next to the primary
    A/B's void fabrication rate, with the loop/transport caveat attached."""
    return {
        "scenario_id": scenario.id,
        "sim_fabrication_rate": _fabrication_rate(sim_runs),
        "void_fabrication_rate": void_fabrication_rate,
        "sim_n": len(sim_runs),
        "caveat": _CAVEAT,
    }
