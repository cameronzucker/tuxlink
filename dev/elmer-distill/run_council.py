#!/usr/bin/env python3
"""Multi-model teacher council over the gate — best-of-N, judge-filtered union.

    python3 run_council.py --models gpt-oss:120b,qwen2.5:72b,llama3.3:70b,nemotron:70b,gemma3:27b --n 5

For each (model, scenario): N scaffolded attempts (greedy + sampled), keep any
judge-passing trajectory. Reports a per-scenario x model pass-count matrix, the
UNION coverage (scenarios with >=1 gold from any member), and per-model totals.
Persists the union gold to eval-runs/council/gold/<scenario>.json (the training
data) + eval-runs/council/report.json.
"""
import argparse
import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from elmer_distill.scenario import Scenario           # noqa: E402
from elmer_distill.ollama_client import OllamaClient   # noqa: E402
from elmer_distill.council import run_council          # noqa: E402
from elmer_distill.surface import SYSTEM_PROMPT, load_tools  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--models", default="gpt-oss:120b,qwen2.5:72b,llama3.3:70b,nemotron:70b,gemma3:27b")
    ap.add_argument("--n", type=int, default=5)
    ap.add_argument("--base-url", default="http://127.0.0.1:11434")
    ap.add_argument("--candidates", default=os.path.join(HERE, "gate", "candidates"))
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs", "council"))
    ap.add_argument("--num-ctx", type=int, default=32768)
    ap.add_argument("--max-turns", type=int, default=40)
    ap.add_argument("--max-reprompts", type=int, default=2)
    ap.add_argument("--temperature", type=float, default=0.7)
    ap.add_argument("--limit", type=int, default=0, help="only the first N scenarios (0 = all; smoke sizing)")
    a = ap.parse_args()

    models = [m.strip() for m in a.models.split(",") if m.strip()]
    scns = [Scenario.from_json(json.load(open(p)))
            for p in sorted(glob.glob(os.path.join(a.candidates, "*.json")))]
    if a.limit:
        scns = scns[:a.limit]

    def make_client(temperature, seed):
        return OllamaClient(base_url=a.base_url, num_ctx=a.num_ctx, temperature=temperature, seed=seed)

    gdir = os.path.join(a.out, "gold")
    os.makedirs(gdir, exist_ok=True)

    def progress(model, sid, n_pass, gold, rep):
        # persist gold immediately (survives interruption) + log the cell
        if gold is not None and rep.gold.get(sid, {}).get("model") == model:
            with open(os.path.join(gdir, f"{sid}.json"), "w") as f:
                json.dump({"scenario_id": sid, "gold_model": model, **gold}, f, indent=2)
        cov = len(rep.covered())
        print(f"  [{model.split(':')[0]:<8} {sid:<34}] {n_pass}/{a.n} pass"
              f"{' GOLD' if (gold is not None and rep.gold.get(sid,{}).get('model')==model) else ''}"
              f"  (union {cov}/{len(scns)})", flush=True)

    print(f"[council] {len(scns)} scenarios x {len(models)} models x best-of-{a.n} "
          f"(scaffold+{a.max_reprompts} reprompts, model-outer)", flush=True)
    rep = run_council(make_client, models, scns, SYSTEM_PROMPT, load_tools(), n=a.n,
                      max_turns=a.max_turns, max_reprompts=a.max_reprompts,
                      temperature=a.temperature, progress=progress)

    with open(os.path.join(a.out, "report.json"), "w") as f:
        json.dump({"n": rep.n, "models": rep.models, "per_scenario": rep.per_scenario,
                   "covered": rep.covered(), "uncovered": rep.uncovered(),
                   "gold_model": {sid: g["model"] for sid, g in rep.gold.items()}}, f, indent=2)

    short = [m.split(":")[0][:7] for m in models]
    print("\n  scenario                              " + "  ".join(f"{s:>7}" for s in short) + "   gold")
    for sid in sorted(rep.per_scenario):
        cells = "  ".join(f"{rep.per_scenario[sid][m]:>7}" for m in models)
        gold = rep.gold.get(sid, {}).get("model", "—")
        print(f"  {sid:<36}  {cells}   {gold.split(':')[0]}")
    print("\n  per-model scenarios-with->=1-pass:")
    for m in models:
        c = sum(1 for sid in rep.per_scenario if rep.per_scenario[sid][m] > 0)
        print(f"    {m:<20} {c}/{len(scns)}")
    print(f"\n  UNION coverage: {len(rep.covered())}/{len(scns)}   uncovered: {rep.uncovered()}")
    print(f"  gold + report -> {a.out}/")


if __name__ == "__main__":
    main()
