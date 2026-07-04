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
from elmer_distill.baseline_g0 import run_g0                # noqa: E402
from elmer_distill.surface import SYSTEM_PROMPT, load_tools  # noqa: E402

PREDICATE_FAMILIES = {"emcomm", "blended", "aprs"}


def _is_restraint(family, depth, taint):
    """Any taint-discipline cell (egress-refusal d6 OR taint-honesty d4)."""
    return taint == "pre_tainted" and family in PREDICATE_FAMILIES and depth >= 4


def _is_egress_refusal(family, depth, taint):
    """The strong half: taint drops a gated egress (only the d6 predicate cells)."""
    return taint == "pre_tainted" and scenariogen._drops_gated_egress(family, depth)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="gpt-oss:120b")
    ap.add_argument("--n", type=int, default=2,
                    help="best-of-N attempts per scenario (default 2 = iter-2 parity, so "
                         "composition stays the single changed variable; raise only if the "
                         "volume guard fires, and note it in the run report)")
    ap.add_argument("--seed", type=int, default=1, help="bank composition seed (deterministic)")
    ap.add_argument("--base-url", default="http://127.0.0.1:11434")
    ap.add_argument("--num-ctx", type=int, default=32768)
    ap.add_argument("--max-turns", type=int, default=40)
    ap.add_argument("--max-reprompts", type=int, default=2,
                    help="scaffold verifier reprompts (iter-2 parity: the checklist + reprompt "
                         "loop is what lifts teacher yield from ~5%% raw to usable gold)")
    ap.add_argument("--temperature", type=float, default=0.7)
    ap.add_argument("--min-volume", type=int, default=118,
                    help="fail if fewer gold than this survive (the iter-2 volume floor)")
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs", "gold-v3"))
    a = ap.parse_args()

    bank = scenariogen.generate_balanced(seed=a.seed)
    biting = sum(1 for s in bank if _is_restraint(s.family, s.depth, s.taint_state))
    refusal = sum(1 for s in bank if _is_egress_refusal(s.family, s.depth, s.taint_state))
    print(f"[gold] bank={len(bank)}  taint-discipline={biting} ({biting/len(bank):.0%}, "
          f"egress-refusal={refusal})  model={a.model}  best-of-{a.n}", flush=True)

    gdir = os.path.join(a.out, "gold")
    # Refuse a non-empty gold dir (Codex adrev 2026-07-03 P1): stale trajectories from a
    # prior run would survive an under-yielding rerun and be globbed by run_assemble,
    # silently bypassing this run's --min-volume / composition guarantee.
    import glob as _glob
    if _glob.glob(os.path.join(gdir, "*.json")):
        sys.exit(f"[gold] REFUSING to write into non-empty gold dir {gdir}: it holds gold "
                 f"from a prior run that run_assemble would mix in. Choose a fresh --out "
                 f"(or remove {gdir} yourself) so the volume/composition guard is authoritative.")
    os.makedirs(gdir, exist_ok=True)

    def client_factory(attempt):
        # fresh seed per attempt so best-of-N samples distinct trajectories
        return OllamaClient(base_url=a.base_url, num_ctx=a.num_ctx,
                            temperature=a.temperature, seed=a.seed * 1000 + attempt)

    # SCAFFOLDED gold-gen (iter-2 parity): the raw agentic loop yields ~5% from the 120b
    # (its cold gate score), which starves the pool. run_g0 injects a per-scenario tool
    # checklist + a verifier reprompt loop into the teacher's prompt only; the saved
    # trajectory stays clean, so the student never trains on the checklist.
    def scaffolded(client, model, s, system, tools, max_turns):
        return run_g0(client, model, s, system, tools, exemplars=[],
                      max_reprompts=a.max_reprompts, max_turns=max_turns)

    rep = capture_bestof(client_factory, a.model, bank, SYSTEM_PROMPT, load_tools(),
                         n_attempts=a.n, max_turns=a.max_turns, runner=scaffolded)

    # persist gold + count restraint fraction of the SURVIVING gold (what trains)
    fam_of = lambda t: (t.get("scenario_id") or "").split("-")[0]
    def _parts(sid):
        p = (sid or "").split("-")
        fam = p[0]
        depth = int(p[1][1:]) if len(p) > 1 and p[1].startswith("d") else 0
        taint = "pre_tainted" if "pre_tainted" in (sid or "") else "clean"
        return fam, depth, taint

    gold_restraint = gold_egress_refusal = 0
    for t in rep.gold:
        with open(os.path.join(gdir, f"{t['scenario_id']}.json"), "w") as f:
            json.dump(t, f, indent=2)
        fam, depth, taint = _parts(t["scenario_id"])
        if _is_restraint(fam, depth, taint):
            gold_restraint += 1
        if _is_egress_refusal(fam, depth, taint):
            gold_egress_refusal += 1

    n_gold = len(rep.gold)
    report = {
        "n_attempts": a.n, "temperature": a.temperature, "seed": a.seed, "model": a.model,
        "bank": len(bank), "bank_restraint": biting,
        "gold": n_gold, "gold_restraint": gold_restraint,
        "gold_egress_refusal": gold_egress_refusal,               # the strong d6-drop half
        "gold_taint_honesty": gold_restraint - gold_egress_refusal,  # the d4 honesty half
        "gold_restraint_frac": (gold_restraint / n_gold) if n_gold else 0.0,
        "yield_rate": rep.yield_rate(),
        "by_cell": {f"{k[0]}-d{k[1]}-{k[2]}": v for k, v in sorted(rep.by_cell.items())},
        "family_counts": dict(Counter(fam_of(t) for t in rep.gold)),
    }
    with open(os.path.join(a.out, "report.json"), "w") as f:
        json.dump(report, f, indent=2)

    print(f"[gold] {n_gold} gold  (restraint {gold_restraint} = {report['gold_restraint_frac']:.0%}: "
          f"egress-refusal {gold_egress_refusal} + taint-honesty {gold_restraint - gold_egress_refusal})  "
          f"yield={rep.yield_rate():.0%}  n={a.n} temp={a.temperature}", flush=True)
    print(f"       family_counts={report['family_counts']}")
    print(f"       gold + report -> {a.out}/")

    if n_gold < a.min_volume:
        sys.exit(f"[gold] VOLUME FLOOR BREACH: {n_gold} < {a.min_volume} gold — teacher yield "
                 f"on the restraint cells was lower than the near-certain floor predicted. "
                 f"Raise BALANCED_WEIGHTS['biting'/'easy_action'] or --n before training "
                 f"(do NOT train under-volume — iter-2's v2 at 59 destabilized).")


if __name__ == "__main__":
    main()
