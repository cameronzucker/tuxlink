#!/usr/bin/env python3
"""Restraint-rebalanced gold generation (tuxlink-grg1i, iter-3).

    python3 run_gold.py --model gpt-oss:120b --n 3 --out eval-runs/gold-v3

Builds the restraint-rebalanced scenario bank (`scenariogen.generate_balanced`),
runs best-of-N teacher capture over it (keeping one judge-passing trajectory per
scenario), and persists the gold trajectories + a composition/yield report. Then
assemble the Harmony JSONL:

    python3 run_assemble.py --gold eval-runs/gold-v3/gold --out eval-runs/train-v3.jsonl

WHY THIS EXISTS (iter-3): iter-1/2's gold was action-dominated, so the LoRA traded
RESTRAINT for ACTION and regressed on the frozen gate (4->3->2). This driver is the
committed, reproducible replacement for the iter-2 ephemeral pod script: it emits a
bank where taint/refusal trajectories are ~47% of scenarios while the near-certain-
yield cells alone clear the 118 gold floor (iter-2's v2 destabilized at 59 gold).

ONE VARIABLE: this changes only the gold COMPOSITION vs iter-2 (same generator cell
shapes, same teacher, same templated prompts) so a gate delta is attributable to the
rebalance. Prompt surface-expansion (run_expand.py) is intentionally NOT applied here
to preserve that isolation; a future iteration can add it as a separate variable.
"""
import argparse
import json
import os
import sys
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from elmer_distill import scenariogen                       # noqa: E402
from elmer_distill.ollama_client import OllamaClient        # noqa: E402
from elmer_distill.teacher import capture_bestof            # noqa: E402
from elmer_distill.surface import SYSTEM_PROMPT, load_tools  # noqa: E402

PREDICATE_FAMILIES = {"emcomm", "blended", "aprs"}


def _is_restraint(sid_family, depth, taint):
    return taint == "pre_tainted" and sid_family in PREDICATE_FAMILIES and depth >= 4


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="gpt-oss:120b")
    ap.add_argument("--n", type=int, default=3, help="best-of-N attempts per scenario")
    ap.add_argument("--seed", type=int, default=1, help="bank composition seed (deterministic)")
    ap.add_argument("--base-url", default="http://127.0.0.1:11434")
    ap.add_argument("--num-ctx", type=int, default=32768)
    ap.add_argument("--max-turns", type=int, default=40)
    ap.add_argument("--temperature", type=float, default=0.7)
    ap.add_argument("--min-volume", type=int, default=118,
                    help="fail if fewer gold than this survive (the iter-2 volume floor)")
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs", "gold-v3"))
    a = ap.parse_args()

    bank = scenariogen.generate_balanced(seed=a.seed)
    biting = sum(1 for s in bank if _is_restraint(s.family, s.depth, s.taint_state))
    print(f"[gold] bank={len(bank)}  restraint-biting={biting} ({biting/len(bank):.0%})  "
          f"model={a.model}  best-of-{a.n}", flush=True)

    gdir = os.path.join(a.out, "gold")
    os.makedirs(gdir, exist_ok=True)

    def client_factory(attempt):
        # fresh seed per attempt so best-of-N samples distinct trajectories
        return OllamaClient(base_url=a.base_url, num_ctx=a.num_ctx,
                            temperature=a.temperature, seed=a.seed * 1000 + attempt)

    rep = capture_bestof(client_factory, a.model, bank, SYSTEM_PROMPT, load_tools(),
                         n_attempts=a.n, max_turns=a.max_turns)

    # persist gold + count restraint fraction of the SURVIVING gold (what trains)
    fam_of = lambda t: (t.get("scenario_id") or "").split("-")[0]
    def _parts(sid):
        p = (sid or "").split("-")
        fam = p[0]
        depth = int(p[1][1:]) if len(p) > 1 and p[1].startswith("d") else 0
        taint = "pre_tainted" if "pre_tainted" in (sid or "") else "clean"
        return fam, depth, taint

    gold_restraint = 0
    for t in rep.gold:
        with open(os.path.join(gdir, f"{t['scenario_id']}.json"), "w") as f:
            json.dump(t, f, indent=2)
        fam, depth, taint = _parts(t["scenario_id"])
        if _is_restraint(fam, depth, taint):
            gold_restraint += 1

    n_gold = len(rep.gold)
    report = {
        "bank": len(bank), "bank_restraint_biting": biting,
        "gold": n_gold, "gold_restraint": gold_restraint,
        "gold_restraint_frac": (gold_restraint / n_gold) if n_gold else 0.0,
        "yield_rate": rep.yield_rate(),
        "by_cell": {f"{k[0]}-d{k[1]}-{k[2]}": v for k, v in sorted(rep.by_cell.items())},
        "family_counts": dict(Counter(fam_of(t) for t in rep.gold)),
    }
    with open(os.path.join(a.out, "report.json"), "w") as f:
        json.dump(report, f, indent=2)

    print(f"[gold] {n_gold} gold trajectories  (restraint {gold_restraint} = "
          f"{report['gold_restraint_frac']:.0%})  yield={rep.yield_rate():.0%}", flush=True)
    print(f"       family_counts={report['family_counts']}")
    print(f"       gold + report -> {a.out}/")

    if n_gold < a.min_volume:
        sys.exit(f"[gold] VOLUME FLOOR BREACH: {n_gold} < {a.min_volume} gold — teacher yield "
                 f"on the restraint cells was lower than the near-certain floor predicted. "
                 f"Raise BALANCED_WEIGHTS['biting'/'easy_action'] or --n before training "
                 f"(do NOT train under-volume — iter-2's v2 at 59 destabilized).")


if __name__ == "__main__":
    main()
