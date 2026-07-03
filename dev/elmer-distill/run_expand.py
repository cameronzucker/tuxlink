#!/usr/bin/env python3
"""Generate the training GENERATOR pool + surface-expand its prompts.

    python3 run_expand.py --n-per-cell 6 --model gpt-oss:120b

scenariogen emits placeholder prompts; this rewrites each into a natural operator
request eliciting the same task-graph (few-shot from the gate prompts). Saves a
pool of Scenario JSONs to eval-runs/generator-expanded/ — this is the input to
gold-gen (run_council --candidates <that dir>). The task-graph spec is preserved
verbatim; only the surface prompt changes.

NOTE: this is the TRAINING pool (distinct from the frozen GATE). Gold-gen over it
produces the training data; the contamination guard keeps the gate out.
"""
import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from elmer_distill import scenariogen                 # noqa: E402
from elmer_distill.expand import load_exemplars, expand  # noqa: E402
from elmer_distill.ollama_client import OllamaClient   # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n-per-cell", type=int, default=6)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--model", default="gpt-oss:120b", help="prompt-author model")
    ap.add_argument("--base-url", default="http://127.0.0.1:11434")
    ap.add_argument("--candidates", default=os.path.join(HERE, "gate", "candidates"))
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs", "generator-expanded"))
    ap.add_argument("--temperature", type=float, default=0.8)
    a = ap.parse_args()

    scns = scenariogen.generate(seed=a.seed, n_per_cell=a.n_per_cell)
    exemplars = load_exemplars(a.candidates)
    client = OllamaClient(base_url=a.base_url, temperature=a.temperature)
    os.makedirs(a.out, exist_ok=True)
    print(f"[expand] {len(scns)} generator scenarios · author={a.model}", flush=True)

    for i, s in enumerate(scns):
        try:
            ex = expand(client, a.model, s, exemplars, temperature=a.temperature)
        except Exception as e:
            print(f"  [{i+1}/{len(scns)}] {s.id}  EXPAND-FAIL {e}", flush=True)
            continue
        with open(os.path.join(a.out, f"{s.id}.json"), "w") as f:
            json.dump(ex.to_json(), f, indent=2)
        print(f"  [{i+1}/{len(scns)}] {s.id}: {ex.prompt[:90]!r}", flush=True)

    print(f"\n  expanded pool -> {a.out}/  ({len(os.listdir(a.out))} scenarios)")
    print("  next: gold-gen ->  python3 run_council.py --models gpt-oss:120b --n 5 "
          f"--candidates {a.out} --out eval-runs/gold-train")


if __name__ == "__main__":
    main()
