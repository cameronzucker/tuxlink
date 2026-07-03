"""Calibration bucketing from base + teacher results (tuxlink-6zkb6)."""
from elmer_distill.eval_run import bucketize


def _res(rows):
    return {"model": "m", "results": [
        {"id": i, "passed": p, "operator_authored": op} for (i, p, op) in rows]}


def test_bucketize_three_ways():
    base = _res([("disc", False, True), ("easy", True, False), ("hard", False, False)])
    teacher = _res([("disc", True, True), ("easy", True, False), ("hard", False, False)])
    cal = bucketize(base, teacher)
    assert cal["discriminating"] == ["disc"]   # base fails, teacher passes
    assert cal["too_easy"] == ["easy"]          # base already passes
    assert cal["too_hard"] == ["hard"]          # teacher also fails
    by = {r["id"]: r for r in cal["per_scenario"]}
    assert by["disc"]["operator_authored"] is True


def test_bucketize_teacher_missing_scenario_treated_as_fail():
    base = _res([("x", False, False)])
    teacher = _res([])   # teacher had no result for x
    cal = bucketize(base, teacher)
    assert cal["too_hard"] == ["x"]
