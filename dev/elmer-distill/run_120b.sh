#!/usr/bin/env bash
# run_120b.sh — the 120b COLD-TRANSFER build, detached + resumable (skips any step whose
# output already exists, so a mid-run disconnect on the paid H200 does not force a restart).
# tuxlink-48nyh. Sibling of run_iter3.sh (the 20b iter-3 run); this one perfects the 120b.
#
#   cd /root/elmer-distill && nohup bash run_120b.sh > /root/120b.log 2>&1 &
#
# Requires (from pod_bootstrap.sh --models gpt-oss:120b,gpt-oss:20b): ollama serving BOTH
# gpt-oss:120b (quality teacher) AND gpt-oss:20b (restraint teacher — the 120b fails taint 1/5,
# so restraint gold is borrowed from the 20b). The train stack (unsloth + cu128 torch) installs
# in step 0. 4-bit is MANDATORY: bf16 is blocked for gpt-oss expert-LoRA (tuxlink-5tfkm) and a
# bf16 120b (~240GB) won't fit one H200 anyway.
set -euo pipefail
cd "$(dirname "$0")"
echo "############ 120b cold-transfer build $(date -u) ############"
say() { printf '\n=== %s ===\n' "$1"; }

# 0. Train stack (identical to iter-3: unsloth resolves CUDA torch, then FORCE cu128 so a
#    cpu-torch never sneaks in — tuxlink-xutv1 pollution class).
say "0 train stack"
# Fresh RunPod base images (Ubuntu 24.04) mark the system Python externally-managed (PEP 668);
# pip SILENTLY refuses to install without this, leaving unsloth/bitsandbytes absent (verified on
# the RTX PRO 6000 pod 2026-07-04). Ephemeral container -> safe to override.
export PIP_BREAK_SYSTEM_PACKAGES=1
# Accelerator guard: this stack is CUDA-only. On an AMD/ROCm box (MI300X) the cu128 wheel is
# wrong and unsloth/bitsandbytes need ROCm builds — run the go/no-go probe first, do NOT bet a
# full build on an unverified ROCm 4-bit path (tuxlink-5tfkm class).
if command -v rocminfo >/dev/null 2>&1 || [[ -e /dev/kfd ]]; then
  echo "ROCm/AMD detected — the CUDA train stack does NOT apply here."
  echo "Run:  python3 smoke/rocm_probe.py   (go/no-go for bitsandbytes-4bit MXFP4 on ROCm)."
  echo "If it PASSES, install the ROCm stack from its header, then re-run this script."
  exit 2
fi
if ! python3 -c "import unsloth" 2>/dev/null; then
  # unsloth resolves the matching torch (2.10.0+cu128, Blackwell-capable) itself — verified on
  # the RTX PRO 6000 (sm_120) 2026-07-04, so do NOT pin torch/torchvision versions here.
  pip install -q "unsloth>=2026.6" "unsloth_zoo>=2026.6" "transformers>=4.56.1,<5" \
                 "peft>=0.13" "trl>=0.20" "datasets>=2.20" "accelerate>=0.34" "bitsandbytes>=0.43"
fi
# Only force a cu128 torch if the install left a NON-CUDA torch (the cpu-torch pollution class);
# don't fight a working CUDA install with exact-version pins.
python3 -c "import torch,sys; sys.exit(0 if torch.cuda.is_available() else 1)" || \
  pip install -q --index-url https://download.pytorch.org/whl/cu128 torch torchvision
python3 -c "import torch; assert torch.cuda.is_available(), 'CUDA torch missing (pollution)'; print('[torch]', torch.__version__)"

# 1. Re-baseline on the REPAIRED gate FIRST (n>=5). The gate changed (schedule false-positive
#    killed); the old 13.2/13.8 numbers are stale. Do NOT train against stale numbers.
say "1 re-baseline (repaired gate, n=5)"
if [[ -f prereg/rebaseline-120b.json ]]; then echo "rebaseline exists — skipping"; else
  python3 run_rebaseline.py --repeats 5 --out prereg/rebaseline-120b.json
fi

# 2. Gold-gen — 120b self-generated grounded-QUALITY gold on the normal (even) bank, tainted
#    cells DROPPED (injection-refusal is punted to the tool-layer guard + operator, not trained;
#    operator decision 2026-07-04). Naturalistic prompts (expand). This run is pure cold-transfer
#    of grounded tool-use quality — no restraint borrow, no restraint oversampling.
say "2 gold-gen (120b quality, normal bank, drop-taint, expand)"
if [[ -f eval-runs/gold-120b/report.json ]]; then echo "gold-120b exists — skipping"; else
  python3 run_gold.py --model gpt-oss:120b --bank normal --n-per-cell 6 --drop-taint \
                      --expand-prompts --n 2 --min-volume 40 --out eval-runs/gold-120b
fi
python3 -c "import json; r=json.load(open('eval-runs/gold-120b/report.json')); \
print(f\"[gold] {r['gold']} gold  yield {r['yield_rate']:.0%}  expanded={r.get('expanded_prompts')}  \
families={r.get('family_counts')}\")"

# 3. Assemble Harmony JSONL (loss-masked to assistant spans; contamination-guarded).
say "3 assemble"
if [[ -f eval-runs/train-120b.jsonl ]]; then echo "train-120b.jsonl exists — skipping"; else
  python3 run_assemble.py --gold eval-runs/gold-120b/gold --out eval-runs/train-120b.jsonl
fi

# 3.5 Release ollama VRAM before training. ollama holds the 120b (~63GB) resident from
#     rebaseline/gold; training needs ~90GB, so 63+90 > 96GB would OOM on the RTX PRO 6000.
#     Steps 4-5 don't use ollama (train=unsloth, peft_eval=in-process transformers).
say "3.5 release ollama VRAM"
ollama stop gpt-oss:120b 2>/dev/null || true
ollama stop gpt-oss:20b 2>/dev/null || true
curl -s http://127.0.0.1:11434/api/chat -d '{"model":"gpt-oss:120b","keep_alive":0,"messages":[]}' >/dev/null 2>&1 || true
curl -s http://127.0.0.1:11434/api/chat -d '{"model":"gpt-oss:20b","keep_alive":0,"messages":[]}' >/dev/null 2>&1 || true
for _ in 1 2 3 4 5 6; do
  used=$(nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits | head -1)
  echo "  VRAM used=${used}MiB"; [ "${used:-99999}" -lt 8000 ] && break; sleep 5
done

# 4. Train — 120b, 4-bit QLoRA, attn+all-experts LoRA (router frozen), 3 epochs.
#    r16 + the default paged_adamw_8bit optimizer fit the 120b on a 96GB card (RTX PRO 6000);
#    on an H200/144GB you may bump --r 32 for a touch more adapter capacity.
say "4 train (120b 4-bit r16, paged 8-bit optim)"
if [[ -f /root/elmer-train/adapter-120b/adapter_config.json ]]; then echo "adapter-120b exists — skipping"; else
  python3 run_train.py --model-id unsloth/gpt-oss-120b --data eval-runs/train-120b.jsonl \
                       --out /root/elmer-train/adapter-120b --precision 4bit --r 16 --epochs 3
fi

# 5. Acceptance — in-process peft eval (base 4-bit + adapter), no bf16 merge. SMOKE (--limit 1)
#    gates the render/generate/parse glue before the full gate (like micro_lora_smoke gates train).
say "5 acceptance eval (peft, cold)"
if [[ ! -f eval-runs/peft/elmer-120b-smoke/results.json ]]; then
  python3 peft_eval.py --model-id unsloth/gpt-oss-120b --adapter /root/elmer-train/adapter-120b \
                       --label elmer-120b-smoke --limit 1
fi
[[ -f eval-runs/peft/elmer-120b-smoke/results.json ]] || { echo "SMOKE FAILED: peft glue did not produce results — fix before the full gate"; exit 1; }
if [[ -f eval-runs/peft/elmer-120b/results.json ]]; then echo "full peft eval exists — skipping"; else
  python3 peft_eval.py --model-id unsloth/gpt-oss-120b --adapter /root/elmer-train/adapter-120b \
                       --label elmer-120b
fi

say "DONE $(date -u)"
python3 - <<'PY'
import json, os
base = json.load(open("prereg/rebaseline-120b.json"))
print("  cold vs scaffold (repaired gate):", base.get("gate_score_expected", {}))
p = "eval-runs/peft/elmer-120b/results.json"
if os.path.exists(p):
    r = json.load(open(p))
    print(f"  elmer-120b (post-train, COLD): {r['passed']}/{r['n']}  "
          f"(agent {r['gate_agent_passed']}/{r['gate_agent_total']}, "
          f"probe {r['probe_operator_passed']}/{r['probe_operator_total']})")
    print("  next: run_quality_eval (parity_artifact should shrink vs the pre-train read)")
PY
