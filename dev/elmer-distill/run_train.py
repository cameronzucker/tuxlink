#!/usr/bin/env python3
"""Phase-A LoRA training on the Harmony gold JSONL — RUN ON THE POD (GPU).

Loads gpt-oss-20b 4-bit, attaches LoRA to attention + ALL MoE experts (router
EXCLUDED — the cracked per-expert targeting from the de-risked smoke), trains on
the loss-masked gold (loss on ASSISTANT spans only, from dataset.assemble's
loss_spans), saves the adapter. Serve via smoke/gguf_export.py (bf16-merge ->
GGUF -> ollama) for the acceptance eval.

  python3 run_train.py --data eval-runs/train.jsonl --out /root/elmer-train/adapter --epochs 3

First real training run — the training PATH is de-risked (smoke passed load + attn/
expert LoRA + backprop); this adds real loss-masked data + full epochs.
"""
import argparse
import json
import re

DEFAULT_MODEL_ID = "unsloth/gpt-oss-20b"   # 120b: --model-id unsloth/gpt-oss-120b
ROUTER_FORBIDDEN = ("router", "gate.weight", "mlp.gate")
ATTENTION_TARGETS = ["q_proj", "k_proj", "v_proj", "o_proj"]

_EXPERT_SUFFIX_RE = re.compile(r"experts\.(gate_up_projs|down_projs)\.\d+$")


def expert_suffixes(module_names):
    """The per-expert LoRA target suffixes discovered from a model's module names —
    MODEL-AGNOSTIC (same gpt-oss MoE layout for 20b and 120b; only the expert COUNT
    differs, and every index is captured). unsloth's wrapper hard-codes the fused
    expert name and drops the per-expert Linear4bit modules, so PEFT is driven with
    these discovered suffixes directly (de-risked smoke recipe)."""
    return sorted(set(
        re.search(r"(gate_up_projs|down_projs)\.\d+$", n).group(0)
        for n in module_names if _EXPERT_SUFFIX_RE.search(n)))


def is_router_param(name):
    """True iff a parameter name belongs to the MoE router/gate (must stay FROZEN —
    training it destabilizes expert routing; Codex-B)."""
    return any(f in name for f in ROUTER_FORBIDDEN)


def _attach_lora(model, r=16, alpha=32):
    """LoRA on attention + every MoE expert; router frozen."""
    from peft import LoraConfig, get_peft_model
    suffixes = expert_suffixes([n for n, _ in model.named_modules()])
    if not suffixes:
        raise RuntimeError("no per-expert modules — layout changed; see smoke/diag_experts.py")
    cfg = LoraConfig(r=r, lora_alpha=alpha, lora_dropout=0.0, bias="none",
                     task_type="CAUSAL_LM",
                     target_modules=ATTENTION_TARGETS + suffixes)
    m = get_peft_model(model, cfg)
    m.enable_input_require_grads()   # backprop through the frozen 4-bit base
    return m


def build_dataset(path, tokenizer, max_len):
    """Tokenize each Harmony row; mask loss to the assistant char-spans (label=-100
    elsewhere) via offset mapping. Drop rows whose spans truncated away entirely."""
    from datasets import Dataset
    rows = [json.loads(l) for l in open(path)]
    out, dropped = [], 0
    for r in rows:
        text, spans = r["text"], r["loss_spans"]
        enc = tokenizer(text, return_offsets_mapping=True, truncation=True, max_length=max_len)
        ids, offs = enc["input_ids"], enc["offset_mapping"]
        labels = [tid if any(not (e <= a or s >= b) for (a, b) in spans) else -100
                  for tid, (s, e) in zip(ids, offs)]
        if all(lab == -100 for lab in labels):
            dropped += 1
            continue
        out.append({"input_ids": ids, "attention_mask": [1] * len(ids), "labels": labels})
    print(f"[train] dataset: {len(out)} usable ({dropped} dropped for full truncation)")
    return Dataset.from_list(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model-id", default=DEFAULT_MODEL_ID,
                    help="base model to LoRA. Default 20b; pass unsloth/gpt-oss-120b for the "
                         "120b cold-transfer build (per-expert targeting is discovered "
                         "dynamically, so no other flag changes between the two).")
    ap.add_argument("--data", required=True)
    ap.add_argument("--out", default="/root/elmer-train/adapter")
    ap.add_argument("--epochs", type=float, default=3)
    ap.add_argument("--max-seq-length", type=int, default=4096)
    ap.add_argument("--lr", type=float, default=2e-4)
    ap.add_argument("--batch", type=int, default=1)
    ap.add_argument("--grad-accum", type=int, default=8)
    ap.add_argument("--r", type=int, default=16)
    ap.add_argument("--precision", choices=["4bit", "bf16"], default="4bit",
                    help="base-weight precision. 4bit = QLoRA (proven, low VRAM). "
                         "bf16 = full-precision frozen base + LoRA (strictly better "
                         "adapter quality, ~40GB+ VRAM — the H200's 144GB fits it "
                         "easily). NOT full fine-tuning either way: only the LoRA "
                         "adapter trains; the base stays frozen.")
    a = ap.parse_args()

    from unsloth import FastLanguageModel
    if a.precision == "bf16":
        import torch
        load_in_4bit, dtype = False, torch.bfloat16
    else:
        load_in_4bit, dtype = True, None
    print(f"[train] model={a.model_id} base precision={a.precision} "
          f"(load_in_4bit={load_in_4bit}) r={a.r}")
    model, tokenizer = FastLanguageModel.from_pretrained(
        model_name=a.model_id, max_seq_length=a.max_seq_length,
        load_in_4bit=load_in_4bit, dtype=dtype)
    model.gradient_checkpointing_enable()
    model = _attach_lora(model, r=a.r)

    tr = [n for n, p in model.named_parameters() if p.requires_grad]
    assert any("q_proj" in n for n in tr), "attention not trainable"
    assert any(("down_projs" in n or "gate_up_projs" in n) for n in tr), "experts not trainable (Codex-B)"
    assert not any(is_router_param(n) for n in tr), "router is trainable!"
    print(f"[train] trainable tensors={len(tr)} (attn+experts, router frozen)")

    ds = build_dataset(a.data, tokenizer, a.max_seq_length)
    if len(ds) == 0:
        raise SystemExit("empty training set")

    from transformers import Trainer, TrainingArguments, DataCollatorForSeq2Seq
    args = TrainingArguments(
        output_dir="/root/elmer-train/run", num_train_epochs=a.epochs,
        per_device_train_batch_size=a.batch, gradient_accumulation_steps=a.grad_accum,
        learning_rate=a.lr, warmup_ratio=0.05, lr_scheduler_type="cosine",
        logging_steps=2, save_strategy="no", bf16=True, report_to=[])
    collator = DataCollatorForSeq2Seq(tokenizer, padding=True, label_pad_token_id=-100)
    trainer = Trainer(model=model, args=args, train_dataset=ds, data_collator=collator)
    trainer.train()
    model.save_pretrained(a.out)
    print(f"[train] adapter saved -> {a.out}")
    print("[train] next: adapt smoke/gguf_export.py (ADAPTER=%s) -> ollama, then run_eval" % a.out)


if __name__ == "__main__":
    main()
