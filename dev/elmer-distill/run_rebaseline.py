#!/usr/bin/env python3
"""Clean re-baseline of the frozen gate (tuxlink-grg1i, post-Fable adrev 2026-07-03).

The prior base/teacher numbers were graded under DIFFERENT judge versions and the
on-disk artifacts disagreed by 4/16 (Fable B1); the 16-item gate showed 0/6->6/6
variance on identical task graphs (H1). This re-establishes the baseline HONESTLY:

  base-20b AND teacher-120b, COLD and SCAFFOLDED, n repeats each, on the CURRENT
  judge, reporting per-scenario pass RATES (not a single pass@1).

  - cold vs scaffold answers H3: is the "teacher plateaued at 3/16" a capability
    plateau or just a cold-elicitation artifact? (scaffolding took the 120b's
    evidence yield 12%->88%; if it also lifts the 20b, the discriminating-zone
    framing is measuring elicitation, not capability.)
  - repeats quantify the H1 noise: which scenarios are stable (0 or n) vs coin-flips.

Writes a COMMITTED summary (decision-bearing data must be auditable — the old
eval-runs are gitignored). Raw transcripts stay pod-side.

  python3 run_rebaseline.py --repeats 5 --out prereg/rebaseline-2026-07-03.json
"""
import argparse
import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from elmer_distill.scenario import Scenario                 # noqa: E402
from elmer_distill.ollama_client import OllamaClient        # noqa: E402
from elmer_distill.teacher import run_scenario              # noqa: E402
from elmer_distill.baseline_g0 import run_g0                # noqa: E402
from elmer_distill.judge import Judge                       # noqa: E402
from elmer_distill.surface import SYSTEM_PROMPT, load_tools  # noqa: E402


def _run_one(cond, client, model, scn, tools, max_turns, max_reprompts):
    if cond == "cold":
        return run_scenario(client, model, scn, SYSTEM_PROMPT, tools, max_turns)
    return run_g0(client, model, scn, SYSTEM_PROMPT, tools, exemplars=[],
                  max_reprompts=max_reprompts, max_turns=max_turns)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--models", default="gpt-oss:20b,gpt-oss:120b")
    ap.add_argument("--conditions", default="cold,scaffold")
    ap.add_argument("--repeats", type=int, default=5)
    ap.add_argument("--temperature", type=float, default=0.7)
    ap.add_argument("--base-url", default="http://127.0.0.1:11434")
    ap.add_argument("--num-ctx", type=int, default=32768)
    ap.add_argument("--max-reprompts", type=int, default=2)
    ap.add_argument("--candidates", default=os.path.join(HERE, "gate", "candidates"))
    ap.add_argument("--out", default=os.path.join(HERE, "prereg", "rebaseline.json"))
    a = ap.parse_args()

    models = [m.strip() for m in a.models.split(",") if m.strip()]
    conditions = [c.strip() for c in a.conditions.split(",") if c.strip()]
    scns = [Scenario.from_json(json.load(open(p)))
            for p in sorted(glob.glob(os.path.join(a.candidates, "*.json")))]
    tools = load_tools()
    judge = Judge()

    # summary[model][cond][scenario_id] = passes (out of repeats)
    summary = {m: {c: {} for c in conditions} for m in models}
    for model in models:
        for cond in conditions:
            max_turns = 40 if cond == "scaffold" else 20
            for scn in scns:
                passes = 0
                for i in range(a.repeats):
                    client = OllamaClient(base_url=a.base_url, num_ctx=a.num_ctx,
                                          temperature=a.temperature, seed=i + 1)
                    traj = _run_one(cond, client, model, scn, tools, max_turns, a.max_reprompts)
                    if judge.score(scn, traj, armed=scn.spec.requires_arm).passed:
                        passes += 1
                summary[model][cond][scn.id] = passes
                print(f"  [{model:<12} {cond:<8} {scn.id:<34}] {passes}/{a.repeats}", flush=True)

    n = a.repeats
    report = {"repeats": n, "temperature": a.temperature, "models": models,
              "conditions": conditions, "n_scenarios": len(scns), "per_scenario": summary,
              "gate_score_expected": {}, "noisy_scenarios": {}}
    for model in models:
        for cond in conditions:
            passes = summary[model][cond]
            # expected gate score = sum of per-scenario pass RATES (fractional 0..16)
            report["gate_score_expected"][f"{model}/{cond}"] = round(sum(passes.values()) / n, 2)
            report["noisy_scenarios"][f"{model}/{cond}"] = sorted(
                sid for sid, p in passes.items() if 0 < p < n)

    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    with open(a.out, "w") as f:
        json.dump(report, f, indent=2)

    print("\n=== expected gate score (sum of per-scenario pass rates, /16) ===")
    for k, v in report["gate_score_expected"].items():
        noisy = len(report["noisy_scenarios"][k])
        print(f"  {k:<26} {v:>5}/16   ({noisy} noisy scenarios)")
    print(f"\nsummary -> {a.out}")


if __name__ == "__main__":
    main()
