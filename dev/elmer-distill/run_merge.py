#!/usr/bin/env python3
"""A1 merge: unsloth `save_pretrained_merged` — the free first attempt at a servable fused
checkpoint (tuxlink-pt2xo). unsloth's saver is documented to re-fuse gpt-oss experts, but is
known-broken for dense per-expert LoRA (#3701 -> HOLLOW). So this is speculative: always run
`run_gate.py <out>` on the result. If it hollows or emits the per-expert layout, the A3 stacking
re-fuser (`src/elmer_distill/refuse.py`) fuses the per-expert bf16 output instead.

  python3 run_merge.py --model-id unsloth/gpt-oss-120b \
      --adapter /root/elmer-train/adapter-120b --out /workspace/merged-a1 --method merged_16bit

`merged_16bit` dequantizes the 4-bit base + bakes the deltas to bf16 (what GGUF/vLLM need); do NOT
use `mxfp4` on a dequantized merge ("No MXFP4 tensors found" — llama.cpp #15146 / unsloth #3817).
"""
import argparse


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model-id", default="unsloth/gpt-oss-120b")
    ap.add_argument("--adapter", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--method", default="merged_16bit", help="merged_16bit (bf16) | mxfp4 (broken here)")
    ap.add_argument("--max-seq-length", type=int, default=4096)
    a = ap.parse_args()

    from unsloth import FastLanguageModel
    from peft import PeftModel

    print(f"[merge] loading {a.model_id} (4-bit) + adapter {a.adapter}", flush=True)
    model, tok = FastLanguageModel.from_pretrained(
        model_name=a.model_id, max_seq_length=a.max_seq_length, load_in_4bit=True, dtype=None)
    model = PeftModel.from_pretrained(model, a.adapter)
    print(f"[merge] save_pretrained_merged(save_method={a.method}) -> {a.out}", flush=True)
    model.save_pretrained_merged(a.out, tok, save_method=a.method)
    print("[merge] A1 MERGE DONE", flush=True)


if __name__ == "__main__":
    main()
