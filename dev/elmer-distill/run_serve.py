#!/usr/bin/env python3
"""Serve a LoRA-tuned gpt-oss-20b through ollama for the acceptance eval — POD.

Bypasses the bnb-4bit->GGUF wall: reload the base in bf16, apply + merge the
trained adapter, save plain bf16, convert THAT to GGUF (converter handles bf16),
`ollama create`. The eval then hits it like any other model.

  python3 run_serve.py --adapter /root/elmer-train/adapter --tag elmer-20b
"""
import argparse
import os
import subprocess
import sys

BASE = "unsloth/gpt-oss-20b"


def log(m):
    print(f"[serve] {m}", flush=True)


def ensure_converter():
    """unsloth bundles llama.cpp's converter on first GGUF export; on a fresh pod it
    may not exist yet. Find it, or clone llama.cpp as a fallback."""
    for p in ("/root/.unsloth/llama.cpp/unsloth_convert_hf_to_gguf.py",
              "/root/llama.cpp/convert_hf_to_gguf.py"):
        if os.path.exists(p):
            return p
    log("converter not found — cloning llama.cpp ...")
    subprocess.run(["git", "clone", "--depth", "1",
                    "https://github.com/ggml-org/llama.cpp", "/root/llama.cpp"], check=True)
    subprocess.run([sys.executable, "-m", "pip", "install", "-q",
                    "-r", "/root/llama.cpp/requirements/requirements-convert_hf_to_gguf.txt"],
                   check=False)
    return "/root/llama.cpp/convert_hf_to_gguf.py"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--adapter", required=True)
    ap.add_argument("--tag", default="elmer-20b")
    ap.add_argument("--base", default=BASE)
    ap.add_argument("--workdir", default="/root/elmer-serve")
    a = ap.parse_args()
    os.makedirs(a.workdir, exist_ok=True)
    merged = os.path.join(a.workdir, "merged16")
    gguf = os.path.join(a.workdir, f"{a.tag}.gguf")

    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer
    from peft import PeftModel

    log("loading base in bf16 (CPU) ...")
    base = AutoModelForCausalLM.from_pretrained(a.base, torch_dtype=torch.bfloat16, device_map="cpu")
    tok = AutoTokenizer.from_pretrained(a.base)

    log("applying + merging adapter ...")
    m = PeftModel.from_pretrained(base, a.adapter).merge_and_unload()
    m.save_pretrained(merged, safe_serialization=True)
    tok.save_pretrained(merged)

    converter = ensure_converter()
    log(f"converting to GGUF via {converter} ...")
    rc = subprocess.run([sys.executable, converter, "--outfile", gguf, "--outtype", "bf16", merged]).returncode
    if rc != 0:
        sys.exit(f"[serve] GGUF convert FAILED rc={rc}")

    mf = os.path.join(a.workdir, "Modelfile")
    open(mf, "w").write(f"FROM {gguf}\n")
    subprocess.run(["ollama", "create", a.tag, "-f", mf], check=True)
    log(f"ollama model '{a.tag}' created — ready for: run_eval.py --model {a.tag} --label {a.tag}")


if __name__ == "__main__":
    main()
