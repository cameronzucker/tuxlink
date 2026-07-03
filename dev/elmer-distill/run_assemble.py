#!/usr/bin/env python3
"""Assemble judge-passing gold trajectories into a Harmony SFT JSONL.

    python3 run_assemble.py --gold eval-runs/gold-seed/gold --out eval-runs/train.jsonl

Each output row: {"text": <harmony training text>, "loss_spans": [[start,end],...]}
with loss masked to ASSISTANT-generated content only (mandatory for agentic SFT —
otherwise the student learns to emit the system prompt / user turns / tool
results). The contamination guard (holdout = the frozen gate ids) refuses to build
if any gold came from the gate — training data must be the generator pool.
"""
import argparse
import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from elmer_distill.dataset import assemble, holdout_ids_from_dir  # noqa: E402
from elmer_distill.surface import SYSTEM_PROMPT                    # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gold", default=os.path.join(HERE, "eval-runs", "gold-seed", "gold"))
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs", "train.jsonl"))
    ap.add_argument("--candidates", default=os.path.join(HERE, "gate", "candidates"),
                    help="frozen-gate dir; its ids are held out of training (contamination guard)")
    a = ap.parse_args()

    gold = []
    for p in sorted(glob.glob(os.path.join(a.gold, "*.json"))):
        gold.append(json.load(open(p)))
    if not gold:
        sys.exit(f"no gold trajectories in {a.gold}")

    holdout = holdout_ids_from_dir(a.candidates)
    stats = assemble(gold, SYSTEM_PROMPT, a.out, holdout_ids=holdout)
    print(f"[assemble] {stats.n} gold trajectories -> {a.out}")
    print(f"  p95 chars={stats.p95_chars}  max chars={stats.max_chars}  (sets Phase-A max_seq_length)")
    print(f"  by family: {stats.family_counts}")


if __name__ == "__main__":
    main()
