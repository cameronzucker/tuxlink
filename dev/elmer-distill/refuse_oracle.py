#!/usr/bin/env python3
"""Content oracle for the re-fuser (tuxlink-pt2xo): the SEMANTIC check the key-gate can't do.

The key-gate proves a checkpoint has the canonical fused LAYOUT. It cannot prove the fused expert
VALUES are correct — a wrong transpose or dropped expert delta passes the gate but corrupts
generations. This oracle closes that gap:

  capture  — load the as-trained model (4-bit base + PEFT adapter, in-process transformers) and
             greedy-decode a fixed prompt set. These completions ARE ground truth.
  verify   — hit a served OpenAI-compatible /v1 endpoint (the GGUF via llama-server) with the same
             prompts, greedy, and diff against the reference. Divergence => the re-fuse is wrong.

Run capture BEFORE any re-export (it needs only base+adapter, never the fused checkpoint). Run
verify after llama-server is up. Prompts include ham-domain items so a dropped expert delta — the
fine-tune lives in the experts — actually shows up (generic prompts under-discriminate).

  python3 refuse_oracle.py capture --model-id unsloth/gpt-oss-120b \
      --adapter /root/elmer-train/adapter-120b --out oracle-reference.json
  python3 refuse_oracle.py verify --ref oracle-reference.json --base-url http://127.0.0.1:8080/v1
"""
import argparse
import difflib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

# A small, fixed, greedy-stable prompt set. Ham-domain items exercise the fine-tuned experts so a
# botched fuse diverges; generic items catch gross corruption. Kept single-turn + tool-free for a
# clean cross-runtime diff (in-process transformers vs llama-server both use the harmony template).
DEFAULT_PROMPTS = [
    "In one sentence, what is the purpose of an antenna tuner?",
    "A station is calling CQ on 14.074 MHz. What mode is most likely in use, and why?",
    "Briefly: what does the Q-code QRM mean?",
    "Explain in two sentences what Winlink is and when an operator would use it.",
    "What is the standard phonetic alphabet word for the letter K?",
    "List three things to check first if an HF transceiver is receiving but not transmitting.",
]


def cmd_capture(a):
    os.environ.setdefault("PYTORCH_CUDA_ALLOC_CONF", "expandable_segments:True")
    prompts = _load_prompts(a.prompts)
    import torch
    from unsloth import FastLanguageModel
    from peft import PeftModel

    print(f"[oracle] loading {a.model_id} (4-bit) + adapter {a.adapter}", flush=True)
    model, tok = FastLanguageModel.from_pretrained(
        model_name=a.model_id, max_seq_length=a.max_seq_length, load_in_4bit=True, dtype=None)
    model = PeftModel.from_pretrained(model, a.adapter)
    FastLanguageModel.for_inference(model)

    records = []
    for i, p in enumerate(prompts):
        ids = tok.apply_chat_template([{"role": "user", "content": p}], add_generation_prompt=True)
        input_ids = torch.tensor([ids], device=model.device)
        out = model.generate(input_ids, attention_mask=torch.ones_like(input_ids),
                             max_new_tokens=a.max_new_tokens, do_sample=False,
                             pad_token_id=tok.pad_token_id or tok.eos_token_id)
        text = tok.decode(out[0][len(ids):], skip_special_tokens=True).strip()
        print(f"[oracle] {i+1}/{len(prompts)} captured ({len(text)} chars)", flush=True)
        records.append({"prompt": p, "completion": text})
    json.dump({"model_id": a.model_id, "adapter": a.adapter, "records": records},
              open(a.out, "w"), indent=2)
    print(f"[oracle] wrote {len(records)} reference completions -> {a.out}")


def cmd_verify(a):
    import requests
    ref = json.load(open(a.ref))
    records = ref["records"]
    worst = 1.0
    rows = []
    for i, r in enumerate(records):
        body = {"model": a.model, "temperature": 0,
                "messages": [{"role": "user", "content": r["prompt"]}],
                "max_tokens": a.max_new_tokens}
        resp = requests.post(f"{a.base_url.rstrip('/')}/chat/completions", json=body, timeout=a.timeout)
        resp.raise_for_status()
        msg = resp.json()["choices"][0]["message"]
        # The captured reference is the raw decode (harmony analysis + final channels). llama.cpp
        # splits them: content=final, reasoning_content=analysis. Reconstruct the same shape so a
        # correct fuse scores high; a broken fuse still diverges visibly.
        content = (msg.get("content") or "").strip()
        reasoning = (msg.get("reasoning_content") or "").strip()
        got = (reasoning + content).strip()
        ratio = max(
            difflib.SequenceMatcher(None, r["completion"], got).ratio(),
            difflib.SequenceMatcher(None, r["completion"], content).ratio(),
        )
        worst = min(worst, ratio)
        status = "OK " if ratio >= a.threshold else "DIFF"
        rows.append((status, ratio, r["prompt"], r["completion"], got))
        print(f"[verify] {status} sim={ratio:.2f}  {r['prompt'][:60]}")
    print("\n=== oracle verify (full pairs for the eyeball read) ===")
    for s, ratio, prompt, want, got in rows:
        print(f"\n  [{s} sim={ratio:.2f}] {prompt}")
        print(f"    as-trained: {want[:320]}")
        print(f"    served    : {got[:320]}")
    n_pass = sum(1 for s, *_ in rows if s == "OK ")
    print(f"\n  {n_pass}/{len(rows)} prompts within similarity threshold {a.threshold}; worst={worst:.2f}")
    if n_pass < len(rows):
        print("  DIVERGENCE — the served checkpoint may not reproduce the as-trained model.")
        print("  Likely causes: wrong expert transpose, dropped expert deltas, or a bad convert.")
        print("  Read the pairs above: greedy across two runtimes is close but not identical, so a")
        print("  low ratio with clearly-equivalent answers is fine; garbage/off-topic is not.")
        return 1
    print("  MATCH — the re-fuse preserved the fine-tuned behavior. Checkpoint is trustworthy.")
    return 0


def _load_prompts(path):
    if not path:
        return DEFAULT_PROMPTS
    if path.endswith(".jsonl"):
        return [json.loads(line)["prompt"] for line in open(path) if line.strip()]
    return json.load(open(path))


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    c = sub.add_parser("capture", help="reference completions from the as-trained model")
    c.add_argument("--model-id", default="unsloth/gpt-oss-120b")
    c.add_argument("--adapter", required=True)
    c.add_argument("--out", default="oracle-reference.json")
    c.add_argument("--prompts", default="", help="optional .jsonl (prompt per line) or .json list")
    c.add_argument("--max-seq-length", type=int, default=4096)
    c.add_argument("--max-new-tokens", type=int, default=256)
    c.set_defaults(func=cmd_capture)

    v = sub.add_parser("verify", help="diff a served /v1 endpoint against the reference")
    v.add_argument("--ref", default="oracle-reference.json")
    v.add_argument("--base-url", default="http://127.0.0.1:8080/v1")
    v.add_argument("--model", default="elmer-120b")
    v.add_argument("--threshold", type=float, default=0.50,
                   help="min char-similarity per prompt (greedy across two runtimes is close, not identical; "
                        "the printed full pairs are the real arbiter for a small prompt set)")
    v.add_argument("--max-new-tokens", type=int, default=256)
    v.add_argument("--timeout", type=float, default=120)
    v.set_defaults(func=cmd_verify)

    a = ap.parse_args()
    rc = a.func(a)
    raise SystemExit(rc or 0)


if __name__ == "__main__":
    main()
