"""OpenAI-compatible API teacher client (tuxlink-48nyh).

A drop-in for OllamaClient so gold-gen / eval can use a cheap hosted OPEN teacher
(DeepSeek-V3/R1, Qwen2.5-72B, Llama-3.1-405B on DeepInfra / Together / Fireworks /
OpenRouter) at per-token pricing instead of renting a multi-GPU pod to self-host.
The harness already injects the client, so once .chat() returns the ollama-shaped
dict, run_gold / run_rebaseline / run_g0 work unchanged.

The two real translations: our multi-turn history uses ollama-style tool results
(`{role:tool, tool_name, content}`) with un-id'd assistant tool_calls, but OpenAI
requires each tool message to carry a `tool_call_id` linking it to the assistant
call; and reasoning models return `reasoning_content` / `<think>…</think>` that must
map to our `thinking` field.
"""
import json

from elmer_distill.api_client import to_openai_messages, from_openai_message, APIClient


def test_tool_call_id_linkage_and_arg_stringification():
    msgs = [
        {"role": "system", "content": "sys"},
        {"role": "user", "content": "do it"},
        {"role": "assistant", "content": "", "tool_calls": [
            {"function": {"name": "find_stations", "arguments": {"bands": ["40m"]}}},
            {"function": {"name": "position_status", "arguments": {}}}]},
        {"role": "tool", "tool_name": "find_stations", "content": '{"stations":[]}'},
        {"role": "tool", "tool_name": "position_status", "content": '{"grid":"DM43"}'},
        {"role": "assistant", "content": "done"},
    ]
    out = to_openai_messages(msgs)
    asst = out[2]
    assert asst["role"] == "assistant" and len(asst["tool_calls"]) == 2
    # OpenAI wants arguments as a JSON STRING, not a dict
    assert all(isinstance(tc["function"]["arguments"], str) for tc in asst["tool_calls"])
    ids = [tc["id"] for tc in asst["tool_calls"]]
    assert len(set(ids)) == 2, "tool_call ids must be unique"
    # the two tool results carry the matching ids, in order
    assert out[3]["role"] == "tool" and out[3]["tool_call_id"] == ids[0]
    assert out[4]["tool_call_id"] == ids[1]
    assert "tool_name" not in out[3], "must translate tool_name -> tool_call_id"


def test_from_openai_maps_reasoning_and_tool_calls():
    oai = {"content": "here", "reasoning_content": "my thinking",
           "tool_calls": [{"id": "c1", "type": "function",
                           "function": {"name": "docs_search", "arguments": '{"q":"x"}'}}]}
    m = from_openai_message(oai)
    assert m["content"] == "here" and m["thinking"] == "my thinking"
    assert m["tool_calls"][0]["function"]["name"] == "docs_search"
    assert m["tool_calls"][0]["function"]["arguments"] == '{"q":"x"}'


def test_think_tags_extracted_from_content():
    """Some providers inline the reasoning as <think>…</think> in content rather than a
    separate field — pull it into `thinking` so the trajectory stays clean."""
    m = from_openai_message({"content": "<think>reasoning here</think>final answer",
                             "tool_calls": []})
    assert m["thinking"].strip() == "reasoning here"
    assert m["content"].strip() == "final answer"


def test_chat_returns_ollama_shape_dropin():
    def fake_transport(url, headers, data):
        assert url.endswith("/chat/completions")
        assert headers["Authorization"] == "Bearer KEY"
        body = json.loads(data)
        assert body["model"] == "deepseek-v3"
        assert body["messages"][0]["role"] == "system"
        assert body["tools"][0]["function"]["name"] == "server_info"
        return json.dumps({"choices": [{"message": {
            "content": "ok", "reasoning_content": "",
            "tool_calls": [{"id": "c1", "type": "function",
                            "function": {"name": "server_info", "arguments": "{}"}}]}}]}).encode()

    c = APIClient(base_url="https://x/v1", api_key="KEY", transport=fake_transport)
    d = c.chat("deepseek-v3",
               [{"role": "system", "content": "s"}, {"role": "user", "content": "u"}],
               tools=[{"type": "function", "function": {"name": "server_info"}}])
    # consumed exactly like an ollama /api/chat response
    msg = d["message"]
    assert msg["content"] == "ok"
    assert msg["tool_calls"][0]["function"]["name"] == "server_info"


def test_chat_retries_transient_then_succeeds():
    calls = {"n": 0}

    def flaky(url, headers, data):
        calls["n"] += 1
        if calls["n"] < 3:
            raise TimeoutError("transient")
        return json.dumps({"choices": [{"message": {"content": "done", "tool_calls": []}}]}).encode()

    c = APIClient(base_url="https://x/v1", api_key="K", transport=flaky, retries=3, backoff=0)
    d = c.chat("m", [{"role": "user", "content": "hi"}], tools=[])
    assert d["message"]["content"] == "done" and calls["n"] == 3
