#!/usr/bin/env python3
"""CLI: run one model over the frozen gate bank, persist transcripts + results.

    python3 run_eval.py --model gpt-oss:20b --label base-20b
    python3 run_eval.py --model elmer-lora-phaseA --label lora-phaseA

Writes eval-runs/<label>/<scenario>.json (full transcripts) + results.json.
Empirical gate pass/fail prints as a table; the 7 operator_authored rows are the
held-out probe (their transcripts feed probe.render_cards for the hand read).

On the pod, ollama serves gpt-oss:20b at 127.0.0.1:11434 (the default). The
teacher (gpt-oss:120b) uses the same CLI with --model gpt-oss:120b for the
calibration base-vs-teacher comparison.
"""
import argparse
import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from elmer_distill.scenario import Scenario          # noqa: E402
from elmer_distill.ollama_client import OllamaClient  # noqa: E402
from elmer_distill.eval_run import evaluate           # noqa: E402
from elmer_distill.surface import SYSTEM_PROMPT, load_tools  # noqa: E402

TOOLS = load_tools()


def load_bank(candidates_dir):
    scns = []
    for p in sorted(glob.glob(os.path.join(candidates_dir, "*.json"))):
        with open(p) as f:
            scns.append(Scenario.from_json(json.load(f)))
    return scns


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="gpt-oss:20b")
    ap.add_argument("--label", required=True)
    ap.add_argument("--base-url", default="http://127.0.0.1:11434")
    ap.add_argument("--candidates", default=os.path.join(HERE, "gate", "candidates"))
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs"))
    ap.add_argument("--num-ctx", type=int, default=32768)
    ap.add_argument("--max-turns", type=int, default=20)
    a = ap.parse_args()

    scns = load_bank(a.candidates)
    client = OllamaClient(base_url=a.base_url, num_ctx=a.num_ctx)
    print(f"[run_eval] {len(scns)} scenarios · model={a.model} · label={a.label} · {a.base_url}")
    summ = evaluate(client, a.model, scns, SYSTEM_PROMPT, TOOLS, a.out, a.label,
                    max_turns=a.max_turns)

    print("\n  pass  scenario                              family      op?")
    print("  ----  ------------------------------------  ----------  ---")
    for r in summ.results:
        mark = " ✓ " if r["passed"] else " ✗ "
        op = "OP " if r["operator_authored"] else "   "
        print(f"  [{mark}] {r['id']:<36}  {r['family']:<10}  {op}")
    print(f"\n  GATE (agent-authored): {summ.gate_agent_passed}/{summ.gate_agent_total} pass")
    print(f"  PROBE (operator held-out): {summ.probe_operator_passed}/{summ.probe_operator_total} pass")
    print(f"  results.json -> {os.path.join(a.out, a.label, 'results.json')}")


if __name__ == "__main__":
    main()
