#!/usr/bin/env python3
"""Quant sweep — serve the tuned model at several quantizations, run the FROZEN GATE
on each, report the quant/quality curve + the distribution recommendation. POD-only.

    python3 run_quant_sweep.py --bf16-gguf /root/elmer-serve/elmer-20b.gguf \
        --quants Q4_K_M,Q5_K_M,Q8_0 --label-prefix elmer-20b

Answers the distribution question empirically ("does Q4 cost us gate scenarios?").
gpt-oss is MXFP4-native, so ~4-bit is near its home operating point — but the gate is
deterministic, so we MEASURE per-quant pass rate and ship the smallest quant that
holds the best score (see elmer_distill.quant_sweep.recommend).

Quantization uses ollama's built-in quantizer (`ollama create <tag> -q <quant>` from a
Modelfile pointing FROM the full-precision GGUF that run_serve produced) — no llama.cpp
build required. Run AFTER run_serve, on a VETTED model (one that actually beat base on
the gate); it is a distribution-prep step, not part of every training loop.
"""
import argparse
import glob
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from elmer_distill.scenario import Scenario              # noqa: E402
from elmer_distill.ollama_client import OllamaClient      # noqa: E402
from elmer_distill.eval_run import evaluate               # noqa: E402
from elmer_distill.surface import SYSTEM_PROMPT, load_tools  # noqa: E402
from elmer_distill.quant_sweep import sweep_report, recommend  # noqa: E402


def load_bank(candidates_dir):
    scns = []
    for p in sorted(glob.glob(os.path.join(candidates_dir, "*.json"))):
        with open(p) as f:
            scns.append(Scenario.from_json(json.load(f)))
    return scns


def _ollama_size_bytes(tag):
    """Best-effort model size from `ollama list` (e.g. '12 GB' -> bytes). 0 on miss."""
    try:
        out = subprocess.run(["ollama", "list"], capture_output=True, text=True).stdout
    except Exception:
        return 0
    mult = {"KB": 1e3, "MB": 1e6, "GB": 1e9, "TB": 1e12}
    for line in out.splitlines():
        parts = line.split()
        if parts and (parts[0] == tag or parts[0].startswith(tag + ":")):
            for i, tok in enumerate(parts):
                if tok in mult and i > 0:
                    try:
                        return int(float(parts[i - 1]) * mult[tok])
                    except ValueError:
                        return 0
    return 0


def _base_gate(out_dir, base_label):
    p = os.path.join(out_dir, base_label, "results.json")
    if os.path.exists(p):
        return json.load(open(p)).get("gate_agent_passed")
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--bf16-gguf", required=True, help="full-precision GGUF from run_serve")
    ap.add_argument("--quants", default="Q4_K_M,Q5_K_M,Q8_0")
    ap.add_argument("--label-prefix", default="elmer-20b")
    ap.add_argument("--candidates", default=os.path.join(HERE, "gate", "candidates"))
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs"))
    ap.add_argument("--workdir", default="/root/elmer-quant")
    ap.add_argument("--base-url", default="http://127.0.0.1:11434")
    ap.add_argument("--num-ctx", type=int, default=32768)
    ap.add_argument("--max-turns", type=int, default=20)
    ap.add_argument("--base-label", default="base-20b", help="eval label whose gate score is the Δ baseline")
    a = ap.parse_args()

    if not os.path.exists(a.bf16_gguf):
        sys.exit(f"[quant_sweep] source GGUF not found: {a.bf16_gguf} (run run_serve first)")
    os.makedirs(a.workdir, exist_ok=True)
    quants = [q.strip() for q in a.quants.split(",") if q.strip()]
    scns = load_bank(a.candidates)
    tools = load_tools()
    client = OllamaClient(base_url=a.base_url, num_ctx=a.num_ctx)
    base_gate = _base_gate(a.out, a.base_label)

    mf = os.path.join(a.workdir, "Modelfile")
    with open(mf, "w") as f:
        f.write(f"FROM {a.bf16_gguf}\n")

    print(f"[quant_sweep] {len(scns)} gate scenarios x {len(quants)} quants "
          f"(base gate={base_gate}) from {a.bf16_gguf}", flush=True)
    rows = []
    for quant in quants:
        tag = f"{a.label_prefix}-{quant.lower().replace('_', '')}"
        print(f"\n[quant_sweep] === {quant} -> ollama '{tag}' ===", flush=True)
        r = subprocess.run(["ollama", "create", tag, "-f", mf, "-q", quant],
                           capture_output=True, text=True)
        if r.returncode != 0:
            print(f"[quant_sweep] ollama create -q {quant} FAILED rc={r.returncode}\n{r.stderr[-800:]}",
                  flush=True)
            rows.append({"quant": quant, "ok": False})
            continue
        summ = evaluate(client, tag, scns, SYSTEM_PROMPT, tools, a.out,
                        f"{a.label_prefix}-{quant}", max_turns=a.max_turns)
        rows.append({
            "quant": quant, "ok": True,
            "gate_passed": summ.gate_agent_passed, "gate_total": summ.gate_agent_total,
            "probe_passed": summ.probe_operator_passed, "probe_total": summ.probe_operator_total,
            "size_bytes": _ollama_size_bytes(tag),
        })
        print(f"[quant_sweep] {quant}: gate {summ.gate_agent_passed}/{summ.gate_agent_total} "
              f"probe {summ.probe_operator_passed}/{summ.probe_operator_total}", flush=True)

    report = sweep_report(rows, base_gate=base_gate)
    print("\n" + report, flush=True)
    with open(os.path.join(a.out, "quant_sweep.json"), "w") as f:
        json.dump({"base_gate": base_gate, "rows": rows, "recommend": recommend(rows)}, f, indent=2)
    print(f"\n[quant_sweep] -> {os.path.join(a.out, 'quant_sweep.json')}", flush=True)


if __name__ == "__main__":
    main()
