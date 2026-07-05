#!/usr/bin/env python3
"""CLI: mechanical key-gate a gpt-oss checkpoint BEFORE feeding it to a server (tuxlink-pt2xo).

    python3 run_gate.py /root/elmer-merged/            # dir with index.json or *.safetensors
    python3 run_gate.py /root/elmer-merged/model.safetensors.index.json

Exit 0 = canonical FUSED layout (servable). Exit 1 = FAIL (unfused / hollow / residual-quant),
so it composes as a pipeline guard:

    python3 run_gate.py "$MERGED" && python3 "$CONVERTER" --outfile elmer.gguf "$MERGED"

This runs GPU-free (stdlib only) — always run it on the pod after any merge/re-export, and
locally on any checkpoint pulled down. A wrong layout does NOT error at serve time; vLLM
silently corrupts. This gate is the cheap tripwire.
"""
import argparse
import sys

# top-level script: make the src/ package importable the same way the other run_*.py do
sys.path.insert(0, __file__.rsplit("/", 1)[0] + "/src")
from elmer_distill.key_gate import gate_checkpoint  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("checkpoint", help="directory, *.index.json, or *.safetensors")
    a = ap.parse_args()
    result = gate_checkpoint(a.checkpoint)
    print(result.summary())
    return 0 if result.passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
