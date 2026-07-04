#!/usr/bin/env python3
"""In-process acceptance eval of a LoRA-tuned gpt-oss (4-bit base + PEFT adapter) — POD.

Loads the 4-bit base (--model-id) + trained adapter (--adapter) and drives the frozen gate
COLD (no scaffold — cold-transfer is the 120b build's whole goal) through the same agentic loop
+ Judge as run_eval, writing results.json. This AVOIDS the bf16 merge wall (run_serve) that a
120b (~240GB bf16) cannot fit on one H200: it reuses the exact 4-bit base+adapter layout the
training smoke already de-risks, minus backprop. Fits one H200; also runs on any >=80GB box
(incl. a Spark-class 128GB unified box) for the eventual trickle-down role.

  # SMOKE FIRST (validate the render/generate/parse glue on ONE scenario — this glue is
  # GPU/vocab-dependent and cannot run on the dev Pi; treat like smoke/micro_lora_smoke.py):
  python3 peft_eval.py --model-id unsloth/gpt-oss-120b --adapter /root/elmer-train/adapter-120b \
      --label elmer-120b-smoke --limit 1
  # FULL gate:
  python3 peft_eval.py --model-id unsloth/gpt-oss-120b --adapter /root/elmer-train/adapter-120b \
      --label elmer-120b

Input rendering uses the tokenizer's Harmony chat template (handles tool schemas); output is
parsed with the openai_harmony encoding (hf_client.PeftHFClient) — the documented gpt-oss recipe.
Compare the result to the COLD base-120b re-baseline: cold-transfer should lift the cold score
toward the scaffolded ~13/16, and run_quality_eval's parity_artifact cell should shrink.
"""
import argparse
import glob
import json
import os
import sys

# Reduce allocator fragmentation on the 96GB card (the KV cache grows across the multi-turn loop);
# must be set before torch is imported (lazily, in main()).
os.environ.setdefault("PYTORCH_CUDA_ALLOC_CONF", "expandable_segments:True")

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "src"))

from elmer_distill.scenario import Scenario                 # noqa: E402
from elmer_distill.hf_client import PeftHFClient, tools_for_gpt_oss_template  # noqa: E402
from elmer_distill.surface import SYSTEM_PROMPT, load_tools  # noqa: E402
from elmer_distill.eval_run import evaluate                 # noqa: E402
# harmony (openai_harmony) + torch + unsloth are imported lazily in main() so this CLI stays
# import-safe on a host without the GPU/harmony stack (the dev Pi); they exist on the pod.


def _load(model_id, adapter, max_seq):
    from unsloth import FastLanguageModel
    from peft import PeftModel
    model, tokenizer = FastLanguageModel.from_pretrained(
        model_name=model_id, max_seq_length=max_seq, load_in_4bit=True, dtype=None)
    model = PeftModel.from_pretrained(model, adapter)        # base 4-bit + trained adapter
    FastLanguageModel.for_inference(model)
    return model, tokenizer


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model-id", default="unsloth/gpt-oss-120b")
    ap.add_argument("--adapter", required=True, help="trained PEFT adapter dir (run_train --out)")
    ap.add_argument("--label", required=True)
    ap.add_argument("--out", default=os.path.join(HERE, "eval-runs", "peft"))
    ap.add_argument("--candidates", default=os.path.join(HERE, "gate", "candidates"))
    ap.add_argument("--max-seq-length", type=int, default=4096)
    ap.add_argument("--max-new-tokens", type=int, default=2048)
    ap.add_argument("--max-turns", type=int, default=20)
    ap.add_argument("--temperature", type=float, default=0.0)   # greedy: the canonical cold answer
    ap.add_argument("--limit", type=int, default=0, help="smoke: cap the gate at N scenarios (0=all)")
    a = ap.parse_args()

    scns = [Scenario.from_json(json.load(open(p)))
            for p in sorted(glob.glob(os.path.join(a.candidates, "*.json")))]
    if a.limit:
        scns = scns[:a.limit]
    print(f"[peft-eval] {a.model_id} + {a.adapter} on {len(scns)} scenarios (cold)", flush=True)

    import torch
    from openai_harmony import Role
    from elmer_distill.harmony import _enc                    # lazy: needs openai_harmony (pod)
    model, tokenizer = _load(a.model_id, a.adapter, a.max_seq_length)
    enc = _enc()

    def build_prompt(messages, tools):
        # tokenizer's Harmony chat template injects the tool schemas the model was trained on;
        # it wants the function objects (not the OpenAI wrapper) — see tools_for_gpt_oss_template
        return tokenizer.apply_chat_template(
            messages, tools=tools_for_gpt_oss_template(tools), add_generation_prompt=True)

    # Stop at end-of-turn: <|return|> (final) OR <|call|> (tool call). tok.eos_token_id is ONLY
    # <|return|>, so a tool-calling turn otherwise runs to max_new_tokens of garbage and OOMs the
    # KV cache across the multi-turn loop (pod bring-up 2026-07-04).
    stop_ids = enc.stop_tokens_for_assistant_actions()   # [200002, 200012]
    pad_id = tokenizer.pad_token_id if tokenizer.pad_token_id is not None else stop_ids[0]

    def generate_fn(prompt_ids):
        input_ids = torch.tensor([prompt_ids], device=model.device)
        attn = torch.ones_like(input_ids)                # silence the missing-attention-mask warning
        out = model.generate(input_ids, attention_mask=attn, max_new_tokens=a.max_new_tokens,
                             do_sample=a.temperature > 0, temperature=max(a.temperature, 1e-5),
                             eos_token_id=stop_ids, pad_token_id=pad_id)
        return out[0][len(prompt_ids):].tolist()             # completion ids only

    client = PeftHFClient(enc, build_prompt, generate_fn, Role.ASSISTANT)
    summ = evaluate(client, a.label, scns, SYSTEM_PROMPT, load_tools(),
                    a.out, a.label, max_turns=a.max_turns)
    print(f"\n=== {a.label} (COLD) — SCORED gate: {summ.scored_passed}/{summ.scored_total} "
          f"(injection-refusal cells excluded from grade) ===")
    print(f"    observed (measured, not graded): injection-refusal {summ.unscored_passed}/{summ.unscored_total}")
    print(f"    honesty measurement — false 'sent' claims: {summ.false_sent_claims}/{summ.n} "
          f"({summ.unscored_false_sent} on injection cells, where an injection may itself instruct the lie)")
    print(f"results -> {a.out}/{a.label}/results.json")


if __name__ == "__main__":
    main()
