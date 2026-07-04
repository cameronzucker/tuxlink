"""In-process transformers+peft client — eval a LoRA-tuned gpt-oss WITHOUT ollama/GGUF.

Why this exists (tuxlink-48nyh, 120b build): serving the trained 120b through the 20b's
`run_serve` path needs a bf16 base merge (~240GB) that does not fit one H200. This client
instead drives the SAME 4-bit base+adapter layout the training smoke already de-risks
(load_in_4bit base + PEFT adapter, minus backprop), so the acceptance eval fits one H200 and
avoids the merge entirely. It exposes the OllamaClient `.chat()` contract so the existing
agentic loop (teacher.run_scenario / baseline_g0.run_g0) and Judge run unchanged.

TOKEN-SPACE DISCIPLINE: render AND parse both go through the one `openai_harmony` encoding
(never mix the HF tokenizer's ids with the harmony parser) so the completion tokens parse back
cleanly — the same encoding harmony.py already trusts for training-token rendering.

POD-SMOKE GATE: the model-load + generate glue (`PeftHFClient.chat`) cannot run on the dev Pi
(no GPU, no vocab). Validate it on ONE scenario on the pod before the full eval — exactly as
`smoke/micro_lora_smoke.py` gates training. The parse aggregation below is pure and unit-tested
offline (it is where the real bugs hide); the glue around it is thin.
"""


def tools_for_gpt_oss_template(tools):
    """Make `tools` safe for the gpt-oss Harmony chat template (pod bring-up 2026-07-04).

    The template's `render_tool_namespace` unwraps the OpenAI wrapper ITSELF (`set tool =
    tool.function`), so it wants the WRAPPED `{"type":"function","function":{...}}` form that
    load_tools() already produces — do NOT unwrap. But it then reads `tool.description` and each
    `param_spec.description` under a STRICT jinja undefined that RAISES on a missing key
    (UndefinedError: 'dict object' has no attribute 'description'). So guarantee a `description`
    on every function AND every parameter property; leave the wrapper intact."""
    out = []
    for t in tools:
        fn = dict(t.get("function", t) if isinstance(t, dict) else {})
        fn.setdefault("description", fn.get("name", ""))
        params = fn.get("parameters")
        if isinstance(params, dict) and isinstance(params.get("properties"), dict):
            props = {}
            for name, spec in params["properties"].items():
                if isinstance(spec, dict):
                    spec = {**spec}
                    spec.setdefault("description", "")
                props[name] = spec
            fn["parameters"] = {**params, "properties": props}
        out.append({"type": "function", "function": fn})
    return out


def _text_of(msg_dict):
    return "".join(c.get("text", "") for c in (msg_dict.get("content") or [])
                   if isinstance(c, dict))


def aggregate_completion(msg_dicts):
    """Collapse one turn's worth of parsed Harmony completion messages into the single
    `{"content", "tool_calls", "thinking"}` message the agentic loop expects.

    gpt-oss emits a turn as SEVERAL messages: an `analysis`-channel message (the private
    reasoning), zero+ `commentary` messages addressed to `functions.<name>` (tool calls), and
    a `final`-channel message (the user-facing answer). This mirrors harmony.parse_trajectory's
    recipient/channel filtering but aggregates into ONE assistant message: thinking = analysis,
    tool_calls = every functions.* recipient, content = the final channel."""
    thinking, tool_calls, final = [], [], []
    for d in msg_dicts:
        recipient = d.get("recipient") or ""
        channel = d.get("channel")
        text = _text_of(d)
        if recipient.startswith("functions."):
            tool_calls.append({"function": {"name": recipient[len("functions."):],
                                            "arguments": text}})
        elif channel == "final":
            final.append(text)
        elif channel == "analysis":
            thinking.append(text)
    return {"content": "".join(final), "tool_calls": tool_calls, "thinking": "".join(thinking)}


class PeftHFClient:
    """Drive a HF causal LM (4-bit gpt-oss base + PEFT adapter) through the `.chat()` contract.

    `generate_fn(prompt_token_ids: list[int]) -> list[int]` returns the COMPLETION token ids
    (new tokens only). Injected so the render→parse pipeline is testable with a fake generator
    and the real one (model.generate) is supplied on the pod. `enc` is the openai_harmony
    encoding (harmony._enc()); `build_prompt(messages, tools) -> list[int]` renders the
    conversation for completion in the SAME encoding.
    """

    def __init__(self, enc, build_prompt, generate_fn, role_assistant):
        self.enc = enc
        self.build_prompt = build_prompt
        self.generate_fn = generate_fn
        self.role_assistant = role_assistant

    def chat(self, model, messages, tools, temperature=None):
        prompt_ids = self.build_prompt(messages, tools)
        completion_ids = self.generate_fn(prompt_ids)
        parsed = self.enc.parse_messages_from_completion_tokens(completion_ids, self.role_assistant)
        return {"message": aggregate_completion([m.to_dict() for m in parsed])}
