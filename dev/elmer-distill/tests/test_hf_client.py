"""In-process peft-eval client (tuxlink-48nyh, 120b build). The pure completion-aggregation
logic is tested offline; the GPU generate glue is pod-smoke-gated (no GPU/vocab on the Pi)."""
from elmer_distill.hf_client import aggregate_completion, PeftHFClient, tools_for_gpt_oss_template


def test_tools_kept_wrapped_with_descriptions_for_gpt_oss_template():
    """gpt-oss's template unwraps the wrapper itself (set tool = tool.function) and reads
    description under a strict undefined — so KEEP the wrapper and guarantee a description on
    every function + parameter property (pod bring-up 2026-07-04)."""
    # a function with NO description and a param property with NO description (both would raise)
    tools = [{"type": "function", "function": {
        "name": "find_stations",
        "parameters": {"type": "object", "properties": {"bands": {"type": "array"}}}}}]
    out = tools_for_gpt_oss_template(tools)
    fn = out[0]["function"]
    assert out[0]["type"] == "function"                      # wrapper preserved
    assert fn["description"] == "find_stations"              # filled from name
    assert fn["parameters"]["properties"]["bands"]["description"] == ""   # param desc filled
    # an already-complete tool is preserved (description untouched)
    good = [{"type": "function", "function": {"name": "x", "description": "keep me",
                                              "parameters": {"type": "object", "properties": {}}}}]
    assert tools_for_gpt_oss_template(good)[0]["function"]["description"] == "keep me"


def _msg(channel=None, recipient=None, text=""):
    return {"channel": channel, "recipient": recipient, "content": [{"text": text}]}


def test_aggregate_splits_thinking_toolcalls_and_final():
    parsed = [
        _msg(channel="analysis", text="let me check status"),
        _msg(channel="commentary", recipient="functions.find_stations", text='{"bands":["30m"]}'),
        _msg(channel="commentary", recipient="functions.predict_path", text='{"frequencies_khz":[10125]}'),
        _msg(channel="final", text="Top 30m gateways staged."),
    ]
    out = aggregate_completion(parsed)
    assert out["thinking"] == "let me check status"
    assert out["content"] == "Top 30m gateways staged."
    assert [tc["function"]["name"] for tc in out["tool_calls"]] == ["find_stations", "predict_path"]
    assert out["tool_calls"][0]["function"]["arguments"] == '{"bands":["30m"]}'


def test_aggregate_toolcall_only_turn_has_empty_final():
    parsed = [_msg(channel="commentary", recipient="functions.position_status", text="{}")]
    out = aggregate_completion(parsed)
    assert out["content"] == "" and out["thinking"] == ""
    assert len(out["tool_calls"]) == 1


def test_aggregate_final_only_turn_has_no_toolcalls():
    out = aggregate_completion([_msg(channel="final", text="All done, nothing to send.")])
    assert out["tool_calls"] == [] and out["content"] == "All done, nothing to send."


def test_aggregate_ignores_non_function_recipients():
    # a commentary message addressed to the user/assistant (not functions.*) is not a tool call
    parsed = [_msg(channel="commentary", recipient="assistant", text="thinking out loud"),
              _msg(channel="final", text="answer")]
    out = aggregate_completion(parsed)
    assert out["tool_calls"] == [] and out["content"] == "answer"


class _FakeMsg:
    def __init__(self, d):
        self._d = d
    def to_dict(self):
        return self._d


class _FakeEnc:
    """Stands in for the openai_harmony encoding: records the completion ids it was asked to
    parse and returns canned messages."""
    def __init__(self, messages):
        self._messages = messages
        self.parsed_ids = None
    def parse_messages_from_completion_tokens(self, ids, role):
        self.parsed_ids = ids
        self.role = role
        return self._messages


def test_client_chat_pipes_prompt_through_generate_and_parse():
    enc = _FakeEnc([_FakeMsg(_msg(channel="final", text="hello"))])
    seen = {}
    def build_prompt(messages, tools):
        seen["messages"], seen["tools"] = messages, tools
        return [1, 2, 3]                       # prompt token ids
    def generate_fn(prompt_ids):
        seen["prompt_ids"] = prompt_ids
        return [9, 9, 9]                        # completion token ids
    client = PeftHFClient(enc, build_prompt, generate_fn, role_assistant="ASSISTANT")

    resp = client.chat("m", [{"role": "user", "content": "hi"}], tools=[{"name": "x"}])
    assert seen["prompt_ids"] == [1, 2, 3]      # build_prompt output fed to generate
    assert enc.parsed_ids == [9, 9, 9]          # generate output fed to the parser
    assert resp["message"]["content"] == "hello"
    assert resp["message"]["tool_calls"] == []
