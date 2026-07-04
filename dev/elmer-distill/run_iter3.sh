#!/usr/bin/env bash
# run_iter3.sh — drive the full iter-3 restraint-rebalance pipeline on the pod,
# detached + resumable (skips any step whose output already exists, so a mid-run
# disconnect on the paid H200 does not force a restart). tuxlink-grg1i.
#
#   cd /root/elmer-distill && nohup bash run_iter3.sh > /root/iter3.log 2>&1 &
#
# Requires: ollama serving gpt-oss:120b (teacher) + gpt-oss:20b (student) — from
# pod_bootstrap.sh. The train stack (unsloth + torch cu128) is installed in step 0.
set -euo pipefail
cd "$(dirname "$0")"
echo "############ iter-3 pipeline $(date -u) ############"

say() { printf '\n=== %s ===\n' "$1"; }

# 0. Train stack. Install unsloth first (it resolves its CUDA torch), THEN force the
#    cu128 torch build so a cpu-torch never sneaks in (tuxlink-xutv1 pollution class).
say "0 train stack"
if ! python3 -c "import unsloth" 2>/dev/null; then
  pip install -q "unsloth>=2026.6" "unsloth_zoo>=2026.6" "transformers>=4.56.1,<5" \
                 "peft>=0.13" "trl>=0.20" "datasets>=2.20" "accelerate>=0.34" "bitsandbytes>=0.43"
fi
pip install -q --index-url https://download.pytorch.org/whl/cu128 torch==2.10.0 torchvision==0.25.0
python3 -c "import torch; assert torch.cuda.is_available(), 'CUDA torch missing (pollution)'; print('[torch]', torch.__version__)"

# 1. Gold-gen — restraint-rebalanced bank, best-of-2 (iter-2 parity). Hard >=118 guard.
say "1 gold-gen"
if [[ -f eval-runs/gold-v3/report.json ]]; then
  echo "gold-v3 exists — skipping"
else
  python3 run_gold.py --model gpt-oss:120b --n 2 --out eval-runs/gold-v3
fi
python3 -c "import json; r=json.load(open('eval-runs/gold-v3/report.json')); \
print(f\"[gold] {r['gold']} gold  restraint {r['gold_restraint']} ({r['gold_restraint_frac']:.0%}: refusal {r['gold_egress_refusal']} + honesty {r['gold_taint_honesty']})  yield {r['yield_rate']:.0%}\")"

# 2. Assemble Harmony JSONL.
say "2 assemble"
if [[ -f eval-runs/train-v3.jsonl ]]; then echo "train-v3.jsonl exists — skipping"; else
  python3 run_assemble.py --gold eval-runs/gold-v3/gold --out eval-runs/train-v3.jsonl
fi

# 3. Base eval on THIS pod (parity — expect ~4/16).
say "3 base eval"
if [[ -f eval-runs/base-20b-v3/results.json ]]; then echo "base eval exists — skipping"; else
  python3 run_eval.py --model gpt-oss:20b --label base-20b-v3
fi

# 4. Train — 4bit QLoRA, r32 (v1 recipe), 3 epochs. bf16 is BLOCKED (tuxlink-5tfkm).
say "4 train"
if [[ -f /root/elmer-train/adapter-v3/adapter_config.json ]]; then echo "adapter-v3 exists — skipping"; else
  python3 run_train.py --data eval-runs/train-v3.jsonl --out /root/elmer-train/adapter-v3 \
                       --precision 4bit --r 32 --epochs 3
fi

# 5. Serve (bf16-merge -> GGUF -> ollama). Pollutes torch, but training is done.
say "5 serve"
if ollama list | grep -q 'elmer-20b-v3'; then echo "elmer-20b-v3 served — skipping"; else
  python3 run_serve.py --adapter /root/elmer-train/adapter-v3 --tag elmer-20b-v3
fi

# 6. Student eval.
say "6 student eval"
python3 run_eval.py --model elmer-20b-v3 --label elmer-20b-v3

say "DONE $(date -u)"
python3 - <<'PY'
import json
for lbl in ("base-20b-v3", "elmer-20b-v3"):
    r = json.load(open(f"eval-runs/{lbl}/results.json"))
    print(f"  {lbl:16} {r['passed']}/{r['n']}  "
          f"(agent {r['gate_agent_passed']}/{r['gate_agent_total']}, "
          f"probe {r['probe_operator_passed']}/{r['probe_operator_total']})")
PY
