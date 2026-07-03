"""Bypass unsloth's get_peft_model filter — drive PEFT LoRA directly on the
unsloth-loaded 4-bit gpt-oss, explicitly targeting the per-expert Linear4bit modules.
"""
import re
from unsloth import FastLanguageModel
from peft import LoraConfig, get_peft_model

m, tok = FastLanguageModel.from_pretrained(
    "unsloth/gpt-oss-20b", max_seq_length=4096, load_in_4bit=True, dtype=None)

mods = [n for n, _ in m.named_modules()]
suffixes = sorted(set(
    re.search(r"(gate_up_projs|down_projs)\.\d+$", n).group(0)
    for n in mods if re.search(r"experts\.(gate_up_projs|down_projs)\.\d+$", n)))

cfg = LoraConfig(
    r=16, lora_alpha=32, lora_dropout=0.0, bias="none", task_type="CAUSAL_LM",
    target_modules=["q_proj", "k_proj", "v_proj", "o_proj"] + suffixes)
m2 = get_peft_model(m, cfg)

tr = [n for n, p in m2.named_parameters() if p.requires_grad]
n_ex = sum(1 for n in tr if "expert" in n.lower())
n_at = sum(1 for n in tr if "q_proj" in n or "v_proj" in n)
n_router = sum(1 for n in tr if "router" in n.lower() or "mlp.gate" in n.lower())
print("RAW_PEFT trainable=", len(tr), "EXPERT=", n_ex, "attn=", n_at, "router=", n_router)
print("expert_sample=", [n for n in tr if "expert" in n.lower()][:3])
