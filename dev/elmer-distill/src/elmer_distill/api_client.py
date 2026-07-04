"""OpenAI-compatible API client — a drop-in for OllamaClient (tuxlink-48nyh).

Lets gold-gen / eval drive a cheap hosted OPEN teacher (DeepSeek-V3/R1, Qwen2.5-72B,
Llama-3.1-405B on DeepInfra / Together / Fireworks / OpenRouter) at per-token pricing
instead of renting a multi-GPU pod to self-host a 405B/671B. The harness injects the
client and only reads the ollama-shaped response, so run_gold / run_rebaseline / run_g0
work unchanged once .chat() returns `{"message": {content, thinking, tool_calls}}`.

Dependency-free (stdlib urllib) so it runs anywhere the rest of the harness does. The
teacher is an OPEN model just hosted per-token — NOT frontier gold (scope call
project_elmer_no_frontier_gold_scope_20b), so this does not change the gold's provenance.
"""
import json
import os
import re
import time
import urllib.error
import urllib.request

_THINK = re.compile(r"<think>(.*?)</think>", re.DOTALL)


def to_openai_messages(messages):
    """Translate ollama-style history to OpenAI chat format. The load-bearing part is
    tool-call-id linkage: our history carries `{role:tool, tool_name, content}` with
    un-id'd assistant tool_calls, but OpenAI requires each tool message to reference the
    `tool_call_id` of the assistant call it answers. Assistant tool_calls are id'd in
    order; the following tool messages are matched to those ids in order (the harness
    appends one tool result per tool_call, in call order)."""
    out, pending = [], []
    for m in messages:
        role = m.get("role")
        if role == "assistant":
            om = {"role": "assistant", "content": m.get("content") or ""}
            tcs = m.get("tool_calls") or []
            if tcs:
                pending = []
                oai = []
                for i, tc in enumerate(tcs):
                    fn = tc.get("function", {}) or {}
                    args = fn.get("arguments", {})
                    if not isinstance(args, str):
                        args = json.dumps(args)
                    cid = f"call_{len(out)}_{i}"
                    pending.append(cid)
                    oai.append({"id": cid, "type": "function",
                                "function": {"name": fn.get("name", ""), "arguments": args}})
                om["tool_calls"] = oai
            out.append(om)
        elif role == "tool":
            cid = pending.pop(0) if pending else f"call_{len(out)}_0"
            out.append({"role": "tool", "tool_call_id": cid, "content": m.get("content") or ""})
        else:  # system, user
            out.append({"role": role, "content": m.get("content") or ""})
    return out


def from_openai_message(msg):
    """Map an OpenAI response message to the ollama shape the harness consumes:
    `{content, thinking, tool_calls:[{function:{name, arguments}}]}`. Reasoning comes
    from `reasoning_content`/`reasoning`, or is extracted from inline <think>…</think>."""
    content = msg.get("content") or ""
    thinking = msg.get("reasoning_content") or msg.get("reasoning") or ""
    if not thinking:
        found = _THINK.findall(content)
        if found:
            thinking = "\n".join(found).strip()
            content = _THINK.sub("", content).strip()
    tcs = []
    for tc in (msg.get("tool_calls") or []):
        fn = tc.get("function", {}) or {}
        tcs.append({"function": {"name": fn.get("name", ""),
                                 "arguments": fn.get("arguments", "{}")}})
    return {"role": "assistant", "content": content, "thinking": thinking, "tool_calls": tcs}


def _urllib_transport(url, headers, data):
    req = urllib.request.Request(url, data=data, headers=headers)
    with urllib.request.urlopen(req, timeout=3600) as resp:
        return resp.read()


class APIClient:
    """Injectable OpenAI-compatible client. `transport(url, headers, data_bytes) -> bytes`
    is swappable for tests; the default posts via urllib. `api_key` falls back to
    $ELMER_TEACHER_API_KEY. Interface matches OllamaClient.chat exactly."""

    def __init__(self, base_url, api_key=None, num_ctx=32768, temperature=0, seed=None,
                 max_tokens=8192, retries=4, backoff=2.0, transport=None):
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key or os.environ.get("ELMER_TEACHER_API_KEY", "")
        self.num_ctx = num_ctx           # accepted for interface parity (unused by OpenAI API)
        self.temperature = temperature
        self.seed = seed
        self.max_tokens = max_tokens     # reasoning models (R1) need generous headroom
        self.retries = retries
        self.backoff = backoff
        self.transport = transport or _urllib_transport

    def chat(self, model, messages, tools, temperature=None):
        body = {
            "model": model,
            "messages": to_openai_messages(messages),
            "temperature": self.temperature if temperature is None else temperature,
            "max_tokens": self.max_tokens,
            "stream": False,
        }
        if tools:
            body["tools"] = tools            # already OpenAI-shaped in this harness
            body["tool_choice"] = "auto"
        if self.seed is not None:
            body["seed"] = self.seed
        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        data = json.dumps(body).encode()
        url = self.base_url + "/chat/completions"

        last = None
        for attempt in range(self.retries + 1):
            try:
                raw = self.transport(url, headers, data)
                d = json.loads(raw)
                choice = (d.get("choices") or [{}])[0]
                return {"message": from_openai_message(choice.get("message", {}) or {})}
            except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError, OSError) as e:
                last = e
                if attempt < self.retries:
                    time.sleep(self.backoff * (attempt + 1))
        raise last
