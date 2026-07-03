"""Discover the gpt-oss expert module structure and try to attach LoRA to the
per-expert Linears explicitly, bypassing unsloth's (wrong) singular-name MoE guess.
"""
import re
from unsloth import FastLanguageModel

m, tok = FastLanguageModel.from_pretrained(
    "unsloth/gpt-oss-20b", max_seq_length=4096, load_in_4bit=True, dtype=None)

mod_names = [n for n, _ in m.named_modules()]
# per-expert Linear modules?  e.g. ...mlp.experts.gate_up_projs.0
expert_mods = [n for n in mod_names if re.search(r"experts\.(gate_up_projs|down_projs)\.\d+$", n)]
print("EXPERT_MODULE_COUNT:", len(expert_mods))
print("EXPERT_MODULE_SAMPLE:", expert_mods[:3])
# container modules (the ModuleList / holder)
containers = [n for n in mod_names if re.search(r"experts\.(gate_up_projs|down_projs)$", n)]
print("CONTAINER_SAMPLE:", containers[:2])
# module type of one expert entry, if any
if expert_mods:
    sub = dict(m.named_modules())[expert_mods[0]]
    print("EXPERT_MODULE_TYPE:", type(sub).__name__)
if containers:
    sub = dict(m.named_modules())[containers[0]]
    print("CONTAINER_TYPE:", type(sub).__name__)

# unique PEFT endswith-keys for the per-expert Linears
suffixes = sorted(set(re.search(r"(gate_up_projs|down_projs)\.\d+$", n).group(0) for n in expert_mods))
print("SUFFIX_KEYS_N:", len(suffixes), "sample:", suffixes[:4])


def report(tag, model):
    tr = [n for n, p in model.named_parameters() if p.requires_grad]
    n_ex = sum(1 for n in tr if "expert" in n.lower())
    n_at = sum(1 for n in tr if "q_proj" in n or "v_proj" in n)
    print(tag, "trainable=", len(tr), "EXPERT=", n_ex, "attn=", n_at)
    print(tag, "expert_sample=", [n for n in tr if "expert" in n.lower()][:3])


targets = ["q_proj", "k_proj", "v_proj", "o_proj"] + suffixes
print("TRYING target_modules with", len(targets), "keys (attn + per-expert Linears)")
try:
    m2 = FastLanguageModel.get_peft_model(
        m, r=16, lora_alpha=32, target_modules=targets,
        use_gradient_checkpointing="unsloth", random_state=0)
    report("[per-expert-modules]", m2)
except Exception as e:
    import traceback
    print("[per-expert-modules] ERR:", repr(e)[:300])
    traceback.print_exc()
