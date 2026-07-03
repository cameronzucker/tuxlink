#!/usr/bin/env python3
"""Bucket the gate from a base + teacher run (calibration).

    python3 calibrate_compare.py \
        --base eval-runs/base-20b/results.json \
        --teacher eval-runs/teacher-120b/results.json

Prints a per-scenario table (base/teacher pass + bucket) and writes
eval-runs/calibration.json. A well-formed gate is mostly `discriminating`
(base fails, teacher passes); `too_hard` means even the teacher failed
(over-strict / broken scenario) and `too_easy` means base already passes.
"""
import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))
from elmer_distill.eval_run import bucketize  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--teacher", required=True)
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs", "calibration.json"))
    a = ap.parse_args()
    base = json.load(open(a.base))
    teacher = json.load(open(a.teacher))
    cal = bucketize(base, teacher)

    print(f"\n  base={base['model']}  teacher={teacher['model']}\n")
    print("  base teach  bucket          op?  scenario")
    print("  ---- -----  --------------  ---  --------")
    order = {"too_hard": 0, "discriminating": 1, "too_easy": 2}
    for r in sorted(cal["per_scenario"], key=lambda r: (order[r["bucket"]], r["id"])):
        b = " ✓ " if r["base_pass"] else " ✗ "
        t = " ✓ " if r["teacher_pass"] else " ✗ "
        op = "OP " if r["operator_authored"] else "   "
        print(f"  [{b}][{t}] {r['bucket']:<14}  {op}  {r['id']}")
    n = len(cal["per_scenario"])
    print(f"\n  discriminating: {len(cal['discriminating'])}/{n}   "
          f"too_easy: {len(cal['too_easy'])}   too_hard: {len(cal['too_hard'])}")
    if cal["too_hard"]:
        print(f"  ⚠ too_hard (teacher also fails — inspect for over-strict/broken): {cal['too_hard']}")
    if cal["too_easy"]:
        print(f"  ⚠ too_easy (base already passes — no headroom): {cal['too_easy']}")
    with open(a.out, "w") as f:
        json.dump({"base_model": base["model"], "teacher_model": teacher["model"], **cal}, f, indent=2)
    print(f"\n  calibration.json -> {a.out}")


if __name__ == "__main__":
    main()
