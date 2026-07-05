# Serving a fine-tuned gpt-oss (re-fuser) — pod runbook

Turnkey sequence for taking the trained per-expert LoRA adapter to an
OpenAI-compatible endpoint. Grounded in the 2026-07-04 serving-wall adrev
(`dev/handoffs/2026-07-04-moss-oak-lichen-120b-trained-serving-wall.md`) and
issue **tuxlink-pt2xo**. Every checkpoint passes through the mechanical key-gate
(`run_gate.py`, committed + tested, GPU-free) before any server touches it.

## The one thing that is settled

unsloth trains gpt-oss as **per-expert LoRA** (`experts.gate_up_projs.<i>` /
`down_projs.<i>`, `router.linear.weight`). vLLM / SGLang / llama.cpp all require
the **canonical fused** layout (`experts.gate_up_proj` / `down_proj` stacked
`(num_experts, …)`, `router.weight`). unsloth's own `save_pretrained_merged`
re-fuse is broken for dense per-expert LoRA (unsloth #3701, unfixed). A wrong
layout is **not** rejected loudly — vLLM silently corrupts. The re-fuse is the
real engineering task; the key-gate is the tripwire.

## Two independent checks — do not conflate them

1. **Key-gate (`run_gate.py`) — LAYOUT.** Cheap, GPU-free, deterministic. Catches
   unfused experts, unfused router, hollow merge (#3701), residual bnb-4bit quant.
2. **Content oracle — SEMANTICS.** The key-gate cannot tell whether the expert
   *training deltas* actually landed. A merge onto a layout-mismatched base can
   emit a checkpoint that PASSES the gate yet has dropped every expert delta
   (see approach A2 below). The only defense is to diff completions against the
   as-trained model.

**Capture the oracle FIRST**, before any re-export, from the as-trained model
via the slow in-process transformers path (`peft_eval.py` — base bnb-4bit +
`PeftModel.from_pretrained`, no merge): 3–5 fixed prompts, greedy decode, save
the completions. Every candidate servable checkpoint must reproduce them.

## Approaches, in order of preference

| # | Method | Layout risk | Content risk |
|---|--------|-------------|--------------|
| **A1** | unsloth `save_pretrained_merged(save_method="merged_16bit")` on the as-trained PeftModel | may hollow (#3701) or emit per-expert | low if it fuses at all |
| **A3** | **Stacking re-fuser**: peft `merge_and_unload` on the **4bit** base (per-expert deltas bake in cleanly) → programmatically stack `…projs.<i>` → `(num_experts,…)` with correct transpose/interleave, rename `router.linear.weight`→`router.weight` | none by construction | none by construction |
| A2 | peft `merge_and_unload` on the **bf16** base (`unsloth/gpt-oss-120b`) | passes gate (fused) | **HIGH — silently drops expert deltas**: bf16 base keeps experts fused, so per-expert adapter keys never bind (bd-5tfkm). Only attention LoRA lands. |

A2 is the trap: it is the fastest to type and it passes the layout gate. Do not
ship it unless the content oracle confirms the expert deltas survived — they will
not. A3 is the durable answer and is what tuxlink-pt2xo delivers; A1 is worth one
attempt because if it fuses, it is free.

## Sequence

```bash
# 0. Provision a CUDA box (≥96 GB VRAM or big-RAM CPU for the merge); bring adapter.
bash pod_bootstrap.sh
# adapter -> /root/elmer-train/adapter-120b   (from elmer-artifacts/adapter-120b-2026-07-04/)

# 1. Content oracle from the as-trained model (ground truth for every check below).
python3 peft_eval.py --adapter /root/elmer-train/adapter-120b \
    --base unsloth/gpt-oss-120b-unsloth-bnb-4bit --oracle-prompts oracle.jsonl \
    --out oracle-reference.json      # save the completions; do NOT skip

# 2a. Try A1 (free if it works).
#     unsloth: model.save_pretrained_merged("/root/merged-a1", tok, save_method="merged_16bit")
python3 run_gate.py /root/merged-a1 && echo "A1 layout OK" || echo "A1 failed gate -> A3"

# 2b. If A1 fails the gate: build/run the stacking re-fuser (A3, tuxlink-pt2xo).
#     clean per-expert merge, then stack:
#     merge_and_unload on the 4bit base -> /root/merged-clean (per-expert bf16)
#     refuse: stack experts.{gate_up,down}_projs.<i> -> (E, …); rename router.linear.weight
python3 run_gate.py /root/merged-fused        # MUST pass before proceeding

# 3. GGUF + serve (llama.cpp is the only route unsloth documents for a fine-tuned gpt-oss).
python3 convert_hf_to_gguf.py /root/merged-fused --outfile elmer-120b.gguf   # bf16 outtype
llama-server -m elmer-120b.gguf --jinja --host 0.0.0.0 --port 8080

# 4. Content oracle diff — the go/no-go. Point api_client at http://…:8080/v1,
#    regenerate the oracle prompts, diff vs oracle-reference.json. Must match.
```

Torch-env hazard (**tuxlink-xutv1**): the GGUF converter's pip requirements
downgrade torch to `+cpu` and break the next training run. Run all trains before
any serve, or isolate the converter install in its own venv.

MXFP4 note: `save_method="mxfp4"` hits "No MXFP4 tensors found" on a dequantized
merge (llama.cpp #15146 / unsloth #3817) — export **bf16**, not mxfp4.

## Scoping decision still open (tuxlink-pt2xo)

Fix unsloth #3701 upstream (minimal, community-contributable) vs. a standalone
stacking re-fuser (more reusable, no unsloth-internals coupling). Settle
empirically on the pod: if A1 hollows exactly as #3701 predicts, the standalone
re-fuser (A3) is the lower-risk build and serves every future fine-tune.
