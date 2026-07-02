#!/usr/bin/env python3
"""Micro-LoRA training-path smoke (Codex E, spec §11a) — RUN ON THE A100 POD.

Proves the training path works BEFORE any real gold-gen/spend:
  1. load openai/gpt-oss-20b via Unsloth
  2. ASSERT LoRA target params include attention q/k/v/o + expert-MLP
     gate/up/down_proj and EXCLUDE the router/gate
  3. 10 training steps on ~2 Harmony examples
  4. merge -> GGUF -> Ollama Modelfile -> load in ollama
  5. send ONE tool-call prompt through the harness; assert a well-formed tool call

Prints PASS/FAIL per stage; exits non-zero on any failure. Record resolved
package versions into requirements-train.txt after the first green run.

Usage on pod:  python3 micro_lora_smoke.py 2>&1 | tee smoke.log
"""
import json
import subprocess
import sys
import urllib.request

MODEL_ID = "unsloth/gpt-oss-20b"          # Unsloth's gpt-oss-20b (BnB-friendly)
OUT_ADAPTER = "/root/elmer-smoke/adapter"
OUT_GGUF = "/root/elmer-smoke/model.gguf"
OLLAMA_TAG = "elmer-smoke-20b"

# LoRA targets: attention + expert-MLP projections; router/gate EXCLUDED.
TARGET_MODULES = ["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"]
ROUTER_FORBIDDEN = ("router", "gate.weight", "mlp.gate")   # must NOT be trainable


def stage(msg):
    print(f"[SMOKE] {msg}", flush=True)


def fail(msg):
    print(f"[SMOKE][FAIL] {msg}", flush=True)
    sys.exit(1)


def main():
    # --- Stage 1: load ---
    stage("loading gpt-oss-20b via Unsloth ...")
    from unsloth import FastLanguageModel
    model, tokenizer = FastLanguageModel.from_pretrained(
        model_name=MODEL_ID, max_seq_length=4096, load_in_4bit=True, dtype=None)
    model = FastLanguageModel.get_peft_model(
        model, r=16, lora_alpha=32, target_modules=TARGET_MODULES,
        use_gradient_checkpointing="unsloth", random_state=0)
    stage("model + LoRA adapter loaded")

    # --- Stage 2: assert trainable param names ---
    trainable = [n for n, p in model.named_parameters() if p.requires_grad]
    assert trainable, "no trainable params"
    hit_attn = any(".q_proj" in n or ".v_proj" in n for n in trainable)
    hit_expert = any("up_proj" in n or "down_proj" in n or "gate_proj" in n for n in trainable)
    hit_router = any(any(r in n for r in ROUTER_FORBIDDEN) for n in trainable)
    if not hit_attn:
        fail("attention projections not trainable")
    if not hit_expert:
        fail("expert-MLP projections not trainable — attention-only underfits (Codex B)")
    if hit_router:
        fail("router is trainable — must be excluded")
    stage(f"target params OK: attn+expert trainable, router excluded ({len(trainable)} tensors)")

    # --- Stage 3: 10 training steps on 2 Harmony examples ---
    stage("running 10 micro training steps ...")
    from trl import SFTTrainer, SFTConfig
    from datasets import Dataset
    examples = _harmony_examples(tokenizer)
    ds = Dataset.from_list([{"text": t} for t in examples])
    trainer = SFTTrainer(
        model=model, tokenizer=tokenizer, train_dataset=ds,
        args=SFTConfig(max_steps=10, per_device_train_batch_size=1,
                       gradient_accumulation_steps=1, learning_rate=2e-4,
                       logging_steps=1, output_dir="/root/elmer-smoke/run", report_to=[]))
    trainer.train()
    model.save_pretrained(OUT_ADAPTER)
    stage("10 steps done; adapter saved")

    # --- Stage 4: merge -> GGUF -> Ollama ---
    stage("merging + exporting GGUF ...")
    model.save_pretrained_gguf("/root/elmer-smoke/gguf", tokenizer, quantization_method="q8_0")
    modelfile = f"FROM {OUT_GGUF}\n"
    with open("/root/elmer-smoke/Modelfile", "w") as f:
        f.write(modelfile)
    subprocess.run(["ollama", "create", OLLAMA_TAG, "-f", "/root/elmer-smoke/Modelfile"], check=True)
    stage("ollama model created")

    # --- Stage 5: one tool-call prompt through the harness ---
    stage("sending one tool-call prompt ...")
    body = {"model": OLLAMA_TAG, "stream": False,
            "messages": [{"role": "user", "content": "What is my grid square?"}],
            "tools": [{"type": "function", "function": {"name": "position_status",
                       "description": "Report the operator's grid.", "parameters": {"type": "object", "properties": {}}}}]}
    req = urllib.request.Request("http://127.0.0.1:11434/api/chat",
                                 data=json.dumps(body).encode(), headers={"Content-Type": "application/json"})
    resp = json.loads(urllib.request.urlopen(req, timeout=600).read())
    tcs = (resp.get("message") or {}).get("tool_calls") or []
    if not tcs:
        fail("tuned model returned no tool call")
    stage(f"tool call OK: {tcs[0]['function']['name']}")
    print("[SMOKE] ALL STAGES PASS", flush=True)


def _harmony_examples(tokenizer):
    """Two tiny gpt-oss chat examples rendered by the tokenizer's chat template."""
    convos = [
        [{"role": "user", "content": "What is my grid?"},
         {"role": "assistant", "content": "Calling position_status."}],
        [{"role": "user", "content": "Closest 80m gateway?"},
         {"role": "assistant", "content": "Calling find_stations for 80m."}],
    ]
    return [tokenizer.apply_chat_template(c, tokenize=False) for c in convos]


if __name__ == "__main__":
    main()
