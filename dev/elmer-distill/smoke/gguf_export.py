"""GGUF export for the LoRA-tuned gpt-oss-20b, bypassing the bnb-4bit->GGUF wall.

unsloth's `save_pretrained_gguf` fails on the bnb-4bit model ("Quant method not yet
supported: 'bitsandbytes'"). Instead: reload the base in bf16 (no bnb), apply the
trained adapter, merge, save a plain bf16 model, then convert THAT to GGUF (the
converter handles bf16). Run on the pod after the training smoke saves its adapter.
"""
import subprocess
import sys

import torch
from transformers import AutoModelForCausalLM, AutoTokenizer
from peft import PeftModel

BASE = "unsloth/gpt-oss-20b"
ADAPTER = "/root/elmer-smoke/adapter"
MERGED = "/root/elmer-smoke/merged16"
GGUF = "/root/elmer-smoke/merged.gguf"
CONVERTER = "/root/.unsloth/llama.cpp/unsloth_convert_hf_to_gguf.py"


def log(m):
    print(f"[GGUF] {m}", flush=True)


log("loading base in bf16 (CPU) ...")
base = AutoModelForCausalLM.from_pretrained(BASE, torch_dtype=torch.bfloat16, device_map="cpu")
tok = AutoTokenizer.from_pretrained(BASE)

log("applying + merging adapter ...")
merged = PeftModel.from_pretrained(base, ADAPTER).merge_and_unload()
merged.save_pretrained(MERGED, safe_serialization=True)
tok.save_pretrained(MERGED)
log("merged bf16 saved; converting to GGUF ...")

rc = subprocess.run([sys.executable, CONVERTER, "--outfile", GGUF, "--outtype", "bf16", MERGED]).returncode
log(f"convert rc={rc}")
if rc != 0:
    log("GGUF convert FAILED — see stderr above")
    sys.exit(1)

log("creating ollama model ...")
mf = f"FROM {GGUF}\n"
open("/root/elmer-smoke/Modelfile.merged", "w").write(mf)
subprocess.run(["ollama", "create", "elmer-smoke-20b", "-f", "/root/elmer-smoke/Modelfile.merged"], check=True)
log("ALL EXPORT PASS")
