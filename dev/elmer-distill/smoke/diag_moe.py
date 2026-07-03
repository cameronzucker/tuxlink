from unsloth import FastLanguageModel

m, tok = FastLanguageModel.from_pretrained(
    "unsloth/gpt-oss-20b", max_seq_length=4096, load_in_4bit=True, dtype=None)

pnames = [n for n, _ in m.named_parameters()]
expert_params = [n for n in pnames if "expert" in n.lower()]
print("EXPERT_PARAM_SAMPLES:", expert_params[:6])
print("HAS_gate_up_proj:", any("gate_up_proj" in n for n in pnames),
      "HAS_experts_down_proj:", any("experts.down_proj" in n for n in pnames))


def report(tag, model):
    tr = [n for n, p in model.named_parameters() if p.requires_grad]
    n_expert = sum(1 for n in tr if "expert" in n.lower())
    n_attn = sum(1 for n in tr if "q_proj" in n or "v_proj" in n)
    ex = [n for n in tr if "expert" in n.lower()][:4]
    print(tag, "trainable=", len(tr), "expert_trainable=", n_expert, "attn_trainable=", n_attn)
    print(tag, "expert_names=", ex)


try:
    m2 = FastLanguageModel.get_peft_model(
        m, r=16, lora_alpha=32,
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj"],
        target_parameters=["mlp.experts.gate_up_proj", "mlp.experts.down_proj"],
        use_gradient_checkpointing="unsloth", random_state=0)
    report("[target_parameters]", m2)
except Exception as e:
    import traceback
    print("[target_parameters] ERR:", repr(e)[:400])
    traceback.print_exc()
