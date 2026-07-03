#!/usr/bin/env python3
"""Run a model over the gate WITH the G0 checklist scaffold (baseline_g0.run_g0).

    python3 run_scaffold.py --model gpt-oss:120b --label teacher-scaffold

The scaffold injects a per-scenario checklist of required tools (+ a verifier
reprompt loop). This measures GOLD YIELD: can the teacher produce a judge-passing
trajectory when told which tools the task needs? A big jump over the raw run means
the too_hard scenarios are achievable (raw zero-shot was the weak link, not the
scenarios); a flat result means the tasks are genuinely beyond the teacher.
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
from elmer_distill.eval_run import evaluate            # noqa: E402
from elmer_distill.baseline_g0 import run_g0           # noqa: E402
from elmer_distill.surface import SYSTEM_PROMPT, load_tools  # noqa: E402


def _scaffold_runner(max_reprompts):
    # adapt run_g0 to evaluate()'s runner(client, model, s, system, tools, max_turns)
    def runner(client, model, s, system, tools, max_turns):
        return run_g0(client, model, s, system, tools, exemplars=[],
                      max_reprompts=max_reprompts, max_turns=max_turns)
    return runner


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="gpt-oss:120b")
    ap.add_argument("--label", required=True)
    ap.add_argument("--base-url", default="http://127.0.0.1:11434")
    ap.add_argument("--candidates", default=os.path.join(HERE, "gate", "candidates"))
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs"))
    ap.add_argument("--num-ctx", type=int, default=32768)
    ap.add_argument("--max-turns", type=int, default=40)
    ap.add_argument("--max-reprompts", type=int, default=2)
    a = ap.parse_args()

    scns = [Scenario.from_json(json.load(open(p)))
            for p in sorted(glob.glob(os.path.join(a.candidates, "*.json")))]
    client = OllamaClient(base_url=a.base_url, num_ctx=a.num_ctx)
    print(f"[run_scaffold] {len(scns)} scenarios · {a.model} · checklist+{a.max_reprompts} reprompts · label={a.label}")
    summ = evaluate(client, a.model, scns, SYSTEM_PROMPT, load_tools(), a.out, a.label,
                    max_turns=a.max_turns, runner=_scaffold_runner(a.max_reprompts))

    for r in summ.results:
        print(f"  [{' ✓ ' if r['passed'] else ' ✗ '}] {r['id']:<36} {'OP' if r['operator_authored'] else '  '}")
    print(f"\n  SCAFFOLDED {a.model}: {summ.passed}/{summ.n} pass "
          f"(gate {summ.gate_agent_passed}/{summ.gate_agent_total}, probe {summ.probe_operator_passed}/{summ.probe_operator_total})")


if __name__ == "__main__":
    main()
