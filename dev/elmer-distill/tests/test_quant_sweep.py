"""Quant-sweep comparison logic (tuxlink-6zkb6).

The distribution question — "does Q4 cost us gate scenarios?" — is answered
empirically by serving the tuned model at several quantizations and running the
frozen gate on each. This module is the PURE part: given per-quant gate results,
recommend the smallest quant that holds the best gate score, and render the table.
gpt-oss is MXFP4-native so ~4-bit is near home base, but the gate is deterministic —
we measure, not assume.
"""
from elmer_distill.quant_sweep import recommend, sweep_report, QUANT_RANK


def _row(quant, gate, ok=True, probe=0, size=0):
    return {"quant": quant, "gate_passed": gate, "gate_total": 16,
            "probe_passed": probe, "probe_total": 7, "size_bytes": size, "ok": ok}


def test_recommend_picks_smallest_quant_holding_best_score():
    # Q5 and Q8 tie for the best gate score; ship the smaller (Q5).
    rows = [_row("Q4_K_M", 8), _row("Q5_K_M", 9), _row("Q8_0", 9)]
    assert recommend(rows) == "Q5_K_M"


def test_recommend_prefers_q4_when_it_holds_the_score():
    # MXFP4-native happy path: Q4 loses nothing -> ship the smallest.
    rows = [_row("Q4_K_M", 9), _row("Q5_K_M", 9), _row("Q8_0", 9)]
    assert recommend(rows) == "Q4_K_M"


def test_recommend_ignores_failed_quants():
    rows = [_row("Q4_K_M", 0, ok=False), _row("Q5_K_M", 7), _row("Q8_0", 7)]
    assert recommend(rows) == "Q5_K_M"


def test_recommend_none_when_all_failed():
    assert recommend([_row("Q4_K_M", 0, ok=False)]) is None


def test_quant_rank_orders_small_to_large():
    assert QUANT_RANK["Q4_K_M"] < QUANT_RANK["Q5_K_M"] < QUANT_RANK["Q8_0"] < QUANT_RANK["BF16"]


def test_report_shows_each_quant_and_marks_recommended():
    rows = [_row("Q4_K_M", 8, size=12_000_000_000), _row("Q5_K_M", 9, size=15_000_000_000)]
    out = sweep_report(rows, base_gate=4)
    assert "Q4_K_M" in out and "Q5_K_M" in out
    assert "8/16" in out and "9/16" in out
    assert "+5" in out                       # Q5 delta vs base 4
    assert "RECOMMEND" in out and "Q5_K_M" in out.split("RECOMMEND")[1]


def test_report_handles_failed_quant_row():
    rows = [_row("Q4_K_M", 9, size=12_000_000_000), _row("Q8_0", 0, ok=False)]
    out = sweep_report(rows, base_gate=4)
    assert "Q8_0" in out and "FAILED" in out
